//! Audio-Engine: ein Dekodier-Thread (Symphonia) füllt einen Puffer, die Ausgabe (cpal/WASAPI)
//! holt sich daraus die Samples. Gesteuert wird über Befehle, Rückmeldungen kommen als Ereignisse.

pub mod dsp;
mod output;
pub mod source;
mod stretch;

use std::{
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

use symphonia::core::{
    codecs::{audio::AudioDecoderOptions, registry::CodecRegistry, CodecParameters},
    errors::Error as SymError,
    formats::{probe::Hint, FormatOptions, SeekMode, SeekTo, TrackType},
    io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    units::Time,
};

pub use output::{device_names, Shared};

/// Was gerade läuft, wird hiermit erkannt: Ereignisse alter Songs werden ignoriert.
pub type Generation = u64;

pub enum Command {
    /// Neue Quelle laden und (ab `start_at` Sekunden) abspielen
    Load { source: Box<dyn MediaSource>, hint: Option<String>, start_at: f64, generation: Generation, lufs: Option<f32> },
    /// Equalizer und Angleichung
    Dsp(dsp::Settings),
    /// Ausgabegerät ("" = Windows-Standard)
    Device(String),
    Play,
    Pause,
    Seek(f64),
    Stop,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// Song läuft (erste Samples sind in der Ausgabe)
    Started { generation: Generation, duration: f64 },
    Position { generation: Generation, seconds: f64 },
    Playing(bool),
    Ended { generation: Generation },
    Error { generation: Generation, message: String },
    /// Lautheit des Songs gemessen (nach 30 s)
    Measured { generation: Generation, lufs: f32 },
}

pub struct Engine {
    commands: mpsc::Sender<Command>,
    shared: Arc<Shared>,
    generation: AtomicU64,
}

impl Engine {
    pub fn start(events: impl Fn(Event) + Send + 'static) -> Arc<Self> {
        let shared = Arc::new(Shared::default());
        let (tx, rx) = mpsc::channel();
        let decoder_shared = shared.clone();
        thread::Builder::new()
            .name("audio-decoder".into())
            .spawn(move || decoder_thread(rx, decoder_shared, events))
            .expect("Audio-Thread");
        Arc::new(Self { commands: tx, shared, generation: AtomicU64::new(0) })
    }

    /// Lädt eine Quelle; gibt die Nummer zurück, unter der ihre Ereignisse ankommen.
    pub fn load(&self, source: Box<dyn MediaSource>, hint: Option<String>, start_at: f64, lufs: Option<f32>) -> Generation {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = self.commands.send(Command::Load { source, hint, start_at, generation, lufs });
        generation
    }

    /// Tempo (1 = normal); die Tonhöhe ändert sich mit
    /// Songzeit dessen, was gerade zu hören ist
    pub fn played_seconds(&self) -> f64 {
        self.shared.played_seconds()
    }

    /// Beim Tempo die Tonhöhe beibehalten
    pub fn set_preserve_pitch(&self, on: bool) {
        self.shared.preserve_pitch.store(on, Ordering::Relaxed);
    }

    pub fn set_rate(&self, rate: f32) {
        self.shared.rate.store(rate.clamp(0.5, 2.0).to_bits(), Ordering::Relaxed);
    }

    /// Blende für den Schlaf-Timer: Ziel (0–1) und Dauer in Sekunden
    pub fn fade(&self, target: f32, seconds: f32) {
        self.shared.set_fade(target, seconds);
    }

    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// Lautstärke 0–1 (bereits als tatsächlicher Verstärkungsfaktor)
    pub fn set_gain(&self, gain: f32) {
        self.shared.gain.store(gain.to_bits(), Ordering::Relaxed);
    }
}

fn codecs() -> &'static CodecRegistry {
    use std::sync::OnceLock;
    static REGISTRY: OnceLock<CodecRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut registry);
        registry.register_audio_decoder::<symphonia_adapter_opus_rs::OpusAdapter>();
        registry
    })
}

