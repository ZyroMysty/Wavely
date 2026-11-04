//! Ausgabe über cpal (unter Windows WASAPI). Der Dekodier-Thread legt fertig umgerechnete Samples
//! (Abtastrate und Kanäle des Geräts) in den gemeinsamen Puffer, der Audio-Callback entnimmt sie.

use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    },
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use super::load_f32;

pub struct Shared {
    buffer: Mutex<Buffer>,
    paused: AtomicBool,
    /// Verstärkung 0–1 (als f32-Bits)
    pub gain: AtomicU32,
    out_rate: AtomicU32,
    out_channels: AtomicU32,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            buffer: Mutex::new(Buffer::default()),
            paused: AtomicBool::new(true),
            gain: AtomicU32::new(1.0f32.to_bits()),
            out_rate: AtomicU32::new(48_000),
            out_channels: AtomicU32::new(2),
        }
    }
}

#[derive(Default)]
struct Buffer {
    samples: VecDeque<f32>,
    /// Songzeit (Sekunden) am Ende des Puffers
    end_seconds: f64,
    resampler: Resampler,
}

/// Lineare Umrechnung der Abtastrate (z. B. 44,1 kHz → 48 kHz), Zustand über Pakete hinweg
#[derive(Default)]
struct Resampler {
    in_rate: u32,
    channels: usize,
    /// Letztes Eingangs-Frame (für die Interpolation über die Paketgrenze)
    last: Vec<f32>,
    /// Position zwischen `last` und dem nächsten Frame (0–1)
    phase: f64,
}

impl Shared {
    pub fn clear(&self) {
        self.clear_at(0.0);
    }

    /// Puffer leeren (Seek): die Songzeit geht bei `seconds` weiter
    pub fn clear_at(&self, seconds: f64) {
        let mut b = self.buffer.lock().unwrap();
        b.samples.clear();
        b.end_seconds = seconds;
        b.resampler = Resampler::default();
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    fn frame_len(&self) -> (f64, usize) {
        (self.out_rate.load(Ordering::Relaxed) as f64, self.out_channels.load(Ordering::Relaxed).max(1) as usize)
    }

    pub fn buffered_seconds(&self) -> f64 {
        let (rate, channels) = self.frame_len();
        self.buffer.lock().unwrap().samples.len() as f64 / channels as f64 / rate
    }

    /// Songzeit dessen, was gerade aus dem Lautsprecher kommt
    pub fn played_seconds(&self) -> f64 {
        let (rate, channels) = self.frame_len();
        let b = self.buffer.lock().unwrap();
        (b.end_seconds - b.samples.len() as f64 / channels as f64 / rate).max(0.0)
    }

    /// Dekodierte Samples (verschachtelt) anhängen; `pts` ist die Songzeit des ersten Frames.
    pub fn push(&self, input: &[f32], in_channels: usize, in_rate: u32, pts: f64) {
        let (out_rate, out_channels) = self.frame_len();
        let mut b = self.buffer.lock().unwrap();
        let frames = input.len() / in_channels;
        if b.samples.is_empty() {
            b.end_seconds = pts;
        }
        b.end_seconds = b.end_seconds.max(pts) + frames as f64 / in_rate as f64;
        let Buffer { samples, resampler, .. } = &mut *b;
        resampler.process(input, in_channels, in_rate, out_rate as u32, out_channels, samples);
    }
}

impl Resampler {
    fn process(&mut self, input: &[f32], in_ch: usize, in_rate: u32, out_rate: u32, out_ch: usize, out: &mut VecDeque<f32>) {
        let frames = input.len() / in_ch;
        if frames == 0 {
            return;
        }
        if self.in_rate != in_rate || self.channels != in_ch {
            *self = Resampler { in_rate, channels: in_ch, last: input[..in_ch].to_vec(), phase: 0.0 };
        }
        let mut write = |l: f32, r: f32, out: &mut VecDeque<f32>| {
            out.push_back(l);
            if out_ch > 1 {
                out.push_back(r);
                for _ in 2..out_ch {
                    out.push_back(0.0);
                }
            }
        };
        let stereo = |frame: &[f32]| -> (f32, f32) {
            if in_ch == 1 {
                (frame[0], frame[0])
            } else {
                (frame[0], frame[1])
            }
        };

        if in_rate == out_rate {
            for frame in input.chunks_exact(in_ch) {
                let (l, r) = stereo(frame);
                write(l, r, out);
            }
            self.last.copy_from_slice(&input[(frames - 1) * in_ch..frames * in_ch]);
            return;
        }

        // Zwischen „last“ (Index -1) und den neuen Frames interpolieren
        let step = in_rate as f64 / out_rate as f64;
        let mut pos = self.phase; // relativ zu Frame -1
        let frame_at = |i: isize, last: &[f32]| -> (f32, f32) {
            if i < 0 {
                stereo(last)
            } else {
                stereo(&input[i as usize * in_ch..i as usize * in_ch + in_ch])
            }
        };
        while pos < frames as f64 {
            let i = pos.floor() as isize - 1;
            let t = (pos - pos.floor()) as f32;
            let (l0, r0) = frame_at(i, &self.last);
            let (l1, r1) = frame_at(i + 1, &self.last);
            write(l0 + (l1 - l0) * t, r0 + (r1 - r0) * t, out);
            pos += step;
        }
        self.phase = pos - frames as f64;
        self.last.copy_from_slice(&input[(frames - 1) * in_ch..frames * in_ch]);
    }
}

pub struct Output {
    _stream: cpal::Stream,
}

impl Output {
    pub fn open(shared: Arc<Shared>) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("Kein Audiogerät gefunden")?;
        let config = device.default_output_config().map_err(|e| format!("Audiogerät: {e}"))?;
        shared.out_rate.store(config.sample_rate(), Ordering::Relaxed);
        shared.out_channels.store(config.channels() as u32, Ordering::Relaxed);
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => build::<f32>(&device, config.config(), shared),
            cpal::SampleFormat::I16 => build::<i16>(&device, config.config(), shared),
            cpal::SampleFormat::I32 => build::<i32>(&device, config.config(), shared),
            cpal::SampleFormat::U16 => build::<u16>(&device, config.config(), shared),
            other => return Err(format!("Audioformat {other:?} wird nicht unterstützt")),
        }?;
        stream.play().map_err(|e| format!("Audioausgabe: {e}"))?;
        Ok(Self { _stream: stream })
    }
}

fn build<T>(device: &cpal::Device, config: cpal::StreamConfig, shared: Arc<Shared>) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    // Lautstärke weich nachführen (kein Knacken beim Ziehen des Reglers)
    let mut gain = load_f32(&shared.gain);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let target = if shared.paused.load(Ordering::Relaxed) { 0.0 } else { load_f32(&shared.gain) };
                let paused = shared.paused.load(Ordering::Relaxed);
                let mut buffer = shared.buffer.lock().unwrap();
                for sample in data.iter_mut() {
                    gain += (target - gain) * 0.002;
                    let value = if paused { 0.0 } else { buffer.samples.pop_front().unwrap_or(0.0) };
                    *sample = T::from_sample(value * gain);
                }
            },
            |e| eprintln!("Audioausgabe: {e}"),
            None,
        )
        .map_err(|e| format!("Audioausgabe: {e}"))
}
