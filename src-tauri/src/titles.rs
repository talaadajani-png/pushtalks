//! Meeting titles without typing: the meeting app's window title (Teams, Google Meet in a
//! browser) or the calendar event happening now. All local; nothing leaves the computer.

/// Window titles that are an app's own screens, not a meeting's name.
const GENERIC: &[&str] = &[
    "",
    "activity",
    "apps",
    "assignments",
    "calendar",
    "calls",
    "chat",
    "communities",
    "files",
    "meet",
    "meeting",
    "meeting compact view",
    "microsoft teams",
    "notifications",
    "people",
    "search",
    "teams",
    "zoom",
    "zoom meeting",
    "zoom workplace",
    "webex",
];

const BROWSER_SUFFIXES: &[&str] = &[
    " - Google Chrome",
    " – Google Chrome",
    " - Microsoft Edge",
    " — Mozilla Firefox",
    " - Mozilla Firefox",
    " - Arc",
    " - Brave",
];

fn generic(t: &str) -> bool {
    GENERIC.contains(&t.trim().to_lowercase().as_str())
}

/// A Google Meet room code such as "abc-defg-hij" is not a name.
fn meet_code(t: &str) -> bool {
    let parts: Vec<&str> = t.split('-').collect();
    parts.len() == 3
        && [3, 4, 3]
            .iter()
            .zip(&parts)
            .all(|(n, p)| p.len() == *n && p.chars().all(|c| c.is_ascii_lowercase()))
}

/// Turns one window's owner app and title into a meeting name, if it names a meeting.
pub fn from_window(owner: &str, title: &str) -> Option<String> {
    let owner_l = owner.to_lowercase();
    let mut t = title.trim().to_string();
    if owner_l.contains("teams") {
        // "Q4 planning | Microsoft Teams", sometimes with a middle part like "| Meeting".
        for suffix in [" | Microsoft Teams", " | Teams"] {
            if let Some(stripped) = t.strip_suffix(suffix) {
                t = stripped.to_string();
            }
        }
        let parts: Vec<&str> = t.split(" | ").map(str::trim).collect();
        // Teams' main window is titled "<section> | <what's open>", e.g. "Chat | PUSH Company
        // Updates": a channel or chat, not a meeting. Only meeting windows are used.
        const SECTIONS: &[&str] = &[
            "chat",
            "teams",
            "calendar",
            "activity",
            "calls",
            "files",
            "apps",
            "communities",
            "people",
            "assignments",
            "notifications",
            "search",
        ];
        if parts.len() > 1 && SECTIONS.contains(&parts[0].to_lowercase().as_str()) {
            return None;
        }
        t = parts
            .into_iter()
            .find(|p| !generic(p))
            .unwrap_or("")
            .to_string();
    } else {
        for s in BROWSER_SUFFIXES {
            if let Some(stripped) = t.strip_suffix(s) {
                t = stripped.to_string();
            }
        }
        // Google Meet tabs: "Meet - Weekly sync" or "Meet – abc-defg-hij".
        let meet = ["Meet - ", "Meet – ", "Meet: "]
            .iter()
            .find_map(|p| t.strip_prefix(p).map(str::to_string));
        t = meet?;
    }
    let t = t.trim();
    (!generic(t) && !meet_code(t) && t.chars().count() >= 3).then(|| t.to_string())
}

