//! Per-frame DPDFNet 48 kHz streaming.
//!
//! The graph is the Apache-2.0 ONNX export: one Vorbis-windowed STFT frame in,
//! the enhanced frame and the RNN state out. ONNX Runtime runs it. The window,
//! hop, and overlap-add match `dpdfnet.stream.StreamEnhancer` (causal, 50% overlap).
//! DPDFNet2 and DPDFNet8 at 48 kHz share that STFT. They differ in the RNN state.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use ndarray::{Array1, Array4, ArrayViewD};
use ort::session::Session;
use ort::value::Tensor;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use rustfft::num_complex::Complex;

use crate::error::{Error, Result};

const WIN: usize = 960;
const HOP: usize = 480;
const BINS: usize = 481;
const DPDFNET8: &str = "dpdfnet8_48khz_hr";
const DPDFNET2: &str = "dpdfnet2_48khz_hr";

pub struct DpdfNet {
    /// Kept alive for the ONNX Runtime environment the session was created with.
    #[allow(dead_code)]
    env: ort::environment::Environment,
    session: Session,
    state: Array1<f32>,
    window: [f32; WIN],
    input: Vec<f32>,
    ola: [f32; WIN],
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    time: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    spec: Array4<f32>,
}

impl DpdfNet {
    pub fn load() -> Result<Self> {
        Self::load_named(DPDFNET8)
    }

    /// `dpdfnet2_48khz_hr` or `dpdfnet8_48khz_hr`. Both are the 960/480 STFT.
    pub fn load_named(stem: &str) -> Result<Self> {
        let path = model_path(stem)?;
        let env = shared_environment()?;
        let mut builder = Session::builder(&env).map_err(|err| Error::msg(err.to_string()))?;
        builder = builder
            .with_intra_threads(1)
            .map_err(|err| Error::msg(err.to_string()))?;
        let session = builder
            .commit_from_file(&path)
            .map_err(|err| Error::msg(format!("DPDFNet session: {err}")))?;
        let bins = spec_bins(&session)?;
        if bins != BINS {
            return Err(Error::msg(format!(
                "{stem} has {bins} frequency bins. Only the 48 kHz exports ({BINS} bins, 20 ms window) run in this pipeline."
            )));
        }
        let state = initial_state(&session)?;
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(WIN);
        let inverse = planner.plan_fft_inverse(WIN);
        let spectrum = forward.make_output_vec();
        Ok(Self {
            env,
            session,
            state,
            window: vorbis_window(),
            input: Vec::with_capacity(WIN * 2),
            ola: [0.0; WIN],
            time: forward.make_input_vec(),
            spectrum,
            spec: Array4::zeros((1, 1, BINS, 2)),
            forward,
            inverse,
        })
    }

    /// One 10 ms hop. The first hop is silence while the 20 ms window fills.
    /// After that, each hop is the committed half of the overlap-add, which
    /// lines up with the input from one hop earlier.
    pub fn process_hop(&mut self, input: &[f32], output: &mut [f32]) -> Result<()> {
        if input.len() != HOP || output.len() != HOP {
            return Err(Error::msg("DPDFNet hop must be 480 samples"));
        }
        self.input.extend_from_slice(input);
        if self.input.len() < WIN {
            output.fill(0.0);
            return Ok(());
        }
        for i in 0..WIN {
            self.time[i] = self.input[i] * self.window[i];
        }
        self.forward
            .process(&mut self.time, &mut self.spectrum)
            .map_err(|err| Error::msg(err.to_string()))?;
        for (bin, value) in self.spectrum.iter().enumerate() {
            self.spec[[0, 0, bin, 0]] = value.re;
            self.spec[[0, 0, bin, 1]] = value.im;
        }
        let spec_in = Tensor::from_array(self.spec.clone()).map_err(|err| Error::msg(err.to_string()))?;
        let state_in = Tensor::from_array(self.state.clone()).map_err(|err| Error::msg(err.to_string()))?;
        let outputs = self
            .session
            .run(ort::inputs![
                "spec" => spec_in,
                "state_in" => state_in,
            ])
            .map_err(|err| Error::msg(format!("DPDFNet inference: {err}")))?;
        let enhanced = outputs["spec_e"]
            .try_extract_array::<f32>()
            .map_err(|err| Error::msg(err.to_string()))?;
        copy_enhanced(&enhanced, &mut self.spectrum);
        if let Some(dc) = self.spectrum.first_mut() {
            dc.im = 0.0;
        }
        if let Some(nyquist) = self.spectrum.last_mut() {
            nyquist.im = 0.0;
        }
        let next_state = outputs["state_out"]
            .try_extract_array::<f32>()
            .map_err(|err| Error::msg(err.to_string()))?;
        copy_state(&next_state, &mut self.state);
        self.inverse
            .process(&mut self.spectrum, &mut self.time)
            .map_err(|err| Error::msg(err.to_string()))?;
        let scale = 1.0 / WIN as f32;
        for i in 0..WIN {
            self.ola[i] += self.time[i] * self.window[i] * scale;
        }
        output.copy_from_slice(&self.ola[..HOP]);
        self.ola.copy_within(HOP..WIN, 0);
        for sample in self.ola[WIN - HOP..].iter_mut() {
            *sample = 0.0;
        }
        self.input.drain(..HOP);
        Ok(())
    }

