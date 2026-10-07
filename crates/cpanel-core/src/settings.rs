//! Idempotent install / removal of the panel's hook entries in a Claude Code
//! `settings.json`.
//!
//! Guarantees:
//! * existing content, unknown keys and key order are preserved;
//! * our entries are recognised by the hook executable's file name, so removal is
//!   exact and reinstalling from another location updates in place;
//! * a file that is not a JSON object with the expected shape is never modified;
//! * files are written atomically (temp file + rename) after a timestamped backup;
//! * when nothing needs to change, nothing is written.

use serde::Serialize;
use serde_json::{json, Map, Value};
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Hook events the panel subscribes to.
pub const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "PermissionDenied",
    "Elicitation",
    "ElicitationResult",
    "SubagentStart",
    "SubagentStop",
    "Stop",
    "StopFailure",
    "Notification",
    "TaskCreated",
    "TaskCompleted",
    "PreCompact",
    "PostCompact",
];

/// File stem that identifies our hook executable inside a settings file.
pub const HOOK_EXE_STEM: &str = "cpanel-hook";

/// The handler to install: executable path and the profile tag passed to it.
#[derive(Clone, Debug, PartialEq)]
pub struct HookCommand {
    /// Absolute path of `cpanel-hook`, with forward slashes.
    pub exe: String,
    pub profile: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallStatus {
    NotInstalled,
    /// Some events are missing, or entries point to another executable or profile.
    Partial,
    Installed,
}

impl InstallStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            InstallStatus::NotInstalled => "not_installed",
            InstallStatus::Partial => "partial",
            InstallStatus::Installed => "installed",
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum SettingsError {
    /// The file is not valid JSON (comments are not supported).
    Parse(String),
    /// Valid JSON with an unexpected structure.
    Shape(String),
    Io(String),
    /// The file is something we will not rewrite (for example a hard link).
    Refused(String),
    /// The file changed on disk while the action was running; nothing was written.
    Changed(String),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingsError::Parse(m) => write!(f, "settings file is not valid JSON: {m}"),
            SettingsError::Shape(m) => write!(f, "settings file has an unexpected structure: {m}"),
            SettingsError::Io(m) => write!(f, "file operation failed: {m}"),
            SettingsError::Refused(m) => write!(f, "refused: {m}"),
            SettingsError::Changed(m) => write!(f, "file changed, retry: {m}"),
        }
    }
}

impl std::error::Error for SettingsError {}

#[derive(Debug, PartialEq)]
pub struct FileOutcome {
    pub changed: bool,
    pub backup: Option<PathBuf>,
}

/// The file that is actually read and replaced: the target of a symbolic link, or
/// the path itself for a regular or missing file. Replacing the link by rename
/// would silently turn it into a regular file and leave the real one untouched.
pub fn resolve_settings_file(path: &Path) -> PathBuf {
    let is_link = fs::symlink_metadata(path).map(|m| m.file_type().is_symlink()).unwrap_or(false);
    if is_link {
        fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    }
}

/// Number of directory entries (hard links) pointing at the file.
fn link_count(path: &Path) -> io::Result<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(fs::metadata(path)?.nlink())
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION};
        let file = fs::File::open(path)?;
        // SAFETY: the handle is valid for the lifetime of `file`; `info` is a valid out-pointer.
        unsafe {
            let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
            if GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(u64::from(info.nNumberOfLinks))
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        Ok(1)
    }
}

// ---------- formatting preservation ----------

struct Format {
    bom: bool,
    crlf: bool,
    indent: String,
    trailing: String,
}

impl Format {
    fn detect(text: &str) -> Self {
        let body = text.trim_start_matches('\u{feff}');
        let indent = body
            .lines()
            .skip(1)
            .find_map(|line| {
                let ws: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                (!ws.is_empty() && line[ws.len()..].starts_with('"')).then_some(ws)
            })
            .unwrap_or_else(|| "  ".to_string());
        Self {
            bom: text.starts_with('\u{feff}'),
            crlf: body.contains("\r\n"),
            indent,
            trailing: body[body.trim_end().len()..].to_string(),
        }
    }

