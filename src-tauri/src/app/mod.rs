//! The Tauri desktop app: state, setup, tray, background workers.

mod calls;
mod commands;
mod dictation;
mod indicator;
mod meeting;
pub mod permissions;
mod tray;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crossbeam_channel::{unbounded, Sender};
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::asr::service::AsrService;
use crate::db::Library;
use crate::meeting::recorder::Recorder;
use crate::models::{self, download};
use crate::settings::{self, DictationMode, Settings};

/// Passed when PushTalks starts at login, so it opens quietly in the menu bar.
const HIDDEN_ARG: &str = "--hidden";

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub lib: Mutex<Library>,
    pub asr: Arc<AsrService>,
    pub models_dir: PathBuf,
    pub recordings_dir: PathBuf,
    pub meeting: Mutex<Option<Recorder>>,
    pub processing: Mutex<Option<PathBuf>>,
    pub queued: Mutex<Vec<PathBuf>>,
    queue_tx: Sender<PathBuf>,
    pub dictation_tx: Sender<dictation::Cmd>,
    models_ready: AtomicBool,
    pub downloading: AtomicBool,
    pub hold_key_active: AtomicBool,
    /// PushTalks itself is using the mic for dictation or a mic test (not a call).
    pub mic_busy: AtomicBool,
    /// The meeting app named in the current "Record this call?" prompt.
    pub detected_call: Mutex<Option<String>>,
    pub tray: OnceLock<tray::TrayItems>,
}

impl AppState {
    pub fn models_ready(&self) -> bool {
        self.models_ready.load(Ordering::Relaxed)
    }

    /// Whether the models are installed (presence and size; hashing ~700 MB here would stall
    /// startup). Loading still verifies every SHA-256 before use.
    pub fn refresh_models_ready(&self) -> bool {
        let model = self.settings.lock().asr_model.clone();
        let ready = models::asr_model(&model).is_some_and(|spec| {
            download::present(&models::SILERO_VAD, &self.models_dir)
                && download::present(spec, &self.models_dir)
        });
        self.models_ready.store(ready, Ordering::Relaxed);
        ready
    }

    pub fn meeting_active(&self) -> bool {
        self.meeting.lock().is_some()
    }

    pub fn queue(&self, dir: PathBuf) {
        self.queued.lock().push(dir.clone());
        let _ = self.queue_tx.send(dir);
    }
}

pub fn show_main(app: &AppHandle, tab: Option<&str>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    if let Some(tab) = tab {
        let _ = app.emit("navigate", tab);
    }
}

/// Registers the toggle shortcut when the dictation mode is "toggle".
pub fn apply_shortcut(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| e.to_string())?;
    if s.dictation_mode == DictationMode::Toggle && !s.toggle_shortcut.trim().is_empty() {
        gs.register(s.toggle_shortcut.as_str())
            .map_err(|e| format!("Couldn't use {} as a shortcut: {e}", s.toggle_shortcut))?;
    }
    Ok(())
}

fn start_hold_key(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.hold_key_active.load(Ordering::Relaxed) {
        return;
    }
    let (tx, rx) = unbounded();
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let ok = crate::dictation::hotkey::listen(tx);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let ok = {
        drop(tx);
        false
    };
    if !ok {
        return;
    }
    state.hold_key_active.store(true, Ordering::Relaxed);
    let app2 = app.clone();
    std::thread::spawn(move || {
        for ev in rx.iter() {
            let state = app2.state::<AppState>();
            let s = state.settings.lock();
            let enabled = s.dictation_mode == DictationMode::Hold && s.hold_key != "off";
            drop(s);
            if enabled {
                let _ = state.dictation_tx.send(ev.into());
            }
        }
    });
}

