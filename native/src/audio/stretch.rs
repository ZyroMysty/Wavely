//! Tempo ändern, ohne dass sich die Tonhöhe mitändert (WSOLA: Stücke des Signals werden im Abstand
//! „Tempo × Schritt“ gelesen, an der ähnlichsten Stelle angesetzt und weich überblendet).

use std::collections::VecDeque;

#[derive(Default)]
pub struct Stretch {
    channels: usize,
    rate: u32,
    /// Eingang (verschachtelt), noch nicht verbraucht
    input: Vec<f32>,
    /// Nominelle Leseposition in Frames relativ zum Anfang von `input`
    pos: f64,
    /// Fortsetzung des zuletzt verwendeten Stücks (für Überblendung und Ähnlichkeitssuche)
    tail: Vec<f32>,
}

impl Stretch {
    pub fn reset(&mut self) {
        *self = Stretch::default();
    }

    /// `input`: Frames mit `channels` Kanälen in Abtastrate `rate`; Ergebnis kommt an `out`.
    pub fn process(&mut self, input: &[f32], channels: usize, rate: u32, tempo: f32, out: &mut VecDeque<f32>) {
        if channels != self.channels || rate != self.rate {
            *self = Stretch { channels, rate, ..Default::default() };
        }
        self.input.extend_from_slice(input);
        let ch = channels;
        let hop = (rate as f64 * 0.030) as usize; // Ausgabe pro Schritt
        let overlap = (rate as f64 * 0.012) as usize; // Überblendung
        let search = (rate as f64 * 0.008) as usize; // Suchbereich ±
        let step = hop as f64 * tempo as f64;
        loop {
            let frames = self.input.len() / ch;
            let nominal = self.pos as usize;
            // Genug Eingang für den ganzen Suchbereich und das Stück?
            if nominal + search + hop + overlap >= frames {
                break;
            }
            let start = if self.tail.is_empty() {
                nominal
            } else {
                // Ähnlichste Stelle zum Ende des vorigen Stücks suchen (Kanäle gemischt, jeder 2. Kandidat)
                let from = nominal.saturating_sub(search);
                let mut best = nominal;
                let mut best_score = f32::MIN;
                let mut q = from;
                while q <= nominal + search {
                    let mut score = 0.0f32;
                    let mut i = 0;
                    while i < overlap {
                        let a = &self.tail[i * ch..i * ch + ch];
                        let b = &self.input[(q + i) * ch..(q + i) * ch + ch];
                        let (sa, sb) = (a.iter().sum::<f32>(), b.iter().sum::<f32>());
                        score += sa * sb;
                        i += 2;
                    }
                    if score > best_score {
                        best_score = score;
                        best = q;
                    }
                    q += 2;
                }
                best
            };
            // Überblenden: Ende des vorigen Stücks → neues Stück
            for i in 0..overlap {
                let t = i as f32 / overlap as f32;
                for c in 0..ch {
                    let new = self.input[(start + i) * ch + c];
                    let value = if self.tail.is_empty() { new } else { self.tail[i * ch + c] * (1.0 - t) + new * t };
                    out.push_back(value);
                }
            }
            // Rest des Stücks unverändert
            out.extend(self.input[(start + overlap) * ch..(start + hop) * ch].iter().copied());
            self.tail = self.input[(start + hop) * ch..(start + hop + overlap) * ch].to_vec();
            self.pos += step;
            // Verbrauchten Eingang wegwerfen
            let drop = (self.pos as usize).saturating_sub(search + 1);
            if drop > 0 {
                self.input.drain(..drop * ch);
                self.pos -= drop as f64;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Länge passt zum Tempo, Tonhöhe (Nulldurchgänge pro Sekunde) bleibt gleich
    #[test]
    fn keeps_pitch() {
        let rate = 48_000u32;
        let input: Vec<f32> = (0..rate * 4).flat_map(|i| {
            let v = (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin();
            [v, v]
        }).collect();
        for tempo in [0.75f32, 1.25, 1.5] {
            let mut s = Stretch::default();
            let mut out = VecDeque::new();
            for chunk in input.chunks(2048) {
                s.process(chunk, 2, rate, tempo, &mut out);
            }
            let frames = out.len() / 2;
            let expected = (rate * 4) as f32 / tempo;
            assert!((frames as f32 - expected).abs() / expected < 0.03, "Länge {frames} statt {expected}");
            let left: Vec<f32> = out.iter().step_by(2).copied().collect();
            let crossings = left.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count() as f32;
            let hz = crossings / (frames as f32 / rate as f32);
            assert!((hz - 440.0).abs() < 8.0, "Tempo {tempo}: {hz} Hz");
        }
    }
}