    /// Speech delay of this frame API against clean VoiceBank audio.
    ///
    /// The Python stream's group delay is 40 ms, and its first returned hop
    /// arrives after one 20 ms window. This API emits a frame every 10 ms,
    /// including the 10 ms spent filling that window, so the aligned delay is
    /// 50 ms. Prefilling the window with zeros was measured and did not remove
    /// the extra hop. DPDFNet2 and DPDFNet8, at 16 kHz and at 48 kHz, share it.
    pub fn latency_samples() -> u32 {
        5 * HOP as u32
    }
}

fn copy_enhanced(view: &ArrayViewD<f32>, spectrum: &mut [Complex<f32>]) {
    for bin in 0..BINS {
        let re = *view.get([0, 0, bin, 0]).unwrap_or(&0.0);
        let im = *view.get([0, 0, bin, 1]).unwrap_or(&0.0);
        spectrum[bin] = Complex::new(re, im);
    }
}

fn copy_state(view: &ArrayViewD<f32>, state: &mut Array1<f32>) {
    let flat: Vec<f32> = view.iter().copied().collect();
    let n = flat.len().min(state.len());
    state.as_slice_mut().unwrap()[..n].copy_from_slice(&flat[..n]);
}

fn vorbis_window() -> [f32; WIN] {
    let mut window = [0.0; WIN];
    let half = WIN as f32 / 2.0;
    for (i, sample) in window.iter_mut().enumerate() {
        let s = ((i as f32 + 0.5) / half * std::f32::consts::FRAC_PI_2).sin();
        *sample = (s * s * std::f32::consts::FRAC_PI_2).sin();
    }
    window
}

fn shared_environment() -> Result<ort::environment::Environment> {
    static ENV: OnceLock<ort::environment::Environment> = OnceLock::new();
    if let Some(env) = ENV.get() {
        return Ok(env.clone());
    }
    match ort::init().with_name("hush").build() {
        Ok(env) => {
            let _ = ENV.set(env.clone());
            Ok(env)
        }
        Err(err) => ENV.get().cloned().ok_or_else(|| Error::msg(err.to_string())),
    }
}

fn model_path(stem: &str) -> Result<PathBuf> {
    let override_var = if stem == DPDFNET2 {
        "HUSH_DPDF_ONNX_2"
    } else {
        "HUSH_DPDF_ONNX"
    };
    if let Ok(path) = std::env::var(override_var) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(Error::msg(format!(
            "{override_var} does not point at a file: {}",
            path.display()
        )));
    }
    let file_name = format!("{stem}.onnx");
    let mut candidates = Vec::new();
    if let Ok(cache) = std::env::var("XDG_CACHE_HOME") {
        candidates.push(PathBuf::from(cache).join("dpdfnet/models").join(&file_name));
    }
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(
            PathBuf::from(home)
                .join(".cache/dpdfnet/models")
                .join(&file_name),
        );
    }
    candidates.push(PathBuf::from("bench/.cache").join(&file_name));
    for path in &candidates {
        if path.is_file() {
            return Ok(path.clone());
        }
    }
    Err(Error::msg(format!(
        "{stem} was not found. Download it with `dpdfnet download {stem}` or set {override_var}."
    )))
}

fn spec_bins(session: &Session) -> Result<usize> {
    let input = session
        .inputs()
        .iter()
        .find(|input| input.name() == "spec")
        .ok_or_else(|| Error::msg("DPDFNet graph has no spec input"))?;
    let ort::value::ValueType::Tensor { shape, .. } = input.dtype() else {
        return Err(Error::msg("DPDFNet spec input is not a tensor"));
    };
    let dims: Vec<i64> = shape.iter().copied().collect();
    let bins = dims.get(2).copied().unwrap_or(-1);
    if bins <= 0 {
        return Err(Error::msg("DPDFNet spec input has a dynamic frequency axis"));
    }
    Ok(bins as usize)
}

