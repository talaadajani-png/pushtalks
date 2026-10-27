//! Benchmark harness (PRD §14): WER, real-time factor and peak RAM for one WAV.
//!
//! cargo run --release --bin benchmark -- <file.wav> [reference.txt] [--threads N] [--json]

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{bail, Result};
use pushtalks_lib::metrics::{peak_rss_bytes, wer};
use pushtalks_lib::{
    asr::VadSettings, audio::resample::read_wav_16k_mono, models::models_dir, pipeline,
};
use serde::Serialize;

#[derive(Serialize)]
struct Report {
    file: String,
    audio_secs: f32,
    segments: usize,
    longest_segment_secs: f32,
    verify_secs: f32,
    load_secs: f32,
    vad_secs: f32,
    asr_secs: f32,
    /// Processing time (VAD + ASR) divided by audio length. Lower is faster; 0.1 = 10x real time.
    rtf: f32,
    peak_ram_mb: Option<f64>,
    wer: Option<f32>,
    threads: i32,
    hypothesis: String,
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut wav: Option<PathBuf> = None;
    let mut reference: Option<PathBuf> = None;
    let mut threads = pipeline::default_threads();
    let mut json = false;
    let mut vad = VadSettings::default();
    let mut fixed_windows: Option<f32> = None;
    let mut beam = false;
    let mut model = String::from("v2");
    while let Some(a) = args.next() {
        match a.as_str() {
            "--threads" => threads = args.next().unwrap_or_default().parse()?,
            "--json" => json = true,
            "--beam" => beam = true,
            "--model" => model = args.next().unwrap_or_default(),
            "--vad-threshold" => vad.threshold = args.next().unwrap_or_default().parse()?,
            "--min-silence" => vad.min_silence_secs = args.next().unwrap_or_default().parse()?,
            "--merge-max" => vad.merge_max_secs = args.next().unwrap_or_default().parse()?,
            "--max-speech" => vad.max_speech_secs = args.next().unwrap_or_default().parse()?,
            // Experiment: skip the VAD and send fixed-length windows instead.
            "--fixed-windows" => fixed_windows = Some(args.next().unwrap_or_default().parse()?),
            _ if wav.is_none() => wav = Some(a.into()),
            _ if reference.is_none() => reference = Some(a.into()),
            _ => bail!("unexpected argument {a}"),
        }
    }
    let Some(wav) = wav else {
        bail!("usage: benchmark <file.wav> [reference.txt] [--threads N] [--json] [--vad-threshold 0.5] [--min-silence 0.5] [--merge-max 25]")
    };

    let Some(asr) = pushtalks_lib::models::asr_model(&model) else {
        bail!("unknown model {model}; try v2 or v3")
    };
    let loaded = pipeline::load_model(&models_dir(), asr, threads, vad, beam)?;
    let samples = read_wav_16k_mono(&wav)?;
    let t = Instant::now();
    let run = match fixed_windows {
        Some(secs) => pipeline::transcribe_fixed_windows(&loaded, &samples, secs)?,
        None => pipeline::transcribe(&loaded, &samples)?,
    };
    let processing = t.elapsed().as_secs_f32();
    let hypothesis = pipeline::full_text(&run);
    let wer = match &reference {
        Some(r) => Some(wer(&std::fs::read_to_string(r)?, &hypothesis)),
        None => None,
    };
    let report = Report {
        file: wav.display().to_string(),
        audio_secs: run.audio_secs,
        segments: run.segments,
        longest_segment_secs: run.longest_segment_secs,
        verify_secs: loaded.verify_time.as_secs_f32(),
        load_secs: loaded.load_time.as_secs_f32(),
        vad_secs: run.vad_time.as_secs_f32(),
        asr_secs: run.asr_time.as_secs_f32(),
        rtf: processing / run.audio_secs.max(0.001),
        peak_ram_mb: peak_rss_bytes().map(|b| b as f64 / 1e6),
        wer,
        threads,
        hypothesis,
    };
    if json {
        println!("{}", serde_json::to_string(&report)?);
    } else {
        println!("file          {}", report.file);
        println!(
            "audio         {:.1} s in {} segments (longest {:.1} s)",
            report.audio_secs, report.segments, report.longest_segment_secs
        );
        println!(
            "verify+load   {:.2} s + {:.2} s",
            report.verify_secs, report.load_secs
        );
        println!(
            "processing    {:.2} s (VAD {:.2} s, ASR {:.2} s)",
            processing, report.vad_secs, report.asr_secs
        );
        println!(
            "RTF           {:.3} ({:.1}x real time)",
            report.rtf,
            1.0 / report.rtf.max(1e-6)
        );
        if let Some(mb) = report.peak_ram_mb {
            println!("peak RAM      {mb:.0} MB");
        }
        if let Some(w) = report.wer {
            println!("WER           {:.1}%", w * 100.0);
        }
    }
    Ok(())
}
