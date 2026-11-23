//! Real-time echo and noise cancellation for a 48 kHz call microphone.
//!
//! The frame pipeline is echo cancellation with WebRTC AEC3, then noise
//! removal with DeepFilterNet3. RNNoise is the low-CPU fallback. Each stage
//! can be bypassed. Suppression strength scales DeepFilterNet's attenuation
//! limit, or the wet/dry mix of RNNoise. Strength zero skips the suppressor
//! and its delay.

mod aec;
mod dpdf;
mod constants;
mod devices;
mod error;
mod ns;
mod pipeline;
mod resample;
mod ring;
mod session;
mod wav;
#[cfg(windows)]
mod winbridge;

pub use constants::{FRAME_SAMPLES, SAMPLE_RATE};
pub use devices::DeviceLists;
pub use error::{Error, Result};
pub use ns::{NsBackend, RNNOISE_LATENCY_SAMPLES};
pub use pipeline::{FrameStats, Pipeline, PipelineConfig, ProcessedAudio, StreamStats};
pub use ring::{RingAttachment, RingPublisher, MIC_NAME, SPEAKER_NAME};
pub use session::{list_devices, RealtimeSession, SessionConfig, SessionStats};
#[cfg(windows)]
pub use winbridge::run_windows_bridge;
pub use wav::{read_wav_mono, write_wav_mono};

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
