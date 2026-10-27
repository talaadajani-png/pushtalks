//! Hold-to-talk key (PRD §6.1): Right Option on macOS, Right Alt on Windows.
//! The global-shortcut plugin only reports key combinations, so a lone modifier needs an OS hook:
//! a listen-only CGEventTap on macOS (Input Monitoring permission) and a low-level keyboard hook
//! on Windows. Pressing any other key while holding it cancels, so Option/Alt shortcuts still work.

use crossbeam_channel::Sender;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldEvent {
    Down,
    Up,
    /// Another key was pressed while holding: it's a shortcut, not dictation.
    Cancel,
}

#[derive(Default)]
struct Tracker {
    held: bool,
    chorded: bool,
}

impl Tracker {
    fn on_hold_key(&mut self, down: bool) -> Option<HoldEvent> {
        match (down, self.held) {
            (true, false) => {
                self.held = true;
                self.chorded = false;
                Some(HoldEvent::Down)
            }
            (false, true) => {
                self.held = false;
                (!self.chorded).then_some(HoldEvent::Up)
            }
            _ => None,
        }
    }

    fn on_other_key(&mut self) -> Option<HoldEvent> {
        if self.held && !self.chorded {
            self.chorded = true;
            Some(HoldEvent::Cancel)
        } else {
            None
        }
    }
}

#[cfg(target_os = "macos")]
pub use mac::listen;
#[cfg(target_os = "windows")]
pub use win::listen;

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
    use core_graphics::event::{
        CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
        CallbackResult, EventField,
    };
    use std::sync::Mutex;

    const RIGHT_OPTION: i64 = 61;
    /// NX_DEVICERALTKEYMASK: set while the right Option key specifically is down.
    const RIGHT_OPTION_MASK: u64 = 0x40;

    /// Starts listening on a background thread. Returns false if the tap couldn't be created,
    /// which means Input Monitoring permission is missing.
    pub fn listen(tx: Sender<HoldEvent>) -> bool {
        let (ok_tx, ok_rx) = crossbeam_channel::bounded(1);
        let spawned = std::thread::Builder::new()
            .name("hold-key".into())
            .spawn(move || {
                let tracker = Mutex::new(Tracker::default());
                let tap = CGEventTap::new(
                    CGEventTapLocation::HID,
                    CGEventTapPlacement::HeadInsertEventTap,
                    CGEventTapOptions::ListenOnly,
                    vec![CGEventType::FlagsChanged, CGEventType::KeyDown],
                    move |_, kind, event| {
                        let Ok(mut t) = tracker.lock() else {
                            return CallbackResult::Keep;
                        };
                        let ev = match kind {
                            CGEventType::FlagsChanged => {
                                let code = event
                                    .get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                                if code == RIGHT_OPTION {
                                    t.on_hold_key(event.get_flags().bits() & RIGHT_OPTION_MASK != 0)
                                } else {
                                    t.on_other_key()
                                }
                            }
                            CGEventType::KeyDown => t.on_other_key(),
                            _ => None,
                        };
                        if let Some(ev) = ev {
                            let _ = tx.send(ev);
                        }
                        CallbackResult::Keep
                    },
                );
                let Ok(tap) = tap else {
                    let _ = ok_tx.send(false);
                    return;
                };
                let Ok(source) = tap.mach_port().create_runloop_source(0) else {
                    let _ = ok_tx.send(false);
                    return;
                };
                // SAFETY: kCFRunLoopCommonModes is an immutable CoreFoundation constant.
                CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
                tap.enable();
                let _ = ok_tx.send(true);
                CFRunLoop::run_current();
            });
        spawned.is_ok() && ok_rx.recv().unwrap_or(false)
    }
}

#[cfg(target_os = "windows")]
mod win {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_RMENU;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, SetWindowsHookExW, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL,
        WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    static STATE: OnceLock<(Sender<HoldEvent>, Mutex<Tracker>)> = OnceLock::new();

    unsafe extern "system" fn hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            if let Some((tx, tracker)) = STATE.get() {
                // SAFETY: for WH_KEYBOARD_LL, lparam points to a KBDLLHOOKSTRUCT.
                let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
                let msg = wparam.0 as u32;
                let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
                let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
                if let Ok(mut t) = tracker.lock() {
                    let ev = if info.vkCode == VK_RMENU.0 as u32 {
                        if down {
                            t.on_hold_key(true)
                        } else if up {
                            t.on_hold_key(false)
                        } else {
                            None
                        }
                    } else if down {
                        t.on_other_key()
                    } else {
                        None
                    };
                    if let Some(ev) = ev {
                        let _ = tx.send(ev);
                    }
                }
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    pub fn listen(tx: Sender<HoldEvent>) -> bool {
        if STATE.set((tx, Mutex::new(Tracker::default()))).is_err() {
            return true;
        }
        let (ok_tx, ok_rx) = crossbeam_channel::bounded(1);
        let spawned = std::thread::Builder::new()
            .name("hold-key".into())
            .spawn(move || unsafe {
                let module = GetModuleHandleW(None).ok();
                let installed =
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), module.map(Into::into), 0);
                let _ = ok_tx.send(installed.is_ok());
                if installed.is_err() {
                    return;
                }
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
            });
        spawned.is_ok() && ok_rx.recv().unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_release_and_chord_cancel() {
        let mut t = Tracker::default();
        assert_eq!(t.on_other_key(), None);
        assert_eq!(t.on_hold_key(true), Some(HoldEvent::Down));
        assert_eq!(t.on_hold_key(true), None);
        assert_eq!(t.on_hold_key(false), Some(HoldEvent::Up));
        assert_eq!(t.on_hold_key(true), Some(HoldEvent::Down));
        assert_eq!(t.on_other_key(), Some(HoldEvent::Cancel));
        assert_eq!(t.on_other_key(), None);
        assert_eq!(t.on_hold_key(false), None);
    }
}
