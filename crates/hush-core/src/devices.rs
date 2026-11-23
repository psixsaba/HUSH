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
    let inputs = named(host.input_devices().map_err(|err| Error::msg(err.to_string()))?)?;
    let outputs = named(host.output_devices().map_err(|err| Error::msg(err.to_string()))?)?;
    Ok(DeviceLists { inputs, outputs })
}

fn named(devices: impl Iterator<Item = cpal::Device>) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for device in devices {
        names.push(device.name().map_err(|err| Error::msg(err.to_string()))?);
    }
    Ok(names)
}

pub fn find_input(host: &Host, name: Option<&str>) -> Result<cpal::Device> {
    find(host.input_devices().map_err(|err| Error::msg(err.to_string()))?, host.default_input_device(), name, "input")
}

pub fn find_output(host: &Host, name: Option<&str>) -> Result<cpal::Device> {
    find(
        host.output_devices().map_err(|err| Error::msg(err.to_string()))?,
        host.default_output_device(),
        name,
        "output",
    )
}

fn find(
    devices: impl Iterator<Item = cpal::Device>,
    default_device: Option<cpal::Device>,
    name: Option<&str>,
    kind: &str,
) -> Result<cpal::Device> {
    if let Some(name) = name {
        for device in devices {
            let device_name = device.name().map_err(|err| Error::msg(err.to_string()))?;
            if device_name == name {
                return Ok(device);
            }
        }
        return Err(Error::msg(format!("no {kind} device named '{name}'")));
    }
    default_device.ok_or_else(|| Error::msg(format!("no default {kind} device")))
}
