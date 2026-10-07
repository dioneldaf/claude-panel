use cpanel_core::presence::{Phase, Presence, PresenceConfig};

const DEBOUNCE: u64 = 2500;
const TIMEOUT: u64 = 6000;

fn presence() -> Presence {
    Presence::new(PresenceConfig { exit_debounce_ms: DEBOUNCE, animation_timeout_ms: TIMEOUT })
}

/// Drives a fresh machine to `Visible` with one session, at time 1000.
fn visible() -> Presence {
    let mut p = presence();
    assert_eq!(p.observe(1, 0), Some(Phase::Entering));
    assert_eq!(p.animation_finished(Phase::Entering, 1000), Some(Phase::Visible));
    p
}

#[test]
fn starts_hidden_and_stays_hidden_without_sessions() {
    let mut p = presence();
    assert_eq!(p.phase(), Phase::Hidden);
    for now in [0, 1000, 60_000, 3_600_000] {
        assert_eq!(p.observe(0, now), None);
    }
    assert_eq!(p.phase(), Phase::Hidden);
}

#[test]
fn first_session_starts_the_entrance_and_its_completion_makes_the_panel_visible() {
    let mut p = presence();
    assert_eq!(p.observe(1, 100), Some(Phase::Entering));
    assert_eq!(p.observe(2, 600), None, "more sessions during the entrance change nothing");
    assert_eq!(p.animation_finished(Phase::Entering, 2100), Some(Phase::Visible));
    assert_eq!(p.observe(2, 3000), None);
    assert_eq!(p.phase(), Phase::Visible);
}

#[test]
fn the_exit_waits_for_the_debounce() {
    let mut p = visible();
    assert_eq!(p.observe(0, 5000), None);
    assert_eq!(p.observe(0, 5000 + DEBOUNCE - 1), None);
    assert_eq!(p.observe(0, 5000 + DEBOUNCE), Some(Phase::Exiting));
}

#[test]
fn a_blip_of_zero_sessions_shorter_than_the_debounce_changes_nothing() {
    let mut p = visible();
    assert_eq!(p.observe(0, 5000), None);
    assert_eq!(p.observe(1, 6000), None, "session restarted");
    // The debounce starts over on the next gap instead of adding up.
    assert_eq!(p.observe(0, 7000), None);
    assert_eq!(p.observe(0, 7000 + DEBOUNCE - 1), None);
    assert_eq!(p.phase(), Phase::Visible);
}

#[test]
fn the_window_is_hidden_only_after_the_exit_animation_reports_done() {
    let mut p = visible();
    p.observe(0, 5000);
    assert_eq!(p.observe(0, 5000 + DEBOUNCE), Some(Phase::Exiting));
    assert_eq!(p.observe(0, 5000 + DEBOUNCE + 1000), None, "still animating");
    assert_eq!(p.phase(), Phase::Exiting);
    assert_eq!(p.animation_finished(Phase::Exiting, 5000 + DEBOUNCE + 2000), Some(Phase::Hidden));
    // And the next session greets again.
    assert_eq!(p.observe(1, 60_000), Some(Phase::Entering));
}

#[test]
fn a_session_appearing_mid_exit_turns_it_around() {
    let mut p = visible();
    p.observe(0, 5000);
    p.observe(0, 5000 + DEBOUNCE);
    assert_eq!(p.observe(1, 5000 + DEBOUNCE + 700), Some(Phase::Entering));
    // The exit animation's late completion must not hide the panel now.
    assert_eq!(p.animation_finished(Phase::Exiting, 5000 + DEBOUNCE + 900), None);
    assert_eq!(p.phase(), Phase::Entering);
    assert_eq!(p.animation_finished(Phase::Entering, 5000 + DEBOUNCE + 1500), Some(Phase::Visible));
}

#[test]
fn sessions_vanishing_mid_entrance_lead_to_the_exit_after_the_debounce() {
    let mut p = presence();
    p.observe(1, 0);
    assert_eq!(p.observe(0, 300), None, "debounced even during the entrance");
    assert_eq!(p.observe(0, 300 + DEBOUNCE), Some(Phase::Exiting));
    // The entrance finishing late is ignored.
    assert_eq!(p.animation_finished(Phase::Entering, 300 + DEBOUNCE + 10), None);
    assert_eq!(p.animation_finished(Phase::Exiting, 300 + DEBOUNCE + 1500), Some(Phase::Hidden));
}

#[test]
fn an_entrance_finishing_during_a_short_gap_still_becomes_visible() {
    let mut p = presence();
    p.observe(1, 0);
    p.observe(0, 1500);
    assert_eq!(p.animation_finished(Phase::Entering, 2000), Some(Phase::Visible));
    // The gap that began at 1500 keeps counting.
    assert_eq!(p.observe(0, 1500 + DEBOUNCE), Some(Phase::Exiting));
}

#[test]
fn completion_reports_for_other_phases_are_ignored() {
    let mut p = presence();
    assert_eq!(p.animation_finished(Phase::Exiting, 0), None);
    assert_eq!(p.animation_finished(Phase::Entering, 0), None);
    assert_eq!(p.animation_finished(Phase::Visible, 0), None);
    assert_eq!(p.phase(), Phase::Hidden);
    let mut p = visible();
    assert_eq!(p.animation_finished(Phase::Exiting, 2000), None);
    assert_eq!(p.phase(), Phase::Visible);
}

#[test]
fn a_frontend_that_never_reports_is_covered_by_timeouts() {
    let mut p = presence();
    p.observe(1, 0);
    assert_eq!(p.observe(1, TIMEOUT - 1), None);
    assert_eq!(p.observe(1, TIMEOUT), Some(Phase::Visible));
    p.observe(0, 10_000);
    assert_eq!(p.observe(0, 10_000 + DEBOUNCE), Some(Phase::Exiting));
    assert_eq!(p.observe(0, 10_000 + DEBOUNCE + TIMEOUT - 1), None);
    assert_eq!(p.observe(0, 10_000 + DEBOUNCE + TIMEOUT), Some(Phase::Hidden));
}

#[test]
fn phases_serialise_as_snake_case_names() {
    assert_eq!(serde_json::to_string(&Phase::Entering).unwrap(), "\"entering\"");
    assert_eq!(serde_json::from_str::<Phase>("\"exiting\"").unwrap(), Phase::Exiting);
    assert!(serde_json::from_str::<Phase>("\"nonsense\"").is_err());
}
