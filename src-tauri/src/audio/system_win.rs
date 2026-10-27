//! System audio on Windows: WASAPI loopback on the default output device. cpal opens an output
//! device as a loopback capture when it's used as an input.

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};

use super::capture::Capture;
use super::mic::open_stream;

pub fn start() -> Result<Capture> {
    Capture::spawn("system", |tx| {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| anyhow!("no audio output device found"))?;
        // An output device has no input format; loopback records in its playback (mix) format.
        let format = device
            .default_output_config()
            .context("reading the speakers' audio format")?;
        let (stream, rate) = open_stream(&device, format, tx)?;
        Ok((stream, rate, "System audio".to_string()))
    })
}
