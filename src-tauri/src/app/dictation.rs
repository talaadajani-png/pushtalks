//! Dictation controller: hold key / toggle shortcut -> mic -> Parakeet -> paste (PRD §6.1).

use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::{indicator, AppState};
use crate::audio::resample::SAMPLE_RATE;
use crate::audio::{capture::Capture, mic};
use crate::dictation::{self, hotkey::HoldEvent, insert};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    Start,
    Stop,
    Cancel,
    Toggle,
}

impl From<HoldEvent> for Cmd {
    fn from(e: HoldEvent) -> Self {
        match e {
            HoldEvent::Down => Cmd::Start,
            HoldEvent::Up => Cmd::Stop,
            HoldEvent::Cancel => Cmd::Cancel,
        }
    }
}

#[derive(Clone, Serialize)]
pub struct DictationState {
    /// "recording", "processing", "done", "empty", "error" or "idle".
    pub state: &'static str,
    pub message: Option<String>,
}

fn emit(app: &AppHandle, state: &'static str, message: Option<String>) {
    let _ = app.emit("dictation-state", DictationState { state, message });
    // Messages need room for words; otherwise the indicator is a small capsule.
    indicator::set_wide(app, matches!(state, "error" | "empty"));
    match state {
        "recording" | "processing" => indicator::show(app),
        "idle" => indicator::hide(app),
        _ => indicator::hide_after(app, Duration::from_millis(1200)),
    }
}

/// Runs for the app's lifetime, turning key events into dictations.
pub fn run(app: AppHandle, rx: Receiver<Cmd>) {
    let mut active: Option<(Capture, Instant)> = None;
    loop {
        // Poll faster while recording so the indicator's sound waves follow the voice.
        let wait = if active.is_some() { 50 } else { 200 };
        let cmd = match rx.recv_timeout(Duration::from_millis(wait)) {
            Ok(c) => Some(c),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if let Some((c, _)) = &active {
            let _ = app.emit("dictation-level", c.level());
        }
        let state = app.state::<AppState>();
        let max = Duration::from_secs(state.settings.lock().max_dictation_secs.max(5) as u64);
        let cmd = match (cmd, &active) {
            (Some(Cmd::Toggle), None) => Some(Cmd::Start),
            (Some(Cmd::Toggle), Some(_)) => Some(Cmd::Stop),
            (None, Some((_, t))) if t.elapsed() >= max => Some(Cmd::Stop),
            (c, _) => c,
        };
        match cmd {
            Some(Cmd::Start) if active.is_none() => {
                if !state.models_ready() {
                    emit(&app, "error", Some("Finish setup in PushTalks first: the speech model isn't installed yet.".into()));
                    continue;
                }
                if state.meeting_active() {
                    // The mic is busy recording a meeting.
                    continue;
                }
                let device = state.settings.lock().mic.clone();
                state
                    .mic_busy
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                match mic::start(device) {
                    Ok(c) => {
                        active = Some((c, Instant::now()));
                        emit(&app, "recording", None);
                        // Warm the model while the user talks.
                        let asr = state.asr.clone();
                        let model = state.settings.lock().asr_model.clone();
                        std::thread::spawn(move || {
                            let _ = asr.get(&model);
                        });
                    }
                    Err(e) => {
                        state
                            .mic_busy
                            .store(false, std::sync::atomic::Ordering::Relaxed);
                        emit(&app, "error", Some(format!("Microphone unavailable: {e}")));
                    }
                }
            }
            Some(Cmd::Cancel) => {
                if let Some((mut c, _)) = active.take() {
                    c.stop();
                    state
                        .mic_busy
                        .store(false, std::sync::atomic::Ordering::Relaxed);
                    emit(&app, "idle", None);
                }
            }
            Some(Cmd::Stop) => {
                if let Some((mut c, _)) = active.take() {
                    c.stop();
                    state
                        .mic_busy
                        .store(false, std::sync::atomic::Ordering::Relaxed);
                    let samples: Vec<f32> = c.chunks.try_iter().flatten().collect();
                    if (samples.len() as f32 / SAMPLE_RATE as f32) < dictation::MIN_SECS {
                        emit(&app, "idle", None);
                        continue;
                    }
                    emit(&app, "processing", None);
                    let app2 = app.clone();
                    std::thread::spawn(move || finish(app2, samples));
                }
            }
            _ => {}
        }
    }
}

/// Transcribes and pastes one dictation. Runs on its own thread, never the UI thread.
fn finish(app: AppHandle, samples: Vec<f32>) {
    let state = app.state::<AppState>();
    let (model, fillers) = {
        let s = state.settings.lock();
        (s.asr_model.clone(), s.remove_fillers)
    };
    let result = (|| -> anyhow::Result<Option<String>> {
        let loaded = state.asr.get(&model)?;
        let rules = state.lib.lock().rules()?;
        dictation::transcribe(&loaded, &samples, &rules, fillers)
    })();
    drop(samples);
    match result {
        Ok(Some(text)) => {
            let _ = state.lib.lock().add_dictation(&text);
            let _ = app.emit("dictations-changed", ());
            match insert::paste(&text) {
                Ok(()) => emit(&app, "done", None),
                Err(e) => emit(
                    &app,
                    "error",
                    Some(format!(
                        "Couldn't paste ({e}). The text is in Recent dictations."
                    )),
                ),
            }
        }
        Ok(None) => emit(&app, "empty", Some("Didn't catch any speech.".into())),
        Err(e) => emit(&app, "error", Some(format!("Transcription failed: {e}"))),
    }
}
