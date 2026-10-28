//! Commands the React frontend calls. Errors come back as user-readable strings.
//! Every command runs off the main thread (`async`): a blocking call there (a dialog, audio
//! device setup, file I/O) freezes the whole window.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State, Wry};
use tauri_plugin_autostart::ManagerExt;

use super::{apply_shortcut, meeting, permissions, AppState};
use crate::audio::mic;
use crate::db::library::{Dictation, MeetingRow, SearchHit};
use crate::models::{self, download};
use crate::output::{self, Meeting, Segment};
use crate::settings::Settings;
use crate::vocab::{rules, Rule, RuleMode};

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub fn handler() -> impl Fn(tauri::ipc::Invoke<Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        app_info,
        settings_get,
        settings_save,
        permissions_status,
        permission_request,
        permission_open_settings,
        hold_key_retry,
        mics_list,
        mic_check,
        models_status,
        models_download,
        meeting_start,
        meeting_stop,
        meeting_recording,
        recordings_unfinished,
        recording_process,
        recording_discard,
        processing_status,
        library_list,
        library_search,
        meeting_get,
        meeting_update,
        meeting_rename_speaker,
        meeting_apply_rule,
        meeting_restore,
        meeting_set_notes,
        claude_connector,
        app_restart,
        voices_list,
        voice_delete,
        voices_forget_all,
        call_prompt_state,
        call_prompt_record,
        call_prompt_dismiss,
        meeting_delete,
        meeting_reveal,
        meeting_open,
        vocab_list,
        vocab_add,
        vocab_delete,
        vocab_never_ask,
        vocab_correction,
        dictations_list,
        dictation_update,
        dictations_clear,
        choose_folder,
    ]
}

// ---- App and settings ----

#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
    os: &'static str,
    notices: &'static str,
    models_dir: PathBuf,
}

#[tauri::command(async)]
fn app_info(state: State<AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        notices: include_str!("../../../THIRD_PARTY_NOTICES.md"),
        models_dir: state.models_dir.clone(),
    }
}

#[tauri::command(async)]
fn settings_get(state: State<AppState>) -> Settings {
    state.settings.lock().clone()
}

#[tauri::command(async)]
fn settings_save(app: AppHandle, state: State<AppState>, settings: Settings) -> Res<Settings> {
    let previous = state.settings.lock().clone();
    if settings.launch_at_login != previous.launch_at_login {
        let al = app.autolaunch();
        if settings.launch_at_login {
            al.enable()
        } else {
            al.disable()
        }
        .map_err(err)?;
    }
    apply_shortcut(&app, &settings)?;
    settings.save_to(&Settings::path()).map_err(err)?;
    *state.settings.lock() = settings.clone();
    if settings.asr_model != previous.asr_model {
        state.asr.unload();
        state.refresh_models_ready();
    }
    Ok(settings)
}

#[tauri::command(async)]
fn choose_folder(app: AppHandle) -> Option<PathBuf> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog()
        .file()
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
}

// ---- Permissions and devices ----

#[derive(Serialize)]
struct PermissionsOut {
    #[serde(flatten)]
    status: permissions::Status,
    hold_key_active: bool,
}

#[tauri::command]
async fn permissions_status(state: State<'_, AppState>) -> Res<PermissionsOut> {
    let status = tauri::async_runtime::spawn_blocking(permissions::status_fresh)
        .await
        .map_err(err)?;
    Ok(PermissionsOut {
        status,
        hold_key_active: state.hold_key_active.load(Ordering::Relaxed),
    })
}

#[tauri::command(async)]
fn permission_request(kind: String) {
    permissions::request(&kind);
}

#[tauri::command(async)]
fn permission_open_settings(kind: String) {
    permissions::open_settings(&kind);
}

/// Tries again to listen for the hold key, after Input Monitoring was granted.
#[tauri::command(async)]
fn hold_key_retry(app: AppHandle) -> bool {
    super::start_hold_key(&app);
    app.state::<AppState>()
        .hold_key_active
        .load(Ordering::Relaxed)
}

