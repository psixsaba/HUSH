use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};
use rtrb::{Consumer, Producer, RingBuffer};
use serde::Serialize;

use crate::constants::{FRAME_SAMPLES, SAMPLE_RATE};
use crate::devices::{self, DeviceLists};
use crate::error::{Error, Result};
use crate::pipeline::{Pipeline, PipelineConfig};
use crate::resample::SincResampler;
use crate::ring::{RingAttachment, RingPublisher};
use crate::NsBackend;

#[derive(Clone, Debug)]
pub struct SessionConfig {
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub far_device: Option<String>,
    pub pipeline: PipelineConfig,
    pub publish_ring: bool,
    /// Keep the virtual-speaker segment alive and play it to the output device.
    pub bridge_speaker: bool,
    /// Also play the cleaned microphone to the output device.
    pub monitor_cleaned: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            input_device: None,
            output_device: None,
            far_device: None,
            pipeline: PipelineConfig::default(),
            publish_ring: true,
            bridge_speaker: true,
            monitor_cleaned: false,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SessionStats {
    pub running: bool,
    pub ready: bool,
    pub frames: u64,
    pub process_ns: u64,
    pub cpu_percent: f32,
    pub algorithmic_latency_samples: u32,
    pub algorithmic_latency_ms: f64,
    pub buffer_samples: u32,
    pub buffer_ms: f64,
    pub ring_underruns: u64,
    pub output_underruns: u64,
    pub input_drops: u64,
    pub ns_backend: NsBackend,
    pub strength: f32,
    pub aec_enabled: bool,
    pub enabled: bool,
    pub error: Option<String>,
}

enum Command {
    Config(PipelineConfig),
    Stop,
}

struct Shared {
    ready: AtomicBool,
    failed: AtomicBool,
    running: AtomicBool,
    frames: AtomicU64,
    process_ns: AtomicU64,
    cpu_bits: AtomicU32,
    latency_samples: AtomicU32,
    buffer_samples: AtomicU32,
    input_drops: AtomicU64,
    output_underruns: AtomicU64,
    error: Mutex<Option<String>>,
    config: Mutex<PipelineConfig>,
}

pub struct RealtimeSession {
    commands: Sender<Command>,
    shared: Arc<Shared>,
    streams: Vec<Stream>,
    worker: Option<JoinHandle<()>>,
}

pub fn list_devices() -> Result<DeviceLists> {
    devices::list_devices()
}

impl RealtimeSession {
    pub fn start(config: SessionConfig) -> Result<Self> {
        let host = devices::host();
        let input = devices::find_input(&host, config.input_device.as_deref())?;
        let output = match &config.output_device {
            Some(name) => Some(devices::find_output(&host, Some(name))?),
            None => None,
        };
        let far = match &config.far_device {
            Some(name) => Some(devices::find_input(&host, Some(name))?),
            None => None,
        };

        let (in_prod, in_cons) = RingBuffer::<f32>::new(SAMPLE_RATE as usize);
        let (out_prod, out_cons) = RingBuffer::<f32>::new(SAMPLE_RATE as usize * 2);
        let (far_prod, far_cons) = RingBuffer::<f32>::new(SAMPLE_RATE as usize);

        let shared = Arc::new(Shared {
            ready: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            running: AtomicBool::new(true),
            frames: AtomicU64::new(0),
            process_ns: AtomicU64::new(0),
            cpu_bits: AtomicU32::new(0),
            latency_samples: AtomicU32::new(0),
            buffer_samples: AtomicU32::new(0),
            input_drops: AtomicU64::new(0),
            output_underruns: AtomicU64::new(0),
            error: Mutex::new(None),
            config: Mutex::new(config.pipeline.clone()),
        });

        let (input_stream, input_rate) =
            open_input(&input, in_prod, Arc::clone(&shared))?;
        let (output_stream, output_rate) = if let Some(output) = output.as_ref() {
            let (stream, rate) = open_output(output, out_cons, Arc::clone(&shared))?;
            (Some(stream), rate)
        } else {
            (None, SAMPLE_RATE)
        };
        let (far_stream, far_rate) = if let Some(far) = far.as_ref() {
            let (stream, rate) = open_input(far, far_prod, Arc::clone(&shared))?;
            (Some(stream), rate)
        } else {
            (None, SAMPLE_RATE)
        };

        input_stream.play()?;
        if let Some(stream) = output_stream.as_ref() {
            stream.play()?;
        }
        if let Some(stream) = far_stream.as_ref() {
            stream.play()?;
        }

        let (tx, rx) = mpsc::channel();
        let worker_shared = Arc::clone(&shared);
        let pipeline_config = config.pipeline.clone();
        let publish_ring = config.publish_ring;
        let has_far = far_stream.is_some();
        let has_output = output_stream.is_some();
        let worker = thread::Builder::new()
            .name("hush-audio".into())
            .spawn(move || {
                worker(
                    rx,
                    worker_shared,
                    pipeline_config,
                    in_cons,
                    out_prod,
                    far_cons,
                    input_rate,
                    output_rate,
                    far_rate,
                    has_far,
                    publish_ring,
                    has_output,
                    config.bridge_speaker,
                    config.monitor_cleaned,
                )
            })
            .map_err(|err| Error::msg(err.to_string()))?;

        let mut streams = vec![input_stream];
        if let Some(stream) = output_stream {
            streams.push(stream);
        }
        if let Some(stream) = far_stream {
            streams.push(stream);
        }

        let session = Self {
            commands: tx,
            shared,
            streams,
            worker: Some(worker),
        };
        session.wait_until_ready()?;
        Ok(session)
    }

