//! Call detection: notices when another app starts using the microphone (a Teams, Zoom or Meet
//! call starting) so PushTalks can offer to record. It never records on its own: starting a
//! recording still needs the user's consent confirmation (PRD §10).

/// Meeting apps PushTalks can name, matched against running process names (lowercase).
const MEETING_APPS: &[(&str, &str)] = &[
    ("msteams", "Microsoft Teams"),
    ("microsoft teams", "Microsoft Teams"),
    ("teams.exe", "Microsoft Teams"),
    ("zoom.us", "Zoom"),
    ("zoom.exe", "Zoom"),
    ("webex", "Webex"),
    ("slack", "Slack"),
    ("facetime", "FaceTime"),
    ("google chrome", "a browser (Google Meet?)"),
    ("chrome.exe", "a browser (Google Meet?)"),
    ("msedge", "a browser (Google Meet?)"),
    ("microsoft edge", "a browser (Google Meet?)"),
    ("arc", "a browser (Google Meet?)"),
    ("safari", "a browser (Google Meet?)"),
    ("firefox", "a browser (Google Meet?)"),
];

/// Picks the most likely meeting app from running process names. Dedicated meeting apps win
/// over browsers.
pub fn likely_app(processes: &[String]) -> Option<&'static str> {
    let lower: Vec<String> = processes.iter().map(|p| p.to_lowercase()).collect();
    let mut browser = None;
    for (needle, name) in MEETING_APPS {
        let hit = lower.iter().any(|p| {
            let base = p.rsplit(['/', '\\']).next().unwrap_or(p);
            base == *needle || (needle.len() > 4 && base.starts_with(needle))
        });
        if hit {
            if name.starts_with("a browser") {
                browser.get_or_insert(*name);
            } else {
                return Some(name);
            }
        }
    }
    browser
}

/// Running process names (best effort; empty if they can't be listed).
pub fn processes() -> Vec<String> {
    #[cfg(unix)]
    let out = std::process::Command::new("ps")
        .args(["-axco", "comm="])
        .output();
    #[cfg(windows)]
    let out = std::process::Command::new("tasklist")
        .args(["/fo", "csv", "/nh"])
        .output();
    out.map(|o| {
        String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| {
                l.split(',')
                    .next()
                    .unwrap_or(l)
                    .trim()
                    .trim_matches('"')
                    .to_string()
            })
            .filter(|l| !l.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

/// True while any app is using the default microphone.
#[cfg(target_os = "macos")]
pub fn mic_in_use() -> bool {
    mac::default_input_running()
}

/// True while any app is using a microphone. Windows records this in the privacy consent
/// store: an app whose `LastUsedTimeStop` is 0 is using the mic right now.
#[cfg(target_os = "windows")]
pub fn mic_in_use() -> bool {
    win::mic_in_use()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn mic_in_use() -> bool {
    false
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;

    #[repr(C)]
    struct PropertyAddress {
        selector: u32,
        scope: u32,
        element: u32,
    }

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectGetPropertyData(
            object: u32,
            address: *const PropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            data_size: *mut u32,
            data: *mut c_void,
        ) -> i32;
    }

    const fn code(s: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*s)
    }

    const SYSTEM_OBJECT: u32 = 1;
    const DEFAULT_INPUT_DEVICE: u32 = code(b"dIn ");
    const RUNNING_SOMEWHERE: u32 = code(b"gone");
    const SCOPE_GLOBAL: u32 = code(b"glob");

    fn get_u32(object: u32, selector: u32) -> Option<u32> {
        let addr = PropertyAddress {
            selector,
            scope: SCOPE_GLOBAL,
            element: 0,
        };
        let mut value: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: valid address struct and a 4-byte out buffer whose size we pass in.
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                &addr,
                0,
                std::ptr::null(),
                &mut size,
                (&mut value as *mut u32).cast(),
            )
        };
        (status == 0).then_some(value)
    }

    pub fn default_input_running() -> bool {
        get_u32(SYSTEM_OBJECT, DEFAULT_INPUT_DEVICE)
            .filter(|d| *d != 0)
            .and_then(|d| get_u32(d, RUNNING_SOMEWHERE))
            .is_some_and(|v| v != 0)
    }
}

#[cfg(target_os = "windows")]
mod win {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
        RRF_RT_REG_QWORD,
    };

    const STORE: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

    /// Any subkey (an app) under `path` whose LastUsedTimeStop is 0, checking one level of
    /// nesting for the "NonPackaged" group of desktop apps.
    fn any_active(root: HKEY, path: &str, depth: u32) -> bool {
        let mut key = HKEY::default();
        // SAFETY: opening a registry key for reading; closed below.
        if unsafe { RegOpenKeyExW(root, &HSTRING::from(path), Some(0), KEY_READ, &mut key) }
            .is_err()
        {
            return false;
        }
        let mut found = false;
        let mut i = 0;
        loop {
            let mut name = [0u16; 512];
            let mut len = name.len() as u32;
            // SAFETY: name buffer and its length are valid for the call.
            let r = unsafe {
                RegEnumKeyExW(
                    key,
                    i,
                    Some(windows::core::PWSTR(name.as_mut_ptr())),
                    &mut len,
                    None,
                    None,
                    None,
                    None,
                )
            };
            if r.is_err() {
                break;
            }
            i += 1;
            let sub = String::from_utf16_lossy(&name[..len as usize]);
            let child = format!("{path}\\{sub}");
            let mut stop: u64 = 1;
            let mut size = 8u32;
            // SAFETY: reading a QWORD into an 8-byte buffer.
            let ok = unsafe {
                RegGetValueW(
                    root,
                    &HSTRING::from(child.as_str()),
                    &HSTRING::from("LastUsedTimeStop"),
                    RRF_RT_REG_QWORD,
                    None,
                    Some((&mut stop as *mut u64).cast()),
                    Some(&mut size),
                )
            }
            .is_ok();
            let is_self = sub.to_lowercase().contains("pushtalks");
            if (ok && stop == 0 && !is_self)
                || (depth > 0 && sub == "NonPackaged" && any_active(root, &child, depth - 1))
            {
                found = true;
                break;
            }
        }
        // SAFETY: key was opened above.
        unsafe {
            let _ = RegCloseKey(key);
        }
        found
    }

    pub fn mic_in_use() -> bool {
        any_active(HKEY_CURRENT_USER, STORE, 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manual check: `cargo test --lib mic_state -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn mic_state() {
        println!("mic in use: {}", mic_in_use());
        println!("likely app: {:?}", likely_app(&processes()));
    }

    #[test]
    fn prefers_meeting_apps_over_browsers() {
        let p = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            likely_app(&p(&["Google Chrome", "zoom.us", "Finder"])),
            Some("Zoom")
        );
        assert_eq!(
            likely_app(&p(&[
                "/Applications/Microsoft Teams.app/Contents/MacOS/MSTeams"
            ])),
            Some("Microsoft Teams")
        );
        assert_eq!(
            likely_app(&p(&["Safari"])),
            Some("a browser (Google Meet?)")
        );
        assert_eq!(likely_app(&p(&["Finder", "Archive Utility"])), None);
        assert_eq!(likely_app(&p(&["Teams.exe"])), Some("Microsoft Teams"));
    }
}