#[tauri::command(async)]
fn mics_list() -> Vec<String> {
    mic::list()
}

#[derive(Serialize)]
struct MicCheck {
    device: String,
    #[serde(flatten)]
    heard: mic::Heard,
}

/// Listens for up to 8 seconds (stopping soon after speech is heard), sending "mic-level"
/// events for a live meter. The first call triggers the OS microphone prompt.
#[tauri::command]
async fn mic_check(app: AppHandle, state: State<'_, AppState>) -> Res<MicCheck> {
    let device = state.settings.lock().mic.clone();
    state.mic_busy.store(true, Ordering::Relaxed);
    tauri::async_runtime::spawn_blocking(move || {
        let result = mic::start(device).map(|mut c| {
            let heard = mic::listen_for_speech(&c, Duration::from_secs(8), |level| {
                let _ = app.emit("mic-level", level);
            });
            c.stop();
            MicCheck {
                device: c.device.clone(),
                heard,
            }
        });
        app.state::<AppState>()
            .mic_busy
            .store(false, Ordering::Relaxed);
        result.map_err(err)
    })
    .await
    .map_err(err)?
}

// ---- Models ----

#[derive(Serialize)]
struct ModelOut {
    id: &'static str,
    label: &'static str,
    license: &'static str,
    size: u64,
    download_size: u64,
    installed: bool,
    required: bool,
}

#[tauri::command]
async fn models_status(state: State<'_, AppState>) -> Res<Vec<ModelOut>> {
    let dir = state.models_dir.clone();
    let model = state.settings.lock().asr_model.clone();
    let out = tauri::async_runtime::spawn_blocking(move || {
        let selected = models::asr_model(&model).map(|m| m.id);
        let mut list: Vec<&models::ModelSpec> = models::ALL_MODELS.iter().collect();
        for m in models::manifest::ASR_MODELS {
            if !list.iter().any(|x| x.id == m.id) {
                list.push(m);
            }
        }
        list.into_iter()
            .map(|m| ModelOut {
                id: m.id,
                label: m.label,
                license: m.license,
                size: m.installed_size(),
                download_size: m.download_size,
                installed: download::verify(m, &dir).is_empty(),
                required: models::ALL_MODELS.iter().any(|x| x.id == m.id) || Some(m.id) == selected,
            })
            .collect()
    })
    .await
    .map_err(err)?;
    state.refresh_models_ready();
    Ok(out)
}

#[derive(Clone, Serialize)]
struct DownloadProgress {
    id: &'static str,
    received: u64,
    total: u64,
}

/// Downloads and verifies every required model in the background. The only network use in the
/// app happens inside `download::install` (PRD §10).
#[tauri::command(async)]
fn models_download(app: AppHandle, state: State<AppState>) -> Res<()> {
    if state.downloading.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let dir = state.models_dir.clone();
    let model = state.settings.lock().asr_model.clone();
    std::thread::spawn(move || {
        let mut specs: Vec<&models::ModelSpec> = models::ALL_MODELS.iter().collect();
        if let Some(m) = models::asr_model(&model) {
            if !specs.iter().any(|x| x.id == m.id) {
                specs.push(m);
            }
        }
        let mut result = Ok(());
        for spec in specs {
            let r = download::install(spec, &dir, |p| {
                let _ = app.emit(
                    "model-progress",
                    DownloadProgress {
                        id: spec.id,
                        received: p.received,
                        total: p.total,
                    },
                );
            });
            if let Err(e) = r {
                result = Err(format!("{}: {e}", spec.label));
                break;
            }
        }
        let state = app.state::<AppState>();
        state.downloading.store(false, Ordering::SeqCst);
        state.refresh_models_ready();
        let _ = app.emit("models-done", result.err());
    });
    Ok(())
}

// ---- Meetings ----

