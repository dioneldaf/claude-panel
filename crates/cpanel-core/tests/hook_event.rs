use cpanel_core::hook_event::{decode, encode, summarize, MAX_DATAGRAM};

#[test]
fn summarizes_a_pre_tool_use_event() {
    let raw = r#"{"session_id":"abc","cwd":"C:\\work\\demo","hook_event_name":"PreToolUse",
        "transcript_path":"C:\\x.jsonl","permission_mode":"default",
        "tool_name":"Bash","tool_input":{"command":"sleep 100","run_in_background":true}}"#;
    let s = summarize(raw, "claude-work", 42).expect("summary");
    assert_eq!(s.event, "PreToolUse");
    assert_eq!(s.session_id, "abc");
    assert_eq!(s.cwd, "C:\\work\\demo");
    assert_eq!(s.profile, "claude-work");
    assert_eq!(s.ts, 42);
    assert_eq!(s.tool.as_deref(), Some("Bash"));
    assert!(s.background);
    assert!(s.keys.contains(&"tool_input".to_string()));
    assert_eq!(s.input_keys, vec!["command".to_string(), "run_in_background".to_string()]);
}

#[test]
fn never_carries_tool_input_values_or_prompts() {
    let raw = r#"{"session_id":"abc","hook_event_name":"UserPromptSubmit","prompt":"TOP-SECRET-PROMPT",
        "tool_input":{"command":"echo TOP-SECRET-COMMAND"}}"#;
    let s = summarize(raw, "p", 1).unwrap();
    let wire = String::from_utf8(encode(&s)).unwrap();
    assert!(!wire.contains("TOP-SECRET"));
}

#[test]
fn summarizes_notification_and_subagent_fields() {
    let raw = r#"{"session_id":"s","hook_event_name":"Notification","notification_type":"permission_prompt","message":"Claude needs your permission to use Bash"}"#;
    let s = summarize(raw, "p", 1).unwrap();
    assert_eq!(s.notification_type.as_deref(), Some("permission_prompt"));
    assert_eq!(s.message.as_deref(), Some("Claude needs your permission to use Bash"));
    assert!(!s.background);

    let raw = r#"{"session_id":"s","hook_event_name":"SubagentStart","agent_id":"a1","agent_type":"Explore"}"#;
    let s = summarize(raw, "p", 1).unwrap();
    assert_eq!(s.agent_id.as_deref(), Some("a1"));
    assert_eq!(s.agent_type.as_deref(), Some("Explore"));
}

#[test]
fn rejects_garbage_and_incomplete_payloads() {
    assert!(summarize("", "p", 1).is_none());
    assert!(summarize("not json", "p", 1).is_none());
    assert!(summarize("[1,2]", "p", 1).is_none());
    assert!(summarize(r#"{"session_id":"s"}"#, "p", 1).is_none());
    assert!(summarize(r#"{"hook_event_name":"Stop"}"#, "p", 1).is_none());
}

#[test]
fn tolerates_wrongly_typed_optional_fields() {
    let raw = r#"{"session_id":"s","hook_event_name":"PreToolUse","tool_name":7,"tool_input":"x","cwd":null}"#;
    let s = summarize(raw, "p", 1).unwrap();
    assert_eq!(s.tool, None);
    assert!(!s.background);
    assert_eq!(s.cwd, "");
}

#[test]
fn round_trips_and_stays_within_one_datagram() {
    let long = "x".repeat(5000);
    let raw = format!(
        r#"{{"session_id":"{long}","hook_event_name":"Notification","cwd":"{long}","message":"{long}","notification_type":"{long}","tool_name":"{long}"}}"#
    );
    let s = summarize(&raw, &long, 9).unwrap();
    let wire = encode(&s);
    assert!(wire.len() <= MAX_DATAGRAM, "datagram too large: {}", wire.len());
    assert_eq!(decode(&wire), Some(s));
}

#[test]
fn decode_rejects_invalid_bytes() {
    assert_eq!(decode(b"\xff\xfe"), None);
    assert_eq!(decode(b"{}"), None);
}

#[test]
fn accept_validates_first_and_yields_only_the_compact_summary_for_logging() {
    use cpanel_core::hook_event::accept;
    // Anything that is not a valid summary is rejected and produces nothing to log.
    assert!(accept(b"\xff\xfe raw bytes").is_none());
    assert!(accept(b"{\"unrelated\":\"SECRET\"}").is_none());
    assert!(accept(&vec![b'x'; 4000]).is_none());

    // Unknown fields a sender might smuggle in never reach the log line.
    let datagram = br#"{"e":"Stop","s":"abc","t":5,"junk":"SECRET-PAYLOAD","tool_input":{"command":"SECRET"}}"#;
    let (summary, line) = accept(datagram).expect("valid summary");
    assert_eq!(summary.event, "Stop");
    assert_eq!(line, r#"{"e":"Stop","s":"abc","t":5}"#);
    assert!(!line.contains("SECRET"));

    // Oversized fields are clipped again on the way in.
    let big = format!(r#"{{"e":"Stop","s":"{}","m":"{}"}}"#, "s".repeat(300), "m".repeat(900));
    let (summary, line) = accept(big.as_bytes()).unwrap();
    assert_eq!(summary.session_id.len(), 80);
    assert!(line.len() <= MAX_DATAGRAM);
}
