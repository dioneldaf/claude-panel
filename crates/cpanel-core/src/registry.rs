//! Adapter for the undocumented Claude Code session registry
//! (`<config dir>/sessions/<pid>.json`).
//!
//! The format is not a public contract, so every field except `pid` and
//! `sessionId` is optional and wrongly typed values are ignored. The same folder
//! holds `<pid>.<hash>.key` secrets: file names are validated *before* any file is
//! opened and only `^\d+\.json$` is ever read.

use serde_json::Value;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Registry files are a few hundred bytes; anything larger is not what we expect.
const MAX_FILE_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct RegistryEntry {
    pub pid: u32,
    pub session_id: String,
    pub cwd: String,
    pub name: Option<String>,
    /// Raw status string (`busy`, `idle`, `shell`, ... ).
    pub status: Option<String>,
    pub status_updated_at: Option<u64>,
    pub updated_at: Option<u64>,
    pub started_at: Option<u64>,
    pub kind: Option<String>,
    /// Process creation time as recorded by Claude Code (Windows FILETIME ticks).
    pub proc_start: Option<u64>,
    /// What a `waiting` session is blocked on, as reported by Claude Code.
    pub waiting_for: Option<String>,
    /// Tag of the config directory this entry was found in.
    pub profile: String,
}

/// Answers whether a registry entry still belongs to a live process.
pub trait ProcessProbe {
    fn is_alive(&self, pid: u32, proc_start: Option<u64>) -> bool;
}

/// A config directory that has a `sessions/` folder.
#[derive(Clone, Debug, PartialEq)]
pub struct ConfigDir {
    /// Directory name without the leading dot, for example `claude-work`.
    pub tag: String,
    pub path: PathBuf,
}

/// True only for `<digits>.json`.
pub fn is_registry_file_name(name: &str) -> bool {
    match name.strip_suffix(".json") {
        Some(stem) => !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_digit()),
        None => false,
    }
}

fn string(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

/// Accepts both JSON numbers and numeric strings.
fn number(v: &Value, key: &str) -> Option<u64> {
    match v.get(key)? {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

/// `waitingFor` was only seen as a key; accept a string or keep any other shape as
/// compact JSON so it can be inspected in the tooltip.
fn waiting_for(v: &Value) -> Option<String> {
    const MAX_CHARS: usize = 120;
    let text = match v.get("waitingFor")? {
        Value::Null => return None,
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    (!text.is_empty()).then(|| text.chars().take(MAX_CHARS).collect())
}

pub fn parse_entry(text: &str, profile: &str) -> Option<RegistryEntry> {
    let v: Value = serde_json::from_str(text).ok()?;
    if !v.is_object() {
        return None;
    }
    Some(RegistryEntry {
        pid: u32::try_from(number(&v, "pid")?).ok()?,
        session_id: string(&v, "sessionId")?,
        cwd: string(&v, "cwd").unwrap_or_default(),
        name: string(&v, "name"),
        status: string(&v, "status"),
        status_updated_at: number(&v, "statusUpdatedAt"),
        updated_at: number(&v, "updatedAt"),
        started_at: number(&v, "startedAt"),
        kind: string(&v, "kind"),
        proc_start: number(&v, "procStart"),
        waiting_for: waiting_for(&v),
        profile: profile.to_string(),
    })
}

fn read_small_file(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut text = String::new();
    file.take(MAX_FILE_BYTES).read_to_string(&mut text).ok()?;
    Some(text)
}

/// Lists the live sessions of one `sessions/` directory, sorted by pid.
/// Unreadable, malformed, mismatching and stale entries are skipped silently.
pub fn scan_sessions_dir(sessions_dir: &Path, profile: &str, probe: &dyn ProcessProbe) -> Vec<RegistryEntry> {
    let Ok(dir) = fs::read_dir(sessions_dir) else {
        return Vec::new();
    };
    let mut entries: Vec<RegistryEntry> = dir
        .flatten()
        .filter_map(|item| {
            let name = item.file_name();
            let name = name.to_str()?;
            if !is_registry_file_name(name) {
                return None;
            }
            let file_pid: u32 = name.strip_suffix(".json")?.parse().ok()?;
            let entry = parse_entry(&read_small_file(&item.path())?, profile)?;
            (entry.pid == file_pid && probe.is_alive(entry.pid, entry.proc_start)).then_some(entry)
        })
        .collect();
    entries.sort_by_key(|e| e.pid);
    entries
}

/// Finds every `~/.claude*` directory that contains a `sessions/` folder.
pub fn discover_config_dirs(home: &Path) -> Vec<ConfigDir> {
    let Ok(dir) = fs::read_dir(home) else {
        return Vec::new();
    };
    let mut found: Vec<ConfigDir> = dir
        .flatten()
        .filter_map(|item| {
            let name = item.file_name();
            let name = name.to_str()?;
            if !name.starts_with(".claude") {
                return None;
            }
            let path = item.path();
            path.join("sessions").is_dir().then(|| ConfigDir { tag: name[1..].to_string(), path })
        })
        .collect();
    found.sort_by(|a, b| a.tag.cmp(&b.tag));
    found
}
