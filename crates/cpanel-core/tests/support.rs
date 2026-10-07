use cpanel_core::eventlog::EventLog;
use cpanel_core::textdiff::diff_lines;
use std::fs;

#[test]
fn event_log_appends_one_line_per_event() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("events.jsonl");
    let log = EventLog::new(path.clone(), 1024);
    log.append("{\"a\":1}");
    log.append("line with\nnewline");
    assert_eq!(fs::read_to_string(&path).unwrap(), "{\"a\":1}\nline with newline\n");
}

#[test]
fn event_log_rotates_to_a_single_previous_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.jsonl");
    let log = EventLog::new(path.clone(), 50);
    for i in 0..30 {
        log.append(&format!("event-number-{i:02}"));
    }
    let current = fs::read_to_string(&path).unwrap();
    let previous = fs::read_to_string(dir.path().join("events.jsonl.1")).unwrap();
    assert!(current.len() <= 50 + 16, "current file stays near the limit");
    assert!(previous.len() <= 50 + 16);
    assert!(current.ends_with("event-number-29\n"));
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2, "only one rotated generation is kept");
}

#[test]
fn event_log_failures_are_silent() {
    let dir = tempfile::tempdir().unwrap();
    // The target path is a directory: appending cannot work and must not panic.
    EventLog::new(dir.path().to_path_buf(), 10).append("x");
}

#[test]
fn diff_reports_only_changed_lines_with_markers() {
    let old = "a\nb\nc\nd\n";
    let new = "a\nb\nX\nY\nc\n";
    assert_eq!(diff_lines(old, new), "+ X\n+ Y\n- d\n");
    assert_eq!(diff_lines(old, old), "");
    assert_eq!(diff_lines("", "one\n"), "+ one\n");
}