pub fn run() {
    let settings_path = Settings::path();
    let settings = Settings::load_from(&settings_path);
    let data = settings::app_data_dir();
    let lib = match Library::open(&data.join("library.sqlite")) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("can't open the library: {e}");
            std::process::exit(1);
        }
    };
    let models_dir = models::models_dir();
    let (queue_tx, queue_rx) = unbounded();
    let (dictation_tx, dictation_rx) = unbounded();
    let state = AppState {
        settings: Mutex::new(settings),
        lib: Mutex::new(lib),
        asr: Arc::new(AsrService::new(models_dir.clone())),
        models_dir,
        recordings_dir: data.join("recordings"),
        meeting: Mutex::new(None),
        processing: Mutex::new(None),
        queued: Mutex::new(Vec::new()),
        queue_tx,
        dictation_tx,
        models_ready: AtomicBool::new(false),
        downloading: AtomicBool::new(false),
        hold_key_active: AtomicBool::new(false),
        mic_busy: AtomicBool::new(false),
        detected_call: Mutex::new(None),
        tray: OnceLock::new(),
    };

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_main(app, None)
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        let _ = app
                            .state::<AppState>()
                            .dictation_tx
                            .send(dictation::Cmd::Toggle);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN_ARG]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(move |app| {
            let handle = app.handle().clone();
            // A regular app: Dock icon, Cmd-Tab and Launchpad. The menu bar icon stays too.
            let state = handle.state::<AppState>();
            std::fs::create_dir_all(&state.recordings_dir)?;
            state.refresh_models_ready();
            let items = tray::create(&handle)?;
            let _ = state.tray.set(items);
            indicator::create(&handle)?;
            calls::create_prompt(&handle)?;
            let settings = state.settings.lock().clone();
            if let Err(e) = apply_shortcut(&handle, &settings) {
                eprintln!("{e}");
            }
            start_hold_key(&handle);
            // Development only: PUSHTALKS_PREVIEW_INDICATOR=1 shows the dictation capsule with a
            // fake voice so its look can be checked without dictating.
            #[cfg(debug_assertions)]
            if std::env::var_os("PUSHTALKS_PREVIEW_INDICATOR").is_some() {
                let h = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(3));
                    let _ = h.emit(
                        "dictation-state",
                        dictation::DictationState {
                            state: "recording",
                            message: None,
                        },
                    );
                    indicator::show(&h);
                    for i in 0..400u32 {
                        let l = (((i as f32) / 6.0).sin().abs() * 0.08)
                            * (1.0 + ((i as f32) / 23.0).sin())
                            / 2.0;
                        let _ = h.emit("dictation-level", l);
                        std::thread::sleep(Duration::from_millis(50));
                    }
                });
            }
            // Show the window when the user opens the app; stay in the menu bar at login.
            if !settings.onboarded || !std::env::args().any(|a| a == HIDDEN_ARG) {
                show_main(&handle, None);
            }

            let h = handle.clone();
            std::thread::Builder::new()
                .name("dictation".into())
                .spawn(move || dictation::run(h, dictation_rx))?;
            let h = handle.clone();
            std::thread::Builder::new()
                .name("call-watch".into())
                .spawn(move || calls::watch(h))?;
            let h = handle.clone();
            std::thread::Builder::new()
                .name("meeting-processing".into())
                .spawn(move || meeting::worker(h, queue_rx))?;
            // Housekeeping: unload the idle model, delete expired audio.
            let h = handle.clone();
            std::thread::spawn(move || {
                let mut ticks = 0u64;
                loop {
                    if ticks.is_multiple_of(3600) {
                        meeting::purge_expired(&h);
                    }
                    let state = h.state::<AppState>();
                    let mins = state.settings.lock().unload_after_mins.max(1) as u64;
                    if state.processing.lock().is_none() {
                        state.asr.unload_if_idle(Duration::from_secs(mins * 60));
                    }
                    std::thread::sleep(Duration::from_secs(30));
                    ticks += 30;
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the main window keeps PushTalks running in the menu bar.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(commands::handler())
        .build(tauri::generate_context!());
    match result {
        Ok(app) => app.run(|app, event| {
            // Quitting mid-meeting stops and saves the recording properly; it then appears
            // under "Recordings waiting to be transcribed" on the next launch.
            if let tauri::RunEvent::Exit = event {
                let rec = app.state::<AppState>().meeting.lock().take();
                if let Some(rec) = rec {
                    if let Err(e) = rec.stop() {
                        eprintln!("saving the recording on quit failed: {e}");
                    }
                }
            }
            // Opening PushTalks again (Finder, Spotlight, Dock) while it runs shows the window.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                show_main(app, None);
            }
        }),
        Err(e) => {
            eprintln!("PushTalks failed to start: {e}");
            std::process::exit(1);
        }
    }
}
