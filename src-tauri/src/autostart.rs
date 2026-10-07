//! Registry adapter for the launch-at-sign-in entry. The policy (when to write,
//! correct or leave it) is `cpanel_core::autostart`; this file only reads and
//! writes one per-user string value.

use cpanel_core::autostart::{is_enabled, is_installed_build, startup_plan, toggle_plan, Plan, RunAction, RUN_KEY};
use std::path::PathBuf;

/// Name of the value under the `Run` key. The uninstaller deletes the same name.
pub fn value_name() -> &'static str {
    cpanel_core::PRODUCT_NAME
}

#[cfg(windows)]
mod registry {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Reads a string value under `HKEY_CURRENT_USER\<key>`.
    pub fn read(key: &str, name: &str) -> Option<String> {
        let (key, name) = (wide(key), wide(name));
        let mut buffer = vec![0u16; 2048];
        let mut bytes = (buffer.len() * 2) as u32;
        // SAFETY: all pointers refer to live, correctly sized buffers.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let len = (bytes as usize / 2).min(buffer.len());
        let text = String::from_utf16_lossy(&buffer[..len]);
        Some(text.trim_end_matches('\0').to_string())
    }

    /// Creates or replaces a string value (the key is created when missing).
    pub fn write(key: &str, name: &str, value: &str) -> bool {
        let (key, name, value) = (wide(key), wide(name), wide(value));
        // SAFETY: `value` is a NUL-terminated UTF-16 buffer of the stated byte length.
        let status = unsafe {
            RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), REG_SZ, value.as_ptr().cast(), (value.len() * 2) as u32)
        };
        status == ERROR_SUCCESS
    }

    /// Deletes a value; a value that does not exist counts as removed.
    pub fn remove(key: &str, name: &str) -> bool {
        let (wide_key, wide_name) = (wide(key), wide(name));
        // SAFETY: both strings are NUL-terminated.
        let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, wide_key.as_ptr(), wide_name.as_ptr()) };
        status == ERROR_SUCCESS || read(key, name).is_none()
    }
}

#[cfg(not(windows))]
mod registry {
    pub fn read(_key: &str, _name: &str) -> Option<String> {
        None
    }
    pub fn write(_key: &str, _name: &str, _value: &str) -> bool {
        false
    }
    pub fn remove(_key: &str, _name: &str) -> bool {
        true
    }
}

fn current_exe() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

fn apply(plan: &Plan) {
    match &plan.action {
        RunAction::Write(command) => {
            registry::write(RUN_KEY, value_name(), command);
        }
        RunAction::Remove => {
            registry::remove(RUN_KEY, value_name());
        }
        RunAction::None => {}
    }
}

/// Whether the entry currently launches this executable (the real state shown in the tray).
pub fn enabled() -> bool {
    current_exe().is_some_and(|exe| is_enabled(registry::read(RUN_KEY, value_name()).as_deref(), &exe))
}

/// Applies the start-up policy and returns the preference to persist.
pub fn reconcile_at_startup(preference: Option<bool>) -> Option<bool> {
    let Some(exe) = current_exe() else {
        return preference;
    };
    let entry = registry::read(RUN_KEY, value_name());
    let plan = startup_plan(preference, is_installed_build(&exe), entry.as_deref(), &exe);
    apply(&plan);
    plan.preference
}

/// Applies the user's explicit choice and returns the preference to persist.
pub fn set(enable: bool) -> Option<bool> {
    let Some(exe) = current_exe() else {
        return Some(enable);
    };
    let plan = toggle_plan(enable, &exe);
    apply(&plan);
    plan.preference
}

#[cfg(all(test, windows))]
mod tests {
    use super::registry::{read, remove, write};

    /// Exercises the registry calls on a throwaway key of our own, never on `Run`.
    #[test]
    fn string_values_round_trip_under_a_private_test_key() {
        let key = format!(r"Software\ClaudePanelSelfTest-{}", std::process::id());
        let name = "Claude Panel";
        let value = r#""C:\Program Files\Claude Panel\claude-panel.exe" --autostart"#;
        assert_eq!(read(&key, name), None);
        assert!(write(&key, name, value));
        assert_eq!(read(&key, name).as_deref(), Some(value));
        assert!(write(&key, name, "replaced"));
        assert_eq!(read(&key, name).as_deref(), Some("replaced"));
        assert!(remove(&key, name));
        assert_eq!(read(&key, name), None);
        assert!(remove(&key, name), "removing a missing value is not an error");

        // Leave no trace: delete the now empty key.
        let wide: Vec<u16> = key.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: NUL-terminated key name.
        unsafe {
            windows_sys::Win32::System::Registry::RegDeleteKeyW(
                windows_sys::Win32::System::Registry::HKEY_CURRENT_USER,
                wide.as_ptr(),
            );
        }
    }
}