#[tauri::command(async)]
fn meeting_start(
    app: AppHandle,
    title: Option<String>,
    consent_confirmed: bool,
    mic_only: bool,
) -> Res<()> {
    let state = app.state::<AppState>();
    if !state.models_ready() {
        return Err("Install the speech model first (Settings > Model).".into());
    }
    meeting::start(&app, title, consent_confirmed, mic_only).map_err(err)
}

#[tauri::command(async)]
fn meeting_stop(app: AppHandle) -> Res<()> {
    meeting::stop(&app).map_err(err)
}

#[tauri::command(async)]
fn meeting_recording(state: State<AppState>) -> bool {
    state.meeting_active()
}

#[tauri::command(async)]
fn recordings_unfinished(app: AppHandle) -> Vec<meeting::Unfinished> {
    meeting::unfinished(&app)
}

/// Only directories inside the recordings folder may be processed or deleted.
fn recording_dir(state: &AppState, dir: &Path) -> Res<PathBuf> {
    let root = state.recordings_dir.canonicalize().map_err(err)?;
    let dir = dir.canonicalize().map_err(err)?;
    if dir.parent() == Some(root.as_path()) {
        Ok(dir)
    } else {
        Err("That isn't a PushTalks recording.".into())
    }
}

#[tauri::command(async)]
fn recording_process(state: State<AppState>, dir: PathBuf) -> Res<()> {
    let dir = recording_dir(&state, &dir)?;
    state.queue(dir);
    Ok(())
}

#[tauri::command(async)]
fn recording_discard(state: State<AppState>, dir: PathBuf) -> Res<()> {
    let dir = recording_dir(&state, &dir)?;
    fs::remove_dir_all(dir).map_err(err)
}

#[derive(Serialize)]
struct ProcessingStatus {
    current: Option<String>,
    queued: usize,
}

#[tauri::command(async)]
fn processing_status(state: State<AppState>) -> ProcessingStatus {
    let current = state
        .processing
        .lock()
        .as_ref()
        .and_then(|d| crate::meeting::recorder::Meta::load(d).ok())
        .map(|m| m.title);
    ProcessingStatus {
        current,
        queued: state.queued.lock().len(),
    }
}

// ---- Library ----

#[tauri::command(async)]
fn library_list(state: State<AppState>) -> Res<Vec<MeetingRow>> {
    state.lib.lock().meetings().map_err(err)
}

#[tauri::command(async)]
fn library_search(state: State<AppState>, query: String) -> Res<Vec<SearchHit>> {
    state.lib.lock().search(&query).map_err(err)
}

#[derive(Serialize)]
struct Suggestion {
    segment: usize,
    rule_id: i64,
    heard: String,
    spelling: String,
}

#[derive(Serialize)]
struct MeetingOut {
    row: MeetingRow,
    segments: Vec<Segment>,
    asr_model: String,
    speakers: Vec<String>,
    notes: Option<String>,
    /// Places where a "confirm first" spelling (common word) could apply.
    suggestions: Vec<Suggestion>,
}

fn load(state: &AppState, id: i64) -> Res<(MeetingRow, Meeting)> {
    state.lib.lock().load_meeting(id).map_err(err)
}

fn out(state: &AppState, row: MeetingRow, m: Meeting) -> Res<MeetingOut> {
    let vocab = state.lib.lock().rules().map_err(err)?;
    let mut suggestions = Vec::new();
    for (i, s) in m.segments.iter().enumerate() {
        let tokens: Vec<&str> = s.text.split_whitespace().collect();
        for h in rules::find(&tokens, &vocab, RuleMode::Confirm) {
            if let Some(r) = vocab.iter().find(|r| r.id == h.rule_id) {
                suggestions.push(Suggestion {
                    segment: i,
                    rule_id: r.id,
                    heard: r.heard.clone(),
                    spelling: r.spelling.clone(),
                });
            }
        }
    }
    Ok(MeetingOut {
        speakers: m.speakers(),
        row,
        asr_model: m.asr_model,
        segments: m.segments,
        notes: m.notes,
        suggestions,
    })
}

