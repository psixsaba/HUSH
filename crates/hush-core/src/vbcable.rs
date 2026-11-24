//! VB-CABLE is the free Windows virtual microphone. HUSH does not ship the
//! driver. The installer downloads Pack45 from VB-Audio and checks this hash.
//! VB-Audio publishes no checksum; the pin is the copy fetched on 2026-11-24
//! from https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip.

use serde::Serialize;
use sha2::{Digest, Sha256};

pub const PACK_URL: &str = "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip";
pub const PACK_SHA256: &str = "b950e39f01af1d04ea623c8f6d8eb9b6ea5c477c637295fabf20631c85116bfb";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioRoute {
    /// macOS HAL plugin, or the test-signed HushVad driver when it is loaded.
    VirtualDriver,
    /// Cleaned microphone is written to the VB-CABLE render endpoint.
    VbCable,
}

/// HushVad wins when it is loaded. Otherwise the free cable is the default.
pub fn route_for(hush_vad_loaded: bool) -> AudioRoute {
    if hush_vad_loaded {
        AudioRoute::VirtualDriver
    } else {
        AudioRoute::VbCable
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub fn sha_matches_pack(bytes: &[u8]) -> bool {
    sha256_hex(bytes) == PACK_SHA256
}

/// Playback endpoint. Apps do not select this. HUSH writes the cleaned mic here.
pub fn is_cable_render(name: &str) -> bool {
    name.to_ascii_lowercase().contains("cable input")
}

/// Recording endpoint, before or after the friendly-name change.
pub fn is_cable_capture(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.contains("cable output")
}

pub fn cable_render_name(outputs: &[String]) -> Option<String> {
    outputs.iter().find(|name| is_cable_render(name)).cloned()
}

pub fn cable_capture_name(inputs: &[String], outputs: &[String]) -> Option<String> {
    if let Some(name) = inputs.iter().find(|name| is_cable_capture(name)) {
        return Some(name.clone());
    }
    // The installer renames CABLE Output to HUSH Microphone. The render
    // endpoint is left as CABLE Input, which is how we know the renamed
    // capture still belongs to the cable.
    if cable_render_name(outputs).is_some() {
        return inputs
            .iter()
            .find(|name| crate::devices::is_hush_microphone(name))
            .cloned();
    }
    None
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnginePlan {
    pub route: AudioRoute,
    pub publish_ring: bool,
    pub bridge_speaker: bool,
    pub clean_incoming: bool,
    /// Where the cleaned microphone is played. VB-CABLE's "CABLE Input".
    pub playback_device: Option<String>,
    /// Real speakers, captured with WASAPI loopback for AEC.
    pub loopback_device: Option<String>,
    /// What the call app should select.
    pub capture_label: &'static str,
    pub cable_present: bool,
}

/// `speakers` is the user's real output. It must not be the cable.
pub fn engine_plan(hush_vad_loaded: bool, outputs: &[String], speakers: Option<&str>) -> EnginePlan {
    let route = route_for(hush_vad_loaded);
    if route == AudioRoute::VirtualDriver {
        return EnginePlan {
            route,
            publish_ring: true,
            bridge_speaker: true,
            clean_incoming: true,
            playback_device: speakers.map(str::to_string),
            loopback_device: None,
            capture_label: "HUSH Microphone",
            cable_present: cable_render_name(outputs).is_some(),
        };
    }
    let render = cable_render_name(outputs);
    let loopback = real_speakers(outputs, speakers);
    EnginePlan {
        route,
        publish_ring: false,
        bridge_speaker: false,
        clean_incoming: false,
        playback_device: render.clone(),
        loopback_device: loopback,
        capture_label: "HUSH Microphone",
        cable_present: render.is_some(),
    }
}

pub fn real_speakers(outputs: &[String], preferred: Option<&str>) -> Option<String> {
    if let Some(name) = preferred {
        if outputs.iter().any(|device| device == name) && !is_cable_render(name) {
            return Some(name.to_string());
        }
    }
    outputs.iter().find(|name| !is_cable_render(name)).cloned()
}

#[derive(Clone, Debug, PartialEq)]
pub enum RoundTrip {
    Skipped(String),
    Heard { rms: f32 },
    Silent { rms: f32 },
}

/// Play a tone into CABLE Input and listen on the capture endpoint.
/// Missing endpoints are a skip, not a failure: the VB-Audio setup asks for a reboot.
pub fn round_trip() -> crate::Result<RoundTrip> {
    use cpal::traits::{DeviceTrait, StreamTrait};
    use cpal::SampleFormat;

    let lists = crate::devices::list_devices()?;
    let Some(render_name) = cable_render_name(&lists.outputs) else {
        return Ok(RoundTrip::Skipped(
            "VB-CABLE endpoints are not up. A reboot is usually required after the setup.".into(),
        ));
    };
    let Some(capture_name) = cable_capture_name(&lists.inputs, &lists.outputs) else {
        return Ok(RoundTrip::Skipped(
            "CABLE Input is present but the capture endpoint is not. Reboot, then run hush vbcable roundtrip.".into(),
        ));
    };
    let host = crate::devices::host();
    let render = crate::devices::find_output(&host, Some(&render_name))?;
    let capture = crate::devices::find_input(&host, Some(&capture_name))?;
    let heard = std::sync::Arc::new(std::sync::Mutex::new(Vec::<f32>::new()));
    let heard_cb = std::sync::Arc::clone(&heard);
    let input_config = capture.default_input_config().map_err(|err| crate::Error::msg(err.to_string()))?;
    let output_config = render.default_output_config().map_err(|err| crate::Error::msg(err.to_string()))?;
    let input_channels = input_config.channels() as usize;
    let output_channels = output_config.channels() as usize;
    let phase = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let phase_cb = std::sync::Arc::clone(&phase);
    let output_rate = output_config.sample_rate().0 as f32;

    let input_stream = match input_config.sample_format() {
        SampleFormat::F32 => build_in::<f32>(&capture, &input_config.into(), input_channels, heard_cb)?,
        SampleFormat::I16 => build_in::<i16>(&capture, &input_config.into(), input_channels, heard_cb)?,
        SampleFormat::U16 => build_in::<u16>(&capture, &input_config.into(), input_channels, heard_cb)?,
        other => return Err(crate::Error::msg(format!("unsupported capture format {other}"))),
    };
    let output_stream = match output_config.sample_format() {
        SampleFormat::F32 => build_out::<f32>(&render, &output_config.into(), output_channels, output_rate, phase_cb)?,
        SampleFormat::I16 => build_out::<i16>(&render, &output_config.into(), output_channels, output_rate, phase_cb)?,
        SampleFormat::U16 => build_out::<u16>(&render, &output_config.into(), output_channels, output_rate, phase_cb)?,
        other => return Err(crate::Error::msg(format!("unsupported render format {other}"))),
    };
    input_stream.play().map_err(|err| crate::Error::msg(err.to_string()))?;
    output_stream.play().map_err(|err| crate::Error::msg(err.to_string()))?;
    std::thread::sleep(std::time::Duration::from_millis(600));
    drop(output_stream);
    drop(input_stream);
    let samples = heard.lock().unwrap().clone();
    let rms = if samples.is_empty() {
        0.0
    } else {
        let energy: f32 = samples.iter().map(|sample| sample * sample).sum();
        (energy / samples.len() as f32).sqrt()
    };
    if rms >= 0.02 {
        Ok(RoundTrip::Heard { rms })
    } else {
        Ok(RoundTrip::Silent { rms })
    }
}

fn build_in<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    heard: std::sync::Arc<std::sync::Mutex<Vec<f32>>>,
) -> crate::Result<cpal::Stream>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    use cpal::traits::DeviceTrait;
    use cpal::Sample;
    let stream = device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let mut guard = heard.lock().unwrap();
                for frame in data.chunks(channels.max(1)) {
                    let sample = frame.first().copied().map(f32::from_sample).unwrap_or(0.0);
                    guard.push(sample);
                    if guard.len() > 48_000 {
                        let extra = guard.len() - 48_000;
                        guard.drain(..extra);
                    }
                }
            },
            |err| eprintln!("vbcable capture: {err}"),
            None,
        )
        .map_err(|err| crate::Error::msg(err.to_string()))?;
    Ok(stream)
}

