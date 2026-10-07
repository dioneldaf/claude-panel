//! Small rotating log of received hook datagrams, used to tune the event mapping.
//! Logging is best-effort: every failure is swallowed.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

pub struct EventLog {
    path: PathBuf,
    max_bytes: u64,
}

impl EventLog {
    /// `max_bytes` is the size at which the file is rotated to `<name>.1`.
    pub fn new(path: PathBuf, max_bytes: u64) -> Self {
        Self { path, max_bytes }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Appends one line, rotating first when the file reached the limit.
    pub fn append(&self, line: &str) {
        let _ = self.try_append(line);
    }

    fn try_append(&self, line: &str) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        if fs::metadata(&self.path).map(|m| m.len() >= self.max_bytes).unwrap_or(false) {
            let mut rotated = self.path.clone().into_os_string();
            rotated.push(".1");
            let _ = fs::remove_file(&rotated);
            fs::rename(&self.path, &rotated)?;
        }
        let mut file = fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
        let mut record = line.replace(['\r', '\n'], " ");
        record.push('\n');
        file.write_all(record.as_bytes())
    }
}
