//! Window position, mode, mute flag and sound preference persisted across restarts.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct UiState {
    /// Physical screen coordinates of the window's top-left corner.
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub mini: bool,
    pub muted: bool,
    /// Launch at sign-in: `None` until the default was applied or the user chose.
    pub autostart: Option<bool>,
    /// Play the notification sound with each toast. Muting silences both.
    pub sound: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self { x: None, y: None, mini: false, muted: false, autostart: None, sound: true }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_sound_defaults_to_on_for_new_and_older_files() {
        assert!(UiState::default().sound);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ui-state.json");
        assert!(UiState::load(&path).sound, "missing file");
        fs::write(&path, r#"{"x":10,"y":20,"mini":true,"muted":false}"#).unwrap();
        let older = UiState::load(&path);
        assert!(older.sound && older.mini && older.x == Some(10));
    }

    #[test]
    fn notification_sound_preference_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("ui-state.json");
        let state = UiState { sound: false, muted: true, ..UiState::default() };
        state.save(&path);
        assert_eq!(UiState::load(&path), state);
    }
}
