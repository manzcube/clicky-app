use std::{fs, path::PathBuf, sync::Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    /// Any model you've pulled with `ollama pull`.
    pub model: String,
    pub host: String,

    /// The bead trails your cursor around the screen.
    pub follow_cursor: bool,

    /// Watch for text selections and offer to act on them.
    /// Costs you a clipboard round-trip on every selection — see watcher.rs.
    pub watch_selection: bool,

    /// Open the panel the moment you highlight something, instead of waiting
    /// for you to click the bead.
    pub auto_open: bool,

    /// Give the panel your keyboard when it auto-opens. Off by default: a
    /// panel that grabs focus every time you highlight a word will fight you
    /// for every keystroke. Turn it on if you mostly type your questions.
    pub auto_open_focus: bool,

    /// An auto-opened panel you never touch closes itself after this long.
    pub auto_close_ms: u64,

    /// Where the bead sits relative to the cursor, in pixels.
    pub offset_x: i32,
    pub offset_y: i32,

    /// 0.0 = teleports with the cursor, 0.2 = lags behind softly.
    pub follow_easing: f64,

    pub target_language: String,

    /// Leave empty for the default (Cmd/Ctrl + Shift + Space).
    pub hotkey: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: "gemma3:4b".into(),
            host: "http://127.0.0.1:11434".into(),
            follow_cursor: true,
            watch_selection: true,
            auto_open: true,
            auto_open_focus: false,
            auto_close_ms: 14_000,
            offset_x: 22,
            offset_y: 22,
            follow_easing: 0.22,
            target_language: "English".into(),
            hotkey: String::new(),
        }
    }
}

pub struct Store {
    pub inner: Mutex<Settings>,
    pub path: PathBuf,
}

impl Store {
    pub fn load(app: &AppHandle) -> Self {
        let dir = app
            .path()
            .app_config_dir()
            .unwrap_or_else(|_| PathBuf::from("."));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("settings.json");

        let settings = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        let store = Self {
            inner: Mutex::new(settings),
            path,
        };
        store.save();
        store
    }

    pub fn get(&self) -> Settings {
        self.inner.lock().unwrap().clone()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&*self.inner.lock().unwrap()) {
            let _ = fs::write(&self.path, json);
        }
    }
}

/// Env vars win over the file, so you can try a model without editing anything:
///   CLICKY_MODEL=qwen3:4b clicky
pub fn apply_env(s: &mut Settings) {
    if let Ok(m) = std::env::var("CLICKY_MODEL") {
        s.model = m;
    }
    if let Ok(h) = std::env::var("CLICKY_HOST") {
        s.host = h;
    }
}
