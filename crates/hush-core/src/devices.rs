use cpal::traits::{DeviceTrait, HostTrait};
use cpal::Host;
use serde::Serialize;

use crate::error::{Error, Result};

#[derive(Clone, Debug, Serialize)]
pub struct DeviceLists {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

pub fn host() -> Host {
    cpal::default_host()
}

pub fn list_devices() -> Result<DeviceLists> {
    let host = host();
    let mut inputs = named(host.input_devices().map_err(|err| Error::msg(err.to_string()))?)?;
    let mut outputs = named(host.output_devices().map_err(|err| Error::msg(err.to_string()))?)?;
    // cpal 0.15 dropped non-default outputs (HUSH Speaker on a Mac whose
    // default output is the built-in speakers). 0.16 fixes that probe.
    // Merge host.devices() as well, so a virtual endpoint that Core Audio
    // or WASAPI can see is still listed if one of the filtered iterators
    // misses it.
    if let Ok(all) = host.devices() {
        for device in all {
            let Ok(name) = device.name() else {
                continue;
            };
            if device.supports_input() {
                push_unique(&mut inputs, name.clone());
            }
            if device.supports_output() {
                push_unique(&mut outputs, name);
            }
        }
    }
    Ok(DeviceLists { inputs, outputs })
}

fn named(devices: impl Iterator<Item = cpal::Device>) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for device in devices {
        names.push(device.name().map_err(|err| Error::msg(err.to_string()))?);
    }
    Ok(names)
}

fn push_unique(names: &mut Vec<String>, name: String) {
    if !names.iter().any(|existing| existing == &name) {
        names.push(name);
    }
}

pub fn find_input(host: &Host, name: Option<&str>) -> Result<cpal::Device> {
    find_kind(host, name, "input", true)
}

pub fn find_output(host: &Host, name: Option<&str>) -> Result<cpal::Device> {
    find_kind(host, name, "output", false)
}

fn find_kind(host: &Host, name: Option<&str>, kind: &str, input: bool) -> Result<cpal::Device> {
    if let Some(name) = name {
        let filtered = if input {
            scan(host.input_devices().map_err(|err| Error::msg(err.to_string()))?, name)?
        } else {
            scan(host.output_devices().map_err(|err| Error::msg(err.to_string()))?, name)?
        };
        if let Some(device) = filtered {
            return Ok(device);
        }
        if let Ok(all) = host.devices() {
            for device in all {
                let matches = if input {
                    device.supports_input()
                } else {
                    device.supports_output()
                };
                if !matches {
                    continue;
                }
                let device_name = device.name().map_err(|err| Error::msg(err.to_string()))?;
                if device_name == name {
                    return Ok(device);
                }
            }
        }
        return Err(Error::msg(format!("no {kind} device named '{name}'")));
    }
    let default = if input {
        host.default_input_device()
    } else {
        host.default_output_device()
    };
    default.ok_or_else(|| Error::msg(format!("no default {kind} device")))
}

fn scan(devices: impl Iterator<Item = cpal::Device>, name: &str) -> Result<Option<cpal::Device>> {
    for device in devices {
        let device_name = device.name().map_err(|err| Error::msg(err.to_string()))?;
        if device_name == name {
            return Ok(Some(device));
        }
    }
    Ok(None)
}

/// True when this endpoint is one of ours. WASAPI sometimes appends the
/// adapter name, so the match is on the words, not the whole string.
/// Windows self-test is the caller. The helpers stay available on every
/// target so the name rules can be tested without a driver.
pub fn is_hush_microphone(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.contains("hush") && name.contains("microphone") && !name.contains("speaker")
}

pub fn is_hush_speaker(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.contains("hush") && name.contains("speaker")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_endpoint_names() {
        assert!(is_hush_microphone("HUSH Microphone"));
        assert!(is_hush_microphone("Microphone (HUSH Microphone)"));
        assert!(!is_hush_microphone("HUSH Speaker"));
        assert!(is_hush_speaker("HUSH Speaker"));
        assert!(!is_hush_speaker("MacBook Air Speakers"));
    }
}
