//! User-session bridge between the shared-memory rings and the virtual driver.
//!
//! The kernel miniport cannot open `Local\` mappings created by the logged-in
//! user. This loop runs in that session: it copies HUSH Microphone into
//! `HUSH_IOCTL_WRITE_CAPTURE` and copies `HUSH_IOCTL_READ_RENDER` into HUSH
//! Speaker. The numbers match `drivers/windows/ioctl.h`.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, Stream, StreamConfig};

use crate::constants::FRAME_SAMPLES;
use crate::devices::{self, is_hush_microphone, is_hush_speaker};
use crate::error::{Error, Result};
use crate::ioctl::{self, AUDIO_BYTES};
use crate::ring::{RingAttachment, MIC_NAME, SPEAKER_NAME};

const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const FILE_SHARE_READ_WRITE: u32 = 0x0000_0003;
const OPEN_EXISTING: u32 = 3;
const INVALID_HANDLE: isize = -1;
const ERROR_ALREADY_EXISTS: u32 = 183;

#[repr(C)]
struct HushIoctlAudio {
    sample_rate: u32,
    channels: u32,
    frames: u32,
    samples: [f32; FRAME_SAMPLES],
}

const _: () = assert!(std::mem::size_of::<HushIoctlAudio>() == AUDIO_BYTES);

unsafe extern "system" {
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *mut core::ffi::c_void,
        disposition: u32,
        flags: u32,
        template: isize,
    ) -> isize;
    fn DeviceIoControl(
        handle: isize,
        code: u32,
        input: *const u8,
        input_len: u32,
        output: *mut u8,
        output_len: u32,
        returned: *mut u32,
        overlapped: *mut core::ffi::c_void,
    ) -> i32;
    fn CloseHandle(handle: isize) -> i32;
    fn CreateMutexW(security: *mut core::ffi::c_void, initial_owner: i32, name: *const u16) -> isize;
    fn GetLastError() -> u32;
}

struct Device(isize);

impl Device {
    fn open() -> Option<Self> {
        let name: Vec<u16> = "\\\\.\\HushVad\0".encode_utf16().collect();
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ_WRITE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                0,
            )
        };
        if handle == 0 || handle == INVALID_HANDLE {
            None
        } else {
            Some(Self(handle))
        }
    }

    fn write_capture(&self, samples: &[f32]) -> bool {
        let mut packet = HushIoctlAudio {
            sample_rate: 48_000,
            channels: 1,
            frames: samples.len().min(FRAME_SAMPLES) as u32,
            samples: [0.0; FRAME_SAMPLES],
        };
        let n = samples.len().min(FRAME_SAMPLES);
        packet.samples[..n].copy_from_slice(&samples[..n]);
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                self.0,
                ioctl::WRITE_CAPTURE,
                &packet as *const HushIoctlAudio as *const u8,
                std::mem::size_of::<HushIoctlAudio>() as u32,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        ok != 0
    }

    fn read_render(&self) -> Option<Vec<f32>> {
        let mut packet = HushIoctlAudio {
            sample_rate: 0,
            channels: 0,
            frames: 0,
            samples: [0.0; FRAME_SAMPLES],
        };
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                self.0,
                ioctl::READ_RENDER,
                std::ptr::null(),
                0,
                &mut packet as *mut HushIoctlAudio as *mut u8,
                std::mem::size_of::<HushIoctlAudio>() as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return None;
        }
        let n = (packet.frames as usize).min(FRAME_SAMPLES);
        if n == 0 {
            return Some(Vec::new());
        }
        Some(packet.samples[..n].to_vec())
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// Held for the life of the process that owns the bridge. A second HUSH
/// process sees the named mutex and leaves the driver alone.
pub struct BridgeGuard(isize);