    fn render(&self, value: &Value) -> String {
        let mut out = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(self.indent.as_bytes());
        let mut ser = serde_json::Serializer::with_formatter(&mut out, formatter);
        value.serialize(&mut ser).expect("serialising a JSON value to memory cannot fail");
        let mut body = String::from_utf8(out).expect("serde_json emits UTF-8");
        if self.crlf {
            // Raw newlines never occur inside JSON strings, so this only touches layout.
            body = body.replace('\n', "\r\n");
        }
        format!("{}{}{}", if self.bom { "\u{feff}" } else { "" }, body, self.trailing)
    }
}

fn parse(text: &str) -> Result<Value, SettingsError> {
    let value: Value =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| SettingsError::Parse(e.to_string()))?;
    if !value.is_object() {
        return Err(SettingsError::Shape("top level is not an object".into()));
    }
    Ok(value)
}

// ---------- entry identification ----------

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").to_lowercase()
}

/// A handler is ours when its `command` is a path whose file name is the hook executable.
fn is_ours(handler: &Value) -> bool {
    let Some(command) = handler.get("command").and_then(Value::as_str) else {
        return false;
    };
    let normalized = normalize_path(command);
    let file = normalized.rsplit('/').next().unwrap_or("");
    file == HOOK_EXE_STEM || file.strip_suffix(".exe") == Some(HOOK_EXE_STEM)
}

fn desired_handler(cmd: &HookCommand) -> Value {
    json!({
        "type": "command",
        "command": cmd.exe,
        "args": ["--profile", cmd.profile],
        "async": true,
        "timeout": 5
    })
}

fn handlers_of(group: &Value) -> Option<&Vec<Value>> {
    group.get("hooks").and_then(Value::as_array)
}

// ---------- pure transformations ----------

/// Returns the settings text with our handler present exactly once per event.
/// Returns the input unchanged when it is already up to date.
pub fn install_hooks(text: &str, cmd: &HookCommand) -> Result<String, SettingsError> {
    let original = parse(text)?;
    let mut root = original.clone();
    let wanted = desired_handler(cmd);

    let hooks = root
        .as_object_mut()
        .expect("checked by parse")
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| SettingsError::Shape("\"hooks\" is not an object".into()))?;

    for event in HOOK_EVENTS {
        let groups = hooks
            .entry(*event)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| SettingsError::Shape(format!("\"hooks.{event}\" is not an array")))?;

        let mut placed = false;
        groups.retain_mut(|group| {
            let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            // Update the first of our handlers in place and drop any duplicates.
            let before = handlers.len();
            handlers.retain_mut(|handler| {
                if !is_ours(handler) {
                    return true;
                }
                if placed {
                    return false;
                }
                placed = true;
                if *handler != wanted {
                    *handler = wanted.clone();
                }
                true
            });
            // Drop a group only when removing a duplicate is what emptied it.
            !(handlers.len() != before && handlers.is_empty())
        });
        if !placed {
            groups.push(json!({ "hooks": [wanted.clone()] }));
        }
    }

    if root == original {
        return Ok(text.to_string());
    }
    Ok(Format::detect(text).render(&root))
}

/// Returns the settings text without any of our handlers.
/// Returns the input unchanged when there is nothing to remove.
pub fn remove_hooks(text: &str) -> Result<String, SettingsError> {
    let original = parse(text)?;
    let mut root = original.clone();
    let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) else {
        return Ok(text.to_string());
    };

    let mut removed_any = false;
    let mut emptied_events = Vec::new();
    for (event, groups) in hooks.iter_mut() {
        let Some(groups) = groups.as_array_mut() else {
            continue;
        };
        let mut removed_here = false;
        groups.retain_mut(|group| {
            let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            let before = handlers.len();
            handlers.retain(|handler| !is_ours(handler));
            let removed = handlers.len() != before;
            removed_here |= removed;
            // Drop a group only when removing our handler is what emptied it.
            !(removed && handlers.is_empty())
        });
        removed_any |= removed_here;
        if removed_here && groups.is_empty() {
            emptied_events.push(event.clone());
        }
    }
    if !removed_any {
        return Ok(text.to_string());
    }
    for event in emptied_events {
        hooks.shift_remove(&event);
    }
    if hooks.is_empty() {
        root.as_object_mut().expect("checked by parse").shift_remove("hooks");
    }
    Ok(Format::detect(text).render(&root))
}

