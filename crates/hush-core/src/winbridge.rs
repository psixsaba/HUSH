//! User-session bridge between the shared-memory rings and the virtual driver.
//!
//! The kernel miniport cannot open `Local\` mappings created by the logged-in
//! user. This loop runs in that session: it copies HUSH Microphone into
//! `HUSH_IOCTL_WRITE_CAPTURE` and copies `HUSH_IOCTL_READ_RENDER` into HUSH
//! Speaker. The numbers match `drivers/windows/ioctl.h`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::constants::FRAME_SAMPLES;
use crate::error::{Error, Result};
use crate::ring::{RingAttachment, MIC_NAME, SPEAKER_NAME};

const WRITE_CAPTURE: u32 = 0x8000_2000;
const READ_RENDER: u32 = 0x8000_2004;
const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const FILE_SHARE_READ_WRITE: u32 = 0x0000_0003;
const OPEN_EXISTING: u32 = 3;
const INVALID_HANDLE: isize = -1;

#[repr(C)]
struct HushIoctlAudio {
    sample_rate: u32,
    channels: u32,
    frames: u32,
    samples: [f32; FRAME_SAMPLES],
}

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
}

struct Device(isize);

impl Device {
    fn open() -> Option<Self> {
        let name: Vec<u16> = "\\\\.\\HushVad".encode_utf16().chain(std::iter::once(0)).collect();
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
        packet.samples[..samples.len().min(FRAME_SAMPLES)].copy_from_slice(&samples[..samples.len().min(FRAME_SAMPLES)]);
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                self.0,
                WRITE_CAPTURE,
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
                READ_RENDER,
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
