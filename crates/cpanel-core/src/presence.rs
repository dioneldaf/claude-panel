//! Decides whether the panel is on screen. Pure state machine: session counts,
//! animation completions and the clock go in; phase changes come out.
//!
//! The panel is hidden while there are no sessions. The first session triggers an
//! entrance animation, and the last one leaving triggers an exit animation after a
//! debounce, so a session restart or a registry blip does not cause a goodbye/hello
//! flap. Both animations can be interrupted by the opposite event.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// No sessions: the window is hidden and nothing renders.
    Hidden,
    /// The entrance animation is playing.
    Entering,
    Visible,
    /// The exit animation is playing; the window is hidden when it reports done.
    Exiting,
}

#[derive(Clone, Debug)]
pub struct PresenceConfig {
    /// How long the session count must stay at zero before the exit starts.
    pub exit_debounce_ms: u64,
    /// Upper bound for an animation; covers a frontend that never reports back.
    pub animation_timeout_ms: u64,
}

impl Default for PresenceConfig {
    fn default() -> Self {
        Self { exit_debounce_ms: 2500, animation_timeout_ms: 6000 }
    }
}

pub struct Presence {
    config: PresenceConfig,
    phase: Phase,
    phase_since: u64,
    /// Start of the current run of zero sessions.
    zero_since: Option<u64>,
}

impl Presence {
    pub fn new(config: PresenceConfig) -> Self {
        Self { config, phase: Phase::Hidden, phase_since: 0, zero_since: None }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    fn enter(&mut self, phase: Phase, now: u64) -> Option<Phase> {
        self.phase = phase;
        self.phase_since = now;
        Some(phase)
    }

    /// Feeds the current number of sessions. Also serves as the clock tick, so it
    /// must be called regularly. Returns the new phase when it changed.
    pub fn observe(&mut self, sessions: usize, now: u64) -> Option<Phase> {
        if sessions == 0 {
            self.zero_since.get_or_insert(now);
        } else {
            self.zero_since = None;
        }
        let gone = self.zero_since.is_some_and(|since| now.saturating_sub(since) >= self.config.exit_debounce_ms);
        let timed_out = now.saturating_sub(self.phase_since) >= self.config.animation_timeout_ms;
        match self.phase {
            Phase::Hidden if sessions > 0 => self.enter(Phase::Entering, now),
            Phase::Entering | Phase::Visible if gone => self.enter(Phase::Exiting, now),
            Phase::Entering if timed_out => self.enter(Phase::Visible, now),
            Phase::Exiting if sessions > 0 => self.enter(Phase::Entering, now),
            Phase::Exiting if timed_out => self.enter(Phase::Hidden, now),
            _ => None,
        }
    }

    /// The frontend finished the animation of `finished`. Reports that do not
    /// match the current phase (an interrupted animation ending late) are ignored.
    pub fn animation_finished(&mut self, finished: Phase, now: u64) -> Option<Phase> {
        match (self.phase, finished) {
            (Phase::Entering, Phase::Entering) => self.enter(Phase::Visible, now),
            (Phase::Exiting, Phase::Exiting) => self.enter(Phase::Hidden, now),
            _ => None,
        }
    }
}
