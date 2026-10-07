use cpanel_core::autostart::{command_line, is_enabled, is_installed_build, startup_plan, toggle_plan, Plan, RunAction};
use std::fs;
use std::path::{Path, PathBuf};

fn exe() -> PathBuf {
    PathBuf::from(r"C:\Users\example\AppData\Local\Claude Panel\claude-panel.exe")
}

fn write() -> RunAction {
    RunAction::Write(command_line(&exe()))
}

#[test]
fn the_command_line_quotes_the_path_and_marks_the_launch_as_automatic() {
    assert_eq!(command_line(&exe()), r#""C:\Users\example\AppData\Local\Claude Panel\claude-panel.exe" --autostart"#);
    // Verbatim prefixes from canonicalised paths never reach the registry.
    assert_eq!(command_line(Path::new(r"\\?\C:\Apps\claude-panel.exe")), r#""C:\Apps\claude-panel.exe" --autostart"#);
}

#[test]
fn enabled_means_the_entry_points_at_this_executable() {
    let cmd = command_line(&exe());
    assert!(is_enabled(Some(&cmd), &exe()));
    assert!(is_enabled(Some(&cmd.to_uppercase().replace("--AUTOSTART", "--autostart")), &exe()), "paths compare case-insensitively");
    assert!(!is_enabled(None, &exe()));
    assert!(!is_enabled(Some(r#""D:\old\claude-panel.exe" --autostart"#), &exe()));
    assert!(!is_enabled(Some(""), &exe()));
}

#[test]
fn first_run_of_an_installed_build_enables_it_and_records_the_choice() {
    assert_eq!(startup_plan(None, true, None, &exe()), Plan { action: write(), preference: Some(true) });
}

#[test]
fn development_and_portable_builds_never_register_by_default() {
    assert_eq!(startup_plan(None, false, None, &exe()), Plan { action: RunAction::None, preference: None });
    // Even with the preference on (shared with an installed copy), a build that
    // is not installed leaves the entry alone.
    let other = r#""C:\Installed\claude-panel.exe" --autostart"#;
    assert_eq!(startup_plan(Some(true), false, Some(other), &exe()), Plan { action: RunAction::None, preference: Some(true) });
}

#[test]
fn an_upgrade_does_not_re_enable_what_the_user_turned_off() {
    assert_eq!(startup_plan(Some(false), true, None, &exe()), Plan { action: RunAction::None, preference: Some(false) });
}

#[test]
fn a_moved_installation_corrects_the_entry_and_a_correct_one_is_left_alone() {
    let stale = r#""D:\old place\claude-panel.exe" --autostart"#;
    assert_eq!(startup_plan(Some(true), true, Some(stale), &exe()), Plan { action: write(), preference: Some(true) });
    assert_eq!(startup_plan(Some(true), true, None, &exe()), Plan { action: write(), preference: Some(true) });
    let current = command_line(&exe());
    assert_eq!(startup_plan(Some(true), true, Some(&current), &exe()), Plan { action: RunAction::None, preference: Some(true) });
}

#[test]
fn the_tray_toggle_is_explicit_and_always_recorded() {
    assert_eq!(toggle_plan(true, &exe()), Plan { action: write(), preference: Some(true) });
    assert_eq!(toggle_plan(false, &exe()), Plan { action: RunAction::Remove, preference: Some(false) });
}

#[test]
fn only_a_directory_with_the_uninstaller_counts_as_installed() {
    let tmp = tempfile::tempdir().unwrap();
    let make = |parts: &[&str], uninstaller: bool| {
        let dir = parts.iter().fold(tmp.path().to_path_buf(), |p, part| p.join(part));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("claude-panel.exe"), b"app").unwrap();
        if uninstaller {
            fs::write(dir.join("uninstall.exe"), b"uninstaller").unwrap();
        }
        dir.join("claude-panel.exe")
    };
    assert!(is_installed_build(&make(&["Programs", "Claude Panel"], true)));
    assert!(!is_installed_build(&make(&["Downloads", "portable"], false)), "portable folder");
    assert!(!is_installed_build(&make(&["repo", "target", "release"], false)));
    assert!(!is_installed_build(&make(&["repo", "target", "debug"], true)), "never a build directory");
    assert!(!is_installed_build(Path::new("claude-panel.exe")));
}
