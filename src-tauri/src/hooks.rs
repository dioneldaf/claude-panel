//! Locating config directories and the hook executable, and applying the
//! install / remove actions to each profile's `settings.json`.

use cpanel_core::model::ProfileView;
use cpanel_core::registry::{discover_config_dirs, ConfigDir};
use cpanel_core::settings::{
    backup_stamp, hook_status, install_hooks, install_hooks_file, installed_hook_commands, remove_hooks,
    remove_hooks_file, HookCommand, InstallStatus, HOOK_EXE_STEM,
};
use cpanel_core::textdiff::diff_lines;
use serde::Serialize;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Overrides auto-discovery with a `;`-separated list of config directories.
/// Used to try the hook actions against copies instead of the real profiles.
pub const CONFIG_DIRS_ENV: &str = "CPANEL_CONFIG_DIRS";

const NO_HOOK_EXE: &str = "cpanel-hook executable not found next to the application";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Install,
    Remove,
}

#[derive(Serialize, Clone, Debug)]
pub struct ActionResult {
    /// Profile tag written into the hook arguments.
    pub tag: String,
    /// Other profiles whose settings resolve to the same file (handled by this one action).
    pub shared_with: Vec<String>,
    pub file: String,
    pub ok: bool,
    pub changed: bool,
    pub backup: Option<String>,
    pub error: Option<String>,
}

/// One settings file to act on, after merging profiles that share it.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    /// Alphabetically first tag of the profiles sharing the file; deterministic.
    pub tag: String,
    pub shared_with: Vec<String>,
    pub file: PathBuf,
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from)
}

fn tag_of(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    name.trim_start_matches('.').to_string()
}

/// Config directories to watch: explicit list, environment override, or discovery.
pub fn config_dirs(explicit: &[PathBuf]) -> Vec<ConfigDir> {
    let from_env: Vec<PathBuf> = std::env::var(CONFIG_DIRS_ENV)
        .map(|v| v.split(';').filter(|p| !p.trim().is_empty()).map(|p| PathBuf::from(p.trim())).collect())
        .unwrap_or_default();
    let listed = if explicit.is_empty() { from_env.as_slice() } else { explicit };
    if !listed.is_empty() {
        return listed.iter().map(|path| ConfigDir { tag: tag_of(path), path: path.clone() }).collect();
    }
    home_dir().map(|home| discover_config_dirs(&home)).unwrap_or_default()
}

/// Finds `cpanel-hook` for an application executable at `app_exe`.
///
/// Every supported layout keeps the two executables side by side: the installed
/// directory, the portable folder, `target/release` and `target/debug`. The path
/// is returned with forward slashes, as it is written into `settings.json`.
pub fn find_hook_exe(app_exe: &Path) -> Option<String> {
    let name = if cfg!(windows) { format!("{HOOK_EXE_STEM}.exe") } else { HOOK_EXE_STEM.to_string() };
    let candidate = app_exe.parent()?.join(name);
    candidate.is_file().then(|| display(&candidate).replace('\\', "/"))
}

/// Path of `cpanel-hook` next to the running executable.
pub fn hook_exe() -> Option<String> {
    find_hook_exe(&std::env::current_exe().ok()?)
}

/// Status string for the frontend. `broken` means our entries point at an
/// executable that no longer exists (the application was moved or deleted without
/// removing its hooks), which makes Claude Code report an error for every event.
fn status_of(text: &str, cmd: &HookCommand) -> &'static str {
    let status = hook_status(text, cmd);
    let dangling = installed_hook_commands(text).iter().any(|command| !Path::new(command).is_file());
    if status != InstallStatus::NotInstalled && dangling {
        "broken"
    } else {
        status.as_str()
    }
}

fn settings_path(dir: &ConfigDir) -> PathBuf {
    dir.path.join("settings.json")
}

/// Identity of the file behind a path: symbolic links, junctions and `.`/`..`
/// resolved. Falls back to the resolved parent for a file that does not exist yet.
fn identity(file: &Path) -> PathBuf {
    if let Ok(real) = std::fs::canonicalize(file) {
        return real;
    }
    match (file.parent().and_then(|p| std::fs::canonicalize(p).ok()), file.file_name()) {
        (Some(parent), Some(name)) => parent.join(name),
        _ => file.to_path_buf(),
    }
}