    fn wait_until_ready(&self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(90);
        while Instant::now() < deadline {
            if self.shared.ready.load(Ordering::Acquire) {
                return Ok(());
            }
            if self.shared.failed.load(Ordering::Acquire) {
                let err = self
                    .shared
                    .error
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or_else(|| "audio pipeline failed to start".into());
                return Err(Error::msg(err));
            }
            thread::sleep(Duration::from_millis(20));
        }
        Err(Error::msg("timed out loading the audio pipeline"))
    }

    pub fn set_config(&self, config: PipelineConfig) {
        let config = config.sanitized();
        *self.shared.config.lock().unwrap() = config.clone();
        let _ = self.commands.send(Command::Config(config));
    }

    pub fn stats(&self) -> SessionStats {
        let config = self.shared.config.lock().unwrap().clone();
        let latency = self.shared.latency_samples.load(Ordering::Relaxed);
        let buffer = self.shared.buffer_samples.load(Ordering::Relaxed);
        let cpu_bits = self.shared.cpu_bits.load(Ordering::Relaxed);
        SessionStats {
            running: self.shared.running.load(Ordering::Relaxed),
            ready: self.shared.ready.load(Ordering::Relaxed),
            frames: self.shared.frames.load(Ordering::Relaxed),
            process_ns: self.shared.process_ns.load(Ordering::Relaxed),
            cpu_percent: f32::from_bits(cpu_bits),
            algorithmic_latency_samples: latency,
            algorithmic_latency_ms: latency as f64 / SAMPLE_RATE as f64 * 1000.0,
            buffer_samples: buffer,
            buffer_ms: buffer as f64 / SAMPLE_RATE as f64 * 1000.0,
            ring_underruns: 0,
            output_underruns: self.shared.output_underruns.load(Ordering::Relaxed),
            input_drops: self.shared.input_drops.load(Ordering::Relaxed),
            ns_backend: config.ns_backend,
            strength: config.strength,
            aec_enabled: config.aec_enabled,
            enabled: config.enabled,
            error: self.shared.error.lock().unwrap().clone(),
        }
    }

