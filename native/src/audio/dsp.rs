//! Klang wie bisher im Web-Audio-Signalweg (sound.svelte.js):
//!   Samples → Equalizer (10 Bänder) → Vorverstärkung → Angleichung → (Ausgabe: Lautstärke/Blende → Begrenzer)
//!              └→ Lautheitsmessung (vor dem Equalizer, K-Filter wie EBU R128)

pub const EQ_BANDS: [f32; 10] = [32.0, 64.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];

/// Ziel-Lautheit (ähnlich Spotify/YouTube) und wie weit angeglichen wird
pub const TARGET_LUFS: f32 = -14.0;
const MAX_BOOST_DB: f32 = 6.0;
const MAX_CUT_DB: f32 = -14.0;
/// Annahme, solange ein Song noch nicht gemessen ist
pub const DEFAULT_LUFS: f32 = -10.0;

pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

pub fn normalize_db(lufs: f32) -> f32 {
    (TARGET_LUFS - lufs).clamp(MAX_CUT_DB, MAX_BOOST_DB)
}

#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Peaking,
    LowShelf,
    HighShelf,
    HighPass,
}

impl Biquad {
    /// Formeln aus dem „Audio EQ Cookbook“ (wie BiquadFilterNode)
    fn new(kind: Kind, freq: f32, q: f32, gain_db: f32, rate: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * (freq / rate).min(0.49);
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let (b0, b1, b2, a0, a1, a2) = match kind {
            Kind::Peaking => (1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a),
            Kind::LowShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos + s),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                    a * ((a + 1.0) - (a - 1.0) * cos - s),
                    (a + 1.0) + (a - 1.0) * cos + s,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                    (a + 1.0) + (a - 1.0) * cos - s,
                )
            }
            Kind::HighShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos + s),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                    a * ((a + 1.0) + (a - 1.0) * cos - s),
                    (a + 1.0) - (a - 1.0) * cos + s,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos),
                    (a + 1.0) - (a - 1.0) * cos - s,
                )
            }
            Kind::HighPass => ((1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
        };
        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
    }
}

/// Zustand eines Filters für einen Kanal
#[derive(Clone, Copy, Default)]
struct State {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl State {
    #[inline]
    fn run(&mut self, c: &Biquad, x: f32) -> f32 {
        let y = c.b0 * x + c.b1 * self.x1 + c.b2 * self.x2 - c.a1 * self.y1 - c.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Einstellungen des Signalwegs (von der Oberfläche)
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub eq_enabled: bool,
    pub eq_gains: [f32; 10],
    pub normalize: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { eq_enabled: false, eq_gains: [0.0; 10], normalize: false }
    }
}

pub struct Processor {
    rate: u32,
    channels: usize,
    settings: Settings,
    eq: Vec<Biquad>,
    eq_state: Vec<Vec<State>>,
    preamp: f32,
    /// Angleichung: Ziel und aktueller Wert (weich nachgeführt)
    normalize_target: f32,
    normalize_gain: f32,
    /// Lautheitsmessung
    k_filters: [Biquad; 2],
    k_state: Vec<[State; 2]>,
    block_sum: f64,
    block_len: usize,
    pub blocks: Vec<f64>,
    /// Gespeicherter Messwert des Songs (dann wird nicht nachgeführt)
    known_lufs: Option<f32>,
}

impl Processor {
    pub fn new(settings: Settings) -> Self {
        let mut p = Self {
            rate: 0,
            channels: 0,
            settings,
            eq: Vec::new(),
            eq_state: Vec::new(),
            preamp: 1.0,
            normalize_target: 1.0,
            normalize_gain: 1.0,
            k_filters: [Biquad::default(); 2],
            k_state: Vec::new(),
            block_sum: 0.0,
            block_len: 0,
            blocks: Vec::new(),
            known_lufs: None,
        };
        p.configure(48_000, 2);
        p
    }

