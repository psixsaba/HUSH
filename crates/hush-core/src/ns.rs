use deep_filter_rt::tract::DfTract;
use ndarray::Array2;
use nnnoiseless::DenoiseState;

use crate::constants::FRAME_SAMPLES;
use crate::error::{Error, Result};

/// Which noise suppressor runs after echo cancellation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NsBackend {
    Off,
    /// DeepFilterNet3 on tract. Full-band, about 30 ms of model delay.
    DeepFilterNet3,
    /// RNNoise via nnnoiseless. One 10 ms hop of overlap-add delay.
    RnNoise,
}

impl NsBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            NsBackend::Off => "off",
            NsBackend::DeepFilterNet3 => "deepfilter",
            NsBackend::RnNoise => "rnnoise",
        }
    }

    pub fn parse(text: &str) -> Result<Self> {
        match text {
            "off" | "none" => Ok(NsBackend::Off),
            "df" | "df3" | "deepfilter" | "deepfilternet3" => Ok(NsBackend::DeepFilterNet3),
            "rnn" | "rnnoise" => Ok(NsBackend::RnNoise),
            other => Err(Error::msg(format!(
                "unknown noise suppressor '{other}', expected deepfilter, rnnoise, or off"
            ))),
        }
    }
}

/// RNNoise's synthesis hop. The first frame fades in; the stream is not
/// shortened, so this constant is the alignment shift.
pub const RNNOISE_LATENCY_SAMPLES: u32 = FRAME_SAMPLES as u32;

pub struct NoiseSuppressor {
    stage: Stage,
    strength: f32,
}

enum Stage {
    Off,
    DeepFilter(DfStage),
    RnNoise(RnnStage),
}

struct DfStage {
    model: DfTract,
    delay: u32,
    noisy: Array2<f32>,
    enhanced: Array2<f32>,
}

struct RnnStage {
    state: Box<DenoiseState<'static>>,
    delay: DelayLine,
}

struct DelayLine {
    buf: Vec<f32>,
    pos: usize,
}

impl DelayLine {
    fn new(delay: usize) -> Self {
        Self {
            buf: vec![0.0; delay.max(1)],
            pos: 0,
        }
    }

    fn push(&mut self, sample: f32) -> f32 {
        let out = self.buf[self.pos];
        self.buf[self.pos] = sample;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
        out
    }
}

impl NoiseSuppressor {
    pub fn new(backend: NsBackend, strength: f32) -> Result<Self> {
        let strength = strength.clamp(0.0, 1.0);
        let stage = match backend {
            NsBackend::Off => Stage::Off,
            NsBackend::DeepFilterNet3 => Stage::DeepFilter(DfStage::load()?),
            NsBackend::RnNoise => Stage::RnNoise(RnnStage::new()),
        };
        let mut suppressor = Self { stage, strength };
        suppressor.apply_strength();
        Ok(suppressor)
    }

    pub fn set_strength(&mut self, strength: f32) {
        self.strength = strength.clamp(0.0, 1.0);
        self.apply_strength();
    }

    fn apply_strength(&mut self) {
        if let Stage::DeepFilter(stage) = &mut self.stage {
            let limit = if self.strength <= 0.0 {
                0.0
            } else {
                self.strength * 100.0
            };
            stage.model.set_atten_lim(limit);
        }
    }

    /// Samples of delay inserted into the transmitted audio.
    /// Strength 0 skips the model, so it adds none.
    pub fn latency_samples(&self) -> u32 {
        if self.strength <= 0.0 {
            return 0;
        }
        match &self.stage {
            Stage::Off => 0,
            Stage::DeepFilter(stage) => stage.delay,
            Stage::RnNoise(_) => RNNOISE_LATENCY_SAMPLES,
        }
    }

    pub fn process(&mut self, input: &[f32], output: &mut [f32]) -> Result<()> {
        if input.len() != FRAME_SAMPLES || output.len() != FRAME_SAMPLES {
            return Err(Error::msg(format!(
                "noise suppressor expects {FRAME_SAMPLES}-sample frames"
            )));
        }
        if self.strength <= 0.0 {
            output.copy_from_slice(input);
            return Ok(());
        }
        match &mut self.stage {
            Stage::Off => output.copy_from_slice(input),
            Stage::DeepFilter(stage) => stage.process(input, output)?,
            Stage::RnNoise(stage) => stage.process(input, output, self.strength),
        }
        Ok(())
    }
}

impl DfStage {
    fn load() -> Result<Self> {
        let model = DfTract::default();
        if model.sr != 48_000 {
            return Err(Error::msg(format!(
                "DeepFilterNet3 sample rate is {}, expected 48000",
                model.sr
            )));
        }
        if model.hop_size != FRAME_SAMPLES {
            return Err(Error::msg(format!(
                "DeepFilterNet3 hop is {} samples, expected {FRAME_SAMPLES}",
                model.hop_size
            )));
        }
        if model.ch != 1 {
            return Err(Error::msg(format!(
                "DeepFilterNet3 channel count is {}, expected 1",
                model.ch
            )));
        }
        let delay = model.delay() as u32;
        Ok(Self {
            noisy: Array2::zeros((model.ch, model.hop_size)),
            enhanced: Array2::zeros((model.ch, model.hop_size)),
            delay,
            model,
        })
    }

    fn process(&mut self, input: &[f32], output: &mut [f32]) -> Result<()> {
        for (dst, src) in self.noisy.row_mut(0).iter_mut().zip(input.iter()) {
            *dst = *src;
        }
        self.model
            .process(self.noisy.view(), self.enhanced.view_mut())
            .map_err(|err| Error::msg(err.to_string()))?;
        for (dst, src) in output.iter_mut().zip(self.enhanced.row(0).iter()) {
            *dst = *src;
        }
        Ok(())
    }
}

impl RnnStage {
    fn new() -> Self {
        Self {
            state: DenoiseState::new(),
            delay: DelayLine::new(RNNOISE_LATENCY_SAMPLES as usize),
        }
    }

    fn process(&mut self, input: &[f32], output: &mut [f32], strength: f32) {
        let mut scaled = [0.0f32; FRAME_SAMPLES];
        let mut wet = [0.0f32; FRAME_SAMPLES];
        for (dst, src) in scaled.iter_mut().zip(input.iter()) {
            *dst = (*src * 32768.0).clamp(-32768.0, 32767.0);
        }
        self.state.process_frame(&mut wet, &scaled);
        for (i, dst) in output.iter_mut().enumerate() {
            let denoised = wet[i] / 32768.0;
            if strength >= 0.999 {
                *dst = denoised;
            } else {
                let dry = self.delay.push(input[i]);
                *dst = dry * (1.0 - strength) + denoised * strength;
            }
        }
    }
}
