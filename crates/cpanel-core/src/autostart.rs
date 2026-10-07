//! Launch-at-sign-in policy. Pure decisions; the registry access lives in the
//! application crate.
//!
//! The entry is a per-user value under the `Run` key (no administrator rights, no
//! scheduled task, no service). It is enabled by default only for an installed
//! build, corrected when the installation moves, and never re-enabled once the
//! user has turned it off.

use std::path::Path;

/// Per-user registry key (under `HKEY_CURRENT_USER`) holding sign-in commands.
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Marks a launch made by Windows at sign-in, as opposed to one made by the user.
pub const AUTOSTART_FLAG: &str = "--autostart";
/// File the NSIS installer places next to the application.
const UNINSTALLER: &str = "uninstall.exe";

#[derive(Clone, Debug, PartialEq)]
pub enum RunAction {
    None,
    /// Write this command line as the entry.
    Write(String),
    Remove,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub action: RunAction,
    /// Preference to persist: `None` until a default was applied or the user chose.
    pub preference: Option<bool>,
}

fn plain(exe: &Path) -> String {
    let text = exe.to_string_lossy();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
}

/// The command line stored in the registry for `exe`.
pub fn command_line(exe: &Path) -> String {
    format!("\"{}\" {AUTOSTART_FLAG}", plain(exe))
}

/// Whether the existing entry launches exactly this executable.
pub fn is_enabled(entry: Option<&str>, exe: &Path) -> bool {
    entry.is_some_and(|value| value.eq_ignore_ascii_case(&command_line(exe)))
}

/// True for a copy placed by the installer: the uninstaller sits next to it and it
/// is not inside a Cargo `target` directory. Portable and development builds are
/// not "installed" and never register themselves by default.
pub fn is_installed_build(exe: &Path) -> bool {
    let in_target = exe.components().any(|c| c.as_os_str().eq_ignore_ascii_case("target"));
    !in_target && exe.parent().is_some_and(|dir| dir.join(UNINSTALLER).is_file())
}

/// What to do when the application starts.
pub fn startup_plan(preference: Option<bool>, installed: bool, entry: Option<&str>, exe: &Path) -> Plan {
    let action = match (preference, installed) {
        // First run of an installed build: on by default.
        (None, true) => RunAction::Write(command_line(exe)),
        // Wanted, but missing or pointing at a previous location: correct it.
        (Some(true), true) if !is_enabled(entry, exe) => RunAction::Write(command_line(exe)),
        _ => RunAction::None,
    };
    let preference = if preference.is_none() && installed { Some(true) } else { preference };
    Plan { action, preference }
}

/// What to do when the user ticks or clears the tray checkbox.
pub fn toggle_plan(enable: bool, exe: &Path) -> Plan {
    let action = if enable { RunAction::Write(command_line(exe)) } else { RunAction::Remove };
    Plan { action, preference: Some(enable) }
}
