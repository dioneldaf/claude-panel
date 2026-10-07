use cpanel_core::settings::{
    backup_stamp, hook_status, install_hooks, install_hooks_file, remove_hooks, remove_hooks_file,
    HookCommand, InstallStatus, SettingsError, HOOK_EVENTS,
};
use serde_json::Value;
use std::fs;

/// Mimics the structure of a real settings.json: unrelated keys, permissions, and
/// third-party hooks using `args`, `async`, matchers and shell-form commands.
const FIXTURE: &str = r#"{
  "model": "opus",
  "permissions": {
    "allow": [
      "Bash(git status)"
    ],
    "deny": []
  },
  "hooks": {
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "example-indexer prompt-hook"
          }
        ]
      },
      {
        "matcher": "",
        "hooks": [
          {
            "type": "command",
            "command": "example-tool refresh --quiet --cwd \"${CLAUDE_PROJECT_DIR:-$PWD}\" || true"
          }
        ]
      },
      {
        "hooks": [
          {
            "type": "command",
            "command": "C:/Program Files/Git/bin/bash.exe",
            "args": [
              "C:/Users/example/.claude/hooks/on-prompt.sh"
            ],
            "async": true
          }
        ]
      }
    ],
    "Notification": [
      {
        "matcher": "permission_prompt",
        "hooks": [
          {
            "type": "command",
            "command": "C:/Program Files/Git/bin/bash.exe",
            "args": [
              "C:/Users/example/.claude/hooks/notify.sh",
              "5",
              "lock,raised_hand",
              "Permiso requerido: acción"
            ],
            "async": true
          }
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "C:/Program Files/Git/bin/bash.exe",
            "args": [
              "C:/Users/example/.claude/hooks/on-stop.sh"
            ],
            "async": true
          }
        ]
      }
    ],
    "PermissionDenied": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "C:/Program Files/Git/bin/bash.exe",
            "args": [
              "C:/Users/example/.claude/hooks/notify.sh",
              "1",
              "no_entry",
              "Llamada denegada automáticamente"
            ],
            "async": true
          }
        ]
      }
    ]
  },
  "theme": "dark",
  "futureUnknownKey": {
    "nested": [
      1,
      2.5,
      null
    ]
  }
}
"#;

fn cmd() -> HookCommand {
    HookCommand { exe: "C:/Apps/Claude Panel/cpanel-hook.exe".into(), profile: "claude-work".into() }
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

fn ours(v: &Value, event: &str) -> Vec<Value> {
    v["hooks"][event]
        .as_array()
        .map(|groups| {
            groups
                .iter()
                .flat_map(|g| g["hooks"].as_array().cloned().unwrap_or_default())
                .filter(|h| h["command"].as_str().is_some_and(|c| c.contains("cpanel-hook")))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn install_adds_exactly_one_async_exec_form_handler_per_event() {
    let out = json(&install_hooks(FIXTURE, &cmd()).unwrap());
    for event in HOOK_EVENTS {
        let handlers = ours(&out, event);
        assert_eq!(handlers.len(), 1, "{event}");
        let h = &handlers[0];
        assert_eq!(h["type"], "command");
        assert_eq!(h["command"], "C:/Apps/Claude Panel/cpanel-hook.exe");
        assert_eq!(h["args"], serde_json::json!(["--profile", "claude-work"]));
        assert_eq!(h["async"], true);
        assert_eq!(h["timeout"], 5);
    }
}

#[test]
fn install_covers_the_events_the_reducer_relies_on() {
    for needed in [
        "SessionStart", "SessionEnd", "UserPromptSubmit", "PreToolUse", "PostToolUse",
        "PostToolUseFailure", "PermissionRequest", "Elicitation", "ElicitationResult",
        "SubagentStart", "SubagentStop", "Stop", "StopFailure", "Notification",
    ] {
        assert!(HOOK_EVENTS.contains(&needed), "{needed}");
    }
}

#[test]
fn install_preserves_every_existing_entry_key_and_order() {
    let before = json(FIXTURE);
    let after = json(&install_hooks(FIXTURE, &cmd()).unwrap());

    // Top-level keys: same keys, same order.
    let keys = |v: &Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys(&before), keys(&after));
    for key in ["model", "permissions", "theme", "futureUnknownKey"] {
        assert_eq!(before[key], after[key], "{key}");
    }
    // Existing hook events keep their position and their groups come first, untouched.
    let before_events = keys(&before["hooks"]);
    let after_events = keys(&after["hooks"]);
    assert_eq!(&after_events[..before_events.len()], &before_events[..]);
    for event in &before_events {
        let old = before["hooks"][event].as_array().unwrap();
        let new = after["hooks"][event].as_array().unwrap();
        assert_eq!(&new[..old.len()], &old[..], "{event}");
    }
}

#[test]
fn install_is_idempotent() {
    let once = install_hooks(FIXTURE, &cmd()).unwrap();
    let twice = install_hooks(&once, &cmd()).unwrap();
    assert_eq!(once, twice);
}

#[test]
fn reinstalling_with_another_path_or_profile_updates_in_place() {
    let once = install_hooks(FIXTURE, &cmd()).unwrap();
    let moved = HookCommand { exe: "D:/elsewhere/cpanel-hook.exe".into(), profile: "claude".into() };
    let out = json(&install_hooks(&once, &moved).unwrap());
    for event in HOOK_EVENTS {
        let handlers = ours(&out, event);
        assert_eq!(handlers.len(), 1, "{event}");
        assert_eq!(handlers[0]["command"], "D:/elsewhere/cpanel-hook.exe");
        assert_eq!(handlers[0]["args"], serde_json::json!(["--profile", "claude"]));
    }
}

#[test]
fn remove_restores_the_original_file_byte_for_byte() {
    let installed = install_hooks(FIXTURE, &cmd()).unwrap();
    assert_ne!(installed, FIXTURE);
    assert_eq!(remove_hooks(&installed).unwrap(), FIXTURE);
}

#[test]
fn remove_is_a_no_op_when_nothing_is_installed() {
    assert_eq!(remove_hooks(FIXTURE).unwrap(), FIXTURE);
}

#[test]
fn remove_only_deletes_our_handler_inside_a_shared_group() {
    let text = r#"{"hooks":{"Stop":[{"hooks":[
        {"type":"command","command":"other.exe","async":true},
        {"type":"command","command":"C:\\Apps\\cpanel-hook.exe","args":["--profile","x"]}
    ]}]}}"#;
    let out = json(&remove_hooks(text).unwrap());
    assert_eq!(out, serde_json::json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":"other.exe","async":true}]}]}}));
}

