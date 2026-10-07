//! Compact summary of a Claude Code hook payload and its wire format.
//!
//! The summary deliberately carries no prompt text and no tool input values: only
//! the fields the reducer needs plus key *names*, which are enough to tune the
//! mapping later from the debug log.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Upper bound of one encoded summary; safely below the loopback datagram limit.
pub const MAX_DATAGRAM: usize = 1400;

const MAX_ID: usize = 80;
const MAX_CWD: usize = 260;
const MAX_NAME: usize = 64;
const MAX_MESSAGE: usize = 160;
const MAX_KEYS: usize = 16;
const MAX_KEY_LEN: usize = 32;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct HookSummary {
    #[serde(rename = "e")]
    pub event: String,
    #[serde(rename = "s")]
    pub session_id: String,
    #[serde(rename = "c", default, skip_serializing_if = "String::is_empty")]
    pub cwd: String,
    /// Config directory tag passed to the hook through `--profile`.
    #[serde(rename = "p", default, skip_serializing_if = "String::is_empty")]
    pub profile: String,
    /// Epoch milliseconds, stamped by the hook process.
    #[serde(rename = "t", default)]
    pub ts: u64,
    #[serde(rename = "tool", default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// `tool_input.run_in_background == true`.
    #[serde(rename = "bg", default, skip_serializing_if = "is_false")]
    pub background: bool,
    #[serde(rename = "n", default, skip_serializing_if = "Option::is_none")]
    pub notification_type: Option<String>,
    #[serde(rename = "a", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(rename = "at", default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    /// Notification text (never a prompt).
    #[serde(rename = "m", default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Top-level key names of the payload.
    #[serde(rename = "k", default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
    /// Key names of `tool_input`.
    #[serde(rename = "ik", default, skip_serializing_if = "Vec::is_empty")]
    pub input_keys: Vec<String>,
}

fn is_false(v: &bool) -> bool {
    !*v
}

fn clip(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn text(v: &Value, key: &str, max: usize) -> Option<String> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(|s| clip(s, max))
}

fn key_names(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_object)
        .map(|o| o.keys().take(MAX_KEYS).map(|k| clip(k, MAX_KEY_LEN)).collect())
        .unwrap_or_default()
}

/// Extracts a summary from the JSON a hook receives on stdin.
/// Returns `None` unless the payload is an object with a session id and an event name.
pub fn summarize(raw: &str, profile: &str, ts: u64) -> Option<HookSummary> {
    let v: Value = serde_json::from_str(raw.trim_start_matches('\u{feff}')).ok()?;
    summarize_value(&v, profile, ts)
}

pub fn summarize_value(v: &Value, profile: &str, ts: u64) -> Option<HookSummary> {
    if !v.is_object() {
        return None;
    }
    let tool_input = v.get("tool_input");
    Some(HookSummary {
        event: text(v, "hook_event_name", MAX_NAME)?,
        session_id: text(v, "session_id", MAX_ID)?,
        cwd: text(v, "cwd", MAX_CWD).unwrap_or_default(),
        profile: clip(profile, MAX_NAME),
        ts,
        tool: text(v, "tool_name", MAX_NAME),
        background: tool_input
            .and_then(|i| i.get("run_in_background"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        notification_type: text(v, "notification_type", MAX_NAME),
        agent_id: text(v, "agent_id", MAX_ID),
        agent_type: text(v, "agent_type", MAX_NAME),
        message: if v.get("hook_event_name").and_then(Value::as_str) == Some("Notification") {
            text(v, "message", MAX_MESSAGE)
        } else {
            None
        },
        keys: key_names(Some(v)),
        input_keys: key_names(tool_input),
    })
}

/// Encodes a summary as one UTF-8 JSON datagram no larger than [`MAX_DATAGRAM`].
pub fn encode(summary: &HookSummary) -> Vec<u8> {
    let mut s = summary.clone();
    loop {
        let bytes = serde_json::to_vec(&s).unwrap_or_default();
        if bytes.len() <= MAX_DATAGRAM {
            return bytes;
        }
        // Drop the least important data first. Non-ASCII text can exceed the
        // character-based limits, so shrink until the datagram fits.
        if !s.keys.is_empty() || !s.input_keys.is_empty() {
            s.keys.clear();
            s.input_keys.clear();
        } else if s.message.is_some() {
            s.message = None;
        } else if !s.cwd.is_empty() {
            s.cwd.clear();
        } else {
            let half = |t: &str| clip(t, t.chars().count() / 2);
            s.session_id = half(&s.session_id);
            s.profile = half(&s.profile);
            s.event = half(&s.event);
            s.tool = s.tool.as_deref().map(half);
            s.notification_type = s.notification_type.as_deref().map(half);
            s.agent_id = s.agent_id.as_deref().map(half);
            s.agent_type = s.agent_type.as_deref().map(half);
        }
    }
}

/// Validates a received datagram and returns the summary together with the line
/// to log. The line is re-serialised from the decoded, re-clipped summary, so
/// unknown fields and raw bytes from whoever sent the datagram never reach disk.
pub fn accept(datagram: &[u8]) -> Option<(HookSummary, String)> {
    let s = decode(datagram)?;
    let opt = |v: Option<String>, max: usize| v.map(|t| clip(&t, max));
    let summary = HookSummary {
        event: clip(&s.event, MAX_NAME),
        session_id: clip(&s.session_id, MAX_ID),
        cwd: clip(&s.cwd, MAX_CWD),
        profile: clip(&s.profile, MAX_NAME),
        ts: s.ts,
        tool: opt(s.tool, MAX_NAME),
        background: s.background,
        notification_type: opt(s.notification_type, MAX_NAME),
        agent_id: opt(s.agent_id, MAX_ID),
        agent_type: opt(s.agent_type, MAX_NAME),
        message: opt(s.message, MAX_MESSAGE),
        keys: s.keys.iter().take(MAX_KEYS).map(|k| clip(k, MAX_KEY_LEN)).collect(),
        input_keys: s.input_keys.iter().take(MAX_KEYS).map(|k| clip(k, MAX_KEY_LEN)).collect(),
    };
    let line = String::from_utf8(encode(&summary)).ok()?;
    Some((summary, line))
}

/// Decodes a datagram produced by [`encode`].
pub fn decode(bytes: &[u8]) -> Option<HookSummary> {
    let s: HookSummary = serde_json::from_slice(bytes).ok()?;
    (!s.event.is_empty() && !s.session_id.is_empty()).then_some(s)
}