struct Current {
    format: Box<dyn symphonia::core::formats::FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::audio::AudioDecoder>,
    track_id: u32,
    time_base: Option<symphonia::core::units::TimeBase>,
    generation: Generation,
    started: bool,
    duration: f64,
}

fn open(source: Box<dyn MediaSource>, hint: Option<&str>) -> Result<(Box<dyn symphonia::core::formats::FormatReader>, Box<dyn symphonia::core::codecs::audio::AudioDecoder>, u32, Option<symphonia::core::units::TimeBase>, f64), String> {
    let mss = MediaSourceStream::new(source, MediaSourceStreamOptions::default());
    let mut probe_hint = Hint::new();
    if let Some(ext) = hint {
        probe_hint.with_extension(ext);
    }
    let format = symphonia::default::get_probe()
        .probe(&probe_hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| format!("Format nicht erkannt: {e}"))?;
    let track = format
        .default_track(TrackType::Audio)
        .or_else(|| format.first_track_known_codec(TrackType::Audio))
        .ok_or("Keine Audiospur gefunden")?;
    let Some(CodecParameters::Audio(params)) = &track.codec_params else {
        return Err("Keine Audiospur gefunden".into());
    };
    let decoder = codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| format!("Audioformat wird nicht unterstützt: {e}"))?;
    let time_base = track.time_base;
    let duration = match (time_base, track.duration) {
        (Some(tb), Some(d)) => tb.calc_duration(d).map(|t| t.as_secs_f64()).unwrap_or(0.0),
        _ => 0.0,
    };
    let track_id = track.id;
    Ok((format, decoder, track_id, time_base, duration))
}

