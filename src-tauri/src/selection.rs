//! Reading the text the user has highlighted in *another* application.
//!
//! Two ways to do this:
//!
//!   1. Accessibility APIs (AXSelectedText on macOS, UI Automation on Windows).
//!      Clean, but a surprising number of apps don't implement it — Electron
//!      apps and some browsers return nothing.
//!
//!   2. Send the copy keystroke, read the clipboard, put the clipboard back.
//!      Slightly grubby, works essentially everywhere.
//!
//! This is (2). It's what most tools in this category actually ship.

use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::{thread, time::Duration};

#[cfg(target_os = "macos")]
const MOD: Key = Key::Meta;
#[cfg(not(target_os = "macos"))]
const MOD: Key = Key::Control;

fn chord(c: char) -> Result<(), String> {
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    enigo
        .key(MOD, Direction::Press)
        .map_err(|e| e.to_string())?;
    enigo
        .key(Key::Unicode(c), Direction::Click)
        .map_err(|e| e.to_string())?;
    enigo
        .key(MOD, Direction::Release)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Returns whatever is highlighted in the frontmost app, or "" if nothing is.
///
/// Must run on the main thread on macOS — call it from the shortcut handler,
/// not from inside a spawned task.
pub fn capture() -> String {
    let mut cb = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    let previous = cb.get_text().unwrap_or_default();

    // A sentinel lets us tell "user selected nothing" apart from "user selected
    // the exact text that was already on the clipboard".
    let _ = cb.set_text("\u{0}clicky\u{0}");
    thread::sleep(Duration::from_millis(30));

    if chord('c').is_err() {
        let _ = cb.set_text(previous);
        return String::new();
    }

    // Apps vary a lot in how fast they service a copy. Poll instead of guessing.
    let mut found = String::new();
    for _ in 0..12 {
        thread::sleep(Duration::from_millis(20));
        if let Ok(now) = cb.get_text() {
            if now != "\u{0}clicky\u{0}" {
                found = now;
                break;
            }
        }
    }

    let _ = cb.set_text(previous);
    found.trim().to_string()
}

/// Replaces the user's selection with `text` in whatever app is now frontmost.
///
/// Hide the clicky window *before* calling this, or the paste lands in clicky.
pub fn replace(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    let previous = cb.get_text().unwrap_or_default();

    cb.set_text(text).map_err(|e| e.to_string())?;
    thread::sleep(Duration::from_millis(60));
    chord('v')?;

    // Give the target app time to read the clipboard before we take it back.
    thread::sleep(Duration::from_millis(220));
    let _ = cb.set_text(previous);
    Ok(())
}
