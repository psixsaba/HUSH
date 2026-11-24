//! IOCTL codes shared with `drivers/windows/ioctl.h`.
//!
//! `CTL_CODE(0x8000, function, METHOD_BUFFERED, FILE_ANY_ACCESS)`.
//! The Windows bridge is the production caller. The unit test checks the
//! numbers on every target, so a non-Windows library build has no other use.
#![allow(dead_code)]

pub const WRITE_CAPTURE: u32 = ctl(0x8000, 0x800);
pub const READ_RENDER: u32 = ctl(0x8000, 0x801);
pub const AUDIO_BYTES: usize = 12 + 480 * 4;

const fn ctl(device_type: u32, function: u32) -> u32 {
    (device_type << 16) | (function << 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_the_header() {
        assert_eq!(WRITE_CAPTURE, 0x8000_2000);
        assert_eq!(READ_RENDER, 0x8000_2004);
        assert_eq!(AUDIO_BYTES, 1932);
    }
}
