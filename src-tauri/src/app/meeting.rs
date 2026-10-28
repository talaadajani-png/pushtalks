//! Meeting controller: consent-gated recording, a background processing queue, retention
//! (PRD §6.2, §10).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use chrono::Local;
use crossbeam_channel::Receiver;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::{tray, AppState};
use crate::asr::diarize::Diarizer;
use crate::audio::{self, mic};
use crate::meeting::pipeline;
use crate::meeting::recorder::{self, Meta, Recorder, Status};
use crate::models;
use crate::output::{self, Meeting};
use crate::settings::Retention;

#[derive(Clone, Serialize)]
pub struct Tick {
    pub recording: bool,
    pub elapsed_secs: u64,
    pub mic_level: f32,
    pub system_level: f32,
    pub title: String,
    pub mic_only: bool,
}

#[derive(Clone, Serialize)]
pub struct Progress {
    pub id: String,
    pub title: String,
    pub fraction: f32,
    pub eta_secs: Option<u64>,
}

/// Starts a recording. Refuses unless the user confirmed the consent reminder (PRD §10).
pub fn start(
    app: &AppHandle,
    title: Option<String>,
    consent_confirmed: bool,
    mic_only: bool,
) -> Result<()> {
    if !consent_confirmed {
        bail!("Recording needs your confirmation that everyone in the meeting has been told.");
    }
    let state = app.state::<AppState>();
    let mut slot = state.meeting.lock();
    if slot.is_some() {
        bail!("A meeting is already being recorded.");
    }
    let confirmed_at = Local::now();
    // A typed title wins; otherwise the meeting app's window or the calendar event now.
    let title = title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty() && !output::is_generic_title(t))
        .or_else(crate::titles::detect)
        .unwrap_or_else(|| output::default_title(&confirmed_at));
    let system = if mic_only {
        None
    } else {
        Some(audio::start_system().map_err(|e| {
            let fix = if cfg!(target_os = "macos") {
                "Allow Screen & System Audio Recording for PushTalks in System Settings"
            } else {
                "Check that your speakers or headset are connected and selected as the default output in Windows sound settings"
            };
            anyhow!("Computer audio isn't available ({e}). {fix}, or record the mic only.")
        })?)
    };
    let mic_capture = mic::start(state.settings.lock().mic.clone())?;
    let rec = Recorder::start(
        &state.recordings_dir,
        title,
        confirmed_at,
        mic_capture,
        system,
    )?;
    *slot = Some(rec);
    drop(slot);
    tray::set_recording(app, true);
    let app2 = app.clone();
    std::thread::spawn(move || ticker(app2));
    Ok(())
}

/// Emits the timer and levels while recording, and shows the timer in the menu bar.
fn ticker(app: AppHandle) {
    loop {
        let tick = {
            let state = app.state::<AppState>();
            let slot = state.meeting.lock();
            match slot.as_ref() {
                Some(r) => {
                    let (m, s) = r.levels();
                    Tick {
                        recording: true,
                        elapsed_secs: r.elapsed().as_secs(),
                        mic_level: m,
                        system_level: s,
                        title: r.meta.title.clone(),
                        mic_only: !r.meta.has_system,
                    }
                }
                None => return,
            }
        };
        tray::set_timer(&app, Some(tick.elapsed_secs));
        let _ = app.emit("meeting-tick", tick);
        std::thread::sleep(Duration::from_millis(250));
    }
}

pub fn stop(app: &AppHandle) -> Result<()> {
    let state = app.state::<AppState>();
    let rec = state
        .meeting
        .lock()
        .take()
        .ok_or_else(|| anyhow!("No meeting is being recorded."))?;
    tray::set_recording(app, false);
    tray::set_timer(app, None);
    let _ = app.emit(
        "meeting-tick",
        Tick {
            recording: false,
            elapsed_secs: 0,
            mic_level: 0.0,
            system_level: 0.0,
            title: String::new(),
            mic_only: false,
        },
    );
    let (dir, mut meta) = rec.stop()?;
    // No name yet: the meeting window is usually still open at Stop, so look again.
    if output::is_generic_title(&meta.title) {
        if let Some(t) = crate::titles::detect() {
            meta.title = t;
            meta.save(&dir)?;
        }
    }
    state.queue(dir);
    Ok(())
}

