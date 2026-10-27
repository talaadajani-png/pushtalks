//! Dual-track meeting recorder (PRD §6.2). Each track is written to its own 16 kHz 16-bit WAV
//! and flushed every 2 seconds, so a crash loses at most about 2 seconds (NFR: 10 s).

use std::fs;
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::audio::capture::Capture;
use crate::audio::resample::SAMPLE_RATE;

pub const MIC_FILE: &str = "mic.wav";
pub const SYSTEM_FILE: &str = "system.wav";
const META_FILE: &str = "meta.json";
const FLUSH_EVERY: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Recording,
    /// Stopped but not yet transcribed (or processing was interrupted).
    Recorded,
    Done,
}

/// Saved next to the audio so an interrupted recording can be found and processed later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub id: String,
    pub title: String,
    pub started_at: DateTime<Local>,
    /// When the user confirmed the consent reminder (PRD §10).
    pub consent_confirmed_at: DateTime<Local>,
    pub status: Status,
    pub has_system: bool,
}

impl Meta {
    pub fn save(&self, dir: &Path) -> Result<()> {
        let tmp = dir.join("meta.json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        fs::rename(tmp, dir.join(META_FILE))?;
        Ok(())
    }

    pub fn load(dir: &Path) -> Result<Self> {
        Ok(serde_json::from_slice(&fs::read(dir.join(META_FILE))?)?)
    }
}

/// Recordings that never finished processing (for example after a crash), oldest first.
pub fn unfinished(root: &Path) -> Vec<(PathBuf, Meta)> {
    let mut out: Vec<(PathBuf, Meta)> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            let meta = Meta::load(&dir).ok()?;
            (meta.status != Status::Done).then_some((dir, meta))
        })
        .collect();
    out.sort_by_key(|(_, m)| m.started_at);
    out
}

/// Fixes the size fields of a WAV file that was never finalized (the app crashed or was
/// killed mid-recording), so the audio written so far can still be read. Safe to call on a
/// healthy file: sizes are only rewritten when they don't match the file's length.
pub fn repair_wav(path: &Path) -> Result<()> {
    use std::io::{Read, Seek, SeekFrom, Write};
    let mut f = fs::OpenOptions::new().read(true).write(true).open(path)?;
    let len = f.metadata()?.len();
    let mut head = [0u8; 12];
    if len < 12
        || f.read_exact(&mut head).is_err()
        || &head[0..4] != b"RIFF"
        || &head[8..12] != b"WAVE"
    {
        anyhow::bail!("{} isn't a WAV file", path.display());
    }
    // Walk the chunks to the "data" chunk.
    let mut pos = 12u64;
    while pos + 8 <= len {
        let mut chunk = [0u8; 8];
        f.seek(SeekFrom::Start(pos))?;
        f.read_exact(&mut chunk)?;
        let size = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]) as u64;
        if &chunk[0..4] == b"data" {
            // Whole 16-bit samples only.
            let data_len = (len - pos - 8) & !1;
            if size != data_len {
                f.seek(SeekFrom::Start(pos + 4))?;
                f.write_all(&(data_len as u32).to_le_bytes())?;
                f.seek(SeekFrom::Start(4))?;
                f.write_all(&((pos + 8 + data_len - 8) as u32).to_le_bytes())?;
            }
            return Ok(());
        }
        pos += 8 + size + (size & 1);
    }
    anyhow::bail!("{} has no audio data", path.display())
}

struct Track {
    capture: Capture,
    writer: JoinHandle<Result<()>>,
}

fn spawn_writer(capture: &Capture, path: PathBuf) -> Result<JoinHandle<Result<()>>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut wav = hound::WavWriter::create(&path, spec)
        .with_context(|| format!("creating {}", path.display()))?;
    let rx = capture.chunks.clone();
    Ok(std::thread::Builder::new()
        .name("wav-writer".into())
        .spawn(move || {
            let mut last_flush = Instant::now();
            // Ends when the capture stops and its channel closes.
            for chunk in rx.iter() {
                for s in chunk {
                    wav.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)?;
                }
                if last_flush.elapsed() >= FLUSH_EVERY {
                    wav.flush()?;
                    last_flush = Instant::now();
                }
            }
            wav.finalize()?;
            Ok(())
        })?)
}

pub struct Recorder {
    pub dir: PathBuf,
    pub meta: Meta,
    started: Instant,
    mic: Track,
    system: Option<Track>,
}

impl Recorder {
    /// Starts recording. `system` is None for mic-only (in-room) meetings. The caller must have
    /// shown the consent reminder and had it confirmed; `consent_confirmed_at` records when.
    pub fn start(
        root: &Path,
        title: String,
        consent_confirmed_at: DateTime<Local>,
        mic: Capture,
        system: Option<Capture>,
    ) -> Result<Self> {
        let started_at = Local::now();
        let id = format!(
            "{}-{}",
            started_at.format("%Y%m%d-%H%M%S"),
            &uuid::Uuid::new_v4().to_string()[..8]
        );
        let dir = root.join(&id);
        fs::create_dir_all(&dir)?;
        let meta = Meta {
            id,
            title,
            started_at,
            consent_confirmed_at,
            status: Status::Recording,
            has_system: system.is_some(),
        };
        meta.save(&dir)?;
        let mic_writer = spawn_writer(&mic, dir.join(MIC_FILE))?;
        let system = match system {
            Some(c) => {
                let writer = spawn_writer(&c, dir.join(SYSTEM_FILE))?;
                Some(Track { capture: c, writer })
            }
            None => None,
        };
        Ok(Self {
            dir,
            meta,
            started: Instant::now(),
            mic: Track {
                capture: mic,
                writer: mic_writer,
            },
            system,
        })
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// (mic, system) input levels for the UI meters.
    pub fn levels(&self) -> (f32, f32) {
        (
            self.mic.capture.level(),
            self.system.as_ref().map_or(0.0, |t| t.capture.level()),
        )
    }

    /// Stops both tracks, finishes the WAV files and marks the recording ready to process.
    pub fn stop(mut self) -> Result<(PathBuf, Meta)> {
        self.mic.capture.stop();
        if let Some(t) = self.system.as_mut() {
            t.capture.stop();
        }
        let mut first_err = None;
        for t in std::iter::once(self.mic).chain(self.system) {
            if let Err(e) = t
                .writer
                .join()
                .unwrap_or_else(|_| Err(anyhow::anyhow!("writer crashed")))
            {
                first_err.get_or_insert(e);
            }
        }
        self.meta.status = Status::Recorded;
        self.meta.save(&self.dir)?;
        match first_err {
            Some(e) => Err(e),
            None => Ok((self.dir, self.meta)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repairs_a_wav_that_was_never_finalized() {
        let path = std::env::temp_dir().join(format!("pt-crash-{}.wav", std::process::id()));
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..16_000 {
            w.write_sample((i % 100) as i16).unwrap();
        }
        // Simulate a crash: the writer is never finalized or dropped.
        std::mem::forget(w);
        repair_wav(&path).unwrap();
        let r = hound::WavReader::open(&path).unwrap();
        assert!(r.duration() >= 8_000, "{}", r.duration());
        // A healthy file is left as it is.
        let before = fs::read(&path).unwrap();
        repair_wav(&path).unwrap();
        assert_eq!(before, fs::read(&path).unwrap());
        fs::remove_file(path).unwrap();
    }
}
