/// Every stage in the shipped pipeline runs at 48 kHz.
pub const SAMPLE_RATE: u32 = 48_000;

/// 10 ms. WebRTC AEC3, DeepFilterNet3 and RNNoise all consume this quantum.
pub const FRAME_SAMPLES: usize = 480;

pub const FRAME_NS: u64 = 10_000_000;