/// Keeps the computer at full speed and awake while a meeting is processed. Without it, macOS
/// App Nap and Windows efficiency mode throttle the app once its window is hidden, and the
/// computer may sleep: a 45-minute meeting then took hours instead of minutes. Fewer inference
/// threads than cores leave room for the user's other work instead of a lowered priority.
struct FullSpeed {
    #[cfg(target_os = "macos")]
    activity:
        objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2::runtime::NSObjectProtocol>>,
}

impl FullSpeed {
    fn start() -> Self {
        #[cfg(target_os = "windows")]
        // SAFETY: plain Win32 calls on this process and thread with a correctly sized struct.
        unsafe {
            use windows_sys::Win32::System::Power::{
                SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
            };
            use windows_sys::Win32::System::Threading::{
                GetCurrentProcess, ProcessPowerThrottling, SetProcessInformation,
                PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
                PROCESS_POWER_THROTTLING_STATE,
            };
            // Opt out of efficiency mode (EcoQoS) for the whole process.
            let state = PROCESS_POWER_THROTTLING_STATE {
                Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
                ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
                StateMask: 0,
            };
            SetProcessInformation(
                GetCurrentProcess(),
                ProcessPowerThrottling,
                &state as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            );
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
        }
        Self {
            #[cfg(target_os = "macos")]
            activity: objc2_foundation::NSProcessInfo::processInfo()
                .beginActivityWithOptions_reason(
                    objc2_foundation::NSActivityOptions::UserInitiated,
                    &objc2_foundation::NSString::from_str("Transcribing a meeting"),
                ),
        }
    }
}

impl Drop for FullSpeed {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        // SAFETY: the token came from beginActivityWithOptions on the same NSProcessInfo.
        unsafe {
            objc2_foundation::NSProcessInfo::processInfo().endActivity(&self.activity);
        }
        #[cfg(target_os = "windows")]
        // SAFETY: plain Win32 calls; the worker thread set these flags in `start`.
        unsafe {
            use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS};
            SetThreadExecutionState(ES_CONTINUOUS);
        }
    }
}

/// Processes queued recordings one at a time, for the app's lifetime.
pub fn worker(app: AppHandle, rx: Receiver<PathBuf>) {
    for dir in rx.iter() {
        let state = app.state::<AppState>();
        state.queued.lock().retain(|d| d != &dir);
        *state.processing.lock() = Some(dir.clone());
        let full_speed = FullSpeed::start();
        let result = process(&app, &dir);
        drop(full_speed);
        *state.processing.lock() = None;
        match result {
            Ok(id) => {
                let _ = app.emit("processing-done", id);
            }
            Err(e) => {
                eprintln!("meeting processing failed: {e}");
                let _ = app.emit("processing-error", e.to_string());
            }
        }
        let _ = app.emit("library-changed", ());
    }
}

