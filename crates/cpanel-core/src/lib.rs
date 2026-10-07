//! Core of Claude Panel.
//!
//! Everything here is free of UI and network code. `reducer` and `notify` are pure
//! (inputs + clock in, state out); `registry` and `settings` touch the filesystem
//! only through narrow, test-covered functions.

pub mod autostart;
pub mod demo;
pub mod eventlog;
pub mod hook_event;
pub mod hookstate;
pub mod model;
pub mod notify;
pub mod presence;
pub mod reducer;
pub mod registry;
pub mod settings;
pub mod textdiff;

/// Product name shown in the tray, command-line help and messages. The window
/// title and installer name come from `productName` in `src-tauri/tauri.conf.json`
/// and the frontend copy from `src/branding.ts`; a test keeps the three in sync.
pub const PRODUCT_NAME: &str = "Claude Panel";

/// Bundle identifier; names the per-user data folders. Must equal `identifier`
/// in `src-tauri/tauri.conf.json` (checked by scripts/branding.test.mjs).
pub const APP_IDENTIFIER: &str = "io.github.dioneldaf.claude-panel";

/// Loopback UDP port the hook binary sends summaries to.
pub const DEFAULT_PORT: u16 = 47615;
/// Environment variable overriding [`DEFAULT_PORT`] for both the panel and the hook.
pub const PORT_ENV: &str = "CPANEL_PORT";

/// Resolves the UDP port from the environment, falling back to the default.
pub fn resolve_port(env_value: Option<&str>) -> u16 {
    env_value.and_then(|v| v.trim().parse::<u16>().ok()).filter(|p| *p != 0).unwrap_or(DEFAULT_PORT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_resolution_is_defensive() {
        assert_eq!(resolve_port(None), DEFAULT_PORT);
        assert_eq!(resolve_port(Some("")), DEFAULT_PORT);
        assert_eq!(resolve_port(Some("abc")), DEFAULT_PORT);
        assert_eq!(resolve_port(Some("0")), DEFAULT_PORT);
        assert_eq!(resolve_port(Some(" 5000 ")), 5000);
    }
}
