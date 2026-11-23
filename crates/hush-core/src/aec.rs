use webrtc_audio_processing::Processor;
use webrtc_audio_processing_config::{Config, EchoCanceller, HighPassFilter};

use crate::constants::{FRAME_SAMPLES, SAMPLE_RATE};
use crate::error::{Error, Result};

/// WebRTC AEC3. The canceller writes a capture frame that stays aligned with
/// the microphone frame: the echo-path delay it estimates is not extra latency
/// on the transmitted audio.
pub struct Aec {
    processor: Processor,
    render: Vec<Vec<f32>>,
    capture: Vec<Vec<f32>>,
}

impl Aec {
    pub fn new() -> Result<Self> {
        let processor = Processor::new(SAMPLE_RATE).map_err(|err| Error::msg(err.to_string()))?;
        let config = Config {
            echo_canceller: Some(EchoCanceller::Full {
                stream_delay_ms: None,
            }),
            // AEC3 expects the capture high-pass. It is part of this stage,
            // so bypassing AEC bypasses the filter as well.
            high_pass_filter: Some(HighPassFilter {
                apply_in_full_band: true,
            }),
            noise_suppression: None,
            gain_controller: None,
            capture_amplifier: None,
            ..Config::default()
        };
        processor.set_config(config);
        let frame = processor.num_samples_per_frame();
        if frame != FRAME_SAMPLES {
            return Err(Error::msg(format!(
                "WebRTC AEC3 frame is {frame} samples, expected {FRAME_SAMPLES}"
            )));
        }
        Ok(Self {
            processor,
            render: vec![vec![0.0; FRAME_SAMPLES]],
            capture: vec![vec![0.0; FRAME_SAMPLES]],
        })
    }

    pub fn process(&mut self, near: &[f32], far: &[f32], out: &mut [f32]) -> Result<()> {
        self.render[0].copy_from_slice(far);
        self.capture[0].copy_from_slice(near);
        self.processor
            .process_render_frame(&mut self.render)
            .map_err(|err| Error::msg(err.to_string()))?;
        self.processor
            .process_capture_frame(&mut self.capture)
            .map_err(|err| Error::msg(err.to_string()))?;
        out.copy_from_slice(&self.capture[0]);
        Ok(())
    }
}
