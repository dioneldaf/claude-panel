use cpanel_core::model::{SessionState, SessionView};
use cpanel_core::notify::{NoticeKind, Notifier, NotifierConfig};

fn view(id: &str, state: SessionState) -> SessionView {
    SessionView {
        id: id.into(),
        name: format!("name-{id}"),
        cwd: "C:\\w\\proj".into(),
        folder: "proj".into(),
        profile: "claude".into(),
        pid: 1,
        state,
        since: 0,
        raw_status: None,
        waiting_for: None,
        hooks_seen: true,
        subagents: 0,
        background: 0,
    }
}

fn notifier() -> Notifier {
    Notifier::new(NotifierConfig { settle_done_ms: 1500, settle_needs_you_ms: 400, min_gap_ms: 3000 })
}

use SessionState::*;

#[test]
fn sessions_seen_for_the_first_time_never_notify() {
    let mut n = notifier();
    assert!(n.observe(&[view("a", Done), view("b", NeedsYou)], 0).is_empty());
    assert!(n.observe(&[view("a", Done), view("b", NeedsYou)], 60_000).is_empty());
}

#[test]
fn working_to_done_notifies_once_after_the_settle_delay() {
    let mut n = notifier();
    n.observe(&[view("a", Working)], 0);
    assert!(n.observe(&[view("a", Done)], 1000).is_empty(), "not settled yet");
    assert!(n.observe(&[view("a", Done)], 2000).is_empty(), "still inside the delay");
    let out = n.observe(&[view("a", Done)], 2500);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].kind, NoticeKind::Done);
    assert_eq!(out[0].session_id, "a");
    assert_eq!(out[0].name, "name-a");
    assert_eq!(out[0].folder, "proj");
    assert!(n.observe(&[view("a", Done)], 9000).is_empty(), "only once per stint");
}

#[test]
fn a_flap_back_to_working_inside_the_delay_is_swallowed() {
    let mut n = notifier();
    n.observe(&[view("a", Working)], 0);
    n.observe(&[view("a", Done)], 1000);
    n.observe(&[view("a", Working)], 1500);
    assert!(n.observe(&[view("a", Working)], 5000).is_empty());
    // The real end of turn still notifies.
    n.observe(&[view("a", Done)], 6000);
    assert_eq!(n.observe(&[view("a", Done)], 7500).len(), 1);
}

#[test]
fn needs_you_notifies_quickly() {
    let mut n = notifier();
    n.observe(&[view("a", Working)], 0);
    assert!(n.observe(&[view("a", NeedsYou)], 1000).is_empty());
    let out = n.observe(&[view("a", NeedsYou)], 1400);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].kind, NoticeKind::NeedsYou);
}

#[test]
fn done_reached_from_waiting_states_notifies_but_not_from_paused() {
    for from in [WaitingSubagent, WaitingProcess, NeedsYou] {
        let mut n = notifier();
        n.observe(&[view("a", from)], 0);
        n.observe(&[view("a", from)], 1000);
        n.observe(&[view("a", Done)], 2000);
        assert_eq!(n.observe(&[view("a", Done)], 3500).len(), 1, "{from:?}");
    }
    let mut n = notifier();
    n.observe(&[view("a", Paused)], 0);
    n.observe(&[view("a", Done)], 2000);
    assert!(n.observe(&[view("a", Done)], 3500).is_empty());
}

#[test]
fn waiting_states_and_paused_never_notify() {
    let mut n = notifier();
    n.observe(&[view("a", Working)], 0);
    for (i, s) in [WaitingSubagent, WaitingProcess, Paused, Working].into_iter().enumerate() {
        let t = 10_000 * (i as u64 + 1);
        assert!(n.observe(&[view("a", s)], t).is_empty());
        assert!(n.observe(&[view("a", s)], t + 5000).is_empty());
    }
}

#[test]
fn repeated_notices_of_the_same_kind_respect_the_minimum_gap() {
    let mut n = notifier();
    n.observe(&[view("a", Working)], 0);
    n.observe(&[view("a", NeedsYou)], 1000);
    assert_eq!(n.observe(&[view("a", NeedsYou)], 1400).len(), 1);
    n.observe(&[view("a", Working)], 1500);
    n.observe(&[view("a", NeedsYou)], 1600);
    assert!(n.observe(&[view("a", NeedsYou)], 2100).is_empty(), "second prompt 700 ms later is debounced");
    // A prompt well after the gap notifies again.
    n.observe(&[view("a", Working)], 10_000);
    n.observe(&[view("a", NeedsYou)], 11_000);
    assert_eq!(n.observe(&[view("a", NeedsYou)], 11_400).len(), 1);
}

#[test]
fn sessions_are_tracked_independently_and_forgotten_when_gone() {
    let mut n = notifier();
    n.observe(&[view("a", Working), view("b", Working)], 0);
    n.observe(&[view("a", Done), view("b", Working)], 1000);
    let out = n.observe(&[view("a", Done), view("b", NeedsYou)], 2500);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].session_id, "a");
    // "b" disappears and comes back: treated as new, so no notice.
    n.observe(&[view("a", Done)], 3000);
    assert!(n.observe(&[view("a", Done), view("b", Done)], 4000).is_empty());
    assert!(n.observe(&[view("a", Done), view("b", Done)], 9000).is_empty());
}
