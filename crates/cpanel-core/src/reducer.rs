//! Pure state reducer: registry snapshots + hook events + clock -> per-session state.
//!
//! Two sources feed each session:
//!
//! * the **registry** gives discovery and a coarse status (`busy` / `idle` / `shell`);
//! * **hooks** give fine-grained phases (permission prompts, subagents, end of turn).
//!
//! Whichever source reported a *change* most recently decides between working and
//! idle. A registry rewrite that keeps the same status value is not a change, so it
//! cannot hide a pending permission prompt; a genuine registry transition (for
//! example an interrupt, which fires no hook) overrides a stale hook phase.

use crate::hook_event::HookSummary;
use crate::model::{folder_of, SessionState, SessionView};
use crate::registry::RegistryEntry;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct ReducerConfig {
    /// Idle time after which a session is shown as paused.
    pub paused_after_ms: u64,
    /// Subagent counters are ignored once a session has been silent for this long.
    pub subagent_ttl_ms: u64,
    /// Hook state of sessions absent from the registry is dropped after this long.
    pub orphan_ttl_ms: u64,
    /// Upper bound of session ids tracked from hooks alone (not in the registry).
    /// Anyone on the machine can send datagrams, so this bounds memory.
    pub max_hook_only_sessions: usize,
}

impl Default for ReducerConfig {
    fn default() -> Self {
        Self { paused_after_ms: 10 * 60_000, subagent_ttl_ms: 30 * 60_000, orphan_ttl_ms: 60 * 60_000, max_hook_only_sessions: 64 }
    }
}

/// Coarse meaning of a raw registry status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Coarse {
    Busy,
    Idle,
    /// Idle with a background shell still running.
    Shell,
    /// Blocked on the user (a prompt is showing); comes with a `waitingFor` field.
    Waiting,
    Unknown,
}

