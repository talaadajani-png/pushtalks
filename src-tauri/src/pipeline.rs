//! WAV in, timed segments out: load and verify models, VAD, then Parakeet per segment.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};

use crate::asr::{Engine, Transcript, Vad, VadSettings};
use crate::audio::resample::SAMPLE_RATE;
use crate::models::{self, download, PARAKEET_V2_INT8, SILERO_VAD};

pub struct Loaded {
    pub engine: Engine,
    pub vad: Vad,
    pub verify_time: Duration,
    pub load_time: Duration,
}

/// Verifies every model file against its pinned SHA-256, then loads the models.
/// Refuses to load anything that doesn't match (PRD §7.3).
pub fn load(models_dir: &Path, threads: i32) -> Result<Loaded> {
    load_with(models_dir, threads, VadSettings::default())
}

/// Like [`load`], with explicit VAD settings (used by the benchmark to tune them).
pub fn load_with(models_dir: &Path, threads: i32, vad: VadSettings) -> Result<Loaded> {
    load_full(models_dir, threads, vad, false)
}

/// Like [`load_with`], also choosing beam search decoding (benchmark experiments).
pub fn load_full(models_dir: &Path, threads: i32, vad: VadSettings, beam: bool) -> Result<Loaded> {
    load_model(models_dir, &PARAKEET_V2_INT8, threads, vad, beam)
}

/// Like [`load_full`], with an explicit speech model from the manifest.
pub fn load_model(
    models_dir: &Path,
    asr: &crate::models::ModelSpec,
    threads: i32,
    vad: VadSettings,
    beam: bool,
) -> Result<Loaded> {
    let t0 = Instant::now();
    let mut problems = Vec::new();
    for spec in [&SILERO_VAD, asr] {
        for p in download::verify(spec, models_dir) {
            problems.push(format!("{}: {p}", spec.id));
        }
    }
    if !problems.is_empty() {
        bail!(
            "models are missing or don't match their pinned SHA-256 ({}). Run `pushtalks-cli fetch-models`.",
            problems.join("; ")
        );
    }
    let verify_time = t0.elapsed();
    let t1 = Instant::now();
    let engine = Engine::load_model(models_dir, asr, threads, beam)?;
    let vad = Vad::with_settings(&SILERO_VAD.file_path(models_dir, "silero_vad.onnx"), 1, vad);
    Ok(Loaded {
        engine,
        vad,
        verify_time,
        load_time: t1.elapsed(),
    })
}

pub struct Run {
    pub transcripts: Vec<Transcript>,
    pub audio_secs: f32,
    pub vad_time: Duration,
    pub asr_time: Duration,
    pub segments: usize,
    pub longest_segment_secs: f32,
}

/// Runs VAD + ASR over 16 kHz mono samples.
pub fn transcribe(loaded: &Loaded, samples: &[f32]) -> Result<Run> {
    let t0 = Instant::now();
    let segments = loaded.vad.segment(samples)?;
    let vad_time = t0.elapsed();
    let longest = segments
        .iter()
        .map(|s| s.duration_secs())
        .fold(0.0, f32::max);
    let t1 = Instant::now();
    let mut transcripts = Vec::with_capacity(segments.len());
    for seg in &segments {
        let t = loaded.engine.transcribe(seg)?;
        if !t.text.is_empty() {
            transcripts.push(t);
        }
    }
    Ok(Run {
        transcripts,
        audio_secs: samples.len() as f32 / SAMPLE_RATE as f32,
        vad_time,
        asr_time: t1.elapsed(),
        segments: segments.len(),
        longest_segment_secs: longest,
    })
}

/// Benchmark experiment: transcribe fixed windows (no VAD) to compare against VAD segmentation.
pub fn transcribe_fixed_windows(loaded: &Loaded, samples: &[f32], secs: f32) -> Result<Run> {
    let size = ((secs.min(crate::asr::MAX_SEGMENT_SECS)) * SAMPLE_RATE as f32) as usize;
    let segments: Vec<crate::asr::Segment> = samples
        .chunks(size.max(1))
        .enumerate()
        .map(|(i, c)| crate::asr::Segment {
            start_sample: i * size,
            samples: c.to_vec(),
        })
        .collect();
    let longest = segments
        .iter()
        .map(|s| s.duration_secs())
        .fold(0.0, f32::max);
    let t1 = Instant::now();
    let mut transcripts = Vec::with_capacity(segments.len());
    for seg in &segments {
        let t = loaded.engine.transcribe(seg)?;
        if !t.text.is_empty() {
            transcripts.push(t);
        }
    }
    Ok(Run {
        transcripts,
        audio_secs: samples.len() as f32 / SAMPLE_RATE as f32,
        vad_time: Duration::ZERO,
        asr_time: t1.elapsed(),
        segments: segments.len(),
        longest_segment_secs: longest,
    })
}

/// The full text, segments joined by spaces.
pub fn full_text(run: &Run) -> String {
    run.transcripts
        .iter()
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// One thread per physical core, 2 to 8. Intel and AMD chips report two logical cores per
/// physical one (hyper-threading), which only adds contention; Apple chips don't. On a 10-core
/// Mac, 8 threads processed 29 min of meeting in 268 s and 5 threads in 300 s.
pub fn default_threads() -> i32 {
    let per_core = if cfg!(target_arch = "x86_64") { 2 } else { 1 };
    std::thread::available_parallelism()
        .map(|n| (n.get() / per_core).clamp(2, 8) as i32)
        .unwrap_or(4)
}

pub use models::models_dir;
