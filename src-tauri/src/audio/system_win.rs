//! System audio on Windows: WASAPI loopback on the default output device. cpal opens an output
//! device as a loopback capture when it's used as an input.

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};

use super::capture::Capture;
use super::mic::open_stream;

pub fn start() -> Result<Capture> {
    Capture::spawn("system", |tx| {
        let device = speakers()?;
        let label = device
            .description()
            .map(|d| format!("System audio ({})", d.name()))
            .unwrap_or_else(|_| "System audio".to_string());
        // An output device has no input format; loopback records in its playback (mix) format.
        let format = device
            .default_output_config()
            .context("reading the speakers' audio format")?;
        let (stream, rate) = open_stream(&device, format, tx)?;
        Ok((stream, rate, label))
    })
}

/// The current default output as a concrete endpoint. cpal's own default device is a virtual
/// "follow the default" endpoint (ActivateAudioInterfaceAsync), which recorded silence in
/// loopback on a PUSH Windows laptop; loopback on the real endpoint is the documented path.
fn speakers() -> Result<cpal::Device> {
    let host = cpal::default_host();
    let default = host
        .default_output_device()
        .ok_or_else(|| anyhow!("no audio output device found"))?;
    if let Ok(id) = default.id() {
        if let Ok(devices) = host.output_devices() {
            for d in devices {
                if d.id().is_ok_and(|x| x == id) {
                    return Ok(d);
                }
            }
        }
    }
    Ok(default)
}