#[test]
fn similar_looking_commands_are_not_ours() {
    let text = r#"{"hooks":{"Stop":[{"hooks":[
        {"type":"command","command":"C:/tools/not-cpanel-hook.exe"},
        {"type":"command","command":"bash","args":["C:/x/cpanel-hook.exe"]},
        {"type":"command","command":"echo cpanel-hook.exe"}
    ]}]}}"#;
    assert_eq!(json(&remove_hooks(text).unwrap()), json(text));
    assert_eq!(hook_status(text, &cmd()), InstallStatus::NotInstalled);
}

#[test]
fn status_reports_not_installed_partial_and_installed() {
    assert_eq!(hook_status(FIXTURE, &cmd()), InstallStatus::NotInstalled);
    let installed = install_hooks(FIXTURE, &cmd()).unwrap();
    assert_eq!(hook_status(&installed, &cmd()), InstallStatus::Installed);

    // A different executable path means the entries are outdated.
    let moved = HookCommand { exe: "D:/elsewhere/cpanel-hook.exe".into(), profile: "claude-work".into() };
    assert_eq!(hook_status(&installed, &moved), InstallStatus::Partial);

    // Missing events also count as partial.
    let mut v = json(&installed);
    v["hooks"].as_object_mut().unwrap().remove("PreToolUse");
    assert_eq!(hook_status(&v.to_string(), &cmd()), InstallStatus::Partial);

    assert_eq!(hook_status("{ broken", &cmd()), InstallStatus::NotInstalled);
}

#[test]
fn works_on_empty_settings_and_cleans_up_after_itself() {
    let installed = install_hooks("{}", &cmd()).unwrap();
    assert_eq!(hook_status(&installed, &cmd()), InstallStatus::Installed);
    assert_eq!(json(&remove_hooks(&installed).unwrap()), serde_json::json!({}));
}