/// Writes the files (renaming them if the title changed) and re-indexes the meeting.
fn save(state: &AppState, row: &MeetingRow, m: &Meeting) -> Res<MeetingRow> {
    state.lib.lock().save_meeting(row, m).map_err(err)
}

#[tauri::command(async)]
fn meeting_get(state: State<AppState>, id: i64) -> Res<MeetingOut> {
    let (row, m) = load(&state, id)?;
    out(&state, row, m)
}

#[derive(Deserialize)]
struct EditedSegment {
    text: String,
    speaker: String,
}

/// Saves edits from the transcript editor. Edited segments keep the model's words in
/// `original_text` so changes stay auditable.
#[tauri::command(async)]
fn meeting_update(
    app: AppHandle,
    state: State<AppState>,
    id: i64,
    title: String,
    segments: Vec<EditedSegment>,
) -> Res<MeetingOut> {
    let (row, mut m) = load(&state, id)?;
    if segments.len() != m.segments.len() {
        return Err(
            "The transcript changed while you were editing. Reopen it and try again.".into(),
        );
    }
    let title = title.trim();
    if !title.is_empty() {
        m.title = title.to_string();
    }
    for (seg, edit) in m.segments.iter_mut().zip(segments) {
        let text = edit.text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text != seg.text {
            seg.words = output::retime(seg, &text);
            if seg.original_text.is_none() {
                seg.original_text = Some(seg.text.clone());
            }
            seg.text = text;
        }
        let speaker = edit.speaker.trim();
        if !speaker.is_empty() {
            seg.speaker = speaker.to_string();
        }
    }
    let row = save(&state, &row, &m)?;
    let _ = app.emit("library-changed", ());
    out(&state, row, m)
}

#[tauri::command(async)]
fn meeting_rename_speaker(
    app: AppHandle,
    state: State<AppState>,
    id: i64,
    from: String,
    to: String,
) -> Res<Renamed> {
    let to = to.trim();
    if to.is_empty() {
        return Err("Enter a name.".into());
    }
    let (row, mut m) = load(&state, id)?;
    for s in m.segments.iter_mut().filter(|s| s.speaker == from) {
        s.speaker = to.to_string();
    }
    let row = save(&state, &row, &m)?;
    // Voice memory: naming a speaker teaches PushTalks that voice for later meetings.
    let mut learned = false;
    if state.settings.lock().remember_voices {
        let lib = state.lib.lock();
        if let Some(print) = lib.rename_meeting_voiceprint(id, &from, to).map_err(err)? {
            let generic = to == "Me" || to == "Others" || to.starts_with("Speaker ");
            if !generic {
                lib.learn_voice(to, &print).map_err(err)?;
                learned = true;
            }
        }
    }
    let _ = app.emit("library-changed", ());
    Ok(Renamed {
        meeting: out(&state, row, m)?,
        learned_voice: learned,
    })
}

#[derive(Serialize)]
struct Renamed {
    meeting: MeetingOut,
    /// True when PushTalks will now recognise this voice in later meetings.
    learned_voice: bool,
}

#[derive(Serialize)]
struct Applied {
    meeting: MeetingOut,
    changed: usize,
}

