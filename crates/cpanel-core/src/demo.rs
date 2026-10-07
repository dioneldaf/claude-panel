//! Demo mode: fake sessions cycling through every state so the UI can be judged
//! without real Claude Code sessions.

use crate::model::{SessionState, SessionView};

/// Time each demo session spends in one state.
pub const DEMO_STEP_MS: u64 = 4000;

const STATES: [SessionState; 6] = [
    SessionState::Working,
    SessionState::NeedsYou,
    SessionState::Done,
    SessionState::WaitingSubagent,
    SessionState::WaitingProcess,
    SessionState::Paused,
];

const SESSIONS: [(&str, &str, &str); 6] = [
    ("storefront-12", "C:\\work\\storefront", "claude"),
    ("billing-api-03", "C:\\work\\billing-api", "claude"),
    ("docs-site-41", "C:\\work\\docs-site", "claude-work"),
    ("data-pipeline-07", "C:\\work\\data-pipeline", "claude-work"),
    ("mobile-app-22", "C:\\work\\mobile-app", "claude"),
    ("infra-09", "C:\\work\\infra", "claude-work"),
];

/// Deterministic fake sessions for the given clock. At any moment all six states
/// are on screen, and each session advances one state every [`DEMO_STEP_MS`].
pub fn demo_sessions(now: u64) -> Vec<SessionView> {
    let step = now / DEMO_STEP_MS;
    SESSIONS
        .iter()
        .enumerate()
        .map(|(i, (name, cwd, profile))| {
            let state = STATES[((step + i as u64) % STATES.len() as u64) as usize];
            SessionView {
                id: format!("demo-{i}"),
                name: name.to_string(),
                cwd: cwd.to_string(),
                folder: crate::model::folder_of(cwd),
                profile: profile.to_string(),
                pid: 1000 + i as u32,
                state,
                since: step * DEMO_STEP_MS,
                raw_status: None,
                waiting_for: None,
                hooks_seen: true,
                subagents: u32::from(state == SessionState::WaitingSubagent) * 2,
                background: u32::from(state == SessionState::WaitingProcess),
            }
        })
        .collect()
}

/// Length of one demo cycle in `--demo-cycle` mode.
pub const DEMO_CYCLE_MS: u64 = 26_000;
/// Final part of each cycle during which there are no sessions at all.
pub const DEMO_CYCLE_EMPTY_MS: u64 = 10_000;

/// Like [`demo_sessions`], but every cycle ends with a stretch of zero sessions so
/// the exit and entrance animations can be judged without real sessions.
pub fn demo_cycle_sessions(now: u64) -> Vec<SessionView> {
    if now % DEMO_CYCLE_MS >= DEMO_CYCLE_MS - DEMO_CYCLE_EMPTY_MS {
        Vec::new()
    } else {
        demo_sessions(now)
    }
}
