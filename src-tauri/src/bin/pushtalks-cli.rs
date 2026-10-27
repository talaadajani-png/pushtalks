//! PushTalks CLI: fetch and verify models, transcribe a WAV with Parakeet + Silero VAD, and run
//! the meeting pipeline on a recording folder (mic.wav + optional system.wav) without the app.

use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use pushtalks_lib::models::{download, manifest::ALL_MODELS, models_dir};
use pushtalks_lib::output::timestamps::hhmmss_cs;
use pushtalks_lib::{audio::resample::read_wav_16k_mono, pipeline};

#[derive(Parser)]
#[command(
    name = "pushtalks-cli",
    about = "PushTalks Phase 0: on-device transcription with NVIDIA Parakeet"
)]
struct Cli {
    /// Model folder (default: the OS app data folder, PRD §7.3).
    #[arg(long, global = true)]
    models_dir: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Download (resumable) and install every pinned model, checking SHA-256.
    FetchModels {
        /// Also install an optional speech model ("v3") for benchmarking.
        #[arg(long)]
        model: Option<String>,
    },
    /// Re-check every installed model file against its pinned SHA-256.
    VerifyModels,
    /// Transcribe a WAV file and print timed segments.
    Transcribe {
        wav: PathBuf,
        /// Print JSON segments with word timestamps instead of text.
        #[arg(long)]
        json: bool,
        #[arg(long, default_value_t = pipeline::default_threads())]
        threads: i32,
    },
    /// Process a recording folder like the app does and write the Markdown + JSON transcript.
    Meeting {
        /// Folder with mic.wav and optionally system.wav (16 kHz mono 16-bit).
        dir: PathBuf,
        /// Where to write the transcript files.
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value = "Test meeting")]
        title: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let dir = cli.models_dir.unwrap_or_else(models_dir);
    match cli.cmd {
        Cmd::FetchModels { model } => {
            let mut specs: Vec<&pushtalks_lib::models::ModelSpec> = ALL_MODELS.iter().collect();
            if let Some(name) = model {
                let extra = pushtalks_lib::models::asr_model(&name)
                    .ok_or_else(|| anyhow::anyhow!("unknown model {name}; try v2 or v3"))?;
                if !specs.iter().any(|s| s.id == extra.id) {
                    specs.push(extra);
                }
            }
            for spec in specs {
                eprintln!(
                    "{} ({:.0} MB download)",
                    spec.label,
                    spec.download_size as f64 / 1e6
                );
                download::install(spec, &dir, |p| {
                    eprint!(
                        "\r  {:>5.1}%",
                        100.0 * p.received as f64 / p.total.max(1) as f64
                    );
                    std::io::stderr().flush().ok();
                })?;
                eprintln!("\r  installed and verified");
            }
            eprintln!("Models are in {}", dir.display());
        }
        Cmd::VerifyModels => {
            let mut ok = true;
            for spec in ALL_MODELS {
                let problems = download::verify(spec, &dir);
                if problems.is_empty() {
                    println!(
                        "OK   {} ({:.0} MB)",
                        spec.label,
                        spec.installed_size() as f64 / 1e6
                    );
                } else {
                    ok = false;
                    println!("FAIL {}: {}", spec.label, problems.join("; "));
                }
            }
            if !ok {
                std::process::exit(1);
            }
        }
        Cmd::Transcribe { wav, json, threads } => {
            let loaded = pipeline::load(&dir, threads)?;
            let samples = read_wav_16k_mono(&wav)?;
            let run = pipeline::transcribe(&loaded, &samples)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&run.transcripts)?);
            } else {
                for t in &run.transcripts {
                    println!(
                        "[{} --> {}] {}",
                        hhmmss_cs(t.start),
                        hhmmss_cs(t.end),
                        t.text
                    );
                }
            }
        }
        Cmd::Meeting {
            dir: rec,
            out,
            title,
        } => {
            use pushtalks_lib::asr::diarize::Diarizer;
            use pushtalks_lib::output::Meeting;
            let threads = pipeline::default_threads();
            let loaded = pipeline::load(&dir, threads)?;
            let diarizer = Diarizer::load(&dir, threads)?;
            eprintln!(
                "diarization: {}",
                if diarizer.is_some() {
                    "on"
                } else {
                    "off (models not installed)"
                }
            );
            let t0 = std::time::Instant::now();
            let processed = pushtalks_lib::meeting::pipeline::process(
                &loaded,
                diarizer.as_ref(),
                &rec,
                &[],
                None,
                |f| eprint!("\r  {:>5.1}%", f * 100.0),
            )?;
            let (segments, duration) = (processed.segments, processed.duration_secs);
            eprintln!(
                "\r  {:.1} s of audio in {:.1} s",
                duration,
                t0.elapsed().as_secs_f32()
            );
            let m = Meeting {
                title,
                started_at: chrono::Local::now(),
                duration_secs: duration,
                asr_model: pushtalks_lib::models::PARAKEET_V2_INT8.id.into(),
                segments,
                notes: None,
            };
            println!("{}", m.write(&out, None)?.display());
        }
    }
    Ok(())
}
