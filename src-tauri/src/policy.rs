//! Decisions about showing the window, kept pure so they can be tested.
//!
//! Invariant: while the presence phase is `Hidden` (no sessions) the window is
//! never shown. A shown window whose content is hidden would be an invisible,
//! always-on-top rectangle that swallows clicks.

use cpanel_core::presence::Phase;

/// Whether anything may call `show()` on the window in this phase.
pub fn may_show(phase: Phase) -> bool {
    phase != Phase::Hidden
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MiniToggle {
    /// Nothing is on screen: flip the stored mode and tell the frontend, no window change.
    PersistOnly,
    /// Let the frontend animate the switch; `show_first` un-hides a window the user had hidden.
    Animate { show_first: bool },
}

/// Tray "Toggle mini mode".
pub fn tray_mini_toggle(phase: Phase, window_visible: bool) -> MiniToggle {
    match phase {
        Phase::Hidden => MiniToggle::PersistOnly,
        // Only a settled panel is brought back; during the entrance the frontend
        // shows the window itself, and an exit must not be undone from here.
        Phase::Visible => MiniToggle::Animate { show_first: !window_visible },
        Phase::Entering | Phase::Exiting => MiniToggle::Animate { show_first: false },
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum VisibilityToggle {
    Ignore,
    Show,
    Hide,
}

/// Tray "Show / Hide" and a left click on the tray icon.
pub fn tray_visibility_toggle(phase: Phase, window_visible: bool) -> VisibilityToggle {
    match (may_show(phase), window_visible) {
        (false, true) => VisibilityToggle::Hide,
        (false, false) => VisibilityToggle::Ignore,
        (true, true) => VisibilityToggle::Hide,
        (true, false) => VisibilityToggle::Show,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Phase; 4] = [Phase::Hidden, Phase::Entering, Phase::Visible, Phase::Exiting];

    #[test]
    fn nothing_may_show_the_window_while_there_are_no_sessions() {
        assert!(!may_show(Phase::Hidden));
        for phase in [Phase::Entering, Phase::Visible, Phase::Exiting] {
            assert!(may_show(phase));
        }
    }

    #[test]
    fn toggling_mini_mode_with_no_sessions_only_changes_the_stored_mode() {
        for visible in [false, true] {
            assert_eq!(tray_mini_toggle(Phase::Hidden, visible), MiniToggle::PersistOnly);
        }
    }

    #[test]
    fn toggling_mini_mode_brings_back_a_settled_panel_the_user_had_hidden() {
        assert_eq!(tray_mini_toggle(Phase::Visible, false), MiniToggle::Animate { show_first: true });
        assert_eq!(tray_mini_toggle(Phase::Visible, true), MiniToggle::Animate { show_first: false });
        for phase in [Phase::Entering, Phase::Exiting] {
            for visible in [false, true] {
                assert_eq!(tray_mini_toggle(phase, visible), MiniToggle::Animate { show_first: false });
            }
        }
    }

    #[test]
    fn no_tray_action_ever_shows_the_window_in_the_hidden_phase() {
        for visible in [false, true] {
            assert_ne!(tray_visibility_toggle(Phase::Hidden, visible), VisibilityToggle::Show);
            assert!(!matches!(tray_mini_toggle(Phase::Hidden, visible), MiniToggle::Animate { show_first: true }));
        }
        // A window that is somehow visible while hidden can always be dismissed.
        assert_eq!(tray_visibility_toggle(Phase::Hidden, true), VisibilityToggle::Hide);
    }

    #[test]
    fn show_hide_toggles_whenever_sessions_exist() {
        for phase in ALL.into_iter().filter(|p| *p != Phase::Hidden) {
            assert_eq!(tray_visibility_toggle(phase, true), VisibilityToggle::Hide);
            assert_eq!(tray_visibility_toggle(phase, false), VisibilityToggle::Show);
        }
    }
}
