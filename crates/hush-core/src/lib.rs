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
mod ioctl;
mod ns;
mod pipeline;
mod resample;
mod ring;
mod session;
mod wav;
#[cfg(windows)]
mod loopback;
mod vbcable;
#[cfg(windows)]
mod winbridge;

pub use constants::{FRAME_SAMPLES, SAMPLE_RATE};
pub use devices::{is_hush_microphone, is_hush_speaker, DeviceLists};
pub use dpdf::preferred_model_dir;
pub use error::{Error, Result};
pub use ns::{NsBackend, RNNOISE_LATENCY_SAMPLES};
pub use pipeline::{FrameStats, Pipeline, PipelineConfig, ProcessedAudio, StreamStats};
pub use ring::{RingAttachment, RingPublisher, MIC_NAME, SPEAKER_NAME};
pub use session::{list_devices, RealtimeSession, SessionConfig, SessionStats};
pub use vbcable::{
    cable_capture_name, cable_render_name, engine_plan, is_cable_capture, is_cable_render,
    round_trip, route_for, sha256_hex, sha_matches_pack, AudioRoute, EnginePlan, RoundTrip,
    PACK_SHA256, PACK_URL,
};
#[cfg(windows)]
pub use winbridge::{
    acquire_bridge, hush_vad_loaded, run_windows_bridge, windows_driver_selftest, BridgeGuard,
};
pub use wav::{read_wav_mono, write_wav_mono};

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
