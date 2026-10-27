//! Loads audio files and converts them to the 16 kHz mono f32 the models expect.

use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use sherpa_onnx::LinearResampler;

pub const SAMPLE_RATE: u32 = 16_000;

/// Reads a WAV file (any sample rate, mono or multi-channel, int or float) as 16 kHz mono.
pub fn read_wav_16k_mono(path: &Path) -> Result<Vec<f32>> {
    let mut reader =
        hound::WavReader::open(path).with_context(|| format!("open {}", path.display()))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = (1u64 << (spec.bits_per_sample.saturating_sub(1))) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale))
                .collect::<Result<_, _>>()?
        }
    };
    let mono = downmix(&interleaved, channels);
    to_16k(&mono, spec.sample_rate)
}

/// Averages interleaved channels into one.
pub fn downmix(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

/// Resamples mono audio to 16 kHz.
pub fn to_16k(mono: &[f32], rate: u32) -> Result<Vec<f32>> {
    if rate == SAMPLE_RATE {
        return Ok(mono.to_vec());
    }
    if rate == 0 {
        bail!("invalid sample rate 0");
    }
    let r = LinearResampler::create(rate as i32, SAMPLE_RATE as i32)
        .ok_or_else(|| anyhow!("could not create a {rate} Hz -> 16 kHz resampler"))?;
    Ok(r.resample(mono, true))
}

/// Streaming resampler to 16 kHz for live capture: feed chunks as they arrive.
pub struct StreamResampler {
    inner: Option<LinearResampler>,
}

impl StreamResampler {
    pub fn new(rate: u32) -> Result<Self> {
        if rate == 0 {
            bail!("invalid sample rate 0");
        }
        let inner = if rate == SAMPLE_RATE {
            None
        } else {
            Some(
                LinearResampler::create(rate as i32, SAMPLE_RATE as i32)
                    .ok_or_else(|| anyhow!("could not create a {rate} Hz -> 16 kHz resampler"))?,
            )
        };
        Ok(Self { inner })
    }

    pub fn push(&mut self, mono: &[f32]) -> Vec<f32> {
        match &self.inner {
            Some(r) => r.resample(mono, false),
            None => mono.to_vec(),
        }
    }

    pub fn finish(&mut self) -> Vec<f32> {
        match &self.inner {
            Some(r) => r.resample(&[], true),
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmixes_stereo() {
        assert_eq!(downmix(&[1.0, 0.0, 0.5, 0.5], 2), vec![0.5, 0.5]);
        assert_eq!(downmix(&[0.1, 0.2], 1), vec![0.1, 0.2]);
    }

    #[test]
    fn streaming_resample_matches_rate() {
        let mut r = StreamResampler::new(48_000).unwrap();
        let mut n = 0;
        for _ in 0..10 {
            n += r.push(&[0.0; 4800]).len();
        }
        n += r.finish().len();
        assert!((15_900..=16_100).contains(&n), "{n}");
    }
}