/// Applies one learned spelling to this transcript. `segment` limits it to one segment
/// (confirming a common-word suggestion); otherwise it applies everywhere.
#[tauri::command(async)]
fn meeting_apply_rule(
    state: State<AppState>,
    id: i64,
    rule_id: i64,
    segment: Option<usize>,
) -> Res<Applied> {
    let rule: Rule = state
        .lib
        .lock()
        .rules()
        .map_err(err)?
        .into_iter()
        .find(|r| r.id == rule_id)
        .ok_or("That spelling was deleted.")?;
    let forced = Rule {
        mode: RuleMode::Auto,
        ..rule
    };
    let (row, mut m) = load(&state, id)?;
    let mut changed = 0;
    for (i, seg) in m.segments.iter_mut().enumerate() {
        if segment.is_some_and(|only| only != i) {
            continue;
        }
        if let Some(text) = rules::apply_text(&seg.text, std::slice::from_ref(&forced)) {
            if let Some(words) = rules::apply_words(&seg.words, std::slice::from_ref(&forced)) {
                seg.words = words;
            }
            if seg.original_text.is_none() {
                seg.original_text = Some(seg.text.clone());
            }
            seg.text = text;
            changed += 1;
        }
    }
    let row = if changed > 0 {
        save(&state, &row, &m)?
    } else {
        row
    };
    Ok(Applied {
        meeting: out(&state, row, m)?,
        changed,
    })
}

/// Restores a transcript snapshot (undo).
#[tauri::command(async)]
fn meeting_restore(state: State<AppState>, id: i64, segments: Vec<Segment>) -> Res<MeetingOut> {
    let (row, mut m) = load(&state, id)?;
    m.segments = segments;
    let row = save(&state, &row, &m)?;
    out(&state, row, m)
}

#[tauri::command(async)]
fn meeting_delete(app: AppHandle, state: State<AppState>, id: i64, delete_files: bool) -> Res<()> {
    let row = state.lib.lock().meeting(id).map_err(err)?;
    if let Some(row) = row {
        if delete_files {
            let _ = fs::remove_file(&row.md_path);
            let _ = fs::remove_file(row.md_path.with_extension("json"));
        }
        if let Some(audio) = &row.audio_dir {
            if audio.starts_with(&state.recordings_dir) {
                let _ = fs::remove_dir_all(audio);
            }
        }
    }
    state.lib.lock().delete_meeting(id).map_err(err)?;
    let _ = app.emit("library-changed", ());
    Ok(())
}

fn md_path(state: &AppState, id: i64) -> Res<PathBuf> {
    Ok(state
        .lib
        .lock()
        .meeting(id)
        .map_err(err)?
        .ok_or("Meeting not found.")?
        .md_path)
}

#[tauri::command(async)]
fn meeting_reveal(app: AppHandle, state: State<AppState>, id: i64) -> Res<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(md_path(&state, id)?)
        .map_err(err)
}

#[tauri::command(async)]
fn meeting_open(app: AppHandle, state: State<AppState>, id: i64) -> Res<()> {
    use tauri_plugin_opener::OpenerExt;
    let path = md_path(&state, id)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(err)
}

/// Saves minutes edited in PushTalks (empty text clears them).
#[tauri::command(async)]
fn meeting_set_notes(
    app: AppHandle,
    state: State<AppState>,
    id: i64,
    notes: String,
) -> Res<MeetingOut> {
    let (row, mut m) = load(&state, id)?;
    let notes = notes.trim();
    m.notes = (!notes.is_empty()).then(|| notes.to_string());
    let row = save(&state, &row, &m)?;
    let _ = app.emit("library-changed", ());
    out(&state, row, m)
}

#[derive(Serialize)]
struct Connector {
    path: PathBuf,
    installed: bool,
    claude_code: String,
    claude_desktop: String,
    /// Pasted into Claude Code: writes minutes for new meetings every 30 minutes.
    auto_minutes_prompt: String,
}

/// Where the Claude connector (`pushtalks-mcp`) lives and how to add it to Claude.
#[tauri::command(async)]
fn claude_connector() -> Res<Connector> {
    let exe = std::env::current_exe().map_err(err)?;
    let name = if cfg!(windows) {
        "pushtalks-mcp.exe"
    } else {
        "pushtalks-mcp"
    };
    let path = exe.with_file_name(name);
    let shown = path.to_string_lossy().to_string();
    let desktop = serde_json::json!({ "mcpServers": { "pushtalks": { "command": shown } } });
    Ok(Connector {
        installed: path.exists(),
        claude_code: format!("claude mcp add --scope user pushtalks -- \"{shown}\""),
        claude_desktop: serde_json::to_string_pretty(&desktop).map_err(err)?,
        auto_minutes_prompt: crate::mcp::auto_minutes_prompt(),
        path,
    })
}