fn process(app: &AppHandle, dir: &Path) -> Result<i64> {
    let state = app.state::<AppState>();
    let mut meta = Meta::load(dir)?;
    let (model, out_dir, retention) = {
        let s = state.settings.lock();
        (s.asr_model.clone(), s.output_dir.clone(), s.retention)
    };
    let spec = models::asr_model(&model).ok_or_else(|| anyhow!("unknown speech model {model}"))?;
    let loaded = state.asr.get(&model)?;
    let diarizer = if meta.has_system {
        Diarizer::load(&state.models_dir, crate::pipeline::default_threads()).unwrap_or(None)
    } else {
        None
    };
    let vocab = state.lib.lock().rules()?;
    let remember = state.settings.lock().remember_voices;
    let voices = if remember {
        Some(state.lib.lock().voices()?)
    } else {
        None
    };
    let t0 = Instant::now();
    let mut last = Instant::now() - Duration::from_secs(1);
    let processed = pipeline::process(
        &loaded,
        diarizer.as_ref(),
        dir,
        &vocab,
        voices.as_deref(),
        |f| {
            if last.elapsed() < Duration::from_millis(500) && f < 1.0 {
                return;
            }
            last = Instant::now();
            let eta = (f > 0.02).then(|| (t0.elapsed().as_secs_f32() * (1.0 - f) / f) as u64);
            let _ = app.emit(
                "processing",
                Progress {
                    id: meta.id.clone(),
                    title: meta.title.clone(),
                    fraction: f,
                    eta_secs: eta,
                },
            );
        },
    )?;
    let (segments, duration) = (processed.segments, processed.duration_secs);
    drop(loaded);
    log_timings(duration, &processed.timings, t0.elapsed().as_secs_f32());
    let meeting = Meeting {
        title: meta.title.clone(),
        started_at: meta.started_at,
        duration_secs: duration,
        asr_model: spec.id.to_string(),
        segments,
        notes: None,
    };
    let md = meeting.write(&out_dir, None)?;
    let lib = state.lib.lock();
    let id = lib.upsert_meeting(
        None,
        &meeting.title,
        &meeting.started_at,
        duration,
        &md,
        &meeting.search_body(),
    )?;
    if remember {
        lib.set_meeting_voiceprints(id, &processed.voiceprints)?;
    }
    meta.status = Status::Done;
    match retention {
        Retention::Delete => {
            drop(lib);
            fs::remove_dir_all(dir)?;
        }
        Retention::KeepDays(days) => {
            meta.save(dir)?;
            lib.set_audio(
                id,
                Some(dir),
                Some(Local::now() + chrono::Duration::days(days as i64)),
            )?;
        }
        Retention::Forever => {
            meta.save(dir)?;
            lib.set_audio(id, Some(dir), None)?;
        }
    }
    Ok(id)
}

/// Appends one line of stage durations to `processing-times.log` in the app data folder, so
/// slow processing on a tester's PC can be measured. Durations and settings only, never content.
fn log_timings(audio_secs: f32, t: &pipeline::Timings, total: f32) {
    use std::io::Write;
    let line = format!(
        "{} v{} {} threads={} audio={:.0}s total={:.0}s mic_asr={:.0}s system_asr={:.0}s diarize={:.0}s rtf={:.2}\n",
        Local::now().format("%Y-%m-%d %H:%M"),
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        crate::pipeline::default_threads(),
        audio_secs,
        total,
        t.mic_asr,
        t.system_asr,
        t.diarize,
        total / audio_secs.max(1.0),
    );
    let path = crate::settings::app_data_dir().join("processing-times.log");
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Deletes kept audio whose retention period has passed.
pub fn purge_expired(app: &AppHandle) {
    let state = app.state::<AppState>();
    let lib = state.lib.lock();
    if let Ok(expired) = lib.expired_audio(&Local::now()) {
        for (id, dir) in expired {
            if dir.starts_with(&state.recordings_dir) {
                let _ = fs::remove_dir_all(&dir);
            }
            let _ = lib.set_audio(id, None, None);
        }
    }
}

#[derive(Serialize)]
pub struct Unfinished {
    pub dir: PathBuf,
    pub title: String,
    pub started_at: chrono::DateTime<Local>,
    pub status: Status,
}

/// Recordings left unprocessed, for example after a crash, excluding the ones in progress.
pub fn unfinished(app: &AppHandle) -> Vec<Unfinished> {
    let state = app.state::<AppState>();
    let busy: Vec<PathBuf> = state
        .meeting
        .lock()
        .as_ref()
        .map(|r| r.dir.clone())
        .into_iter()
        .chain(state.processing.lock().clone())
        .chain(state.queued.lock().iter().cloned())
        .collect();
    recorder::unfinished(&state.recordings_dir)
        .into_iter()
        .filter(|(d, _)| !busy.contains(d))
        .map(|(dir, m)| Unfinished {
            dir,
            title: m.title,
            started_at: m.started_at,
            status: m.status,
        })
        .collect()
}