#[test]
fn refuses_to_touch_files_it_does_not_understand() {
    assert!(matches!(install_hooks("{ // comment\n}", &cmd()), Err(SettingsError::Parse(_))));
    assert!(matches!(install_hooks("[1,2,3]", &cmd()), Err(SettingsError::Shape(_))));
    assert!(matches!(install_hooks(r#"{"hooks": []}"#, &cmd()), Err(SettingsError::Shape(_))));
    assert!(matches!(install_hooks(r#"{"hooks": {"Stop": {}}}"#, &cmd()), Err(SettingsError::Shape(_))));
    assert!(matches!(remove_hooks("nope"), Err(SettingsError::Parse(_))));
}

#[test]
fn keeps_indentation_line_endings_and_missing_trailing_newline() {
    let four = "{\r\n    \"model\": \"opus\"\r\n}";
    let out = install_hooks(four, &cmd()).unwrap();
    assert!(out.starts_with("{\r\n    \"model\": \"opus\",\r\n    \"hooks\": {\r\n        \""));
    assert!(!out.contains("\r\r"));
    assert!(out.ends_with('}'));
    assert_eq!(remove_hooks(&out).unwrap(), four);
}

#[test]
fn file_install_writes_a_backup_first_and_leaves_no_temp_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, FIXTURE).unwrap();

    let outcome = install_hooks_file(&path, &cmd(), "20261007-120000").unwrap();
    assert!(outcome.changed);
    let backup = outcome.backup.expect("backup path");
    assert_eq!(backup, dir.path().join("settings.json.cpanel-backup-20261007-120000"));
    assert_eq!(fs::read_to_string(&backup).unwrap(), FIXTURE);
    assert_eq!(hook_status(&fs::read_to_string(&path).unwrap(), &cmd()), InstallStatus::Installed);

    // Idempotent: a second run changes nothing and creates no further backup.
    let again = install_hooks_file(&path, &cmd(), "20261007-120001").unwrap();
    assert!(!again.changed);
    assert!(again.backup.is_none());

    let removed = remove_hooks_file(&path, "20261007-120002").unwrap();
    assert!(removed.changed);
    assert_eq!(fs::read_to_string(&path).unwrap(), FIXTURE);

    let mut names: Vec<String> =
        fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "settings.json",
            "settings.json.cpanel-backup-20261007-120000",
            "settings.json.cpanel-backup-20261007-120002",
        ]
    );
}

#[test]
fn file_install_never_overwrites_an_existing_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, FIXTURE).unwrap();
    fs::write(dir.path().join("settings.json.cpanel-backup-S"), "PRECIOUS").unwrap();
    let outcome = install_hooks_file(&path, &cmd(), "S").unwrap();
    let backup = outcome.backup.unwrap();
    assert_ne!(backup, dir.path().join("settings.json.cpanel-backup-S"));
    assert_eq!(fs::read_to_string(dir.path().join("settings.json.cpanel-backup-S")).unwrap(), "PRECIOUS");
    assert_eq!(fs::read_to_string(backup).unwrap(), FIXTURE);
}

#[test]
fn file_install_leaves_an_invalid_file_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, "{ broken").unwrap();
    assert!(install_hooks_file(&path, &cmd(), "S").is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "{ broken");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn file_install_creates_a_missing_settings_file_without_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let outcome = install_hooks_file(&path, &cmd(), "S").unwrap();
    assert!(outcome.changed);
    assert!(outcome.backup.is_none());
    assert_eq!(hook_status(&fs::read_to_string(&path).unwrap(), &cmd()), InstallStatus::Installed);
    // Removing from a missing file is a no-op.
    let other = dir.path().join("absent.json");
    assert!(!remove_hooks_file(&other, "S").unwrap().changed);
    assert!(!other.exists());
}

#[test]
fn backup_stamp_is_a_utc_calendar_timestamp() {
    assert_eq!(backup_stamp(0), "19700101-000000");
    assert_eq!(backup_stamp(951_782_400), "20000229-000000");
    assert_eq!(backup_stamp(1_791_359_011), "20261007-074331");
}

// ---------- links ----------

/// Creates a file symlink, or returns false where the account may not create them
/// (Windows without Developer Mode); callers then skip the test.
fn try_symlink(target: &std::path::Path, link: &std::path::Path) -> bool {
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_file(target, link);
    #[cfg(not(windows))]
    let made = std::os::unix::fs::symlink(target, link);
    if made.is_err() {
        eprintln!("skipped: symlink creation is not permitted on this account");
    }
    made.is_ok()
}

