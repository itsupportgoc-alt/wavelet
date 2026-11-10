//! Small procedural music generator used for the built-in demo songs.
//! Every sample is computed straight from its index, so seeking is instant.

use rodio::source::SeekError;
use rodio::Source;
use std::f64::consts::TAU;
use std::time::Duration;

pub const SR: u32 = 44_100;

#[derive(Clone, Debug)]
pub struct SynthSpec {
    pub seed: u64,
    pub bpm: f32,
    pub root: i32,
    pub minor: bool,
    pub prog: [i32; 4],
    /// 0 soft sine, 1 square-ish, 2 triangle-ish, 3 bright pluck
    pub lead: u8,
    /// 0 none, 1 soft, 2 full
    pub drums: u8,
    pub secs: u32,
    pub swing: f32,
}

const MAJOR: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
const MINOR: [i32; 7] = [0, 2, 3, 5, 7, 8, 10];

fn mix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn noise(n: u64) -> f64 {
    (mix64(n) >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
}

fn hz(m: f64) -> f64 {
    440.0 * 2f64.powf((m - 69.0) / 12.0)
}

impl SynthSpec {
    fn scale(&self) -> &'static [i32; 7] {
        if self.minor {
            &MINOR
        } else {
            &MAJOR
        }
    }

    fn deg(&self, d: i32) -> i32 {
        let s = self.scale();
        s[d.rem_euclid(7) as usize] + 12 * d.div_euclid(7)
    }

    fn beat_len(&self) -> f64 {
        60.0 / self.bpm as f64
    }

    fn bars_total(&self) -> i64 {
        (self.secs as f64 / (self.beat_len() * 4.0)) as i64
    }

    fn chord_of_bar(&self, bar: i64) -> i32 {
        self.prog[bar.rem_euclid(4) as usize]
    }

    fn is_intro(&self, bar: i64) -> bool {
        bar < 4
    }
    fn is_outro(&self, bar: i64) -> bool {
        bar >= self.bars_total() - 3
    }
    fn is_break(&self, bar: i64) -> bool {
        self.bars_total() > 28 && bar.rem_euclid(32) >= 20 && bar.rem_euclid(32) < 24
    }

    /// Note (midi) played at an eighth-note step for a voice, if any.
    fn note_at(&self, step: i64, voice: u8) -> Option<f64> {
        if step < 0 {
            return None;
        }
        let bar = step.div_euclid(8);
        let pos = step.rem_euclid(8) as usize;
        let chord = self.chord_of_bar(bar);
        let root = self.root as f64;
        match voice {
            0 => {
                let pat: [bool; 8] = match self.drums {
                    0 => [true, false, false, false, false, false, false, false],
                    1 => [true, false, false, true, false, false, true, false],
                    _ => [true, false, true, true, false, true, true, false],
                };
                if pat[pos] {
                    let lift = if pos == 6 && self.drums > 1 { 12.0 } else { 0.0 };
                    Some(root - 12.0 + self.deg(chord) as f64 + lift)
                } else {
                    None
                }
            }
            _ => {
                if self.is_intro(bar) || self.is_outro(bar) {
                    return None;
                }
                let phrase = step.rem_euclid(32) as u64;
                let section = (step.div_euclid(128)).rem_euclid(2) as u64;
                let h = mix64(self.seed ^ phrase.wrapping_mul(0x9E37) ^ (section << 40));
                let density = if self.drums == 0 { 70 } else { 140 };
                let slow_skip = self.drums == 0 && pos % 2 == 1;
                if (h & 0xff) < density && !slow_skip {
                    let offs = [0, 2, 4, 7, 2, 4, 1, 5];
                    let o = offs[((h >> 8) % 8) as usize];
                    Some(root + 24.0 + self.deg(chord + o) as f64)
                } else {
                    None
                }
            }
        }
    }

    fn wave(&self, voice: u8, m: f64, t: f64) -> f64 {
        let f = hz(m);
        let p = TAU * f * t;
        if voice == 0 {
            return p.sin() + 0.25 * (2.0 * p).sin();
        }
        match self.lead {
            0 => p.sin() + 0.2 * (2.0 * p).sin(),
            1 => (p.sin() + (3.0 * p).sin() / 3.0 + (5.0 * p).sin() / 5.0) * 0.8,
            2 => p.sin() - (3.0 * p).sin() / 9.0 + (5.0 * p).sin() / 25.0,
            _ => (p.sin() + (2.0 * p).sin() / 2.0 + (3.0 * p).sin() / 3.0 + (4.0 * p).sin() / 4.0) * 0.7,
        }
    }

    fn voice(&self, t: f64, voice: u8, decay: f64) -> f64 {
        let step_len = self.beat_len() / 2.0;
        let step_f = t / step_len;
        let cur = step_f.floor() as i64;
        let mut out = 0.0;
        let mut found = 0;
        let mut newer_te: f64 = 0.0;
        for back in 0..16 {
            let s = cur - back;
            if let Some(m) = self.note_at(s, voice) {
                let te = (step_f - s as f64) * step_len;
                let env = (te / 0.006).min(1.0) * (-te * decay).exp();
                let gate = if found == 0 {
                    1.0
                } else {
                    (1.0 - newer_te / 0.012).max(0.0)
                };
                out += env * gate * self.wave(voice, m, t);
                found += 1;
                if found == 2 {
                    break;
                }
                newer_te = te;
            }
        }
        out
    }

    pub fn sample(&self, n: u64) -> f32 {
        let t = n as f64 / SR as f64;
        let total = self.secs as f64;
        let bl = self.beat_len();
        let beat = t / bl;
        let bar = (beat / 4.0).floor() as i64;
        let in_bar = beat - bar as f64 * 4.0;
        let chord = self.chord_of_bar(bar);
        let root = self.root as f64;
        let drums_on = self.drums > 0 && !self.is_intro(bar) && !self.is_break(bar) && !self.is_outro(bar);

        let mut out = 0.0;

        // Pad chord
        let tb = in_bar * bl;
        let bar_len = 4.0 * bl;
        let penv = (tb / 0.12).min(1.0) * ((bar_len - tb) / 0.12).min(1.0);
        for k in 0..3 {
            let m = root + 12.0 + self.deg(chord + 2 * k) as f64;
            let f = hz(m);
            let ph = TAU * f * t;
            out += 0.04 * penv * (ph.sin() + 0.3 * (2.0 * ph).sin() + 0.4 * (TAU * f * 1.004 * t).sin());
        }

        // Bass
        if !self.is_intro(bar) || self.drums == 0 {
            out += 0.20 * self.voice(t, 0, 5.0);
        }

        // Lead melody
        let lead_amp = match self.lead {
            1 => 0.07,
            3 => 0.09,
            _ => 0.11,
        };
        out += lead_amp * self.voice(t, 1, if self.drums == 0 { 2.5 } else { 7.0 });

        if drums_on {
            // Kick on each beat
            let tk = beat.fract() * bl;
            if tk < 0.35 {
                let phase = 50.0 * tk + 110.0 * (1.0 - (-tk * 30.0).exp()) / 30.0;
                let amp = if self.drums == 1 { 0.35 } else { 0.5 };
                out += amp * (-tk * 10.0).exp() * (TAU * phase).sin();
            }
            // Snare / clap on 2 and 4
            let bi = in_bar.floor() as i64;
            if bi == 1 || bi == 3 {
                let ts = (in_bar - bi as f64) * bl;
                let amp = if self.drums == 1 { 0.07 } else { 0.14 };
                out += amp * (-ts * 18.0).exp() * noise(n ^ 0xABCDEF)
                    + 0.08 * (-ts * 30.0).exp() * (TAU * 185.0 * t).sin();
            }
            // Hi-hats on off-beats (with swing)
            let eighth = beat * 2.0;
            let e_idx = eighth.floor() as i64;
            if e_idx.rem_euclid(2) == 1 {
                let te = (eighth.fract() - self.swing as f64).max(-1.0) * bl / 2.0;
                if te >= 0.0 {
                    out += 0.05 * (-te * 70.0).exp() * noise(n);
                }
            }
        }

        let fade = (t / 1.5).min(1.0) * ((total - t) / 3.0).clamp(0.0, 1.0);
        let gain = if self.drums == 0 { 1.8 } else { 1.1 };
        ((out * gain).tanh() * 0.8 * fade) as f32
    }
}