/// Merges profiles whose `settings.json` is one and the same file (symlinked
/// profiles or directories), so it is modified once and with a single profile tag.
pub fn targets(dirs: &[ConfigDir]) -> Vec<Target> {
    let mut groups: Vec<(PathBuf, PathBuf, Vec<String>)> = Vec::new();
    for dir in dirs {
        let file = settings_path(dir);
        let key = identity(&file);
        match groups.iter_mut().find(|(k, _, _)| *k == key) {
            Some((_, _, tags)) => tags.push(dir.tag.clone()),
            None => groups.push((key, file, vec![dir.tag.clone()])),
        }
    }
    groups
        .into_iter()
        .map(|(_, file, mut tags)| {
            tags.sort();
            tags.dedup();
            let tag = tags.remove(0);
            Target { tag, shared_with: tags, file }
        })
        .collect()
}

fn command_for(tag: &str, exe: &str) -> HookCommand {
    HookCommand { exe: exe.to_string(), profile: tag.to_string() }
}

/// Windows canonical paths carry a `\\?\` prefix that only confuses readers.
fn display(path: &Path) -> String {
    let text = path.to_string_lossy();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
}

/// Read-only: reports the installation state of every profile. Profiles sharing
/// one file are judged against the tag that an install would write.
pub fn profile_views(dirs: &[ConfigDir], exe: Option<&str>) -> Vec<ProfileView> {
    let targets = targets(dirs);
    dirs.iter()
        .map(|dir| {
            let file = settings_path(dir);
            let key = identity(&file);
            let tag = targets.iter().find(|t| identity(&t.file) == key).map_or(dir.tag.as_str(), |t| t.tag.as_str());
            // Without our own hook executable the entries can still be judged as dangling.
            let wanted = command_for(tag, exe.unwrap_or(""));
            let status = match std::fs::read_to_string(&file) {
                Ok(text) => status_of(&text, &wanted),
                Err(_) => InstallStatus::NotInstalled.as_str(),
            };
            ProfileView { tag: dir.tag.clone(), dir: dir.path.to_string_lossy().into_owned(), hooks: status.into() }
        })
        .collect()
}

fn stamp_now() -> String {
    backup_stamp(SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0))
}

/// Applies the action once per distinct settings file. Each file is backed up
/// first and written atomically; a failure in one does not affect the others.
pub fn apply(action: Action, dirs: &[ConfigDir], exe: Option<&str>) -> Vec<ActionResult> {
    let stamp = stamp_now();
    targets(dirs)
        .into_iter()
        .map(|target| {
            let outcome = match (action, exe) {
                (Action::Install, Some(exe)) => {
                    install_hooks_file(&target.file, &command_for(&target.tag, exe), &stamp).map_err(|e| e.to_string())
                }
                (Action::Install, None) => Err(NO_HOOK_EXE.to_string()),
                (Action::Remove, _) => remove_hooks_file(&target.file, &stamp).map_err(|e| e.to_string()),
            };
            let (ok, changed, backup, error) = match outcome {
                Ok(o) => (true, o.changed, o.backup.map(|b| display(&b)), None),
                Err(e) => (false, false, None, Some(e)),
            };
            ActionResult {
                tag: target.tag,
                shared_with: target.shared_with,
                file: display(&target.file),
                ok,
                changed,
                backup,
                error,
            }
        })
        .collect()
}