/// The first meeting name found among on-screen windows. Window titles are only visible with
/// Screen Recording permission, which meeting recording needs anyway.
#[cfg(target_os = "macos")]
pub fn meeting_window_title() -> Option<String> {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::CFString;
    use core_graphics::window::{
        copy_window_info, kCGNullWindowID, kCGWindowListOptionOnScreenOnly,
    };

    let list = copy_window_info(kCGWindowListOptionOnScreenOnly, kCGNullWindowID)?;
    let key_owner = CFString::from_static_string("kCGWindowOwnerName");
    let key_name = CFString::from_static_string("kCGWindowName");
    for item in list.iter() {
        // SAFETY: CGWindowListCopyWindowInfo returns an array of CFDictionary.
        let dict: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_get_rule(*item as _) };
        let text = |k: &CFString| {
            dict.find(k)
                .and_then(|v| v.downcast::<CFString>())
                .map(|s| s.to_string())
        };
        if let (Some(owner), Some(name)) = (text(&key_owner), text(&key_name)) {
            if let Some(t) = from_window(&owner, &name) {
                return Some(t);
            }
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
pub fn meeting_window_title() -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
mod calendar {
    use objc2_event_kit::{EKAuthorizationStatus, EKEntityType, EKEventStore};
    use objc2_foundation::NSDate;

    pub fn allowed() -> bool {
        // SAFETY: a class method with a plain enum argument.
        let s = unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) };
        s == EKAuthorizationStatus::FullAccess
    }

    pub fn request() {
        // SAFETY: the store is kept alive (leaked) so the async request can finish; the
        // completion block does nothing and the next status check reads the answer.
        unsafe {
            let store = EKEventStore::new();
            let block = block2::RcBlock::new(
                |_: objc2::runtime::Bool, _: *mut objc2_foundation::NSError| {},
            );
            store.requestFullAccessToEventsWithCompletion(&*block as *const _ as *mut _);
            std::mem::forget(block);
            std::mem::forget(store);
        }
    }

    /// The title of the event going on now (or starting within 10 minutes), if any.
    pub fn current_event_title() -> Option<String> {
        if !allowed() {
            return None;
        }
        // SAFETY: plain EventKit queries on a fresh store, on this thread only.
        unsafe {
            let store = EKEventStore::new();
            let from = NSDate::dateWithTimeIntervalSinceNow(-12.0 * 3600.0);
            let to = NSDate::dateWithTimeIntervalSinceNow(10.0 * 60.0);
            let pred = store.predicateForEventsWithStartDate_endDate_calendars(&from, &to, None);
            let events = store.eventsMatchingPredicate(&pred);
            let mut best: Option<(f64, String)> = None;
            for e in events.iter() {
                if e.isAllDay() {
                    continue;
                }
                let start = e.startDate().timeIntervalSinceNow();
                let end = e.endDate().timeIntervalSinceNow();
                // Happening now, or about to start.
                if end < 0.0 || start > 600.0 {
                    continue;
                }
                let title = e.title().to_string();
                if title.trim().is_empty() {
                    continue;
                }
                let distance = start.abs();
                if best.as_ref().is_none_or(|(d, _)| distance < *d) {
                    best = Some((distance, title));
                }
            }
            best.map(|(_, t)| t.trim().to_string())
        }
    }
}

#[cfg(target_os = "macos")]
pub use calendar::{allowed as calendar_allowed, current_event_title, request as request_calendar};

#[cfg(not(target_os = "macos"))]
pub fn calendar_allowed() -> bool {
    false
}
#[cfg(not(target_os = "macos"))]
pub fn request_calendar() {}
#[cfg(not(target_os = "macos"))]
pub fn current_event_title() -> Option<String> {
    None
}

/// Where a title came from, best first.
pub fn detect() -> Option<String> {
    meeting_window_title().or_else(current_event_title)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manual check: `cargo test --lib titles::tests::live -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live() {
        println!("meeting window: {:?}", meeting_window_title());
        println!("calendar allowed: {}", calendar_allowed());
    }

    #[test]
    fn names_from_meeting_windows() {
        assert_eq!(
            from_window("Microsoft Teams", "Q4 planning | Microsoft Teams").as_deref(),
            Some("Q4 planning")
        );
        assert_eq!(
            from_window("MSTeams", "Meeting | Weekly sync | Microsoft Teams").as_deref(),
            Some("Weekly sync")
        );
        assert_eq!(from_window("MSTeams", "Chat | Microsoft Teams"), None);
        assert_eq!(from_window("MSTeams", "Chat | PUSH Company Updates"), None);
        assert_eq!(from_window("MSTeams", "Calendar | Week of Oct 19"), None);
        assert_eq!(
            from_window("MSTeams", "Brand sync").as_deref(),
            Some("Brand sync")
        );
        assert_eq!(from_window("MSTeams", "Calendar | Microsoft Teams"), None);
        assert_eq!(
            from_window("Google Chrome", "Meet - Brand review - Google Chrome").as_deref(),
            Some("Brand review")
        );
        assert_eq!(
            from_window("Google Chrome", "Meet – abc-defg-hij - Google Chrome"),
            None
        );
        assert_eq!(
            from_window("Google Chrome", "Inbox - Gmail - Google Chrome"),
            None
        );
        assert_eq!(from_window("zoom.us", "Zoom Meeting"), None);
    }
}
