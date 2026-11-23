use crate::error::{Error, Result};

const TAPS: usize = 96;
const WING: usize = TAPS / 2;

/// Windowed-sinc resampler. Equal rates are a copy. Downsampling applies a
/// low-pass at the destination Nyquist so images are not folded back in.
pub struct SincResampler {
    step: f64,
    cutoff: f64,
    phase: f64,
    history: Vec<f32>,
}

impl SincResampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Result<Self> {
        if from_rate == 0 || to_rate == 0 {
            return Err(Error::msg("sample rate must be non-zero"));
        }
        Ok(Self {
            step: from_rate as f64 / to_rate as f64,
            cutoff: (to_rate as f64 / from_rate as f64).min(1.0),
            phase: 0.0,
            history: Vec::with_capacity(TAPS + 4096),
        })
    }

    pub fn process(&mut self, input: &[f32], last: bool) -> Vec<f32> {
        self.history.extend_from_slice(input);
        let mut out = Vec::new();
        let limit = if last {
            self.history.len() as f64
        } else if self.history.len() > WING {
            (self.history.len() - WING) as f64
        } else {
            return out;
        };
        while self.phase < limit {
            out.push(self.interpolate(self.phase));
            self.phase += self.step;
        }
        let drop = (self.phase as usize).saturating_sub(WING + 2);
        if drop > 0 && drop < self.history.len() {
            self.history.drain(..drop);
            self.phase -= drop as f64;
        }
        out
    }

    fn interpolate(&self, pos: f64) -> f32 {
        let center = pos.round() as isize;
        let mut acc = 0.0f32;
        let mut norm = 0.0f32;
        for k in 0..TAPS {
            let idx = center + k as isize - (WING as isize - 1);
            let offset = pos - idx as f64;
            let weight = kernel(offset, self.cutoff);
            norm += weight;
            if idx >= 0 {
                if let Some(sample) = self.history.get(idx as usize) {
                    acc += *sample * weight;
                }
            }
        }
        if norm.abs() > 1e-6 {
            acc / norm
        } else {
            0.0
        }
    }
}

fn kernel(offset: f64, cutoff: f64) -> f32 {
    let x = offset.abs();
    if x >= WING as f64 {
        return 0.0;
    }
    let t = cutoff * offset;
    let sinc = if t.abs() < 1e-8 {
        cutoff
    } else {
        cutoff * (t * std::f64::consts::PI).sin() / (t * std::f64::consts::PI)
    };
    let wpos = offset / WING as f64;
    let pi = std::f64::consts::PI;
    let window = 0.35875
        + 0.48829 * (pi * wpos).cos()
        + 0.14128 * (2.0 * pi * wpos).cos()
        + 0.01168 * (3.0 * pi * wpos).cos();
    (sinc * window) as f32
}

pub fn resample_mono(input: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>> {
    if from_rate == to_rate {
        return Ok(input.to_vec());
    }
    let mut resampler = SincResampler::new(from_rate, to_rate)?;
    Ok(resampler.process(input, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_rate_is_a_copy() {
        let input = vec![0.1, -0.2, 0.3];
        let out = resample_mono(&input, 48_000, 48_000).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn downsample_length_and_tone() {
        let sr = 48_000u32;
        let n = sr as usize;
        let mut input = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f32 / sr as f32;
            input.push((2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5);
        }
        let out = resample_mono(&input, sr, 16_000).unwrap();
        let expected = (n as f32 * 16_000.0 / sr as f32).round() as isize;
        assert!((out.len() as isize - expected).abs() <= 4, "len {}", out.len());
        let rms = (out.iter().map(|s| s * s).sum::<f32>() / out.len() as f32).sqrt();
        assert!(rms > 0.2 && rms < 0.5, "rms {rms}");
        assert!(out.iter().all(|s| s.is_finite()));
    }
}