#[test]
fn file_install_through_a_symlink_keeps_the_link_and_updates_the_target() {
    let dir = tempfile::tempdir().unwrap();
    let real_dir = dir.path().join("dotfiles");
    let profile = dir.path().join("profile");
    fs::create_dir_all(&real_dir).unwrap();
    fs::create_dir_all(&profile).unwrap();
    let target = real_dir.join("settings.json");
    let link = profile.join("settings.json");
    fs::write(&target, FIXTURE).unwrap();
    if !try_symlink(&target, &link) {
        return;
    }

    let outcome = install_hooks_file(&link, &cmd(), "S").unwrap();
    assert!(outcome.changed);
    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "the link must survive");
    assert_eq!(hook_status(&fs::read_to_string(&target).unwrap(), &cmd()), InstallStatus::Installed);
    // The backup sits next to the real file, and nothing else is left in the profile.
    assert_eq!(fs::read_to_string(real_dir.join("settings.json.cpanel-backup-S")).unwrap(), FIXTURE);
    assert_eq!(fs::read_dir(&profile).unwrap().count(), 1);

    assert!(remove_hooks_file(&link, "T").unwrap().changed);
    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(fs::read_to_string(&target).unwrap(), FIXTURE);
}

#[test]
fn symlinked_settings_resolve_to_the_real_file() {
    use cpanel_core::settings::resolve_settings_file;
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("real.json");
    fs::write(&target, "{}").unwrap();
    // A regular or missing file is used as given.
    assert_eq!(resolve_settings_file(&target), target);
    assert_eq!(resolve_settings_file(&dir.path().join("absent.json")), dir.path().join("absent.json"));
    let link = dir.path().join("settings.json");
    if !try_symlink(&target, &link) {
        return;
    }
    assert_eq!(resolve_settings_file(&link), fs::canonicalize(&target).unwrap());
}

#[test]
fn file_install_refuses_hard_linked_settings_and_leaves_them_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::create_dir_all(&a).unwrap();
    fs::create_dir_all(&b).unwrap();
    let first = a.join("settings.json");
    let second = b.join("settings.json");
    fs::write(&first, FIXTURE).unwrap();
    if fs::hard_link(&first, &second).is_err() {
        eprintln!("skipped: hard links are not supported here");
        return;
    }
    for path in [&first, &second] {
        let err = install_hooks_file(path, &cmd(), "S").unwrap_err();
        assert!(matches!(err, SettingsError::Refused(_)), "{err:?}");
        assert!(err.to_string().contains("hard link"), "{err}");
    }
    assert!(matches!(remove_hooks_file(&first, "S"), Err(SettingsError::Refused(_))));
    assert_eq!(fs::read_to_string(&first).unwrap(), FIXTURE);
    assert_eq!(fs::read_to_string(&second).unwrap(), FIXTURE);
    assert_eq!(fs::read_dir(&a).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&b).unwrap().count(), 1);
    // Still one file with two names.
    fs::write(&first, "{}").unwrap();
    assert_eq!(fs::read_to_string(&second).unwrap(), "{}");
}

// ---------- uninstall safety ----------

#[test]
fn file_remove_is_a_strict_no_op_when_no_entries_exist() {
    // The uninstaller runs the remove action unconditionally: on a profile that
    // never had our hooks it must not write, back up, or even touch the file.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, FIXTURE).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(30));

    for _ in 0..2 {
        let outcome = remove_hooks_file(&path, "S").unwrap();
        assert!(!outcome.changed);
        assert!(outcome.backup.is_none());
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), FIXTURE);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified, "file must not be rewritten");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "no backup, no temp file");
}

#[test]
fn file_remove_skips_a_file_it_cannot_parse_without_touching_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, "{ // jsonc\n}").unwrap();
    assert!(remove_hooks_file(&path, "S").is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "{ // jsonc\n}");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn lists_the_executables_our_entries_point_to() {
    use cpanel_core::settings::installed_hook_commands;
    assert!(installed_hook_commands(FIXTURE).is_empty());
    assert!(installed_hook_commands("{ broken").is_empty());
    let installed = install_hooks(FIXTURE, &cmd()).unwrap();
    assert_eq!(installed_hook_commands(&installed), vec!["C:/Apps/Claude Panel/cpanel-hook.exe".to_string()]);
    // Entries left behind by another copy of the application are listed too, once each.
    let mixed = r#"{"hooks":{"Stop":[{"hooks":[
        {"type":"command","command":"D:\\old\\cpanel-hook.exe"},
        {"type":"command","command":"D:\\old\\cpanel-hook.exe"},
        {"type":"command","command":"other.exe"}]}],
        "Notification":[{"hooks":[{"type":"command","command":"E:/new/cpanel-hook.exe"}]}]}}"#;
    assert_eq!(
        installed_hook_commands(mixed),
        vec!["D:\\old\\cpanel-hook.exe".to_string(), "E:/new/cpanel-hook.exe".to_string()]
    );
}
