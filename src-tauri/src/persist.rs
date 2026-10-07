//! Window position, mode and mute flag persisted across restarts.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct UiState {
    /// Physical screen coordinates of the window's top-left corner.
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub mini: bool,
    pub muted: bool,
    /// Launch at sign-in: `None` until the default was applied or the user chose.
    pub autostart: Option<bool>,
}

impl UiState {
    /// A missing or corrupt file yields the defaults.
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    }

    /// Best-effort save; preferences are not worth an error dialog.
    pub fn save(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, text);
        }
    }
}
