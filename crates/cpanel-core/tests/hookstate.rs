use cpanel_core::hookstate::{decide, same_path, Decision, Entries, HookState, Record};
use cpanel_core::settings::{entries_state, install_hooks, remove_hooks_where, HookCommand};
use std::path::Path;

const MINE: &str = "C:/Users/example/AppData/Local/Claude Panel/cpanel-hook.exe";
const OTHER: &str = "D:/portable/claude-panel/cpanel-hook.exe";

fn record(exe: &str, restore: bool) -> Record {
    Record { file: "C:/Users/example/.claude/settings.json".into(), profile: "claude".into(), exe: exe.into(), restore }
}

#[test]
fn paths_compare_without_regard_to_case_or_slash_direction() {
    assert!(same_path(MINE, r"c:\users\EXAMPLE\appdata\local\claude panel\CPANEL-HOOK.EXE"));
    assert!(same_path(r"\\?\C:\a\b.exe", "C:/a/b.exe"));
    assert!(!same_path(MINE, OTHER));
}

// ---------- start-up repair decisions ----------

#[test]
fn entries_removed_by_our_own_uninstaller_are_restored() {
    // Upgrade (old uninstaller ran, new version starts) or a cancelled uninstall.
    assert_eq!(decide(&record(MINE, true), MINE, true, Entries::Absent), Decision::Repair);
}

#[test]
fn entries_the_user_deleted_by_hand_are_not_resurrected() {
    assert_eq!(decide(&record(MINE, false), MINE, true, Entries::Absent), Decision::Forget);
}

#[test]
fn dangling_entries_are_repaired_for_a_moved_installation() {
    // The record points at the previous location, which no longer exists.
    let old = "C:/Old Place/cpanel-hook.exe";
    assert_eq!(decide(&record(old, false), MINE, false, Entries::Dangling), Decision::Repair);
    assert_eq!(decide(&record(MINE, false), MINE, true, Entries::Dangling), Decision::Repair);
}

#[test]
fn healthy_entries_and_other_copies_are_left_alone() {
    assert_eq!(decide(&record(MINE, false), MINE, true, Entries::Valid), Decision::Keep);
    assert_eq!(decide(&record(MINE, true), MINE, true, Entries::Valid), Decision::Keep);
    // A record owned by another copy that still exists is never taken over.
    for entries in [Entries::Absent, Entries::Dangling, Entries::Valid] {
        assert_eq!(decide(&record(OTHER, true), MINE, true, entries), Decision::Keep, "{entries:?}");
    }
}

// ---------- the state file ----------

#[test]
fn records_are_upserted_by_file_and_can_be_forgotten() {
    let mut state = HookState::default();
    state.record_install("C:/Users/example/.claude/settings.json", "claude", MINE);
    state.record_install(r"c:\users\example\.claude\SETTINGS.JSON", "claude", OTHER);
    assert_eq!(state.managed.len(), 1, "same file, different spelling");
    assert_eq!(state.managed[0].exe, OTHER);
    assert!(!state.managed[0].restore);
    state.record_install("C:/Users/example/.claude-work/settings.json", "claude-work", MINE);
    assert_eq!(state.managed.len(), 2);
    state.forget("C:/Users/example/.claude/settings.json");
    assert_eq!(state.managed.len(), 1);
    assert_eq!(state.managed[0].profile, "claude-work");
}

#[test]
fn marking_for_restore_creates_the_record_when_hooks_predate_the_state_file() {
    let mut state = HookState::default();
    state.mark_restore("C:/Users/example/.claude/settings.json", "claude", MINE);
    assert_eq!(state.managed, vec![record(MINE, true)]);
    // A record owned by another existing copy is not overwritten.
    let mut state = HookState { managed: vec![record(OTHER, false)] };
    state.mark_restore_if_owned("C:/Users/example/.claude/settings.json", "claude", MINE, |exe| same_path(exe, MINE));
    assert_eq!(state.managed, vec![record(OTHER, false)]);
}

#[test]
fn the_state_file_round_trips_and_tolerates_damage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("hooks-state.json");
    assert_eq!(HookState::load(&path), HookState::default());
    let state = HookState { managed: vec![record(MINE, true)] };
    state.save(&path);
    assert_eq!(HookState::load(&path), state);
    std::fs::write(&path, "{ not json").unwrap();
    assert_eq!(HookState::load(&path), HookState::default());
    // An empty state leaves no file behind.
    HookState::default().save(&path);
    assert!(!path.exists());
}

// ---------- settings helpers used by the uninstaller ----------

fn cmd(exe: &str) -> HookCommand {
    HookCommand { exe: exe.into(), profile: "claude".into() }
}

#[test]
fn scoped_removal_deletes_only_the_matching_installation() {
    // Two copies each have their own entry for Stop.
    let text = format!(
        r#"{{"hooks":{{"Stop":[{{"hooks":[{{"type":"command","command":"{MINE}"}}]}},{{"hooks":[{{"type":"command","command":"{OTHER}"}}]}}]}}}}"#
    );
    let out = remove_hooks_where(&text, |command| same_path(command, MINE)).unwrap();
    assert!(!out.contains("AppData/Local/Claude Panel"));
    assert!(out.contains(OTHER), "the other copy keeps working");
    // Nothing matching: byte-identical, so no write and no backup follow.
    assert_eq!(remove_hooks_where(&out, |command| same_path(command, MINE)).unwrap(), out);
}

#[test]
fn entries_state_distinguishes_absent_dangling_and_valid() {
    assert_eq!(entries_state("{}", |_| true), Entries::Absent);
    assert_eq!(entries_state("{ broken", |_| true), Entries::Absent);
    let installed = install_hooks("{}", &cmd(MINE)).unwrap();
    assert_eq!(entries_state(&installed, |_| true), Entries::Valid);
    assert_eq!(entries_state(&installed, |_| false), Entries::Dangling);
    let _ = Path::new(MINE);
}
