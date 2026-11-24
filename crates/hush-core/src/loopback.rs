//! WASAPI loopback of a playback device. Opening a render endpoint and then
//! capturing it is how IAudioClient sets AUDCLNT_STREAMFLAGS_LOOPBACK.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rtrb::Producer;
use wasapi::{
    DeviceCollection, Direction, SampleType, StreamMode, WaveFormat, initialize_mta,
};

use crate::error::{Error, Result};
use crate::vbcable::is_cable_render;

pub fn spawn(
    device_name: String,
    mut producer: Producer<f32>,
    running: Arc<AtomicBool>,
) -> Result<JoinHandle<()>> {
    let (ready_tx, ready_rx) = mpsc::channel();
    let handle = thread::Builder::new()
        .name("hush-loopback".into())
        .spawn(move || match open_loopback(&device_name) {
            Err(err) => {
                let _ = ready_tx.send(Err(err));
            }
            Ok((mut client, capture, channels, block)) => {
                let _ = ready_tx.send(Ok(()));
                read_loop(&capture, channels, block, &mut producer, &running);
                let _ = client.stop_stream();
            }
        })
        .map_err(|err| Error::msg(err.to_string()))?;
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(handle),
        Ok(Err(err)) => {
            let _ = handle.join();
            Err(err)
        }
        Err(_) => {
            let _ = handle.join();
            Err(Error::msg("the speaker loopback stopped before it started"))
        }
    }
}

fn open_loopback(
    device_name: &str,
) -> Result<(wasapi::AudioClient, wasapi::AudioCaptureClient, usize, usize)> {
    initialize_mta().map_err(|err| Error::msg(format!("COM init failed: {err}")))?;
    let collection = DeviceCollection::new(&Direction::Render)
        .map_err(|err| Error::msg(format!("playback devices: {err}")))?;
    let mut found = None;
    let count = collection
        .get_nbr_devices()
        .map_err(|err| Error::msg(err.to_string()))?;
    for index in 0..count {
        let device = collection
            .get_device_at_index(index)
            .map_err(|err| Error::msg(err.to_string()))?;
        let name = device
            .get_friendlyname()
            .map_err(|err| Error::msg(err.to_string()))?;
        if name == device_name || (device_name.is_empty() && !is_cable_render(&name)) {
            found = Some(device);
            break;
        }
    }
    let device = found.ok_or_else(|| {
        Error::msg(format!("no playback device named '{device_name}' for loopback"))
    })?;
    let mut client = device
        .get_iaudioclient()
        .map_err(|err| Error::msg(format!("audio client: {err}")))?;
    let format = WaveFormat::new(32, 32, &SampleType::Float, 48_000, 2, None);
    let mode = StreamMode::PollingShared {
        autoconvert: true,
        buffer_duration_hns: 200_000,
    };
    // The client came from a render endpoint. Capture on that client is loopback.
    client
        .initialize_client(&format, &Direction::Capture, &mode)
        .map_err(|err| Error::msg(format!("loopback init: {err}")))?;
    let capture = client
        .get_audiocaptureclient()
        .map_err(|err| Error::msg(format!("capture client: {err}")))?;
    client
        .start_stream()
        .map_err(|err| Error::msg(format!("loopback start: {err}")))?;
    let channels = format.get_nchannels() as usize;
    let block = format.get_blockalign() as usize;
    if channels == 0 || block < 4 {
        return Err(Error::msg("the loopback format has no samples"));
    }
    Ok((client, capture, channels, block))
}

fn read_loop(
    capture: &wasapi::AudioCaptureClient,
    channels: usize,
    block: usize,
    producer: &mut Producer<f32>,
    running: &AtomicBool,
) {
    while running.load(Ordering::Acquire) {
        let packet = match capture.get_next_packet_size() {
            Ok(Some(size)) => size,
            Ok(None) => 0,
            Err(_) => break,
        };
        if packet == 0 {
            thread::sleep(Duration::from_millis(2));
            continue;
        }
        let mut bytes = vec![0u8; packet as usize * block];
        let Ok((frames, _)) = capture.read_from_device(&mut bytes) else {
            break;
        };
        let frames = frames as usize;
        for frame in 0..frames {
            let mut sum = 0.0f32;
            for channel in 0..channels {
                let offset = frame * block + channel * 4;
                if offset + 4 > bytes.len() {
                    break;
                }
                let sample = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                sum += sample;
            }
            let mono = (sum / channels as f32).clamp(-1.0, 1.0);
            if producer.push(mono).is_err() {
                break;
            }
        }
    }
}
