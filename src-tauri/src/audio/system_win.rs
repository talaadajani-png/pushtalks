//! System audio on Windows: WASAPI loopback on the default output device. cpal opens an output
//! device as a loopback capture when it's used as an input.

use anyhow::{anyhow, Result};
use cpal::traits::HostTrait;

use super::capture::Capture;
use super::mic::open_stream;

pub fn start() -> Result<Capture> {
    Capture::spawn("system", |tx| {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| anyhow!("no audio output device found"))?;
        let (stream, rate) = open_stream(&device, tx)?;
        Ok((stream, rate, "System audio".to_string()))
    })
}
