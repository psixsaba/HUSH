use std::path::Path;

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};

use crate::constants::SAMPLE_RATE;
use crate::error::Result;
use crate::resample::resample_mono;

pub fn read_wav_mono(path: &Path) -> Result<(Vec<f32>, u32)> {
    let mut reader = WavReader::open(path)?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let interleaved = match spec.sample_format {
        SampleFormat::Float => reader.samples::<f32>().collect::<std::result::Result<Vec<_>, _>>()?,
        SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample.saturating_sub(1).min(31))) as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|value| value as f32 / max))
                .collect::<std::result::Result<Vec<_>, _>>()?
        }
    };
    let mut mono = Vec::with_capacity(interleaved.len() / channels);
    for frame in interleaved.chunks(channels) {
        let sum: f32 = frame.iter().copied().sum();
        mono.push(sum / channels as f32);
    }
    if spec.sample_rate != SAMPLE_RATE {
        mono = resample_mono(&mono, spec.sample_rate, SAMPLE_RATE)?;
    }
    Ok((mono, SAMPLE_RATE))
}

pub fn write_wav_mono(path: &Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer = WavWriter::create(path, spec)?;
    for sample in samples {
        writer.write_sample(*sample)?;
    }
    writer.finalize()?;
    Ok(())
}