    pub fn stop(&mut self) {
        self.shared.running.store(false, Ordering::Release);
        let _ = self.commands.send(Command::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.streams.clear();
    }
}

impl Drop for RealtimeSession {
    fn drop(&mut self) {
        self.stop();
    }
}

fn fail(shared: &Shared, message: String) {
    *shared.error.lock().unwrap() = Some(message);
    shared.failed.store(true, Ordering::Release);
}

fn worker(
    commands: Receiver<Command>,
    shared: Arc<Shared>,
    mut config: PipelineConfig,
    mut input: Consumer<f32>,
    mut output: Producer<f32>,
    mut far: Consumer<f32>,
    input_rate: u32,
    output_rate: u32,
    far_rate: u32,
    has_far: bool,
    publish_ring: bool,
    has_output: bool,
    bridge_speaker: bool,
    monitor_cleaned: bool,
) {
    let pipeline = std::panic::catch_unwind(AssertUnwindSafe(|| Pipeline::new(config.clone())));
    let mut pipeline = match pipeline {
        Ok(Ok(pipeline)) => pipeline,
        Ok(Err(err)) => {
            fail(&shared, err.to_string());
            return;
        }
        Err(_) => {
            fail(&shared, "audio pipeline panicked while loading".into());
            return;
        }
    };

    let mut publisher = if publish_ring {
        match RingPublisher::create() {
            Ok(publisher) => Some(publisher),
            Err(err) => {
                fail(&shared, err.to_string());
                return;
            }
        }
    } else {
        None
    };

    shared.ready.store(true, Ordering::Release);

    let mut input_resampler = (input_rate != SAMPLE_RATE)
        .then(|| SincResampler::new(input_rate, SAMPLE_RATE).ok())
        .flatten();
    let mut output_resampler = (output_rate != SAMPLE_RATE)
        .then(|| SincResampler::new(SAMPLE_RATE, output_rate).ok())
        .flatten();
    let mut far_resampler = (far_rate != SAMPLE_RATE)
        .then(|| SincResampler::new(far_rate, SAMPLE_RATE).ok())
        .flatten();

    let mut native_in = Vec::new();
    let mut at_48k = Vec::new();
    let mut native_far = Vec::new();
    let mut far_48k = Vec::new();
    let mut near_frame = vec![0.0f32; FRAME_SAMPLES];
    let mut far_frame = vec![0.0f32; FRAME_SAMPLES];
    let mut out_frame = vec![0.0f32; FRAME_SAMPLES];
    let mut speaker_frame = vec![0.0f32; FRAME_SAMPLES];

    let speaker_owner = if bridge_speaker {
        match RingPublisher::create_named(crate::ring::SPEAKER_NAME) {
            Ok(owner) => Some(owner),
            Err(err) => {
                fail(&shared, err.to_string());
                return;
            }
        }
    } else {
        None
    };
    let speaker_view = if speaker_owner.is_some() {
        match RingAttachment::attach_named(crate::ring::SPEAKER_NAME) {
            Ok(view) => Some(view),
            Err(err) => {
                fail(&shared, err.to_string());
                return;
            }
        }
    } else {
        None
    };
    let mut speaker_reader = match speaker_view.as_ref().map(|view| view.reader()) {
        Some(Ok(reader)) => Some(reader),
        Some(Err(err)) => {
            fail(&shared, err.to_string());
            return;
        }
        None => None,
    };

    loop {
        if !shared.running.load(Ordering::Acquire) {
            break;
        }
        while let Ok(command) = commands.try_recv() {
            match command {
                Command::Stop => {
                    shared.running.store(false, Ordering::Release);
                }
                Command::Config(next) => {
                    config = next.sanitized();
                    if let Err(err) = pipeline.set_config(config.clone()) {
                        *shared.error.lock().unwrap() = Some(err.to_string());
                    }
                    *shared.config.lock().unwrap() = config.clone();
                }
            }
        }
        if !shared.running.load(Ordering::Acquire) {
            break;
        }

        drain(&mut input, &mut native_in, 8192);
        if let Some(resampler) = input_resampler.as_mut() {
            if !native_in.is_empty() {
                at_48k.extend(resampler.process(&native_in, false));
                native_in.clear();
            }
        } else {
            at_48k.append(&mut native_in);
        }

        if has_far {
            drain(&mut far, &mut native_far, 8192);
            if let Some(resampler) = far_resampler.as_mut() {
                if !native_far.is_empty() {
                    far_48k.extend(resampler.process(&native_far, false));
                    native_far.clear();
                }
            } else {
                far_48k.append(&mut native_far);
            }
        }

        if at_48k.len() > SAMPLE_RATE as usize {
            let drop_n = at_48k.len() - FRAME_SAMPLES * 4;
            at_48k.drain(..drop_n);
            shared.input_drops.fetch_add(drop_n as u64, Ordering::Relaxed);
        }

        if at_48k.len() < FRAME_SAMPLES {
            thread::sleep(Duration::from_millis(2));
            continue;
        }

        let mut processed = 0;
        while at_48k.len() >= FRAME_SAMPLES && processed < 8 {
            near_frame.copy_from_slice(&at_48k[..FRAME_SAMPLES]);
            at_48k.drain(..FRAME_SAMPLES);
            let speaker_live = if let Some(reader) = speaker_reader.as_mut() {
                let missed = reader.read_next(&mut speaker_frame);
                (missed as usize) < FRAME_SAMPLES
            } else {
                false
            };
            let far_ref = if has_far {
                if far_48k.len() >= FRAME_SAMPLES {
                    far_frame.copy_from_slice(&far_48k[..FRAME_SAMPLES]);
                    far_48k.drain(..FRAME_SAMPLES);
                } else {
                    far_frame.fill(0.0);
                }
                Some(far_frame.as_slice())
            } else if speaker_live {
                Some(speaker_frame.as_slice())
            } else {
                None
            };
            match pipeline.process_frame(&near_frame, far_ref, &mut out_frame) {
                Ok(frame_stats) => {
                    shared
                        .frames
                        .fetch_add(1, Ordering::Relaxed);
                    shared
                        .process_ns
                        .store(frame_stats.process_ns, Ordering::Relaxed);
                    shared
                        .cpu_bits
                        .store(frame_stats.cpu_percent.to_bits(), Ordering::Relaxed);
                    shared.latency_samples.store(
                        frame_stats.algorithmic_latency_samples,
                        Ordering::Relaxed,
                    );
                    let buffered = at_48k.len() + input.slots();
                    shared
                        .buffer_samples
                        .store(buffered as u32, Ordering::Relaxed);
                    if let Some(publisher) = publisher.as_ref() {
                        publisher.set_muted(config.muted);
                        publisher.set_latency_samples(frame_stats.algorithmic_latency_samples);
                        publisher.write(&out_frame);
                    }
                    if has_output {
                        let playback = if bridge_speaker && speaker_live {
                            if monitor_cleaned {
                                for (remote, cleaned) in speaker_frame.iter_mut().zip(out_frame.iter()) {
                                    *remote = (*remote + 0.2 * *cleaned).clamp(-1.0, 1.0);
                                }
                            }
                            speaker_frame.as_slice()
                        } else if monitor_cleaned || !bridge_speaker {
                            out_frame.as_slice()
                        } else {
                            &[]
                        };
                        if !playback.is_empty() {
                            push_output(&mut output, &mut output_resampler, playback, output_rate);
                        }
                    }
                }
                Err(err) => {
                    *shared.error.lock().unwrap() = Some(err.to_string());
                }
            }
            processed += 1;
        }
    }

    if let Some(publisher) = publisher.as_mut() {
        publisher.set_active(false);
    }
}

fn push_output(
    output: &mut Producer<f32>,
    resampler: &mut Option<SincResampler>,
    frame: &[f32],
    output_rate: u32,
) {
    if output_rate == SAMPLE_RATE {
        push_samples(output, frame);
        return;
    }
    if let Some(resampler) = resampler.as_mut() {
        let converted = resampler.process(frame, false);
        push_samples(output, &converted);
    }
}

fn push_samples(output: &mut Producer<f32>, samples: &[f32]) {
    for sample in samples {
        if output.push(*sample).is_err() {
            break;
        }
    }
}

fn drain(consumer: &mut Consumer<f32>, dst: &mut Vec<f32>, max: usize) {
    while dst.len() < max {
        match consumer.pop() {
            Ok(sample) => dst.push(sample),
            Err(_) => break,
        }
    }
}

fn open_input(
    device: &cpal::Device,
    producer: Producer<f32>,
    shared: Arc<Shared>,
) -> Result<(Stream, u32)> {
    let supported = device.default_input_config()?;
    let rate = supported.sample_rate().0;
    let channels = supported.channels() as usize;
    let format = supported.sample_format();
    let config = supported.config();
    let stream = build_input(device, &config, format, channels, producer, shared)?;
    Ok((stream, rate))
}

fn open_output(
    device: &cpal::Device,
    consumer: Consumer<f32>,
    shared: Arc<Shared>,
) -> Result<(Stream, u32)> {
    let supported = device.default_output_config()?;
    let rate = supported.sample_rate().0;
    let channels = supported.channels() as usize;
    let format = supported.sample_format();
    let config = supported.config();
    let stream = build_output(device, &config, format, channels, consumer, shared)?;
    Ok((stream, rate))
}

fn build_input(
    device: &cpal::Device,
    config: &StreamConfig,
    format: SampleFormat,
    channels: usize,
    producer: Producer<f32>,
    shared: Arc<Shared>,
) -> Result<Stream> {
    match format {
        SampleFormat::F32 => input_of::<f32>(device, config, channels, producer, shared),
        SampleFormat::I16 => input_of::<i16>(device, config, channels, producer, shared),
        SampleFormat::I32 => input_of::<i32>(device, config, channels, producer, shared),
        SampleFormat::U16 => input_of::<u16>(device, config, channels, producer, shared),
        other => Err(Error::msg(format!("unsupported input sample format {other}"))),
    }
}

fn build_output(
    device: &cpal::Device,
    config: &StreamConfig,
    format: SampleFormat,
    channels: usize,
    consumer: Consumer<f32>,
    shared: Arc<Shared>,
) -> Result<Stream> {
    match format {
        SampleFormat::F32 => output_of::<f32>(device, config, channels, consumer, shared),
        SampleFormat::I16 => output_of::<i16>(device, config, channels, consumer, shared),
        SampleFormat::I32 => output_of::<i32>(device, config, channels, consumer, shared),
        SampleFormat::U16 => output_of::<u16>(device, config, channels, consumer, shared),
        other => Err(Error::msg(format!("unsupported output sample format {other}"))),
    }
}

fn input_of<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    channels: usize,
    mut producer: Producer<f32>,
    shared: Arc<Shared>,
) -> Result<Stream>
where
    T: SizedSample + Send + 'static,
    f32: Sample + FromSample<T>,
{
    let stream = device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                if channels == 0 {
                    return;
                }
                let frames = data.len() / channels;
                for frame in 0..frames {
                    let mut acc = 0.0f32;
                    for channel in 0..channels {
                        acc += f32::from_sample(data[frame * channels + channel]);
                    }
                    if producer.push(acc / channels as f32).is_err() {
                        shared.input_drops.fetch_add(1, Ordering::Relaxed);
                    }
                }
            },
            |err| eprintln!("hush input: {err}"),
            None,
        )
        .map_err(Error::from)?;
    Ok(stream)
}

fn output_of<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    channels: usize,
    mut consumer: Consumer<f32>,
    shared: Arc<Shared>,
) -> Result<Stream>
where
    T: SizedSample + Send + 'static,
    T: FromSample<f32>,
{
    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                if channels == 0 {
                    return;
                }
                let frames = data.len() / channels;
                for frame in 0..frames {
                    let sample = match consumer.pop() {
                        Ok(sample) => sample,
                        Err(_) => {
                            shared.output_underruns.fetch_add(1, Ordering::Relaxed);
                            0.0
                        }
                    };
                    for channel in 0..channels {
                        data[frame * channels + channel] = T::from_sample(sample);
                    }
                }
            },
            |err| eprintln!("hush output: {err}"),
            None,
        )
        .map_err(Error::from)?;
    Ok(stream)
}