impl Drop for BridgeGuard {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// `Some` when this process now owns `Local\HushBridge`. `None` when another
/// HUSH process already does.
pub fn acquire_bridge() -> Option<BridgeGuard> {
    let name: Vec<u16> = "Local\\HushBridge\0".encode_utf16().collect();
    let handle = unsafe { CreateMutexW(std::ptr::null_mut(), 1, name.as_ptr()) };
    if handle == 0 || handle == INVALID_HANDLE {
        return None;
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe { CloseHandle(handle) };
        return None;
    }
    Some(BridgeGuard(handle))
}

/// Block until `stop` is set. Missing driver or rings are retried, not fatal.
pub fn run_windows_bridge(stop: &AtomicBool) -> Result<()> {
    if FRAME_SAMPLES != 480 {
        return Err(Error::msg("the Windows IOCTL frame is 480 samples"));
    }
    while !stop.load(Ordering::Relaxed) {
        let Some(device) = Device::open() else {
            thread::sleep(Duration::from_millis(500));
            continue;
        };
        let Ok(mic) = RingAttachment::attach_named(MIC_NAME) else {
            thread::sleep(Duration::from_millis(200));
            continue;
        };
        let Ok(mut mic_reader) = mic.reader() else {
            thread::sleep(Duration::from_millis(200));
            continue;
        };
        let Ok(speaker) = RingAttachment::attach_named(SPEAKER_NAME) else {
            thread::sleep(Duration::from_millis(200));
            continue;
        };
        let mut frame = [0.0f32; FRAME_SAMPLES];
        while !stop.load(Ordering::Relaxed) {
            let missed = mic_reader.read_next(&mut frame);
            if missed == 0 && !device.write_capture(&frame) {
                break;
            }
            match device.read_render() {
                Some(samples) if !samples.is_empty() => speaker.write(&samples),
                Some(_) => thread::sleep(Duration::from_millis(2)),
                None => break,
            }
        }
    }
    Ok(())
}

/// Endpoints, the control device, and a tone in each direction.
///
/// Quit the tray app first. This takes the bridge mutex so the service does
/// not steal the samples under test.
pub fn windows_driver_selftest() -> Result<String> {
    let _guard = acquire_bridge().ok_or_else(|| {
        Error::msg(
            "another HUSH process owns the bridge. Quit HUSH from the tray, then run selftest again.",
        )
    })?;
    let lists = devices::list_devices()?;
    let mic = lists.inputs.iter().find(|name| is_hush_microphone(name)).cloned();
    let speaker = lists.outputs.iter().find(|name| is_hush_speaker(name)).cloned();
    let (Some(mic_name), Some(speaker_name)) = (mic, speaker) else {
        return Err(Error::msg(format!(
            "Windows is not listing both HUSH endpoints (microphone: {}, speaker: {}). Install the test-signed driver and reboot after `bcdedit /set testsigning on`. See docs/WINDOWS_TEST.md.",
            lists.inputs.iter().any(|name| is_hush_microphone(name)),
            lists.outputs.iter().any(|name| is_hush_speaker(name))
        )));
    };
    let device = Device::open().ok_or_else(|| {
        Error::msg(
            "\\\\.\\HushVad did not open. The driver package is installed only when test signing is on and the device node exists.",
        )
    })?;
    let capture = capture_rms(&mic_name, &device)?;
    if capture < 0.02 {
        return Err(Error::msg(format!(
            "capture path was silent (rms {capture:.4}). WRITE_CAPTURE did not reach HUSH Microphone."
        )));
    }
    let render = render_rms(&speaker_name, &device)?;
    if render < 0.02 {
        return Err(Error::msg(format!(
            "render path was silent (rms {render:.4}). HUSH Speaker did not reach READ_RENDER."
        )));
    }
    Ok(format!(
        "devices: Windows lists {mic_name} and {speaker_name}\ndriver: opened \\\\.\\HushVad\ncapture: tone written through the driver came back from HUSH Microphone (rms {capture:.3})\nrender: tone played to HUSH Speaker came back on the render IOCTL (rms {render:.3})"
    ))
}

fn sine(index: usize) -> f32 {
    (index as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.45
}

fn capture_rms(mic_name: &str, device: &Device) -> Result<f32> {
    let host = devices::host();
    let mic = devices::find_input(&host, Some(mic_name))?;
    let supported = mic.default_input_config()?;
    let (tx, rx) = mpsc::channel::<Vec<f32>>();
    let stream = input_stream(&mic, &supported.config(), supported.sample_format(), supported.channels() as usize, tx)?;
    stream.play()?;
    for frame in 0..40u32 {
        let mut samples = [0.0f32; FRAME_SAMPLES];
        for (i, sample) in samples.iter_mut().enumerate() {
            *sample = sine(frame as usize * FRAME_SAMPLES + i);
        }
        if !device.write_capture(&samples) {
            return Err(Error::msg("WRITE_CAPTURE failed"));
        }
        thread::sleep(Duration::from_millis(10));
    }
    thread::sleep(Duration::from_millis(150));
    drop(stream);
    Ok(rms_from(rx))
}

fn render_rms(speaker_name: &str, device: &Device) -> Result<f32> {
    let host = devices::host();
    let speaker = devices::find_output(&host, Some(speaker_name))?;
    let supported = speaker.default_output_config()?;
    let phase = AtomicU32::new(0);
    let stream = output_stream(
        &speaker,
        &supported.config(),
        supported.sample_format(),
        supported.channels() as usize,
        phase,
    )?;
    stream.play()?;
    let mut sum = 0.0f64;
    let mut count = 0.0f64;
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(700) {
        match device.read_render() {
            Some(samples) => {
                for sample in samples {
                    sum += f64::from(sample) * f64::from(sample);
                    count += 1.0;
                }
            }
            None => return Err(Error::msg("READ_RENDER failed")),
        }
        thread::sleep(Duration::from_millis(10));
    }
    drop(stream);
    if count == 0.0 {
        return Ok(0.0);
    }
    Ok((sum / count).sqrt() as f32)
}

fn rms_from(rx: mpsc::Receiver<Vec<f32>>) -> f32 {
    let mut sum = 0.0f64;
    let mut count = 0.0f64;
    while let Ok(buffer) = rx.try_recv() {
        for sample in buffer {
            sum += f64::from(sample) * f64::from(sample);
            count += 1.0;
        }
    }
    if count == 0.0 {
        0.0
    } else {
        (sum / count).sqrt() as f32
    }
}

fn input_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    format: SampleFormat,
    channels: usize,
    tx: mpsc::Sender<Vec<f32>>,
) -> Result<Stream> {
    let stream = match format {
        SampleFormat::F32 => input_of::<f32>(device, config, channels, tx)?,
        SampleFormat::I16 => input_of::<i16>(device, config, channels, tx)?,
        SampleFormat::I32 => input_of::<i32>(device, config, channels, tx)?,
        SampleFormat::U16 => input_of::<u16>(device, config, channels, tx)?,
        other => return Err(Error::msg(format!("unsupported capture format {other}"))),
    };
    Ok(stream)
}

fn input_of<T>(device: &cpal::Device, config: &StreamConfig, channels: usize, tx: mpsc::Sender<Vec<f32>>) -> Result<Stream>
where
    T: cpal::SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                if channels == 0 {
                    return;
                }
                let mut mono = Vec::with_capacity(data.len() / channels);
                for frame in data.chunks(channels) {
                    let acc: f32 = frame.iter().copied().map(f32::from_sample).sum();
                    mono.push(acc / channels as f32);
                }
                let _ = tx.send(mono);
            },
            |err| eprintln!("hush selftest input: {err}"),
            None,
        )
        .map_err(Error::from)
}

fn output_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    format: SampleFormat,
    channels: usize,
    phase: AtomicU32,
) -> Result<Stream> {
    match format {
        SampleFormat::F32 => output_of::<f32>(device, config, channels, phase),
        SampleFormat::I16 => output_of::<i16>(device, config, channels, phase),
        SampleFormat::I32 => output_of::<i32>(device, config, channels, phase),
        SampleFormat::U16 => output_of::<u16>(device, config, channels, phase),
        other => Err(Error::msg(format!("unsupported render format {other}"))),
    }
}

fn output_of<T>(device: &cpal::Device, config: &StreamConfig, channels: usize, phase: AtomicU32) -> Result<Stream>
where
    T: cpal::SizedSample + Send + 'static,
    T: FromSample<f32>,
{
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                if channels == 0 {
                    return;
                }
                let frames = data.len() / channels;
                for frame in 0..frames {
                    let index = phase.fetch_add(1, Ordering::Relaxed);
                    let sample = T::from_sample(sine(index as usize));
                    for channel in 0..channels {
                        data[frame * channels + channel] = sample;
                    }
                }
            },
            |err| eprintln!("hush selftest output: {err}"),
            None,
        )
        .map_err(Error::from)
}
