//! Text insertion (PRD §6.1): put the text on the clipboard, simulate paste, then restore what
//! the user had on the clipboard before.

use std::thread::sleep;
use std::time::Duration;

use anyhow::{anyhow, Result};
use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

/// Time for the target app to read the clipboard before it's restored.
const RESTORE_AFTER: Duration = Duration::from_millis(400);

enum Saved {
    Text(String),
    Image(arboard::ImageData<'static>),
    Nothing,
}

pub fn paste(text: &str) -> Result<()> {
    let mut cb = Clipboard::new().map_err(|e| anyhow!("clipboard unavailable: {e}"))?;
    let saved = if let Ok(t) = cb.get_text() {
        Saved::Text(t)
    } else if let Ok(img) = cb.get_image() {
        Saved::Image(img.to_owned_img())
    } else {
        Saved::Nothing
    };
    cb.set_text(text.to_string())
        .map_err(|e| anyhow!("couldn't set the clipboard: {e}"))?;
    sleep(Duration::from_millis(40));
    press_paste()?;
    sleep(RESTORE_AFTER);
    // Only restore if nothing else replaced the clipboard in the meantime.
    if cb.get_text().ok().as_deref() == Some(text) {
        let _ = match saved {
            Saved::Text(t) => cb.set_text(t),
            Saved::Image(img) => cb.set_image(img),
            Saved::Nothing => cb.clear(),
        };
    }
    Ok(())
}

fn press_paste() -> Result<()> {
    let mut en =
        Enigo::new(&Settings::default()).map_err(|e| anyhow!("can't simulate keys: {e}"))?;
    // Physical V key codes avoid keyboard-layout lookups (which must run on the main thread on macOS).
    #[cfg(target_os = "macos")]
    let (modifier, v) = (Key::Meta, Key::Other(9));
    #[cfg(not(target_os = "macos"))]
    let (modifier, v) = (Key::Control, Key::Other(0x56));
    en.key(modifier, Direction::Press)?;
    let r = en.key(v, Direction::Click);
    en.key(modifier, Direction::Release)?;
    r?;
    Ok(())
}