fn initial_state(session: &Session) -> Result<Array1<f32>> {
    let input = session
        .inputs()
        .iter()
        .find(|input| input.name() == "state_in")
        .ok_or_else(|| Error::msg("DPDFNet graph has no state_in"))?;
    let ort::value::ValueType::Tensor { shape, .. } = input.dtype() else {
        return Err(Error::msg("DPDFNet state_in is not a tensor"));
    };
    let len = shape.num_elements();
    if len == 0 {
        return Err(Error::msg("DPDFNet state length is zero"));
    }
    let mut state = Array1::zeros(len);
    let metadata = session.metadata().map_err(|err| Error::msg(err.to_string()))?;
    let erb_n = metadata
        .custom("erb_norm_state_size")
        .and_then(|text| text.parse().ok())
        .unwrap_or(BINS);
    if let Some(text) = metadata.custom("erb_norm_init") {
        fill_csv(&mut state, 0, &text);
    }
    if let Some(text) = metadata.custom("spec_norm_init") {
        fill_csv(&mut state, erb_n, &text);
    }
    Ok(state)
}

fn fill_csv(state: &mut Array1<f32>, offset: usize, text: &str) {
    for (i, piece) in text.split(',').enumerate() {
        if let Ok(value) = piece.trim().parse::<f32>() {
            if let Some(slot) = state.get_mut(offset + i) {
                *slot = value;
            }
        }
    }
}

#[cfg(test)]
fn model_file_exists() -> bool {
    model_path(DPDFNET8).is_ok()
}

#[cfg(test)]
fn model2_file_exists() -> bool {
    model_path(DPDFNET2).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn hop_is_finite_and_reports_frame_time() {
        if !model_file_exists() {
            eprintln!("skipping DPDFNet timing; model file is not cached");
            return;
        }
        let mut net = DpdfNet::load().unwrap();
        let mut out = [0.0f32; HOP];
        let mut times = Vec::new();
        for frame_index in 0..300 {
            let frame: Vec<f32> = (0..HOP)
                .map(|i| {
                    let n = frame_index * HOP + i;
                    (n as f32 * 220.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.2
                })
                .collect();
            let started = Instant::now();
            net.process_hop(&frame, &mut out).unwrap();
            if frame_index > 5 {
                times.push(started.elapsed().as_nanos() as u64);
            }
        }
        assert!(out.iter().all(|sample| sample.is_finite()));
        times.sort_unstable();
        let p95 = times[times.len() * 95 / 100];
        eprintln!(
            "dpdfnet8 hop p50 {} us p95 {} us",
            times[times.len() / 2] / 1000,
            p95 / 1000
        );
        assert_eq!(DpdfNet::latency_samples(), 2400);
    }

    #[test]
    fn two_sessions_share_one_runtime() {
        if !model_file_exists() {
            eprintln!("skipping second DPDFNet session; model file is not cached");
            return;
        }
        let mut first = DpdfNet::load().unwrap();
        let mut second = DpdfNet::load().unwrap();
        let frame = [0.1f32; HOP];
        let mut out = [0.0f32; HOP];
        first.process_hop(&frame, &mut out).unwrap();
        second.process_hop(&frame, &mut out).unwrap();
        assert!(out.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn dpdfnet2_hop_is_finite() {
        if !model2_file_exists() {
            eprintln!("skipping DPDFNet2; model file is not cached");
            return;
        }
        let mut net = DpdfNet::load_named("dpdfnet2_48khz_hr").unwrap();
        let mut out = [0.0f32; HOP];
        let mut times = Vec::new();
        for frame_index in 0..40 {
            let frame = [0.05f32; HOP];
            let started = Instant::now();
            net.process_hop(&frame, &mut out).unwrap();
            if frame_index > 5 {
                times.push(started.elapsed().as_nanos() as u64);
            }
        }
        assert!(out.iter().all(|sample| sample.is_finite()));
        times.sort_unstable();
        eprintln!(
            "dpdfnet2 hop p50 {} us p95 {} us",
            times[times.len() / 2] / 1000,
            times[times.len() * 95 / 100] / 1000
        );
    }
}

#[allow(dead_code)]
fn _path_is_used(path: &Path) -> bool {
    path.exists()
}
