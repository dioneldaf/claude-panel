use cpanel_core::registry::{
    discover_config_dirs, is_registry_file_name, parse_entry, scan_sessions_dir, ProcessProbe,
};
use std::fs;

struct Alive(Vec<u32>);
impl ProcessProbe for Alive {
    fn is_alive(&self, pid: u32, _proc_start: Option<u64>) -> bool {
        self.0.contains(&pid)
    }
}

const SAMPLE: &str = r#"{"pid":4242,"sessionId":"a1b2c3d4","cwd":"C:\\work\\example-app",
"startedAt":1700000000000,"procStart":"133500000000000000","version":"2.1.292","kind":"interactive",
"entrypoint":"cli","name":"example-app-07","status":"busy","updatedAt":1700000600500,"statusUpdatedAt":1700000600000}"#;

#[test]
fn only_pid_json_names_are_registry_files() {
    assert!(is_registry_file_name("4242.json"));
    assert!(is_registry_file_name("1.json"));
    assert!(!is_registry_file_name("4242.abcdef.key"));
    assert!(!is_registry_file_name("4242.key"));
    assert!(!is_registry_file_name("4242.json.key"));
    assert!(!is_registry_file_name("4242.json.bak"));
    assert!(!is_registry_file_name("abc.json"));
    assert!(!is_registry_file_name(".json"));
    assert!(!is_registry_file_name("67a8.json"));
    assert!(!is_registry_file_name("4242.JSONX"));
}

#[test]
fn parses_the_observed_format() {
    let e = parse_entry(SAMPLE, "claude-work").unwrap();
    assert_eq!(e.pid, 4242);
    assert_eq!(e.session_id, "a1b2c3d4");
    assert_eq!(e.cwd, "C:\\work\\example-app");
    assert_eq!(e.name.as_deref(), Some("example-app-07"));
    assert_eq!(e.status.as_deref(), Some("busy"));
    assert_eq!(e.status_updated_at, Some(1700000600000));
    assert_eq!(e.started_at, Some(1700000000000));
    assert_eq!(e.proc_start, Some(133500000000000000));
    assert_eq!(e.kind.as_deref(), Some("interactive"));
    assert_eq!(e.profile, "claude-work");
}

#[test]
fn keeps_the_reason_a_session_is_waiting() {
    let e = parse_entry(r#"{"pid":5,"sessionId":"s","status":"waiting","waitingFor":"permission"}"#, "p").unwrap();
    assert_eq!(e.status.as_deref(), Some("waiting"));
    assert_eq!(e.waiting_for.as_deref(), Some("permission"));
    // Unknown shapes are kept as compact, bounded JSON rather than dropped.
    let e = parse_entry(r#"{"pid":5,"sessionId":"s","waitingFor":{"kind":"tool","name":"Bash"}}"#, "p").unwrap();
    assert_eq!(e.waiting_for.as_deref(), Some(r#"{"kind":"tool","name":"Bash"}"#));
    let long = format!(r#"{{"pid":5,"sessionId":"s","waitingFor":"{}"}}"#, "x".repeat(500));
    assert_eq!(parse_entry(&long, "p").unwrap().waiting_for.unwrap().chars().count(), 120);
    let e = parse_entry(r#"{"pid":5,"sessionId":"s","waitingFor":null}"#, "p").unwrap();
    assert_eq!(e.waiting_for, None);
}

#[test]
fn degrades_gracefully_when_the_format_changes() {
    // Only pid and sessionId are required; everything else is optional or may change type.
    let e = parse_entry(r#"{"pid":5,"sessionId":"s","status":{"new":"shape"},"name":12}"#, "p").unwrap();
    assert_eq!(e.status, None);
    assert_eq!(e.name, None);
    assert_eq!(e.cwd, "");
    assert!(parse_entry(r#"{"sessionId":"s"}"#, "p").is_none());
    assert!(parse_entry(r#"{"pid":5}"#, "p").is_none());
    assert!(parse_entry("garbage", "p").is_none());
    assert!(parse_entry("[]", "p").is_none());
}

#[test]
fn scan_ignores_key_files_stale_pids_and_broken_files() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    fs::write(p.join("4242.json"), SAMPLE).unwrap();
    fs::write(p.join("100.json"), r#"{"pid":100,"sessionId":"alive-2","status":"idle"}"#).unwrap();
    // Stale: process no longer exists.
    fs::write(p.join("200.json"), r#"{"pid":200,"sessionId":"stale"}"#).unwrap();
    // Secrets: must never be considered, even when they contain a plausible entry.
    fs::write(p.join("4242.deadbeef.key"), r#"{"pid":4242,"sessionId":"FROM-KEY"}"#).unwrap();
    fs::write(p.join("300.json.key"), r#"{"pid":300,"sessionId":"FROM-KEY"}"#).unwrap();
    // Broken and mismatching files.
    fs::write(p.join("400.json"), "{ not json").unwrap();
    fs::write(p.join("500.json"), r#"{"pid":501,"sessionId":"pid-mismatch"}"#).unwrap();
    fs::create_dir(p.join("600.json")).unwrap();

    let probe = Alive(vec![4242, 100, 300, 400, 500, 501, 600]);
    let found = scan_sessions_dir(p, "prof", &probe);
    let ids: Vec<&str> = found.iter().map(|e| e.session_id.as_str()).collect();
    assert_eq!(ids, vec!["alive-2", "a1b2c3d4"], "sorted by pid, only live json entries");
    assert!(found.iter().all(|e| e.profile == "prof"));
}

#[test]
fn scan_of_a_missing_directory_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    assert!(scan_sessions_dir(&dir.path().join("nope"), "p", &Alive(vec![])).is_empty());
}

#[test]
fn discovers_every_claude_dir_with_a_sessions_folder() {
    let home = tempfile::tempdir().unwrap();
    let h = home.path();
    fs::create_dir_all(h.join(".claude/sessions")).unwrap();
    fs::create_dir_all(h.join(".claude-work/sessions")).unwrap();
    fs::create_dir_all(h.join(".claude-empty")).unwrap();
    fs::create_dir_all(h.join(".config/sessions")).unwrap();
    fs::create_dir_all(h.join("claude/sessions")).unwrap();
    fs::write(h.join(".claude.json"), "{}").unwrap();

    let found = discover_config_dirs(h);
    let tags: Vec<&str> = found.iter().map(|d| d.tag.as_str()).collect();
    assert_eq!(tags, vec!["claude", "claude-work"]);
    assert_eq!(found[1].path, h.join(".claude-work"));
}