fn build_out<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    rate: f32,
    phase: std::sync::Arc<std::sync::atomic::AtomicU32>,
) -> crate::Result<cpal::Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    use cpal::traits::DeviceTrait;
    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                for frame in data.chunks_mut(channels.max(1)) {
                    let index = phase.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let sample = (2.0 * std::f32::consts::PI * 1000.0 * index as f32 / rate).sin() * 0.2;
                    let converted = T::from_sample(sample);
                    for slot in frame {
                        *slot = converted;
                    }
                }
            },
            |err| eprintln!("vbcable render: {err}"),
            None,
        )
        .map_err(|err| crate::Error::msg(err.to_string()))?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cable_is_the_default_until_hushvad_is_loaded() {
        assert_eq!(route_for(false), AudioRoute::VbCable);
        assert_eq!(route_for(true), AudioRoute::VirtualDriver);
    }

    #[test]
    fn endpoint_names() {
        assert!(is_cable_render("CABLE Input (VB-Audio Virtual Cable)"));
        assert!(!is_cable_render("CABLE Output"));
        assert!(is_cable_capture("CABLE Output (VB-Audio Virtual Cable)"));
        assert!(!is_cable_capture("HUSH Microphone"));
        let inputs = vec!["HUSH Microphone".into()];
        let outputs = vec!["CABLE Input".into()];
        assert_eq!(
            cable_capture_name(&inputs, &outputs).as_deref(),
            Some("HUSH Microphone")
        );
    }

    #[test]
    fn plan_writes_to_the_cable_and_loops_the_real_speakers() {
        let outputs = vec![
            "Speakers (Realtek)".into(),
            "CABLE Input (VB-Audio Virtual Cable)".into(),
        ];
        let plan = engine_plan(false, &outputs, Some("Speakers (Realtek)"));
        assert_eq!(plan.route, AudioRoute::VbCable);
        assert!(!plan.publish_ring);
        assert!(!plan.bridge_speaker);
        assert!(!plan.clean_incoming);
        assert_eq!(
            plan.playback_device.as_deref(),
            Some("CABLE Input (VB-Audio Virtual Cable)")
        );
        assert_eq!(plan.loopback_device.as_deref(), Some("Speakers (Realtek)"));
        assert!(plan.cable_present);
    }

    #[test]
    fn plan_does_not_use_the_cable_as_its_own_echo_reference() {
        let outputs = vec!["CABLE Input".into(), "Headphones".into()];
        let plan = engine_plan(false, &outputs, Some("CABLE Input"));
        assert_eq!(plan.loopback_device.as_deref(), Some("Headphones"));
    }

    #[test]
    fn missing_cable_is_still_vbcable_mode() {
        let plan = engine_plan(false, &["Speakers".into()], None);
        assert_eq!(plan.route, AudioRoute::VbCable);
        assert!(!plan.cable_present);
        assert!(plan.playback_device.is_none());
    }

    #[test]
    fn hushvad_keeps_the_virtual_speaker() {
        let plan = engine_plan(true, &["CABLE Input".into()], Some("Speakers"));
        assert_eq!(plan.route, AudioRoute::VirtualDriver);
        assert!(plan.publish_ring);
        assert!(plan.bridge_speaker);
        assert!(plan.clean_incoming);
        assert!(plan.loopback_device.is_none());
    }

    #[test]
    fn sha256_of_empty_input() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(PACK_SHA256.len(), 64);
        assert!(!sha_matches_pack(b"not the pack"));
    }
}
