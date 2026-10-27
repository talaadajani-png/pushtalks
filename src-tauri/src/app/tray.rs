//! Menu bar / system tray (PRD §9): open the app, start or stop a meeting, see the timer.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

use super::{show_main, AppState};

const TRAY_ID: &str = "main";

pub struct TrayItems {
    pub meeting: MenuItem<Wry>,
}

pub fn create(app: &AppHandle) -> tauri::Result<TrayItems> {
    let open = MenuItem::with_id(app, "open", "Open PushTalks", true, None::<&str>)?;
    let meeting = MenuItem::with_id(app, "meeting", "Start meeting…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit PushTalks", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&open, &meeting, &PredefinedMenuItem::separator(app)?, &quit],
    )?;
    #[cfg(target_os = "macos")]
    let icon = Image::from_bytes(include_bytes!("../../icons/trayTemplate@2x.png"))?;
    #[cfg(not(target_os = "macos"))]
    let icon = Image::from_bytes(include_bytes!("../../icons/tray.png"))?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("PushTalks")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app, None),
            "meeting" => {
                let recording = app.state::<AppState>().meeting_active();
                if recording {
                    if let Err(e) = super::meeting::stop(app) {
                        eprintln!("stop meeting: {e}");
                    }
                } else {
                    // Starting needs the consent confirmation, which lives in the main window.
                    show_main(app, Some("meeting"));
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(TrayItems { meeting })
}

pub fn set_recording(app: &AppHandle, recording: bool) {
    let state = app.state::<AppState>();
    if let Some(items) = state.tray.get() {
        let _ = items.meeting.set_text(if recording {
            "Stop meeting"
        } else {
            "Start meeting…"
        });
    }
}

/// Shows "● 12:03" next to the menu bar icon while recording (macOS); a tooltip elsewhere.
pub fn set_timer(app: &AppHandle, secs: Option<u64>) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let text = secs.map(|s| format!("● {:02}:{:02}", s / 60, s % 60));
        #[cfg(target_os = "macos")]
        let _ = tray.set_title(text.as_deref());
        let _ = tray.set_tooltip(Some(text.map_or("PushTalks".to_string(), |t| {
            format!("PushTalks: recording {t}")
        })));
    }
}
