use std::time::Instant;

use serde::Serialize;

use crate::aec::Aec;
use crate::constants::{FRAME_NS, FRAME_SAMPLES, SAMPLE_RATE};
use crate::error::{Error, Result};
use crate::ns::{NoiseSuppressor, NsBackend};

/// Runtime controls. `strength` is the noise-suppression amount: 0 leaves the
/// signal untouched and adds no suppressor delay, 1 runs the selected model
/// at its full attenuation. Echo cancellation is its own bypass.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PipelineConfig {
    pub enabled: bool,
    /// Virtual-microphone mute. The frame is silence and the ring flag is set.
    pub muted: bool,
    pub aec_enabled: bool,
    pub ns_backend: NsBackend,
    pub strength: f32,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            muted: false,
            aec_enabled: false,
            ns_backend: NsBackend::DpdfNet,
            strength: 1.0,
        }
    }
}

impl PipelineConfig {
    pub fn sanitized(mut self) -> Self {
        self.strength = self.strength.clamp(0.0, 1.0);
        self
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct FrameStats {
    pub process_ns: u64,
    pub cpu_percent: f32,
    pub algorithmic_latency_samples: u32,
    pub aec_applied: bool,
    pub ns_applied: bool,
    pub ns_backend: NsBackend,
}

#[derive(Clone, Debug, Serialize)]
pub struct StreamStats {
    pub sample_rate: u32,
    pub frame_samples: usize,
    pub frames: u64,
    pub algorithmic_latency_samples: u32,
    pub algorithmic_latency_ms: f64,
    /// True when the returned buffer has already been shifted by the
    /// algorithmic latency, so it lines up with the input.
    pub aligned: bool,
    pub ns_backend: NsBackend,
    pub aec_enabled: bool,
    pub strength: f32,
    pub process_ns_mean: f64,
    pub process_ns_p50: u64,
    pub process_ns_p95: u64,
    pub process_ns_max: u64,
    pub cpu_percent_mean: f64,
    pub cpu_percent_p95: f64,
}

pub struct ProcessedAudio {
    pub samples: Vec<f32>,
    pub stats: StreamStats,
}

pub struct Pipeline {
    config: PipelineConfig,
    aec: Option<Aec>,
    ns: NoiseSuppressor,
    work: Vec<f32>,
    aec_out: Vec<f32>,
}

impl Pipeline {
    pub fn new(config: PipelineConfig) -> Result<Self> {
        let config = config.sanitized();
        let aec = if config.aec_enabled {
            Some(Aec::new()?)
        } else {
            None
        };
        let ns = NoiseSuppressor::new(config.ns_backend, config.strength)?;
        Ok(Self {
            config,
            aec,
            ns,
            work: vec![0.0; FRAME_SAMPLES],
            aec_out: vec![0.0; FRAME_SAMPLES],
        })
    }

    pub fn config(&self) -> &PipelineConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: PipelineConfig) -> Result<()> {
        let config = config.sanitized();
        if config.aec_enabled && self.aec.is_none() {
            self.aec = Some(Aec::new()?);
        }
        if config.ns_backend != self.config.ns_backend {
            self.ns = NoiseSuppressor::new(config.ns_backend, config.strength)?;
        } else {
            self.ns.set_strength(config.strength);
        }
        self.config = config;
        Ok(())
    }

    /// Delay inserted by the stages that are actually running.
    ///
    /// AEC3 is not included. Its capture frame stays aligned with the
    /// microphone frame; the delay it reports is the loudspeaker-to-mic
    /// path, which is not latency added to the call audio.
    pub fn algorithmic_latency_samples(&self) -> u32 {
        if !self.config.enabled {
            return 0;
        }
        self.ns.latency_samples()
    }

    pub fn process_frame(
        &mut self,
        near: &[f32],
        far: Option<&[f32]>,
        out: &mut [f32],
    ) -> Result<FrameStats> {
        if self.config.muted {
            out.fill(0.0);
            return Ok(FrameStats {
                process_ns: 0,
                cpu_percent: 0.0,
                algorithmic_latency_samples: 0,
                aec_applied: false,
                ns_applied: false,
                ns_backend: self.config.ns_backend,
            });
        }
        if near.len() != FRAME_SAMPLES || out.len() != FRAME_SAMPLES {
            return Err(Error::msg(format!(
                "frames must be {FRAME_SAMPLES} samples at {SAMPLE_RATE} Hz"
            )));
        }
        if let Some(far) = far {
            if far.len() != FRAME_SAMPLES {
                return Err(Error::msg(format!(
                    "far-end frames must be {FRAME_SAMPLES} samples"
                )));
            }
        }

        let started = Instant::now();
        self.work.copy_from_slice(near);

        let mut aec_applied = false;
        if self.config.enabled && self.config.aec_enabled {
            if let (Some(aec), Some(far)) = (self.aec.as_mut(), far) {
                aec.process(&self.work, far, &mut self.aec_out)?;
                self.work.copy_from_slice(&self.aec_out);
                aec_applied = true;
            }
        }

        let ns_applied = self.config.enabled
            && self.config.ns_backend != NsBackend::Off
            && self.config.strength > 0.0;
        if ns_applied {
            self.ns.process(&self.work, out)?;
        } else {
            out.copy_from_slice(&self.work);
        }
        for sample in out.iter_mut() {
            if !sample.is_finite() {
                *sample = 0.0;
            }
        }

        let process_ns = started.elapsed().as_nanos() as u64;
        let latency = self.algorithmic_latency_samples();
        Ok(FrameStats {
            process_ns,
            cpu_percent: process_ns as f32 / FRAME_NS as f32 * 100.0,
            algorithmic_latency_samples: latency,
            aec_applied,
            ns_applied,
            ns_backend: self.config.ns_backend,
        })
    }

    /// Process a whole buffer and return audio aligned with `near`.
    ///
    /// The suppressor delay is flushed with zeros and then removed, so the
    /// returned slice has the same length as `near`. The delay that was
    /// removed is reported on [`StreamStats`].
    pub fn process_buffer(&mut self, near: &[f32], far: Option<&[f32]>) -> Result<ProcessedAudio> {
        let delay = self.algorithmic_latency_samples() as usize;
        let total = near.len() + delay;
        let frames = total.div_ceil(FRAME_SAMPLES);
        let mut rendered = vec![0.0f32; frames * FRAME_SAMPLES];
        let mut near_frame = vec![0.0f32; FRAME_SAMPLES];
        let mut far_frame = vec![0.0f32; FRAME_SAMPLES];
        let mut out_frame = vec![0.0f32; FRAME_SAMPLES];
        let mut times = Vec::with_capacity(frames);

        for frame in 0..frames {
            let start = frame * FRAME_SAMPLES;
            for i in 0..FRAME_SAMPLES {
                let index = start + i;
                near_frame[i] = near.get(index).copied().unwrap_or(0.0);
                if far.is_some() {
                    far_frame[i] = far.and_then(|f| f.get(index)).copied().unwrap_or(0.0);
                }
            }
            let far_ref = if far.is_some() { Some(far_frame.as_slice()) } else { None };
            let stats = self.process_frame(&near_frame, far_ref, &mut out_frame)?;
            rendered[start..start + FRAME_SAMPLES].copy_from_slice(&out_frame);
            times.push(stats.process_ns);
        }

        let mut aligned = vec![0.0f32; near.len()];
        if delay < rendered.len() {
            let available = (rendered.len() - delay).min(near.len());
            aligned[..available].copy_from_slice(&rendered[delay..delay + available]);
        }

        Ok(ProcessedAudio {
            samples: aligned,
            stats: summarize(times, delay as u32, &self.config),
        })
    }
}

fn summarize(mut times: Vec<u64>, delay: u32, config: &PipelineConfig) -> StreamStats {
    times.sort_unstable();
    let frames = times.len() as u64;
    let mean = if times.is_empty() {
        0.0
    } else {
        times.iter().sum::<u64>() as f64 / frames as f64
    };
    let p50 = percentile(&times, 0.50);
    let p95 = percentile(&times, 0.95);
    let max = times.last().copied().unwrap_or(0);
    let cpu = |ns: f64| ns / FRAME_NS as f64 * 100.0;
    StreamStats {
        sample_rate: SAMPLE_RATE,
        frame_samples: FRAME_SAMPLES,
        frames,
        algorithmic_latency_samples: delay,
        algorithmic_latency_ms: delay as f64 / SAMPLE_RATE as f64 * 1000.0,
        aligned: true,
        ns_backend: config.ns_backend,
        aec_enabled: config.aec_enabled,
        strength: config.strength,
        process_ns_mean: mean,
        process_ns_p50: p50,
        process_ns_p95: p95,
        process_ns_max: max,
        cpu_percent_mean: cpu(mean),
        cpu_percent_p95: cpu(p95 as f64),
    }
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(n: usize, freq: f32, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| (i as f32 * freq * 2.0 * std::f32::consts::PI / SAMPLE_RATE as f32).sin() * amp)
            .collect()
    }

