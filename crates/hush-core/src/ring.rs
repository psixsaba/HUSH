use std::ffi::c_char;
use std::marker::PhantomData;

use crate::error::{Error, Result};

/// Mono float ring published for the virtual microphone.
///
/// The Core Audio plugin maps the same POSIX segment. Layout and the read
/// cursor live in `native/ring` so the plugin and this process share one
/// implementation.
pub struct RingPublisher {
    raw: *mut HushRing,
}

/// A second mapping of an existing segment. Used by tests and by anything
/// that wants to observe the virtual mic the way the plugin does.
pub struct RingAttachment {
    raw: *mut HushRing,
}

pub struct RingReader<'a> {
    raw: *mut HushRingReader,
    _life: PhantomData<&'a HushRing>,
}

#[repr(C)]
struct HushRing {
    _private: [u8; 0],
}

#[repr(C)]
struct HushRingReader {
    _private: [u8; 0],
}

unsafe impl Send for RingPublisher {}

pub const MIC_NAME: &str = "/hush-mic";
pub const SPEAKER_NAME: &str = "/hush-spk";

impl RingPublisher {
    pub fn create() -> Result<Self> {
        Self::create_named(MIC_NAME)
    }

    pub fn create_named(name: &str) -> Result<Self> {
        let c_name = std::ffi::CString::new(name).map_err(|_| Error::msg("invalid shared-memory name"))?;
        let raw = unsafe { hush_ring_create_named(c_name.as_ptr()) };
        if raw.is_null() {
            return Err(Error::msg(format!(
                "failed to create the shared memory segment {name}: {}",
                last_error()
            )));
        }
        Ok(Self { raw })
    }

    pub fn set_active(&self, active: bool) {
        unsafe { hush_ring_set_active(self.raw, if active { 1 } else { 0 }) }
    }

    pub fn set_muted(&self, muted: bool) {
        unsafe { hush_ring_set_muted(self.raw, if muted { 1 } else { 0 }) }
    }

    pub fn set_latency_samples(&self, samples: u32) {
        unsafe { hush_ring_set_latency(self.raw, samples) }
    }

    pub fn write(&self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
        unsafe { hush_ring_write(self.raw, samples.as_ptr(), samples.len() as u32) };
    }

    pub fn underruns(&self) -> u64 {
        unsafe { hush_ring_underruns(self.raw) }
    }

    pub fn resyncs(&self) -> u64 {
        unsafe { hush_ring_resyncs(self.raw) }
    }

    /// Drop the name. An existing mapping, including the plugin's, stays valid.
    pub fn unlink(name: &str) -> Result<()> {
        let c_name = std::ffi::CString::new(name).map_err(|_| Error::msg("invalid shared-memory name"))?;
        let rc = unsafe { hush_ring_unlink(c_name.as_ptr()) };
        if rc != 0 {
            return Err(Error::msg(format!(
                "failed to unlink the shared memory segment {name}: {}",
                last_error()
            )));
        }
        Ok(())
    }

    pub fn shm_name() -> &'static str {
        let ptr = unsafe { hush_ring_shm_name() };
        if ptr.is_null() {
            return "";
        }
        unsafe { std::ffi::CStr::from_ptr(ptr) }
            .to_str()
            .unwrap_or("")
    }
}

impl Drop for RingPublisher {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                hush_ring_set_active(self.raw, 0);
                hush_ring_destroy(self.raw);
            }
            self.raw = std::ptr::null_mut();
        }
    }
}

impl RingAttachment {
    pub fn attach() -> Result<Self> {
        Self::attach_named(MIC_NAME)
    }

    pub fn attach_named(name: &str) -> Result<Self> {
        let c_name = std::ffi::CString::new(name).map_err(|_| Error::msg("invalid shared-memory name"))?;
        let raw = unsafe { hush_ring_attach_named(c_name.as_ptr()) };
        if raw.is_null() {
            return Err(Error::msg(format!("shared memory segment {name} is not published yet")));
        }
        Ok(Self { raw })
    }

