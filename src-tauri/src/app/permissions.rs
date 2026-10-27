//! OS permissions the app needs (PRD §9 onboarding). macOS: Microphone, Accessibility (paste),
//! Input Monitoring (hold-to-talk key) and Screen Recording (system audio). Windows needs none.

use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub accessibility: bool,
    pub input_monitoring: bool,
    pub screen_recording: bool,
    /// Optional: names meetings from the calendar event happening now.
    #[serde(default)]
    pub calendar: bool,
}

#[cfg(target_os = "macos")]
mod mac {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::{CFString, CFStringRef};

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXIsProcessTrustedWithOptions(
            options: core_foundation::dictionary::CFDictionaryRef,
        ) -> bool;
        static kAXTrustedCheckOptionPrompt: CFStringRef;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightListenEventAccess() -> bool;
        fn CGRequestListenEventAccess() -> bool;
    }

    pub fn accessibility() -> bool {
        // SAFETY: no arguments; returns a plain bool.
        unsafe { AXIsProcessTrusted() }
    }

    pub fn request_accessibility() -> bool {
        // SAFETY: kAXTrustedCheckOptionPrompt is a constant CFString owned by the framework.
        let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
        let opts = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
        // SAFETY: opts is a valid CFDictionary for the duration of the call.
        unsafe { AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef()) }
    }

    pub fn input_monitoring() -> bool {
        // SAFETY: no arguments; returns a plain bool.
        unsafe { CGPreflightListenEventAccess() }
    }

    pub fn request_input_monitoring() -> bool {
        // SAFETY: no arguments; shows the system prompt the first time.
        unsafe { CGRequestListenEventAccess() }
    }
}

pub fn status() -> Status {
    #[cfg(target_os = "macos")]
    {
        Status {
            accessibility: mac::accessibility(),
            input_monitoring: mac::input_monitoring(),
            screen_recording: crate::audio::system_mac::has_permission(),
            calendar: crate::titles::calendar_allowed(),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Status {
            accessibility: true,
            input_monitoring: true,
            screen_recording: true,
            calendar: false,
        }
    }
}

/// Writes this process's permission status as JSON (the helper run in `main.rs`).
pub fn write_status(path: &Path) {
    if let Ok(json) = serde_json::to_vec(&status()) {
        let _ = std::fs::write(path, json);
    }
}

/// Permission status from a fresh helper process, because macOS keeps answering with what was
/// true when this process first asked (often until a relaunch). Falls back to this process.
pub fn status_fresh() -> Status {
    let fresh = (|| -> Option<Status> {
        let exe = std::env::current_exe().ok()?;
        let out = std::env::temp_dir().join(format!("pushtalks-perm-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&out);
        let mut child = std::process::Command::new(exe)
            .arg("--permissions-out")
            .arg(&out)
            .spawn()
            .ok()?;
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(3) {
            if let Ok(Some(_)) = child.try_wait() {
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        let _ = child.kill();
        let status = serde_json::from_slice(&std::fs::read(&out).ok()?).ok();
        let _ = std::fs::remove_file(&out);
        status
    })();
    fresh.unwrap_or_else(status)
}

/// Asks for a permission: shows the OS prompt where one exists, otherwise opens the
/// matching System Settings pane.
pub fn request(kind: &str) {
    #[cfg(target_os = "macos")]
    {
        let granted = match kind {
            "accessibility" => mac::request_accessibility(),
            "input_monitoring" => mac::request_input_monitoring(),
            "screen_recording" => crate::audio::system_mac::request_permission(),
            "calendar" => {
                crate::titles::request_calendar();
                true
            }
            _ => false,
        };
        if !granted {
            open_settings(kind);
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = kind;
}

pub fn open_settings(kind: &str) {
    #[cfg(target_os = "macos")]
    {
        let pane = match kind {
            "accessibility" => "Privacy_Accessibility",
            "input_monitoring" => "Privacy_ListenEvent",
            "screen_recording" => "Privacy_ScreenCapture",
            "microphone" => "Privacy_Microphone",
            "calendar" => "Privacy_Calendars",
            _ => return,
        };
        let _ = std::process::Command::new("open")
            .arg(format!(
                "x-apple.systempreferences:com.apple.preference.security?{pane}"
            ))
            .spawn();
    }
    #[cfg(target_os = "windows")]
    if kind == "microphone" {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "ms-settings:privacy-microphone"])
            .spawn();
    }
}