    #[test]
    fn bypass_is_identity_and_reports_zero_latency() {
        let mut pipeline = Pipeline::new(PipelineConfig {
            enabled: true,
            muted: false,
            aec_enabled: false,
            ns_backend: NsBackend::Off,
            strength: 1.0,
        })
        .unwrap();
        let input = sine(FRAME_SAMPLES * 5, 440.0, 0.2);
        let processed = pipeline.process_buffer(&input, None).unwrap();
        assert_eq!(processed.stats.algorithmic_latency_samples, 0);
        let err = input
            .iter()
            .zip(processed.samples.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(err < 1e-6, "max error {err}");
    }

    #[test]
    fn disabled_pipeline_ignores_the_suppressor() {
        let mut pipeline = Pipeline::new(PipelineConfig {
            enabled: false,
            muted: false,
            aec_enabled: false,
            ns_backend: NsBackend::RnNoise,
            strength: 1.0,
        })
        .unwrap();
        assert_eq!(pipeline.algorithmic_latency_samples(), 0);
        let input = sine(FRAME_SAMPLES, 220.0, 0.3);
        let mut out = vec![0.0; FRAME_SAMPLES];
        let stats = pipeline.process_frame(&input, None, &mut out).unwrap();
        assert!(!stats.ns_applied);
        assert!((input[10] - out[10]).abs() < 1e-6);
    }

    #[test]
    fn rnnoise_frame_is_finite_and_delayed() {
        let mut pipeline = Pipeline::new(PipelineConfig {
            enabled: true,
            muted: false,
            aec_enabled: false,
            ns_backend: NsBackend::RnNoise,
            strength: 1.0,
        })
        .unwrap();
        assert_eq!(
            pipeline.algorithmic_latency_samples(),
            crate::ns::RNNOISE_LATENCY_SAMPLES
        );
        let input = sine(FRAME_SAMPLES * 20, 180.0, 0.25);
        let processed = pipeline.process_buffer(&input, None).unwrap();
        assert_eq!(processed.samples.len(), input.len());
        assert!(processed.samples.iter().all(|s| s.is_finite()));
        assert!(processed.stats.cpu_percent_mean >= 0.0);
        assert!(processed.stats.process_ns_p95 >= processed.stats.process_ns_p50);
    }

    #[test]
    fn zero_strength_skips_suppressor_latency() {
        let mut pipeline = Pipeline::new(PipelineConfig {
            enabled: true,
            muted: false,
            aec_enabled: false,
            ns_backend: NsBackend::RnNoise,
            strength: 0.0,
        })
        .unwrap();
        assert_eq!(pipeline.algorithmic_latency_samples(), 0);
        let input = sine(FRAME_SAMPLES * 3, 330.0, 0.2);
        let processed = pipeline.process_buffer(&input, None).unwrap();
        let err = input
            .iter()
            .zip(processed.samples.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(err < 1e-5, "max error {err}");
    }

    #[test]
    fn deepfilter_reports_model_delay_and_stays_finite() {
        let mut pipeline = Pipeline::new(PipelineConfig {
            enabled: true,
            muted: false,
            aec_enabled: false,
            ns_backend: NsBackend::DeepFilterNet3,
            strength: 1.0,
        })
        .unwrap();
        let delay = pipeline.algorithmic_latency_samples();
        assert!(delay >= 480, "DeepFilterNet3 delay was {delay}");
        let input = sine(SAMPLE_RATE as usize / 2, 200.0, 0.2);
        let processed = pipeline.process_buffer(&input, None).unwrap();
        assert_eq!(processed.samples.len(), input.len());
        assert!(processed.samples.iter().all(|s| s.is_finite()));
        assert_eq!(processed.stats.algorithmic_latency_samples, delay);
        assert!(processed.stats.algorithmic_latency_ms > 10.0);
    }

    #[test]
    fn aec3_attenuates_a_pure_echo() {
        let mut pipeline = Pipeline::new(PipelineConfig {
            enabled: true,
            muted: false,
            aec_enabled: true,
            ns_backend: NsBackend::Off,
            strength: 0.0,
        })
        .unwrap();
        let seconds = 3;
        let n = SAMPLE_RATE as usize * seconds;
        let mut far = vec![0.0f32; n];
        let mut state: u32 = 0x12345678;
        for (i, sample) in far.iter_mut().enumerate() {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = (state >> 16) as f32 / 65535.0 * 2.0 - 1.0;
            let gate = ((i as f32 / SAMPLE_RATE as f32) * 4.0).sin().abs();
            *sample = noise * 0.4 * gate;
        }
        let delay = (0.08 * SAMPLE_RATE as f32) as usize;
        let mut near = vec![0.0f32; n];
        for i in delay..n {
            near[i] = far[i - delay] * 0.6;
        }
        let processed = pipeline.process_buffer(&near, Some(&far)).unwrap();
        let tail = SAMPLE_RATE as usize / 2;
        let near_power = power(&near[n - tail..]);
        let out_power = power(&processed.samples[n - tail..]);
        let erle_db = 10.0 * (near_power / out_power.max(1e-12)).log10();
        assert!(
            erle_db > 8.0,
            "echo return loss enhancement was {erle_db:.2} dB (near {near_power:.6}, out {out_power:.6})"
        );
        assert_eq!(pipeline.algorithmic_latency_samples(), 0);
    }

    fn power(samples: &[f32]) -> f32 {
        samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32
    }
}