    pub fn reader(&self) -> Result<RingReader<'_>> {
        let raw = unsafe { hush_ring_reader_new(self.raw) };
        if raw.is_null() {
            return Err(Error::msg("failed to allocate the ring reader"));
        }
        Ok(RingReader {
            raw,
            _life: PhantomData,
        })
    }

    pub fn underruns(&self) -> u64 {
        unsafe { hush_ring_underruns(self.raw) }
    }

    pub fn resyncs(&self) -> u64 {
        unsafe { hush_ring_resyncs(self.raw) }
    }

    /// Write samples into a segment this process did not create.
    /// The macOS plugin does the same for HUSH Speaker.
    pub fn write(&self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
        unsafe { hush_ring_write(self.raw, samples.as_ptr(), samples.len() as u32) };
    }
}

impl Drop for RingAttachment {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe { hush_ring_destroy(self.raw) }
            self.raw = std::ptr::null_mut();
        }
    }
}

impl RingReader<'_> {
    /// Returns the number of samples that were silence because the publisher
    /// had not produced them yet.
    pub fn read(&mut self, timestamp_frames: f64, dst: &mut [f32]) -> u32 {
        if dst.is_empty() {
            return 0;
        }
        unsafe { hush_ring_read(self.raw, timestamp_frames, dst.as_mut_ptr(), dst.len() as u32) }
    }

    pub fn reset(&mut self) {
        unsafe { hush_ring_reader_reset(self.raw) }
    }

    /// Pull the next samples, for the virtual-speaker bridge.
    pub fn read_next(&mut self, dst: &mut [f32]) -> u32 {
        if dst.is_empty() {
            return 0;
        }
        unsafe { hush_ring_read_next(self.raw, dst.as_mut_ptr(), dst.len() as u32) }
    }
}

impl Drop for RingReader<'_> {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe { hush_ring_reader_free(self.raw) }
            self.raw = std::ptr::null_mut();
        }
    }
}