fn coarse_of(status: Option<&str>) -> Coarse {
    match status.map(str::to_ascii_lowercase).as_deref() {
        Some("busy") => Coarse::Busy,
        Some("idle") => Coarse::Idle,
        Some("shell") => Coarse::Shell,
        Some("waiting") => Coarse::Waiting,
        _ => Coarse::Unknown,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MainPhase {
    Working,
    Idle,
}

/// A pending request for the user's attention.
#[derive(Clone, Debug)]
struct Attention {
    at: u64,
    /// Subagent that raised it, when known.
    agent: Option<String>,
}

/// Everything learned from hooks about one session id.
#[derive(Clone, Debug, Default)]
struct HookTrack {
    main: Option<(MainPhase, u64)>,
    attention: Option<Attention>,
    subagents: u32,
    background: u32,
    background_at: u64,
    last_event_at: u64,
}

impl HookTrack {
    /// Timestamp of the newest phase information, if any.
    fn phase_at(&self) -> Option<u64> {
        let main = self.main.map(|(_, at)| at);
        let attention = self.attention.as_ref().map(|a| a.at);
        main.max(attention)
    }
}

#[derive(Clone, Debug)]
struct RegistryTrack {
    entry: RegistryEntry,
    /// When the status *value* last changed (not when the file was rewritten).
    changed_at: u64,
    /// Last derived state and the moment it was entered.
    shown: Option<(SessionState, u64)>,
}

pub struct Reducer {
    config: ReducerConfig,
    sessions: HashMap<String, RegistryTrack>,
    hooks: HashMap<String, HookTrack>,
}

const ASKING_TOOLS: [&str; 2] = ["AskUserQuestion", "ExitPlanMode"];
/// Tools that leave something running after the turn. `Monitor` is unverified.
const BACKGROUND_TOOLS: [&str; 2] = ["Bash", "Monitor"];
const ATTENTION_NOTIFICATIONS: [&str; 3] = ["permission_prompt", "agent_needs_input", "elicitation_dialog"];

impl Reducer {
    pub fn new(config: ReducerConfig) -> Self {
        Self { config, sessions: HashMap::new(), hooks: HashMap::new() }
    }

    /// Replaces the set of live sessions with a fresh registry scan.
    pub fn apply_registry(&mut self, entries: Vec<RegistryEntry>, now: u64) {
        let mut next = HashMap::with_capacity(entries.len());
        for entry in entries {
            let reported = entry.status_updated_at.or(entry.updated_at).or(entry.started_at).unwrap_or(now);
            let track = match self.sessions.remove(&entry.session_id) {
                Some(old) if old.entry.status == entry.status => RegistryTrack { entry, ..old },
                Some(old) => RegistryTrack { entry, changed_at: reported.max(old.changed_at + 1), shown: old.shown },
                None => RegistryTrack { entry, changed_at: reported, shown: None },
            };
            let coarse = coarse_of(track.entry.status.as_deref());
            if let Some(hook) = self.hooks.get_mut(&track.entry.session_id) {
                // Plain idle reported after a background task started is evidence that it finished.
                if coarse == Coarse::Idle && hook.background > 0 && track.changed_at > hook.background_at {
                    hook.background = 0;
                }
                // The registry moved on to busy or idle after a prompt was raised: the
                // prompt was answered. Forget it so it cannot resurface later.
                let answered = matches!(coarse, Coarse::Busy | Coarse::Idle | Coarse::Shell)
                    && hook.attention.as_ref().is_some_and(|a| track.changed_at > a.at);
                if answered {
                    hook.attention = None;
                }
            }
            next.insert(track.entry.session_id.clone(), track);
        }
        self.sessions = next;

        let ttl = self.config.orphan_ttl_ms;
        let sessions = &self.sessions;
        self.hooks.retain(|id, h| sessions.contains_key(id) || now.saturating_sub(h.last_event_at) < ttl);
    }

    /// Feeds one hook event.
    pub fn apply_hook(&mut self, ev: &HookSummary, now: u64) {
        let ts = if ev.ts == 0 { now } else { ev.ts };
        if ev.event == "SessionEnd" {
            self.hooks.remove(&ev.session_id);
            return;
        }
        let track = self.hooks.entry(ev.session_id.clone()).or_default();
        track.last_event_at = track.last_event_at.max(ts);

        let from_main = ev.agent_id.is_none();
        let tool = ev.tool.as_deref().unwrap_or("");
        let set_main = |track: &mut HookTrack, phase: MainPhase| {
            if from_main {
                track.main = Some((phase, ts));
            }
        };
        // Progress by the agent that asked (or by the main agent when the origin is
        // unknown) means the user has answered.
        let clear_attention = |track: &mut HookTrack, force: bool| {
            let cleared = match &track.attention {
                Some(a) => force || a.agent.is_none() || a.agent == ev.agent_id,
                None => false,
            };
            if cleared {
                track.attention = None;
            }
        };
        let raise_attention = |track: &mut HookTrack| {
            // Keep the first origin: a Notification usually follows the PermissionRequest.
            if track.attention.is_none() {
                track.attention = Some(Attention { at: ts, agent: ev.agent_id.clone() });
            }
        };

        match ev.event.as_str() {
            "SessionStart" => {
                *track = HookTrack { main: Some((MainPhase::Idle, ts)), last_event_at: ts, ..Default::default() };
            }
            "UserPromptSubmit" => {
                clear_attention(track, true);
                track.main = Some((MainPhase::Working, ts));
            }
            "PreToolUse" => {
                clear_attention(track, false);
                set_main(track, MainPhase::Working);
                if ASKING_TOOLS.contains(&tool) {
                    raise_attention(track);
                }
                if (ev.background && BACKGROUND_TOOLS.contains(&tool)) || tool == "Monitor" {
                    track.background += 1;
                    track.background_at = ts;
                }
            }
            "PostToolUse" | "PostToolUseFailure" | "PermissionDenied" | "ElicitationResult" => {
                clear_attention(track, false);
                set_main(track, MainPhase::Working);
            }
            "PreCompact" | "PostCompact" => set_main(track, MainPhase::Working),
            "PermissionRequest" | "Elicitation" => raise_attention(track),
            "Notification" => {
                let kind = ev.notification_type.as_deref().unwrap_or("");
                if ATTENTION_NOTIFICATIONS.contains(&kind) {
                    raise_attention(track);
                } else if kind == "idle_prompt" && !matches!(track.main, Some((MainPhase::Idle, _))) {
                    // A missed Stop: the session is waiting at the prompt.
                    track.main = Some((MainPhase::Idle, ts));
                }
            }
            "Stop" | "StopFailure" => {
                clear_attention(track, true);
                if from_main {
                    track.main = Some((MainPhase::Idle, ts));
                }
            }
            "SubagentStart" => track.subagents += 1,
            "SubagentStop" => track.subagents = track.subagents.saturating_sub(1),
            // TaskCreated, TaskCompleted and future events only count as activity.
            _ => {}
        }
        self.evict_hook_only_sessions();
    }

    /// Drops the least recently active hook-only sessions beyond the cap.
    fn evict_hook_only_sessions(&mut self) {
        let sessions = &self.sessions;
        let mut orphans: Vec<(u64, String)> = self
            .hooks
            .iter()
            .filter(|(id, _)| !sessions.contains_key(*id))
            .map(|(id, h)| (h.last_event_at, id.clone()))
            .collect();
        let excess = orphans.len().saturating_sub(self.config.max_hook_only_sessions);
        if excess == 0 {
            return;
        }
        orphans.sort();
        for (_, id) in orphans.into_iter().take(excess) {
            self.hooks.remove(&id);
        }
    }

    /// Number of session ids with hook state (diagnostics and tests).
    pub fn tracked_hook_sessions(&self) -> usize {
        self.hooks.len()
    }

    /// Derives the current view. Takes `&mut self` only to remember when each
    /// session entered its state.
    pub fn snapshot(&mut self, now: u64) -> Vec<SessionView> {
        let mut views: Vec<(u64, SessionView)> = Vec::with_capacity(self.sessions.len());
        for (id, track) in self.sessions.iter_mut() {
            let hook = self.hooks.get(id);
            let (state, cause_at) = derive(&self.config, track, hook, now);
            let since = match track.shown {
                Some((prev, since)) if prev == state => since,
                // Pausing is the same idle stint growing old, not a new event.
                Some((SessionState::Done, since)) if state == SessionState::Paused => since,
                Some(_) => {
                    let last_input = hook.map_or(0, |h| h.last_event_at).max(track.changed_at);
                    cause_at.max(last_input).min(now)
                }
                None => cause_at.min(now),
            };
            track.shown = Some((state, since));

            let e = &track.entry;
            let folder = folder_of(&e.cwd);
            let subagents = hook.map_or(0, |h| live_subagents(&self.config, h, now));
            views.push((
                e.started_at.unwrap_or(u64::MAX),
                SessionView {
                    id: id.clone(),
                    name: e.name.clone().unwrap_or_else(|| folder.clone()),
                    cwd: e.cwd.clone(),
                    folder,
                    profile: e.profile.clone(),
                    pid: e.pid,
                    state,
                    since,
                    raw_status: e.status.clone(),
                    waiting_for: e.waiting_for.clone(),
                    hooks_seen: hook.is_some(),
                    subagents,
                    background: hook.map_or(0, |h| h.background),
                },
            ));
        }
        views.sort_by(|a, b| (a.0, a.1.pid, &a.1.id).cmp(&(b.0, b.1.pid, &b.1.id)));
        views.into_iter().map(|(_, v)| v).collect()
    }
}

fn live_subagents(config: &ReducerConfig, hook: &HookTrack, now: u64) -> u32 {
    if now.saturating_sub(hook.last_event_at) > config.subagent_ttl_ms {
        0
    } else {
        hook.subagents
    }
}

/// Returns the state and the timestamp of whatever caused it.
fn derive(config: &ReducerConfig, track: &RegistryTrack, hook: Option<&HookTrack>, now: u64) -> (SessionState, u64) {
    let coarse = coarse_of(track.entry.status.as_deref());
    let hook_phase_at = hook.and_then(HookTrack::phase_at);
    let registry_decides = match hook_phase_at {
        None => true,
        Some(at) => coarse != Coarse::Unknown && track.changed_at > at,
    };

    let idle = |idle_since: u64| -> (SessionState, u64) {
        let subagents = hook.map_or(0, |h| live_subagents(config, h, now));
        let background = hook.map_or(0, |h| h.background);
        if subagents > 0 {
            (SessionState::WaitingSubagent, idle_since)
        } else if background > 0 || coarse == Coarse::Shell {
            (SessionState::WaitingProcess, idle_since)
        } else if now.saturating_sub(idle_since) >= config.paused_after_ms {
            (SessionState::Paused, idle_since)
        } else {
            (SessionState::Done, idle_since)
        }
    };

    if registry_decides {
        return match coarse {
            Coarse::Busy => (SessionState::Working, track.changed_at),
            Coarse::Waiting => (SessionState::NeedsYou, track.changed_at),
            Coarse::Idle | Coarse::Shell | Coarse::Unknown => idle(track.changed_at),
        };
    }
    let hook = hook.expect("hook state exists when the registry does not decide");
    if let Some(attention) = &hook.attention {
        return (SessionState::NeedsYou, attention.at);
    }
    match hook.main {
        Some((MainPhase::Working, at)) => (SessionState::Working, at),
        Some((MainPhase::Idle, at)) => idle(at),
        // Only subagent activity was seen so far: fall back to the registry.
        None => match coarse {
            Coarse::Busy => (SessionState::Working, track.changed_at),
            Coarse::Waiting => (SessionState::NeedsYou, track.changed_at),
            _ => idle(track.changed_at),
        },
    }
}
