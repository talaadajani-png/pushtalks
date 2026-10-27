//! The small floating dictation indicator (PRD §6.1). It must never take focus, or the paste
//! would land in PushTalks instead of the app the user is typing in.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tauri::window::{Effect, EffectState, EffectsBuilder};
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "indicator";
/// A small liquid-glass capsule; wider only when it has to show a message.
const WIDTH: f64 = 116.0;
const WIDE: f64 = 300.0;
const HEIGHT: f64 = 44.0;
const BOTTOM_GAP: f64 = 96.0;

/// Bumped on every show so a pending delayed hide from an earlier dictation is ignored.
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let win = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html#indicator".into()))
        .title("PushTalks indicator")
        .inner_size(WIDTH, HEIGHT)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .visible(false)
        // Native frosted glass behind the transparent page ("liquid glass").
        .effects(
            EffectsBuilder::new()
                .effects([Effect::HudWindow, Effect::Acrylic])
                .state(EffectState::Active)
                .radius(HEIGHT / 2.0)
                .build(),
        )
        .build()?;
    place(&win, WIDTH);
    let _ = win.set_ignore_cursor_events(true);
    Ok(())
}

/// Sizes the window and centres it near the bottom of the main screen.
fn place(win: &tauri::WebviewWindow, width: f64) {
    let _ = win.set_size(tauri::LogicalSize::new(width, HEIGHT));
    if let Ok(Some(m)) = win.primary_monitor() {
        let scale = m.scale_factor();
        let x = m.position().x + ((m.size().width as f64 - width * scale) / 2.0) as i32;
        let y = m.position().y + (m.size().height as f64 - (HEIGHT + BOTTOM_GAP) * scale) as i32;
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
}

pub fn set_wide(app: &AppHandle, wide: bool) {
    if let Some(w) = app.get_webview_window(LABEL) {
        place(&w, if wide { WIDE } else { WIDTH });
    }
}

pub fn show(app: &AppHandle) {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.show();
    }
}

pub fn hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
    }
}

pub fn hide_after(app: &AppHandle, delay: Duration) {
    let gen = GENERATION.load(Ordering::SeqCst);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        if GENERATION.load(Ordering::SeqCst) == gen {
            hide(&app);
        }
    });
}
