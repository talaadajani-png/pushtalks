//! "Record this call?" prompt: watches for another app using the mic and offers to record.
//! It only opens the consent dialog; nothing records until the user confirms (PRD §10).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::window::{Effect, EffectState, EffectsBuilder};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

use super::AppState;
use crate::calls;

pub const LABEL: &str = "callprompt";
const WIDTH: f64 = 340.0;
const HEIGHT: f64 = 104.0;
const POLL: Duration = Duration::from_secs(2);
/// The mic must be in use this many polls in a row (a few seconds), so brief mic use
/// (a voice memo, a dictation app) doesn't trigger the prompt.
const CONFIRM_POLLS: u32 = 3;

/// Set when the user dismisses the prompt; cleared when the call (mic use) ends.
static DISMISSED: AtomicBool = AtomicBool::new(false);

pub fn create_prompt(app: &AppHandle) -> tauri::Result<()> {
    let win =
        WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html#callprompt".into()))
            .title("PushTalks")
            .inner_size(WIDTH, HEIGHT)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .skip_taskbar(true)
            .focused(false)
            .visible(false)
            // Native frosted glass behind the transparent page ("liquid glass").
            .effects(
                EffectsBuilder::new()
                    .effects([Effect::HudWindow, Effect::Acrylic])
                    .state(EffectState::Active)
                    .radius(16.0)
                    .build(),
            )
            .build()?;
    if let Ok(Some(m)) = win.primary_monitor() {
        let scale = m.scale_factor();
        let x = m.position().x + (m.size().width as f64 - (WIDTH + 16.0) * scale) as i32;
        let y = m.position().y + (40.0 * scale) as i32;
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
    Ok(())
}

pub fn hide_prompt(app: &AppHandle, dismissed: bool) {
    if dismissed {
        DISMISSED.store(true, Ordering::Relaxed);
    }
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
    }
}

fn show_prompt(app: &AppHandle, name: Option<&str>) {
    *app.state::<AppState>().detected_call.lock() = name.map(str::to_string);
    let _ = app.emit("call-detected", name);
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.show();
    }
}

/// Runs for the app's lifetime.
pub fn watch(app: AppHandle) {
    let mut streak = 0u32;
    let mut shown = false;
    loop {
        std::thread::sleep(POLL);
        let state = app.state::<AppState>();
        let enabled = state.settings.lock().suggest_recording && state.models_ready();
        let ours = state.meeting_active() || state.mic_busy.load(Ordering::Relaxed);
        let in_use = enabled && !ours && calls::mic_in_use();
        if !in_use {
            if streak > 0 && !ours {
                // The call ended (or PushTalks took over the mic): reset for the next call.
                DISMISSED.store(false, Ordering::Relaxed);
            }
            streak = 0;
            if shown {
                hide_prompt(&app, false);
                shown = false;
            }
            continue;
        }
        streak += 1;
        if streak == CONFIRM_POLLS && !DISMISSED.load(Ordering::Relaxed) {
            let procs = calls::processes();
            show_prompt(&app, calls::likely_app(&procs));
            shown = true;
        }
    }
}
