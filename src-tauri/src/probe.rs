//! Operating-system process probe used to drop stale registry entries.

use cpanel_core::registry::ProcessProbe;

pub struct OsProbe;

#[cfg(windows)]
impl ProcessProbe for OsProbe {
    fn is_alive(&self, pid: u32, proc_start: Option<u64>) -> bool {
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ACCESS_DENIED, FILETIME};
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        const STILL_ACTIVE: u32 = 259;
        /// Registry `procStart` matched the creation FILETIME exactly when observed;
        /// one second of slack guards against rounding in other versions.
        const START_TOLERANCE_TICKS: u64 = 10_000_000;

        // SAFETY: plain Win32 calls with valid out-pointers; the handle is closed below.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                // A process we may not inspect still exists.
                return GetLastError() == ERROR_ACCESS_DENIED;
            }
            let mut code = 0u32;
            let mut alive = GetExitCodeProcess(handle, &mut code) != 0 && code == STILL_ACTIVE;
            if alive {
                if let Some(expected) = proc_start {
                    let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
                    let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
                    if GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0 {
                        let actual = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
                        // A different start time means the pid was reused by another process.
                        alive = actual.abs_diff(expected) <= START_TOLERANCE_TICKS;
                    }
                }
            }
            CloseHandle(handle);
            alive
        }
    }
}

#[cfg(not(windows))]
impl ProcessProbe for OsProbe {
    fn is_alive(&self, pid: u32, _proc_start: Option<u64>) -> bool {
        std::path::Path::new("/proc").join(pid.to_string()).exists() || !std::path::Path::new("/proc").exists()
    }
}
