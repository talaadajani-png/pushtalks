//! Microphone capture through cpal.

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat};

use super::capture::{Capture, RawSender};
use super::resample::downmix;

fn name(d: &Device) -> String {
    d.description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "Unknown microphone".into())
}

/// Input device names for Settings.
pub fn list() -> Vec<String> {
    cpal::default_host()
        .input_devices()
        .map(|it| it.map(|d| name(&d)).collect())
        .unwrap_or_default()
}

fn find(preferred: Option<&str>) -> Result<Device> {
    let host = cpal::default_host();
    if let Some(want) = preferred {
        if let Ok(mut it) = host.input_devices() {
            if let Some(d) = it.find(|d| name(d) == want) {
                return Ok(d);
            }
        }
    }
    host.default_input_device()
        .ok_or_else(|| anyhow!("no microphone found"))
}

/// Builds a cpal input stream on `device` that sends mono f32 samples to `tx`.
pub(crate) fn open_stream(device: &Device, tx: RawSender) -> Result<(cpal::Stream, u32)> {
    let supported = device
        .default_input_config()
        .context("reading the audio device's format")?;
    let channels = supported.channels() as usize;
    let rate = supported.sample_rate();
    let format = supported.sample_format();
    let config = supported.config();
    let err = |e| eprintln!("audio stream error: {e}");
    let stream = match format {
        SampleFormat::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _: &_| {
                let _ = tx.send(downmix(data, channels));
            },
            err,
            None,
        )?,
        SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _: &_| {
                let f: Vec<f32> = data.iter().map(|s| *s as f32 / 32768.0).collect();
                let _ = tx.send(downmix(&f, channels));
            },
            err,
            None,
        )?,
        SampleFormat::I32 => device.build_input_stream(
            config,
            move |data: &[i32], _: &_| {
                let f: Vec<f32> = data.iter().map(|s| *s as f32 / 2_147_483_648.0).collect();
                let _ = tx.send(downmix(&f, channels));
            },
            err,
            None,
        )?,
        SampleFormat::U16 => device.build_input_stream(
            config,
            move |data: &[u16], _: &_| {
                let f: Vec<f32> = data
                    .iter()
                    .map(|s| (*s as f32 - 32768.0) / 32768.0)
                    .collect();
                let _ = tx.send(downmix(&f, channels));
            },
            err,
            None,
        )?,
        other => return Err(anyhow!("unsupported audio sample format {other:?}")),
    };
    stream.play()?;
    Ok((stream, rate))
}

/// Starts capturing the chosen (or default) microphone.
pub fn start(preferred: Option<String>) -> Result<Capture> {
    Capture::spawn("mic", move |tx| {
        let device = find(preferred.as_deref())?;
        let label = name(&device);
        let (stream, rate) = open_stream(&device, tx)?;
        Ok((stream, rate, label))
    })
}

/// What a microphone test heard.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Heard {
    /// Loudest sample, 0.0 to 1.0.
    pub peak: f32,
    /// At least ~0.4 s of audio above speaking level.
    pub speech: bool,
    /// Every sample was exactly zero: the OS is blocking the mic (no permission) rather than
    /// the room being quiet, which always has some noise.
    pub blocked: bool,
}

/// RMS level that counts as someone speaking near the mic.
const SPEECH_RMS: f32 = 0.01;
const SPEECH_SECS: f32 = 0.4;

/// Listens on `capture` for up to `max`, reporting the level of each chunk. Returns early,
/// half a second after speech is heard.
pub fn listen_for_speech(
    capture: &Capture,
    max: std::time::Duration,
    mut on_level: impl FnMut(f32),
) -> Heard {
    use std::time::{Duration, Instant};
    let start = Instant::now();
    let mut peak = 0.0f32;
    let mut nonzero = false;
    let mut speech_samples = 0usize;
    let mut heard_at: Option<Instant> = None;
    while start.elapsed() < max {
        if heard_at.is_some_and(|t| t.elapsed() >= Duration::from_millis(500)) {
            break;
        }
        let Ok(chunk) = capture.chunks.recv_timeout(Duration::from_millis(100)) else {
            continue;
        };
        let level = super::capture::rms(&chunk);
        on_level(level);
        for s in &chunk {
            peak = peak.max(s.abs());
            nonzero |= *s != 0.0;
        }
        if level >= SPEECH_RMS {
            speech_samples += chunk.len();
        }
        let secs = speech_samples as f32 / super::resample::SAMPLE_RATE as f32;
        if secs >= SPEECH_SECS && heard_at.is_none() {
            heard_at = Some(Instant::now());
        }
    }
    Heard {
        peak,
        speech: heard_at.is_some(),
        blocked: !nonzero,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manual check that the default mic delivers sound:
    /// `cargo test --lib mic_records -- --ignored --nocapture` and talk for a few seconds.
    #[test]
    #[ignore]
    fn mic_records() {
        let mut c = start(None).unwrap();
        let mut n = 0;
        let heard = listen_for_speech(&c, std::time::Duration::from_secs(8), |_| n += 1);
        c.stop();
        println!("device {:?}, chunks {n}, {heard:?}", c.device);
    }
}