/// Distinct `command` values of our handlers, in document order. Lets the caller
/// detect entries that point at an executable which no longer exists.
pub fn installed_hook_commands(text: &str) -> Vec<String> {
    let Ok(root) = parse(text) else {
        return Vec::new();
    };
    let mut commands: Vec<String> = Vec::new();
    let events = root.get("hooks").and_then(Value::as_object);
    for groups in events.into_iter().flat_map(|e| e.values()).filter_map(Value::as_array) {
        for handler in groups.iter().filter_map(handlers_of).flatten().filter(|h| is_ours(h)) {
            if let Some(command) = handler.get("command").and_then(Value::as_str) {
                if !commands.iter().any(|c| c == command) {
                    commands.push(command.to_string());
                }
            }
        }
    }
    commands
}

/// Reports whether our handlers are present and point at `cmd`.
pub fn hook_status(text: &str, cmd: &HookCommand) -> InstallStatus {
    let Ok(root) = parse(text) else {
        return InstallStatus::NotInstalled;
    };
    let wanted = desired_handler(cmd);
    let wanted_exe = normalize_path(&cmd.exe);
    let (mut present, mut current) = (0, 0);
    for event in HOOK_EVENTS {
        let ours: Vec<&Value> = root
            .get("hooks")
            .and_then(|h| h.get(*event))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(handlers_of)
            .flatten()
            .filter(|h| is_ours(h))
            .collect();
        if !ours.is_empty() {
            present += 1;
        }
        let up_to_date = ours.len() == 1
            && ours[0].get("command").and_then(Value::as_str).map(normalize_path).as_deref() == Some(&wanted_exe)
            && ours[0].get("args") == wanted.get("args");
        if up_to_date {
            current += 1;
        }
    }
    if present == 0 {
        InstallStatus::NotInstalled
    } else if current == HOOK_EVENTS.len() {
        InstallStatus::Installed
    } else {
        InstallStatus::Partial
    }
}

// ---------- file operations ----------

fn io_err(context: &str, path: &Path, e: io::Error) -> SettingsError {
    SettingsError::Io(format!("{context} {}: {e}", path.display()))
}

fn read_optional(path: &Path) -> Result<Option<String>, SettingsError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err("cannot read", path, e)),
    }
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// Writes a copy of the current content next to the file, never overwriting an
/// existing backup.
fn write_backup(path: &Path, content: &str, stamp: &str) -> Result<PathBuf, SettingsError> {
    for attempt in 0..100 {
        let suffix = if attempt == 0 {
            format!(".cpanel-backup-{stamp}")
        } else {
            format!(".cpanel-backup-{stamp}-{attempt}")
        };
        let backup = sibling(path, &suffix);
        match fs::OpenOptions::new().write(true).create_new(true).open(&backup) {
            Ok(mut file) => {
                file.write_all(content.as_bytes())
                    .and_then(|_| file.sync_all())
                    .map_err(|e| io_err("cannot write backup", &backup, e))?;
                return Ok(backup);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io_err("cannot create backup", &backup, e)),
        }
    }
    Err(SettingsError::Io(format!("too many backups for {}", path.display())))
}

