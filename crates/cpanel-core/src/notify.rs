//! Decides when a state transition deserves an OS notification. Pure: the caller
//! supplies successive session views and the clock, and shows whatever comes back.

use crate::model::{SessionState, SessionView};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct NotifierConfig {
    /// How long a session must stay `done` before the notice fires (swallows flaps).
    pub settle_done_ms: u64,
    /// Same for `needs_you`; shorter because it is urgent.
    pub settle_needs_you_ms: u64,
    /// Minimum time between two notices of the same kind for one session.
    pub min_gap_ms: u64,
}

impl Default for NotifierConfig {
    fn default() -> Self {
        Self { settle_done_ms: 1500, settle_needs_you_ms: 400, min_gap_ms: 3000 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoticeKind {
    NeedsYou,
    Done,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub session_id: String,
    pub name: String,
    pub folder: String,
}

struct Track {
    state: SessionState,
    entered_at: u64,
    /// Last state that was held long enough to count; `None` until the first change.
    came_from: Option<SessionState>,
    handled: bool,
    last_fired: HashMap<NoticeKind, u64>,
}

pub struct Notifier {
    config: NotifierConfig,
    tracks: HashMap<String, Track>,
}

impl Notifier {
    pub fn new(config: NotifierConfig) -> Self {
        Self { config, tracks: HashMap::new() }
    }

    fn settle(&self, state: SessionState) -> u64 {
        match state {
            SessionState::Done => self.config.settle_done_ms,
            SessionState::NeedsYou => self.config.settle_needs_you_ms,
            _ => 0,
        }
    }

    /// Feeds the current sessions and returns the notices that are due now.
    pub fn observe(&mut self, sessions: &[SessionView], now: u64) -> Vec<Notice> {
        self.tracks.retain(|id, _| sessions.iter().any(|s| &s.id == id));
        let mut due = Vec::new();
        for s in sessions {
            let settle_new = self.settle(s.state);
            let Some(track) = self.tracks.get_mut(&s.id) else {
                // First sight is a baseline, never a transition.
                self.tracks.insert(
                    s.id.clone(),
                    Track { state: s.state, entered_at: now, came_from: None, handled: true, last_fired: HashMap::new() },
                );
                continue;
            };
            if track.state != s.state {
                let held = now.saturating_sub(track.entered_at);
                let settle_old = match track.state {
                    SessionState::Done => self.config.settle_done_ms,
                    SessionState::NeedsYou => self.config.settle_needs_you_ms,
                    _ => 0,
                };
                if held >= settle_old {
                    track.came_from = Some(track.state);
                }
                track.state = s.state;
                track.entered_at = now;
                track.handled = false;
            }
            if track.handled || now.saturating_sub(track.entered_at) < settle_new {
                continue;
            }
            track.handled = true;
            let kind = match (s.state, track.came_from) {
                (SessionState::NeedsYou, Some(from)) if from != SessionState::NeedsYou => NoticeKind::NeedsYou,
                (
                    SessionState::Done,
                    Some(
                        SessionState::Working
                        | SessionState::NeedsYou
                        | SessionState::WaitingSubagent
                        | SessionState::WaitingProcess,
                    ),
                ) => NoticeKind::Done,
                _ => continue,
            };
            let recently = track.last_fired.get(&kind).is_some_and(|at| now.saturating_sub(*at) < self.config.min_gap_ms);
            if recently {
                continue;
            }
            track.last_fired.insert(kind, now);
            due.push(Notice { kind, session_id: s.id.clone(), name: s.name.clone(), folder: s.folder.clone() });
        }
        due
    }
}
