//! Record of the settings files the user installed hooks into.
//!
//! It exists so hook entries survive things the user did not ask for: an upgrade
//! that runs the previous uninstaller, a cancelled uninstall, or a moved
//! installation. The uninstaller removes this installation's entries and marks the
//! records for restoration; the application restores them the next time it starts.
//! A completed uninstall deletes the file, so nothing can come back afterwards.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// File name inside the application's configuration directory.
pub const STATE_FILE: &str = "hooks-state.json";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Record {
    /// The settings file, as it was addressed when the hooks were installed.
    pub file: String,
    /// Profile tag written into the hook arguments.
    pub profile: String,
    /// The hook executable the entries point at (identifies the owning copy).
    pub exe: String,
    /// Set by the uninstaller after it removed the entries: restore on next start.
    #[serde(default)]
    pub restore: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct HookState {
    #[serde(default)]
    pub managed: Vec<Record>,
}

/// What this installation's entries in a settings file look like right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entries {
    /// None of our entries are present.
    Absent,
    /// Entries exist, and at least one points at an executable that is gone.
    Dangling,
    Valid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Install the entries again, pointing at this copy.
    Repair,
    /// Drop the record: the user removed the entries on purpose.
    Forget,
    Keep,
}

fn normalize(path: &str) -> String {
    let path = path.strip_prefix(r"\\?\").unwrap_or(path);
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

/// Path equality as Windows sees it: case-insensitive, either slash direction.
pub fn same_path(a: &str, b: &str) -> bool {
    normalize(a) == normalize(b)
}

/// Decides what to do with one record when the application starts.
///
/// `record_exe_exists` says whether the executable the record points at is still
/// on disk. A record belongs to this copy when it names this copy's hook
/// executable, or when its own executable is gone (the installation moved).
pub fn decide(record: &Record, my_exe: &str, record_exe_exists: bool, entries: Entries) -> Decision {
    let mine = same_path(&record.exe, my_exe) || !record_exe_exists;
    if !mine {
        return Decision::Keep;
    }
    match entries {
        Entries::Dangling => Decision::Repair,
        Entries::Absent if record.restore => Decision::Repair,
        // Absent without our uninstaller having removed them: a deliberate edit.
        Entries::Absent => Decision::Forget,
        Entries::Valid => Decision::Keep,
    }
}

impl HookState {
    /// A missing or damaged file is an empty state.
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    }

    /// Best-effort save. An empty state removes the file instead of writing it.
    pub fn save(&self, path: &Path) {
        if self.managed.is_empty() {
            let _ = fs::remove_file(path);
            return;
        }
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, text);
        }
    }

    fn find(&mut self, file: &str) -> Option<&mut Record> {
        self.managed.iter_mut().find(|r| same_path(&r.file, file))
    }

    /// The user installed hooks into `file` with this copy.
    pub fn record_install(&mut self, file: &str, profile: &str, exe: &str) {
        let record = Record { file: file.to_string(), profile: profile.to_string(), exe: exe.to_string(), restore: false };
        match self.find(file) {
            Some(existing) => *existing = Record { file: existing.file.clone(), ..record },
            None => self.managed.push(record),
        }
    }

    /// The user removed the hooks from `file`.
    pub fn forget(&mut self, file: &str) {
        self.managed.retain(|r| !same_path(&r.file, file));
    }

    /// The uninstaller removed this copy's entries from `file`.
    pub fn mark_restore(&mut self, file: &str, profile: &str, exe: &str) {
        self.mark_restore_if_owned(file, profile, exe, |_| true);
    }

    /// Like [`mark_restore`], but leaves a record alone unless `owned` accepts the
    /// executable it currently names (records of other copies are not ours to change).
    pub fn mark_restore_if_owned(&mut self, file: &str, profile: &str, exe: &str, owned: impl Fn(&str) -> bool) {
        match self.find(file) {
            Some(existing) if owned(&existing.exe) => {
                existing.exe = exe.to_string();
                existing.restore = true;
            }
            Some(_) => {}
            None => self.managed.push(Record {
                file: file.to_string(),
                profile: profile.to_string(),
                exe: exe.to_string(),
                restore: true,
            }),
        }
    }
}