/// Replaces `path` atomically: the content is fully written to a temporary
/// sibling first and then renamed over the target.
///
/// Immediately before the rename the file is read again and compared with
/// `expected` (the bytes the new content was derived from; `None` for a file that
/// did not exist). If another program wrote it meanwhile, nothing is replaced.
/// `before_commit` runs just before that check; it exists for tests.
fn write_atomic(
    path: &Path,
    content: &str,
    expected: Option<&str>,
    before_commit: impl FnOnce(),
) -> Result<(), SettingsError> {
    let temp = sibling(path, &format!(".cpanel-tmp-{}", std::process::id()));
    let written = (|| {
        let mut file = fs::File::create(&temp)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()
    })();
    if let Err(e) = written {
        let _ = fs::remove_file(&temp);
        return Err(io_err("cannot write", &temp, e));
    }
    before_commit();
    let result = match read_optional(path) {
        Ok(now) if now.as_deref() == expected => fs::rename(&temp, path).map_err(|e| io_err("cannot write", path, e)),
        Ok(_) => Err(SettingsError::Changed(format!("{} was modified by another program", path.display()))),
        Err(e) => Err(e),
    };
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn apply_to_file(
    path: &Path,
    stamp: &str,
    create_if_missing: bool,
    transform: impl Fn(&str) -> Result<String, SettingsError>,
    before_commit: impl FnOnce(),
) -> Result<FileOutcome, SettingsError> {
    // Work on the real file so a symbolic link survives and the change lands on its target.
    let path = &resolve_settings_file(path);
    let current = read_optional(path)?;
    if current.is_some() && link_count(path).map_err(|e| io_err("cannot inspect", path, e))? > 1 {
        // Rename would detach this name from the other hard links, and writing in
        // place is not atomic. Neither is acceptable for a settings file.
        return Err(SettingsError::Refused(format!(
            "{} is a hard link shared with another path; edit it manually or replace it with a regular file",
            path.display()
        )));
    }
    let updated = match &current {
        Some(text) => transform(text)?,
        None if create_if_missing => transform("{}\n")?,
        None => return Ok(FileOutcome { changed: false, backup: None }),
    };
    if current.as_deref() == Some(updated.as_str()) {
        return Ok(FileOutcome { changed: false, backup: None });
    }
    let backup = match &current {
        Some(text) => Some(write_backup(path, text, stamp)?),
        None => None,
    };
    if let Err(e) = write_atomic(path, &updated, current.as_deref(), before_commit) {
        // Nothing was replaced, so a backup of it would only mislead.
        if let Some(backup) = &backup {
            let _ = fs::remove_file(backup);
        }
        return Err(e);
    }
    Ok(FileOutcome { changed: true, backup })
}

/// Installs the hooks into the settings file at `path` (created when missing).
pub fn install_hooks_file(path: &Path, cmd: &HookCommand, stamp: &str) -> Result<FileOutcome, SettingsError> {
    apply_to_file(path, stamp, true, |text| install_hooks(text, cmd), || {})
}

/// Removes the hooks from the settings file at `path`; a missing file is a no-op.
pub fn remove_hooks_file(path: &Path, stamp: &str) -> Result<FileOutcome, SettingsError> {
    apply_to_file(path, stamp, false, remove_hooks, || {})
}

/// `YYYYMMDD-HHMMSS` in UTC for the given Unix time.
pub fn backup_stamp(epoch_secs: u64) -> String {
    let days = (epoch_secs / 86_400) as i64;
    let secs = epoch_secs % 86_400;
    // Civil-from-days (proleptic Gregorian), after Howard Hinnant.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}{month:02}{day:02}-{:02}{:02}{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd() -> HookCommand {
        HookCommand { exe: "C:/Apps/cpanel-hook.exe".into(), profile: "p".into() }
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> =
            fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn aborts_without_writing_when_the_file_changes_before_the_rename() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "{\n  \"model\": \"opus\"\n}\n").unwrap();
        let concurrent = "{\n  \"model\": \"sonnet\"\n}\n";

        // Another program saves the file after we read it and before we replace it.
        let result = apply_to_file(&path, "S", true, |t| install_hooks(t, &cmd()), || {
            fs::write(&path, concurrent).unwrap();
        });

        let err = result.unwrap_err();
        assert!(matches!(err, SettingsError::Changed(_)), "{err:?}");
        assert!(err.to_string().contains("retry"));
        assert_eq!(fs::read_to_string(&path).unwrap(), concurrent, "the other program's write is preserved");
        assert_eq!(names(dir.path()), vec!["settings.json"], "no temp file and no stale backup");

        // A retry then works on the new content.
        let outcome = install_hooks_file(&path, &cmd(), "S").unwrap();
        assert!(outcome.changed);
        assert_eq!(fs::read_to_string(outcome.backup.unwrap()).unwrap(), concurrent);
        assert!(fs::read_to_string(&path).unwrap().contains("\"model\": \"sonnet\""));
    }

    #[test]
    fn aborts_when_a_file_appears_where_none_existed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let result = apply_to_file(&path, "S", true, |t| install_hooks(t, &cmd()), || {
            fs::write(&path, "{\"theme\":\"dark\"}").unwrap();
        });
        assert!(matches!(result, Err(SettingsError::Changed(_))));
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"theme\":\"dark\"}");
        assert_eq!(names(dir.path()), vec!["settings.json"]);
    }

    #[test]
    fn aborts_when_the_file_is_deleted_before_the_rename() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "{}").unwrap();
        let result = apply_to_file(&path, "S", true, |t| install_hooks(t, &cmd()), || {
            fs::remove_file(&path).unwrap();
        });
        assert!(matches!(result, Err(SettingsError::Changed(_))));
        assert!(names(dir.path()).is_empty());
    }
}