/// Relaunches PushTalks, e.g. after macOS permissions change. Refuses while a meeting is
/// recording so a restart can't cut it short.
#[tauri::command(async)]
fn app_restart(app: AppHandle) -> Res<()> {
    if app.state::<AppState>().meeting_active() {
        return Err("Stop the meeting recording before restarting PushTalks.".into());
    }
    app.restart();
}

#[tauri::command(async)]
fn call_prompt_state(state: State<AppState>) -> Option<String> {
    state.detected_call.lock().clone()
}

/// "Record" in the call prompt: opens the consent dialog in the main window. Recording itself
/// still starts only after the user confirms consent there.
#[tauri::command(async)]
fn call_prompt_record(app: AppHandle) {
    super::calls::hide_prompt(&app, false);
    let app_name = app.state::<AppState>().detected_call.lock().clone();
    super::show_main(&app, Some("meeting"));
    let _ = app.emit("record-call", app_name);
}

#[tauri::command(async)]
fn call_prompt_dismiss(app: AppHandle) {
    super::calls::hide_prompt(&app, true);
}

// ---- Voice memory ----

#[tauri::command(async)]
fn voices_list(state: State<AppState>) -> Res<Vec<crate::voices::Voice>> {
    state.lib.lock().voices().map_err(err)
}

#[tauri::command(async)]
fn voice_delete(state: State<AppState>, id: i64) -> Res<()> {
    state.lib.lock().delete_voice(id).map_err(err)
}

#[tauri::command(async)]
fn voices_forget_all(state: State<AppState>) -> Res<()> {
    state.lib.lock().forget_all_voices().map_err(err)
}

// ---- Vocabulary ----

#[tauri::command(async)]
fn vocab_list(state: State<AppState>) -> Res<Vec<Rule>> {
    state.lib.lock().rules().map_err(err)
}

#[tauri::command(async)]
fn vocab_add(
    state: State<AppState>,
    heard: String,
    spelling: String,
    mode: Option<RuleMode>,
) -> Res<Rule> {
    state
        .lib
        .lock()
        .save_rule(&heard, &spelling, mode)
        .map_err(err)
}

#[tauri::command(async)]
fn vocab_delete(state: State<AppState>, id: i64) -> Res<()> {
    state.lib.lock().delete_rule(id).map_err(err)
}

#[tauri::command(async)]
fn vocab_never_ask(state: State<AppState>, heard: String) -> Res<()> {
    state.lib.lock().never_ask(&heard).map_err(err)
}

/// The correction in an edit, if PushTalks should offer to remember it.
#[tauri::command(async)]
fn vocab_correction(
    state: State<AppState>,
    old: String,
    new: String,
) -> Res<Option<rules::Correction>> {
    let Some(c) = rules::correction(&old, &new) else {
        return Ok(None);
    };
    let lib = state.lib.lock();
    if lib.is_never_ask(&c.heard).map_err(err)? {
        return Ok(None);
    }
    let known = lib
        .rules()
        .map_err(err)?
        .into_iter()
        .any(|r| r.heard.eq_ignore_ascii_case(&c.heard) && r.spelling == c.spelling);
    Ok((!known).then_some(c))
}

// ---- Dictation history ----

#[tauri::command(async)]
fn dictations_list(state: State<AppState>) -> Res<Vec<Dictation>> {
    state.lib.lock().dictations().map_err(err)
}

#[tauri::command(async)]
fn dictation_update(state: State<AppState>, id: i64, text: String) -> Res<()> {
    state.lib.lock().update_dictation(id, &text).map_err(err)
}

#[tauri::command(async)]
fn dictations_clear(state: State<AppState>) -> Res<()> {
    state.lib.lock().clear_dictations().map_err(err)
}
