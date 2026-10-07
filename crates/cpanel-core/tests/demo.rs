use cpanel_core::demo::{demo_sessions, DEMO_STEP_MS};
use cpanel_core::model::SessionState;
use std::collections::HashSet;

#[test]
fn every_state_is_visible_at_any_moment() {
    for step in 0..12u64 {
        let now = 1_000_000 + step * DEMO_STEP_MS;
        let states: HashSet<SessionState> = demo_sessions(now).into_iter().map(|s| s.state).collect();
        assert_eq!(states.len(), 6, "step {step}");
    }
}

#[test]
fn each_session_cycles_through_every_state_with_stable_identity() {
    let first = demo_sessions(0);
    let mut seen: Vec<HashSet<SessionState>> = vec![HashSet::new(); first.len()];
    for step in 0..6u64 {
        let now = step * DEMO_STEP_MS;
        for (i, s) in demo_sessions(now).into_iter().enumerate() {
            assert_eq!(s.id, first[i].id);
            assert_eq!(s.name, first[i].name);
            assert!(s.since <= now);
            seen[i].insert(s.state);
        }
    }
    assert!(seen.iter().all(|set| set.len() == 6));
}

#[test]
fn the_demo_cycle_drops_to_zero_sessions_and_comes_back() {
    use cpanel_core::demo::{demo_cycle_sessions, DEMO_CYCLE_EMPTY_MS, DEMO_CYCLE_MS};
    let on = DEMO_CYCLE_MS - DEMO_CYCLE_EMPTY_MS;
    for cycle in 0..3u64 {
        let start = cycle * DEMO_CYCLE_MS;
        assert_eq!(demo_cycle_sessions(start).len(), 6, "cycle {cycle} starts with sessions");
        assert_eq!(demo_cycle_sessions(start + on - 1).len(), 6);
        assert!(demo_cycle_sessions(start + on).is_empty(), "then every session closes");
        assert!(demo_cycle_sessions(start + DEMO_CYCLE_MS - 1).is_empty());
    }
    // Long enough for the exit debounce, the exit animation and a visible pause.
    assert!(DEMO_CYCLE_EMPTY_MS >= 8000);
    assert_eq!(demo_cycle_sessions(1234), demo_sessions(1234));
}