pub struct SynthSource {
    spec: SynthSpec,
    n: u64,
    total: u64,
}

impl SynthSource {
    pub fn new(spec: SynthSpec) -> Self {
        let total = spec.secs as u64 * SR as u64;
        Self { spec, n: 0, total }
    }
}

impl Iterator for SynthSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.n >= self.total {
            return None;
        }
        let v = self.spec.sample(self.n);
        self.n += 1;
        Some(v)
    }
}

impl Source for SynthSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        1
    }
    fn sample_rate(&self) -> u32 {
        SR
    }
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs(self.spec.secs as u64))
    }
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.n = ((pos.as_secs_f64() * SR as f64) as u64).min(self.total);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_songs_make_sound_without_clipping() {
        let t0 = std::time::Instant::now();
        let mut samples = 0usize;
        for t in crate::library::demo_catalog() {
            let crate::library::TrackSource::Demo(spec) = t.source else { continue };
            // 20 seconds starting after the intro
            let start = 12 * SR as u64;
            let mut peak = 0f32;
            let mut sum = 0f64;
            let n = 20 * SR as u64;
            for i in start..start + n {
                let v = spec.sample(i);
                assert!(v.is_finite());
                peak = peak.max(v.abs());
                sum += (v as f64) * (v as f64);
            }
            samples += n as usize;
            let rms = (sum / n as f64).sqrt();
            println!("{:<22} peak {:.2} rms {:.3}", t.title, peak, rms);
            assert!(peak <= 1.0, "{} clips", t.title);
            assert!(rms > 0.02, "{} is too quiet", t.title);
        }
        let per_sec = samples as f64 / t0.elapsed().as_secs_f64();
        println!("rendered {:.0} samples/s (need {} for realtime)", per_sec, SR);
    }
}