/// Read-only preview of what [`apply`] would change, as `+`/`-` lines per file.
/// Only a missing file is treated as empty; any other read failure is reported,
/// exactly as the real action would fail.
pub fn preview(action: Action, dirs: &[ConfigDir], exe: Option<&str>) -> String {
    let mut out = String::new();
    for target in targets(dirs) {
        out.push_str(&format!("# {}\n", display(&target.file)));
        if !target.shared_with.is_empty() {
            out.push_str(&format!(
                "# shared by profiles {}, {}; hook tag: {}\n",
                target.tag,
                target.shared_with.join(", "),
                target.tag
            ));
        }
        let current = match std::fs::read_to_string(&target.file) {
            Ok(text) => text,
            Err(e) if e.kind() == ErrorKind::NotFound => "{}\n".to_string(),
            Err(e) => {
                out.push_str(&format!("error: cannot read {}: {e}\n", display(&target.file)));
                continue;
            }
        };
        let updated = match (action, exe) {
            (Action::Install, Some(exe)) => install_hooks(&current, &command_for(&target.tag, exe)).map_err(|e| e.to_string()),
            (Action::Install, None) => Err(NO_HOOK_EXE.to_string()),
            (Action::Remove, _) => remove_hooks(&current).map_err(|e| e.to_string()),
        };
        match updated {
            Ok(text) if text == current => out.push_str("(no changes)\n"),
            Ok(text) => out.push_str(&diff_lines(&current, &text)),
            Err(e) => out.push_str(&format!("error: {e}\n")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const EXE: &str = "C:/Apps/cpanel-hook.exe";

    fn dir(tag: &str, path: &Path) -> ConfigDir {
        ConfigDir { tag: tag.into(), path: path.to_path_buf() }
    }

    fn backups(path: &Path) -> usize {
        fs::read_dir(path).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().contains("cpanel-backup")).count()
    }

    #[test]
    fn profiles_resolving_to_the_same_file_are_applied_once_with_the_first_tag() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("settings.json"), "{}\n").unwrap();
        // Two spellings of one directory stand in for two profiles sharing a file.
        let dirs = [dir("zeta", tmp.path()), dir("alpha", &tmp.path().join("."))];

        let found = targets(&dirs);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].tag, "alpha");
        assert_eq!(found[0].shared_with, vec!["zeta".to_string()]);

        let results = apply(Action::Install, &dirs, Some(EXE));
        assert_eq!(results.len(), 1);
        assert!(results[0].ok && results[0].changed, "{results:?}");
        assert_eq!(results[0].tag, "alpha");
        assert_eq!(results[0].shared_with, vec!["zeta".to_string()]);
        let text = fs::read_to_string(tmp.path().join("settings.json")).unwrap();
        assert!(text.contains("\"alpha\"") && !text.contains("\"zeta\""));
        assert_eq!(backups(tmp.path()), 1, "one file, one backup");

        // Both profiles see the shared entries (the fixture executable does not exist,
        // hence "broken" rather than "installed"), and a re-run is a no-op.
        assert!(profile_views(&dirs, Some(EXE)).iter().all(|p| p.hooks == "broken"));
        assert!(!apply(Action::Install, &dirs, Some(EXE))[0].changed);
        assert!(preview(Action::Install, &dirs, Some(EXE)).contains("shared by profiles alpha, zeta; hook tag: alpha"));
    }

    #[test]
    fn symlinked_profiles_share_one_target() {
        let tmp = tempfile::tempdir().unwrap();
        let (real, a, b) = (tmp.path().join("real"), tmp.path().join("a"), tmp.path().join("b"));
        for d in [&real, &a, &b] {
            fs::create_dir_all(d).unwrap();
        }
        fs::write(real.join("settings.json"), "{}\n").unwrap();
        let link = |to: &Path| {
            #[cfg(windows)]
            return std::os::windows::fs::symlink_file(real.join("settings.json"), to).is_ok();
            #[cfg(not(windows))]
            return std::os::unix::fs::symlink(real.join("settings.json"), to).is_ok();
        };
        if !link(&a.join("settings.json")) || !link(&b.join("settings.json")) {
            eprintln!("skipped: symlink creation is not permitted on this account");
            return;
        }
        let dirs = [dir("b", &b), dir("a", &a)];
        let results = apply(Action::Install, &dirs, Some(EXE));
        assert_eq!(results.len(), 1);
        assert_eq!((results[0].tag.as_str(), results[0].shared_with.len()), ("a", 1));
        for d in [&a, &b] {
            assert!(fs::symlink_metadata(d.join("settings.json")).unwrap().file_type().is_symlink());
        }
        assert_eq!(backups(&real), 1);
    }

    #[test]
    fn distinct_files_are_applied_separately_with_their_own_tags() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        let results = apply(Action::Install, &[dir("a", &a), dir("b", &b)], Some(EXE));
        assert_eq!(results.iter().map(|r| r.tag.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
        assert!(results.iter().all(|r| r.ok && r.changed && r.shared_with.is_empty()));
        assert!(fs::read_to_string(b.join("settings.json")).unwrap().contains("\"b\""));
    }

    #[test]
    fn preview_reports_an_unreadable_settings_file_as_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        // A directory in place of the file cannot be read as text.
        fs::create_dir(tmp.path().join("settings.json")).unwrap();
        let dirs = [dir("p", tmp.path())];
        let text = preview(Action::Install, &dirs, Some(EXE));
        assert!(text.contains("error: cannot read"), "{text}");
        assert!(!text.contains("+ "), "must not pretend the file is empty: {text}");
        // Same verdict as the real action.
        let results = apply(Action::Install, &dirs, Some(EXE));
        assert!(!results[0].ok && results[0].error.is_some());
    }

    #[test]
    fn preview_of_a_missing_file_shows_the_additions_and_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let text = preview(Action::Install, &[dir("p", tmp.path())], Some(EXE));
        assert!(text.contains("+ ") && text.contains("cpanel-hook.exe"));
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 0);
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    use std::fs;

    const HOOK: &str = if cfg!(windows) { "cpanel-hook.exe" } else { "cpanel-hook" };

    fn layout(parts: &[&str]) -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = parts.iter().fold(tmp.path().to_path_buf(), |p, part| p.join(part));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("claude-panel.exe"), b"app").unwrap();
        (tmp, dir)
    }

    #[test]
    fn finds_the_hook_next_to_the_app_in_every_supported_layout() {
        for parts in [
            vec!["AppData", "Local", "Claude Panel"], // per-user installation
            vec!["Downloads", "claude-panel-portable"], // portable folder
            vec!["repo", "target", "release"],
            vec!["repo", "target", "debug"],
        ] {
            let (_tmp, dir) = layout(&parts);
            fs::write(dir.join(HOOK), b"hook").unwrap();
            let found = find_hook_exe(&dir.join("claude-panel.exe")).expect("hook next to app");
            assert!(found.ends_with(&format!("/{HOOK}")), "{found}");
            assert!(!found.contains('\\'), "forward slashes only: {found}");
            assert!(Path::new(&found).is_file());
        }
    }

    #[test]
    fn reports_nothing_when_the_hook_is_missing_or_is_not_a_file() {
        let (_tmp, dir) = layout(&["app"]);
        assert_eq!(find_hook_exe(&dir.join("claude-panel.exe")), None);
        fs::create_dir(dir.join(HOOK)).unwrap();
        assert_eq!(find_hook_exe(&dir.join("claude-panel.exe")), None);
        assert_eq!(find_hook_exe(Path::new("claude-panel.exe")), None);
    }

    #[test]
    fn the_installed_path_is_stable_across_upgrades_into_the_same_directory() {
        // Entries written by one version stay "installed" for the next one, because
        // the resolved path does not depend on the version.
        let (_tmp, dir) = layout(&["Claude Panel"]);
        fs::write(dir.join(HOOK), b"hook v1").unwrap();
        let before = find_hook_exe(&dir.join("claude-panel.exe")).unwrap();
        let profile = dir.join("profile");
        fs::create_dir_all(&profile).unwrap();
        let dirs = [ConfigDir { tag: "claude".into(), path: profile.clone() }];
        assert!(apply(Action::Install, &dirs, Some(&before))[0].changed);

        fs::write(dir.join(HOOK), b"hook v2").unwrap();
        fs::write(dir.join("claude-panel.exe"), b"app v2").unwrap();
        let after = find_hook_exe(&dir.join("claude-panel.exe")).unwrap();
        assert_eq!(before, after);
        assert_eq!(profile_views(&dirs, Some(&after))[0].hooks, "installed");
        assert!(!apply(Action::Install, &dirs, Some(&after))[0].changed);
    }

    #[test]
    fn entries_pointing_at_a_missing_executable_are_reported_as_broken() {
        let (_tmp, dir) = layout(&["Claude Panel"]);
        fs::write(dir.join(HOOK), b"hook").unwrap();
        let exe = find_hook_exe(&dir.join("claude-panel.exe")).unwrap();
        let profile = dir.join("profile");
        fs::create_dir_all(&profile).unwrap();
        let dirs = [ConfigDir { tag: "claude".into(), path: profile }];
        apply(Action::Install, &dirs, Some(&exe));
        assert_eq!(profile_views(&dirs, Some(&exe))[0].hooks, "installed");

        // The application folder is deleted without removing the hooks first.
        fs::remove_file(dir.join(HOOK)).unwrap();
        assert_eq!(profile_views(&dirs, None)[0].hooks, "broken");
        // Removal still works without the hook executable and cleans up completely.
        let removed = apply(Action::Remove, &dirs, None);
        assert!(removed[0].ok && removed[0].changed);
        assert_eq!(profile_views(&dirs, None)[0].hooks, "not_installed");
    }
}