fn last_error() -> String {
    let ptr = unsafe { hush_ring_last_error() };
    if ptr.is_null() {
        return "unknown error".into();
    }
    unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

extern "C" {
    fn hush_ring_create_named(name: *const c_char) -> *mut HushRing;
    fn hush_ring_attach_named(name: *const c_char) -> *mut HushRing;
    fn hush_ring_destroy(ring: *mut HushRing);
    fn hush_ring_unlink(name: *const c_char) -> i32;
    fn hush_ring_last_error() -> *const c_char;
    fn hush_ring_set_active(ring: *mut HushRing, active: i32);
    fn hush_ring_set_muted(ring: *mut HushRing, muted: i32);
    fn hush_ring_set_latency(ring: *mut HushRing, samples: u32);
    fn hush_ring_write(ring: *mut HushRing, src: *const f32, n: u32) -> u32;
    fn hush_ring_underruns(ring: *const HushRing) -> u64;
    fn hush_ring_resyncs(ring: *const HushRing) -> u64;
    fn hush_ring_shm_name() -> *const c_char;
    fn hush_ring_reader_new(ring: *mut HushRing) -> *mut HushRingReader;
    fn hush_ring_reader_free(reader: *mut HushRingReader);
    fn hush_ring_reader_reset(reader: *mut HushRingReader);
    fn hush_ring_read(
        reader: *mut HushRingReader,
        timestamp_frames: f64,
        dst: *mut f32,
        n: u32,
    ) -> u32;
    fn hush_ring_read_next(reader: *mut HushRingReader, dst: *mut f32, n: u32) -> u32;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static NAMES: AtomicU32 = AtomicU32::new(0);

    struct TestSegment {
        name: String,
        publisher: RingPublisher,
    }

    impl TestSegment {
        fn create() -> Self {
            let n = NAMES.fetch_add(1, Ordering::Relaxed);
            let name = format!("/h{:x}{:x}", std::process::id(), n);
            assert!(name.len() <= 31, "macOS shm names are at most 31 characters");
            let publisher = RingPublisher::create_named(&name).unwrap();
            Self { name, publisher }
        }
    }

    impl Drop for TestSegment {
        fn drop(&mut self) {
            let _ = RingPublisher::unlink(&self.name);
        }
    }

    #[test]
    fn broadcast_read_underrun_and_resync() {
        let segment = TestSegment::create();
        let publisher = &segment.publisher;
        assert_eq!(RingPublisher::shm_name(), "/hush-mic");
        publisher.set_latency_samples(1440);

        let attachment = RingAttachment::attach_named(&segment.name).unwrap();
        let mut reader = attachment.reader().unwrap();

        let mut signal = vec![0.0f32; 2000];
        for (i, sample) in signal.iter_mut().enumerate() {
            *sample = i as f32 * 0.001;
        }
        publisher.write(&signal);

        let mut dst = vec![0.0f32; 100];
        let missed = reader.read(0.0, &mut dst);
        assert_eq!(missed, 0, "the jitter buffer should sit inside the written region");
        // 20 ms at 48 kHz is 960 samples. write_count is 2000, so the anchor
        // starts at sample 1040.
        assert!(
            (dst[0] - 1.040).abs() < 1e-5,
            "first anchored sample was {}",
            dst[0]
        );
        assert!((dst[99] - 1.139).abs() < 1e-5);

        let mut future = vec![0.0f32; 50];
        let missed = reader.read(10_000.0, &mut future);
        assert_eq!(missed, 50);
        assert!(future.iter().all(|s| *s == 0.0));
        assert!(attachment.underruns() >= 50);

        let mut more = vec![0.0f32; 40_000];
        for (i, sample) in more.iter_mut().enumerate() {
            *sample = 10.0 + i as f32;
        }
        publisher.write(&more);
        reader.reset();
        let mut head = vec![0.0f32; 4];
        let missed = reader.read(0.0, &mut head);
        assert_eq!(missed, 0);
        // After reset, jitter is min(960, write_count). write_count is 42000,
        // so the anchor starts 960 samples behind the end.
        let expected = 10.0 + (40_000 - 960) as f32;
        assert!(
            (head[0] - expected).abs() < 1e-3,
            "resync head {} expected {expected}",
            head[0]
        );

        publisher.set_active(false);
        let mut silent = vec![1.0f32; 8];
        let missed = reader.read(100.0, &mut silent);
        assert_eq!(missed, 8);
        assert!(silent.iter().all(|s| *s == 0.0));
        assert!(attachment.resyncs() == 0 || publisher.resyncs() == attachment.resyncs());
    }

    #[test]
    fn create_reuses_an_existing_segment_without_unlinking() {
        let n = NAMES.fetch_add(1, Ordering::Relaxed);
        let name = format!("/h{:x}{:x}", std::process::id(), n);
        let first = RingPublisher::create_named(&name).unwrap();
        first.write(&[0.5, -0.5, 0.25]);
        drop(first);
        let again = RingPublisher::create_named(&name).unwrap();
        again.write(&[1.0]);
        let attachment = RingAttachment::attach_named(&name).unwrap();
        assert!(attachment.reader().is_ok());
        drop(again);
        drop(attachment);
        RingPublisher::unlink(&name).unwrap();
    }

    #[test]
    fn long_shm_name_is_rejected() {
        let name = format!("/{}", "a".repeat(40));
        match RingPublisher::create_named(&name) {
            Err(err) => {
                let text = err.to_string();
                assert!(
                    text.contains("31") && text.contains(&name),
                    "error was {text}"
                );
            }
            Ok(_) => panic!("a shm name longer than 31 characters must be rejected"),
        }
    }
}
