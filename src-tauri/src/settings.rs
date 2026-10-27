//! User settings (PRD §6.6), stored as JSON in the app data folder.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// What happens to meeting audio after transcription. Default is delete (PRD §10); changing the
/// default for the pilot is a leadership decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mode", content = "days")]
pub enum Retention {
    Delete,
    KeepDays(u32),
    Forever,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictationMode {
    /// Hold the key to talk, release to transcribe.
    Hold,
    /// Press the toggle shortcut once to start and again to stop.
    Toggle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub onboarded: bool,
    pub dictation_mode: DictationMode,
    /// Hold-to-talk key: "right_option" (macOS) / "right_alt" (Windows), or "off".
    pub hold_key: String,
    /// Toggle shortcut in tauri-plugin-global-shortcut syntax.
    pub toggle_shortcut: String,
    pub max_dictation_secs: u32,
    /// Cleanup rules (PRD §6.1).
    pub remove_fillers: bool,
    /// Microphone name as cpal reports it; None = system default.
    pub mic: Option<String>,
    pub output_dir: PathBuf,
    pub retention: Retention,
    /// "v2" (default) or "v3".
    pub asr_model: String,
    pub unload_after_mins: u32,
    pub launch_at_login: bool,
    /// Offer to record when another app starts using the mic (a call starting).
    pub suggest_recording: bool,
    /// Voice memory: name known voices automatically. Off by default; voiceprints are
    /// biometric data and need approval and consent first (PRD §6.3).
    pub remember_voices: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            onboarded: false,
            dictation_mode: DictationMode::Hold,
            hold_key: default_hold_key().into(),
            toggle_shortcut: "CommandOrControl+Shift+Space".into(),
            max_dictation_secs: 120,
            remove_fillers: true,
            mic: None,
            output_dir: default_output_dir(),
            retention: Retention::Delete,
            asr_model: "v2".into(),
            unload_after_mins: 15,
            launch_at_login: false,
            suggest_recording: true,
            remember_voices: false,
        }
    }
}

fn default_hold_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "right_option"
    } else {
        "right_alt"
    }
}

pub fn default_output_dir() -> PathBuf {
    dirs::document_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("PushTalks")
}

/// `~/Library/Application Support/PushTalks` or `%APPDATA%\PushTalks`.
/// `PUSHTALKS_DATA_DIR` overrides it (tests, a second profile).
pub fn app_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("PUSHTALKS_DATA_DIR") {
        return PathBuf::from(dir);
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("PushTalks")
}

impl Settings {
    /// Not `settings.json`: the archived Electron prototype keeps its own settings there.
    pub fn path() -> PathBuf {
        app_data_dir().join("preferences.json")
    }

    pub fn load_from(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        fs::rename(&tmp, path).context("saving settings")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_fills_missing_fields() {
        let dir = std::env::temp_dir().join(format!("pt-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        let s = Settings {
            retention: Retention::KeepDays(7),
            ..Default::default()
        };
        s.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path), s);
        fs::write(&path, r#"{"onboarded":true}"#).unwrap();
        let partial = Settings::load_from(&path);
        assert!(partial.onboarded);
        assert_eq!(partial.retention, Retention::Delete);
        fs::remove_dir_all(dir).unwrap();
    }
}