fn decoder_thread(rx: mpsc::Receiver<Command>, shared: Arc<Shared>, events: impl Fn(Event)) {
    let mut output: Option<output::Output> = None;
    let mut device = String::new();
    let mut processor = dsp::Processor::new(dsp::Settings::default());
    let mut measured = false;
    let mut current: Option<Current> = None;
    let mut paused = true;
    let mut samples: Vec<f32> = Vec::new();
    let mut last_position_sent = -1.0f64;

    loop {
        // Befehle: ohne laufenden Song blockierend warten, sonst nur nachsehen
        let command = if current.is_none() || paused {
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(c) => Some(c),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match rx.try_recv() {
                Ok(c) => Some(c),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        };

        if let Some(command) = command {
            match command {
                Command::Load { source, hint, start_at, generation, lufs } => {
                    shared.clear();
                    current = None;
                    processor.start_track(lufs);
                    measured = lufs.is_some();
                    match open(source, hint.as_deref()) {
                        Ok((mut format, mut decoder, track_id, time_base, duration)) => {
                            if start_at > 0.5 {
                                if let Some(time) = Time::try_from_secs_f64(start_at) {
                                    let _ = format.seek(SeekMode::Accurate, SeekTo::Time { time, track_id: Some(track_id) });
                                    decoder.reset();
                                }
                            }
                            current = Some(Current { format, decoder, track_id, time_base, generation, started: false, duration });
                            paused = false;
                            shared.set_paused(false);
                            last_position_sent = -1.0;
                        }
                        Err(message) => events(Event::Error { generation, message }),
                    }
                }
                Command::Play => {
                    if current.is_some() {
                        paused = false;
                        shared.set_paused(false);
                        events(Event::Playing(true));
                    }
                }
                Command::Pause => {
                    paused = true;
                    shared.set_paused(true);
                    events(Event::Playing(false));
                }
                Command::Seek(seconds) => {
                    if let Some(c) = current.as_mut() {
                        if let Some(time) = Time::try_from_secs_f64(seconds.max(0.0)) {
                            let target = SeekTo::Time { time, track_id: Some(c.track_id) };
                            if c.format.seek(SeekMode::Accurate, target).is_ok() {
                                c.decoder.reset();
                                shared.clear_at(seconds);
                                events(Event::Position { generation: c.generation, seconds });
                            }
                        }
                    }
                }
                Command::Dsp(settings) => processor.set_settings(settings),
                Command::Device(name) => {
                    if name != device {
                        device = name;
                        // Beim nächsten Paket mit dem neuen Gerät öffnen
                        output = None;
                        shared.clear_at(shared.played_seconds());
                    }
                }
                Command::Stop => {
                    current = None;
                    paused = true;
                    shared.clear();
                    shared.set_paused(true);
                }
            }
            continue;
        }

        // Position melden (was gerade aus dem Lautsprecher kommt), etwa alle 250 ms
        if let Some(c) = &current {
            let position = shared.played_seconds();
            if (position - last_position_sent).abs() >= 0.25 {
                last_position_sent = position;
                events(Event::Position { generation: c.generation, seconds: position });
            }
        }

        let Some(c) = current.as_mut() else { continue };
        if paused {
            continue;
        }

        // Puffer voll genug: kurz warten
        // (2 Sekunden: genug Luft, falls der Rechner kurz beschäftigt ist)
        if shared.buffered_seconds() > 2.0 {
            thread::sleep(Duration::from_millis(20));
            continue;
        }

        let packet = match c.format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => {
                // Ende der Datei: warten, bis die Ausgabe alles gespielt hat
                if shared.buffered_seconds() <= 0.02 {
                    let generation = c.generation;
                    current = None;
                    paused = true;
                    events(Event::Ended { generation });
                } else {
                    thread::sleep(Duration::from_millis(20));
                }
                continue;
            }
            Err(SymError::ResetRequired) => {
                c.decoder.reset();
                continue;
            }
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Manche Formate melden das Ende als EOF
                if shared.buffered_seconds() <= 0.02 {
                    let generation = c.generation;
                    current = None;
                    paused = true;
                    events(Event::Ended { generation });
                } else {
                    thread::sleep(Duration::from_millis(20));
                }
                continue;
            }
            Err(e) => {
                let generation = c.generation;
                current = None;
                paused = true;
                shared.clear();
                events(Event::Error { generation, message: format!("Wiedergabefehler: {e}") });
                continue;
            }
        };
        if packet.track_id != c.track_id {
            continue;
        }
        let pts = c.time_base.and_then(|tb| tb.calc_time(packet.pts)).map(|t| t.as_secs_f64()).unwrap_or(0.0);

        let buffer = match c.decoder.decode(&packet) {
            Ok(buffer) => buffer,
            // Kaputtes Paket überspringen
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => {
                let generation = c.generation;
                current = None;
                paused = true;
                events(Event::Error { generation, message: format!("Dekodierfehler: {e}") });
                continue;
            }
        };
        let rate = buffer.spec().rate();
        let channels = buffer.spec().channels().count().max(1);
        samples.clear();
        buffer.copy_to_vec_interleaved(&mut samples);

        processor.process(&mut samples, channels, rate);
        // Nach 30 s gilt die Messung als brauchbar
        if !measured && processor.blocks.len() >= 75 {
            measured = true;
            if let Some(lufs) = processor.integrated() {
                events(Event::Measured { generation: c.generation, lufs });
            }
        }

        // Ausgabe bei Bedarf (erstes Mal) öffnen
        if output.is_none() {
            match output::Output::open(shared.clone(), &device) {
                Ok(o) => output = Some(o),
                Err(message) => {
                    let generation = c.generation;
                    current = None;
                    paused = true;
                    events(Event::Error { generation, message });
                    continue;
                }
            }
        }
        shared.push(&samples, channels, rate, pts);

        if !c.started {
            c.started = true;
            events(Event::Started { generation: c.generation, duration: c.duration });
            events(Event::Playing(true));
        }
    }
}

/// Zahl als Bits für Atomics (f32)
pub(crate) fn load_f32(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}