    fn configure(&mut self, rate: u32, channels: usize) {
        self.rate = rate;
        self.channels = channels;
        let r = rate as f32;
        let gains = if self.settings.eq_enabled { self.settings.eq_gains } else { [0.0; 10] };
        self.eq = EQ_BANDS
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let kind = if i == 0 { Kind::LowShelf } else if i == EQ_BANDS.len() - 1 { Kind::HighShelf } else { Kind::Peaking };
                Biquad::new(kind, *f, 1.4, gains[i], r)
            })
            .collect();
        self.eq_state = vec![vec![State::default(); EQ_BANDS.len()]; channels];
        // Anhebungen vorab ausgleichen, damit der Begrenzer kaum eingreifen muss
        let max_boost = gains.iter().cloned().fold(0.0, f32::max);
        self.preamp = db_to_gain(-max_boost * 0.6);
        self.k_filters = [Biquad::new(Kind::HighPass, 38.0, 0.5, 0.0, r), Biquad::new(Kind::HighShelf, 1500.0, 0.707, 4.0, r)];
        self.k_state = vec![[State::default(); 2]; channels];
    }

    pub fn set_settings(&mut self, settings: Settings) {
        if settings == self.settings {
            return;
        }
        self.settings = settings;
        let (rate, channels) = (self.rate, self.channels);
        self.configure(rate, channels);
        self.update_normalize(true);
    }

    /// Neuer Song: Messung zurücksetzen, gespeicherten Wert sofort anwenden
    pub fn start_track(&mut self, known_lufs: Option<f32>) {
        self.blocks.clear();
        self.block_sum = 0.0;
        self.block_len = 0;
        self.known_lufs = known_lufs;
        for s in self.eq_state.iter_mut().flatten() {
            *s = State::default();
        }
        self.update_normalize(true);
    }

    fn update_normalize(&mut self, immediate: bool) {
        let lufs = self.known_lufs.or_else(|| self.integrated()).unwrap_or(DEFAULT_LUFS);
        self.normalize_target = if self.settings.normalize { db_to_gain(normalize_db(lufs)) } else { 1.0 };
        if immediate {
            self.normalize_gain = self.normalize_target;
        }
    }

    /// Gegatete Lautheit aus 400-ms-Blöcken (absolute Schwelle −70, relative −10 LU)
    pub fn integrated(&self) -> Option<f32> {
        let loud = |ms: f64| -0.691 + 10.0 * ms.log10();
        let abs: Vec<f64> = self.blocks.iter().cloned().filter(|ms| loud(*ms) > -70.0).collect();
        if abs.is_empty() {
            return None;
        }
        let mean = abs.iter().sum::<f64>() / abs.len() as f64;
        let rel: Vec<f64> = abs.into_iter().filter(|ms| loud(*ms) > loud(mean) - 10.0).collect();
        Some(loud(rel.iter().sum::<f64>() / rel.len() as f64) as f32)
    }

    /// Verarbeitet verschachtelte Samples an Ort und Stelle
    pub fn process(&mut self, samples: &mut [f32], channels: usize, rate: u32) {
        if rate != self.rate || channels != self.channels {
            self.configure(rate, channels);
        }
        let block_frames = (rate as usize * 400) / 1000;
        let eq_on = self.settings.eq_enabled;
        for frame in samples.chunks_exact_mut(channels) {
            // Lautheit (vor dem Equalizer)
            let mut energy = 0.0f64;
            for (c, x) in frame.iter().enumerate() {
                let k = &mut self.k_state[c];
                let first = k[0].run(&self.k_filters[0], *x);
                let y = k[1].run(&self.k_filters[1], first);
                energy += (y * y) as f64;
            }
            self.block_sum += energy;
            self.block_len += 1;
            if self.block_len >= block_frames {
                self.blocks.push(self.block_sum / self.block_len as f64);
                self.block_sum = 0.0;
                self.block_len = 0;
                // Unbekannter Song: nach 3 s erste Schätzung, danach langsam nachführen
                if self.known_lufs.is_none() && self.blocks.len() >= 8 && self.blocks.len() % 5 == 0 {
                    self.update_normalize(false);
                }
            }
            // Weich auf das Ziel der Angleichung zugehen (~1 s)
            self.normalize_gain += (self.normalize_target - self.normalize_gain) * 0.00002;
            let gain = self.preamp * self.normalize_gain;
            for (c, x) in frame.iter_mut().enumerate() {
                let mut y = *x;
                if eq_on {
                    for (band, state) in self.eq.iter().zip(self.eq_state[c].iter_mut()) {
                        y = state.run(band, y);
                    }
                }
                *x = y * if eq_on { gain } else { self.normalize_gain };
            }
        }
    }
}
