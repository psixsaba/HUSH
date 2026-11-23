use std::ffi::CString;
use std::os::raw::{c_char, c_int};
use std::panic::{catch_unwind, AssertUnwindSafe};

use hush_core::{FrameStats, Pipeline, PipelineConfig, NsBackend, FRAME_SAMPLES, SAMPLE_RATE};

#[repr(C)]
pub struct HushConfig {
    pub enabled: c_int,
    pub aec_enabled: c_int,
    pub ns_backend: c_int,
    pub strength: f32,
}

#[repr(C)]
pub struct HushFrameStats {
    pub process_ns: u64,
    pub cpu_percent: f32,
    pub algorithmic_latency_samples: u32,
    pub aec_applied: c_int,
    pub ns_applied: c_int,
}

pub struct HushPipeline {
    inner: Pipeline,
}

fn write_err(err: *mut c_char, len: usize, message: &str) {
    if err.is_null() || len == 0 {
        return;
    }
    let text = CString::new(message.replace('\0', " ")).unwrap_or_else(|_| CString::new("error").unwrap());
    let bytes = text.as_bytes_with_nul();
    let copy = bytes.len().min(len);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), err as *mut u8, copy);
        if copy == len {
            *err.add(len - 1) = 0;
        }
    }
}

fn backend(code: c_int) -> Result<NsBackend, &'static str> {
    match code {
        0 => Ok(NsBackend::Off),
        1 => Ok(NsBackend::DeepFilterNet3),
        2 => Ok(NsBackend::RnNoise),
        3 => Ok(NsBackend::DpdfNet),
        4 => Ok(NsBackend::DpdfNet2),
        _ => Err("ns_backend must be 0 (off), 1 (deepfilter), 2 (rnnoise), 3 (dpdfnet), or 4 (dpdfnet2)"),
    }
}

fn config_from(raw: &HushConfig) -> Result<PipelineConfig, &'static str> {
    Ok(PipelineConfig {
        enabled: raw.enabled != 0,
        muted: false,
        aec_enabled: raw.aec_enabled != 0,
        ns_backend: backend(raw.ns_backend)?,
        strength: raw.strength,
        clean_incoming: false,
    })
}

fn store_stats(dst: *mut HushFrameStats, stats: &FrameStats) {
    if dst.is_null() {
        return;
    }
    unsafe {
        *dst = HushFrameStats {
            process_ns: stats.process_ns,
            cpu_percent: stats.cpu_percent,
            algorithmic_latency_samples: stats.algorithmic_latency_samples,
            aec_applied: i32::from(stats.aec_applied),
            ns_applied: i32::from(stats.ns_applied),
        };
    }
}

#[no_mangle]
pub extern "C" fn hush_sample_rate() -> u32 {
    SAMPLE_RATE
}

#[no_mangle]
pub extern "C" fn hush_frame_samples() -> u32 {
    FRAME_SAMPLES as u32
}

#[no_mangle]
pub extern "C" fn hush_pipeline_create(
    config: *const HushConfig,
    err: *mut c_char,
    err_len: usize,
) -> *mut HushPipeline {
    if config.is_null() {
        write_err(err, err_len, "config is null");
        return std::ptr::null_mut();
    }
    let built = catch_unwind(AssertUnwindSafe(|| {
        let raw = unsafe { &*config };
        let config = config_from(raw).map_err(hush_core::Error::msg)?;
        Pipeline::new(config)
    }));
    match built {
        Ok(Ok(inner)) => Box::into_raw(Box::new(HushPipeline { inner })),
        Ok(Err(error)) => {
            write_err(err, err_len, &error.to_string());
            std::ptr::null_mut()
        }
        Err(_) => {
            write_err(err, err_len, "pipeline creation panicked");
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn hush_pipeline_destroy(pipeline: *mut HushPipeline) {
    if pipeline.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(pipeline));
    }
}

#[no_mangle]
pub extern "C" fn hush_pipeline_set_config(
    pipeline: *mut HushPipeline,
    config: *const HushConfig,
    err: *mut c_char,
    err_len: usize,
) -> c_int {
    if pipeline.is_null() || config.is_null() {
        write_err(err, err_len, "null argument");
        return -1;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let raw = unsafe { &*config };
        let config = config_from(raw).map_err(hush_core::Error::msg)?;
        unsafe { &mut *pipeline }.inner.set_config(config)
    }));
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            write_err(err, err_len, &error.to_string());
            -2
        }
        Err(_) => {
            write_err(err, err_len, "set_config panicked");
            -3
        }
    }
}

#[no_mangle]
pub extern "C" fn hush_pipeline_latency_samples(pipeline: *const HushPipeline) -> u32 {
    if pipeline.is_null() {
        return 0;
    }
    unsafe { &*pipeline }.inner.algorithmic_latency_samples()
}

#[no_mangle]
pub extern "C" fn hush_pipeline_process(
    pipeline: *mut HushPipeline,
    near: *const f32,
    far: *const f32,
    out: *mut f32,
    stats: *mut HushFrameStats,
) -> c_int {
    if pipeline.is_null() || near.is_null() || out.is_null() {
        return -1;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let near = unsafe { std::slice::from_raw_parts(near, FRAME_SAMPLES) };
        let far = if far.is_null() {
            None
        } else {
            Some(unsafe { std::slice::from_raw_parts(far, FRAME_SAMPLES) })
        };
        let out = unsafe { std::slice::from_raw_parts_mut(out, FRAME_SAMPLES) };
        unsafe { &mut *pipeline }.inner.process_frame(near, far, out)
    }));
    match result {
        Ok(Ok(frame)) => {
            store_stats(stats, &frame);
            0
        }
        Ok(Err(_)) => -2,
        Err(_) => -3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_api_rnnoise_roundtrip() {
        let config = HushConfig {
            enabled: 1,
            aec_enabled: 0,
            ns_backend: 2,
            strength: 1.0,
        };
        let mut err = [0i8; 256];
        let pipeline = hush_pipeline_create(&config, err.as_mut_ptr(), err.len());
        assert!(!pipeline.is_null(), "create failed");
        assert!(hush_sample_rate() == 48_000);
        assert!(hush_frame_samples() == 480);
        assert!(hush_pipeline_latency_samples(pipeline) > 0);
        let near = vec![0.05f32; 480];
        let mut out = vec![0.0f32; 480];
        let mut stats = HushFrameStats {
            process_ns: 0,
            cpu_percent: 0.0,
            algorithmic_latency_samples: 0,
            aec_applied: 0,
            ns_applied: 0,
        };
        let code = hush_pipeline_process(pipeline, near.as_ptr(), std::ptr::null(), out.as_mut_ptr(), &mut stats);
        assert_eq!(code, 0);
        assert_eq!(stats.ns_applied, 1);
        assert!(out.iter().all(|s| s.is_finite()));
        hush_pipeline_destroy(pipeline);
    }
}
