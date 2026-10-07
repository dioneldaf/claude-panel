//! View model shared with the frontend.

use serde::{Deserialize, Serialize};

/// What a session is doing, from the user's point of view.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// The model is generating or running tools.
    Working,
    /// A permission prompt, question or elicitation is pending.
    NeedsYou,
    /// The turn finished; the session awaits the next prompt.
    Done,
    /// The main agent is idle but subagents are still running.
    WaitingSubagent,
    /// The main agent is idle but a background shell or monitor is still running.
    WaitingProcess,
    /// Idle for a long time.
    Paused,
}

/// One row of the panel.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionView {
    pub id: String,
    pub name: String,
    pub cwd: String,
    /// Last path component of `cwd`.
    pub folder: String,
    /// Tag of the config directory the session belongs to (for example `claude-work`).
    pub profile: String,
    pub pid: u32,
    pub state: SessionState,
    /// Epoch milliseconds at which `state` was entered.
    pub since: u64,
    /// Raw registry status, kept for diagnostics.
    pub raw_status: Option<String>,
    /// What the session is blocked on, when the registry says so.
    pub waiting_for: Option<String>,
    /// Whether at least one hook event was received for this session.
    pub hooks_seen: bool,
    pub subagents: u32,
    pub background: u32,
}

/// Hook installation state of one config directory.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProfileView {
    pub tag: String,
    pub dir: String,
    /// `installed`, `partial` or `not_installed`.
    pub hooks: String,
}

/// Everything the frontend needs to render.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Snapshot {
    pub sessions: Vec<SessionView>,
    pub profiles: Vec<ProfileView>,
    pub demo: bool,
    /// False when the hook listener could not bind its port.
    pub listener_ok: bool,
}

/// Last path component of a Windows or POSIX path.
pub fn folder_of(cwd: &str) -> String {
    cwd.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or("").to_string()
}
