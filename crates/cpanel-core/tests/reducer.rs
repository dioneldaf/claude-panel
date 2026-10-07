use cpanel_core::hook_event::HookSummary;
use cpanel_core::model::{SessionState, SessionView};
use cpanel_core::reducer::{Reducer, ReducerConfig};
use cpanel_core::registry::RegistryEntry;

const MIN: u64 = 60_000;

fn reg(pid: u32, id: &str, status: &str, status_at: u64) -> RegistryEntry {
    RegistryEntry {
        pid,
        session_id: id.into(),
        cwd: format!("C:\\work\\proj-{pid}"),
        name: Some(format!("name-{pid}")),
        status: Some(status.into()),
        status_updated_at: Some(status_at),
        updated_at: Some(status_at),
        started_at: Some(pid as u64),
        kind: Some("interactive".into()),
        proc_start: None,
        waiting_for: None,
        profile: "claude".into(),
    }
}

fn hook(id: &str, event: &str, ts: u64) -> HookSummary {
    HookSummary { event: event.into(), session_id: id.into(), ts, profile: "claude".into(), ..Default::default() }
}

fn tool(id: &str, event: &str, tool: &str, ts: u64) -> HookSummary {
    HookSummary { tool: Some(tool.into()), ..hook(id, event, ts) }
}

fn notification(id: &str, kind: &str, ts: u64) -> HookSummary {
    HookSummary { notification_type: Some(kind.into()), ..hook(id, "Notification", ts) }
}

fn new() -> Reducer {
    Reducer::new(ReducerConfig::default())
}

fn only(r: &mut Reducer, now: u64) -> SessionView {
    let mut v = r.snapshot(now);
    assert_eq!(v.len(), 1, "expected exactly one session");
    v.remove(0)
}

fn state(r: &mut Reducer, now: u64) -> SessionState {
    only(r, now).state
}

// ---------- registry only ----------

#[test]
fn registry_busy_is_working() {
    let mut r = new();
    r.apply_registry(vec![reg(1, "a", "busy", 1000)], 1000);
    let v = only(&mut r, 1000);
    assert_eq!(v.state, SessionState::Working);
    assert_eq!(v.name, "name-1");
    assert_eq!(v.folder, "proj-1");
    assert_eq!(v.profile, "claude");
    assert_eq!(v.raw_status.as_deref(), Some("busy"));
    assert!(!v.hooks_seen);
}

#[test]
fn registry_idle_is_done_then_paused_after_the_threshold() {
    let mut r = new();
    r.apply_registry(vec![reg(1, "a", "idle", 1000)], 1000);
    assert_eq!(state(&mut r, 1000), SessionState::Done);
    assert_eq!(state(&mut r, 1000 + 10 * MIN - 1), SessionState::Done);
    assert_eq!(state(&mut r, 1000 + 10 * MIN), SessionState::Paused);
}

#[test]
fn registry_shell_is_waiting_process_and_never_pauses() {
    let mut r = new();
    r.apply_registry(vec![reg(1, "a", "shell", 1000)], 1000);
    assert_eq!(state(&mut r, 1000), SessionState::WaitingProcess);
    assert_eq!(state(&mut r, 1000 + 60 * MIN), SessionState::WaitingProcess);
}

#[test]
fn registry_waiting_is_needs_you() {
    let mut r = new();
    let mut entry = reg(1, "a", "waiting", 1000);
    entry.waiting_for = Some("permission".into());
    r.apply_registry(vec![entry], 1000);
    let v = only(&mut r, 1000);
    assert_eq!(v.state, SessionState::NeedsYou);
    assert_eq!(v.waiting_for.as_deref(), Some("permission"));
}

#[test]
fn registry_reports_the_prompt_being_answered_before_any_hook_does() {
    // Observed sequence: busy -> waiting (prompt shown) -> busy (granted). No hook
    // fires on approval, so the registry transition is what ends "needs you".
    let mut r = new();
    r.apply_registry(vec![reg(1, "a", "busy", 1000)], 1000);
    r.apply_hook(&tool("a", "PermissionRequest", "Bash", 2000), 2000);
    r.apply_registry(vec![reg(1, "a", "waiting", 2050)], 2100);
    assert_eq!(state(&mut r, 2100), SessionState::NeedsYou);
    r.apply_registry(vec![reg(1, "a", "busy", 9000)], 9100);
    assert_eq!(state(&mut r, 9100), SessionState::Working);
    // The stale prompt must not come back once the registry goes idle later.
    r.apply_registry(vec![reg(1, "a", "idle", 20_000)], 20_100);
    assert_eq!(state(&mut r, 20_100), SessionState::Done);
    // ...nor when a later hook event makes hooks the freshest source again.
    r.apply_hook(&hook("a", "PreCompact", 25_000), 25_000);
    assert_eq!(state(&mut r, 25_000), SessionState::Working);
}

#[test]
fn unknown_or_missing_status_is_treated_as_done_and_keeps_the_raw_value() {
    let mut r = new();
    let mut missing = reg(2, "b", "x", 1000);
    missing.status = None;
    r.apply_registry(vec![reg(1, "a", "quantum-flux", 1000), missing], 1000);
    let v = r.snapshot(1000);
    assert_eq!(v[0].state, SessionState::Done);
    assert_eq!(v[0].raw_status.as_deref(), Some("quantum-flux"));
    assert_eq!(v[1].state, SessionState::Done);
    assert_eq!(v[1].raw_status, None);
}

#[test]
fn name_falls_back_to_folder_and_folder_handles_both_separators() {
    let mut r = new();
    let mut a = reg(1, "a", "busy", 1);
    a.name = None;
    a.cwd = "C:\\work\\My Project\\".into();
    let mut b = reg(2, "b", "busy", 1);
    b.cwd = "/home/me/app".into();
    r.apply_registry(vec![a, b], 1);
    let v = r.snapshot(1);
    assert_eq!(v[0].name, "My Project");
    assert_eq!(v[0].folder, "My Project");
    assert_eq!(v[1].folder, "app");
}

#[test]
fn sessions_are_ordered_by_start_time_and_dropped_when_gone() {
    let mut r = new();
    r.apply_registry(vec![reg(30, "c", "busy", 1), reg(10, "a", "busy", 1), reg(20, "b", "busy", 1)], 1);
    let ids: Vec<String> = r.snapshot(1).into_iter().map(|v| v.id).collect();
    assert_eq!(ids, vec!["a", "b", "c"]);
    r.apply_registry(vec![reg(20, "b", "busy", 1)], 2);
    let ids: Vec<String> = r.snapshot(2).into_iter().map(|v| v.id).collect();
    assert_eq!(ids, vec!["b"]);
}

// ---------- hooks ----------

#[test]
fn hook_events_for_sessions_missing_from_the_registry_are_not_shown() {
    let mut r = new();
    r.apply_hook(&hook("ghost", "UserPromptSubmit", 5), 5);
    assert!(r.snapshot(5).is_empty());
    // ...but are honoured once the registry lists the session.
    r.apply_registry(vec![reg(1, "ghost", "idle", 1)], 6);
    let v = only(&mut r, 6);
    assert_eq!(v.state, SessionState::Working);
    assert!(v.hooks_seen);
}

#[test]
fn permission_request_is_needs_you_even_while_registry_says_busy() {
    let mut r = new();
    r.apply_registry(vec![reg(1, "a", "busy", 1000)], 1000);
    r.apply_hook(&tool("a", "PermissionRequest", "Bash", 2000), 2000);
    assert_eq!(state(&mut r, 2000), SessionState::NeedsYou);
    // A registry rewrite that keeps the same status value must not clear it.
    r.apply_registry(vec![reg(1, "a", "busy", 3000)], 3000);
    assert_eq!(state(&mut r, 3000), SessionState::NeedsYou);
    // The tool finishing does.
    r.apply_hook(&tool("a", "PostToolUse", "Bash", 4000), 4000);
    assert_eq!(state(&mut r, 4000), SessionState::Working);
}

#[test]
fn needs_you_sources() {
    for ev in [
        notification("a", "permission_prompt", 10),
        notification("a", "agent_needs_input", 10),
        notification("a", "elicitation_dialog", 10),
        hook("a", "Elicitation", 10),
        tool("a", "PreToolUse", "AskUserQuestion", 10),
        tool("a", "PreToolUse", "ExitPlanMode", 10),
    ] {
        let mut r = new();
        r.apply_registry(vec![reg(1, "a", "busy", 1)], 1);
        r.apply_hook(&ev, 10);
        assert_eq!(state(&mut r, 10), SessionState::NeedsYou, "{ev:?}");
    }
}

#[test]
fn working_sources() {
    for ev in [
        hook("a", "UserPromptSubmit", 10),
        tool("a", "PreToolUse", "Read", 10),
        tool("a", "PostToolUse", "Read", 10),
        tool("a", "PostToolUseFailure", "Read", 10),
        hook("a", "ElicitationResult", 10),
        hook("a", "PreCompact", 10),
        hook("a", "PostCompact", 10),
    ] {
        let mut r = new();
        r.apply_registry(vec![reg(1, "a", "idle", 1)], 1);
        r.apply_hook(&ev, 10);
        assert_eq!(state(&mut r, 10), SessionState::Working, "{ev:?}");
    }
}

#[test]
fn stop_is_done_and_idle_prompt_does_not_restart_the_pause_clock() {
    let mut r = new();
    r.apply_registry(vec![reg(1, "a", "busy", 1)], 1);
    r.apply_hook(&hook("a", "Stop", 1000), 1000);
    assert_eq!(state(&mut r, 1000), SessionState::Done);
    r.apply_hook(&notification("a", "idle_prompt", 1000 + MIN), 1000 + MIN);
    assert_eq!(state(&mut r, 1000 + MIN), SessionState::Done);
    assert_eq!(state(&mut r, 1000 + 10 * MIN), SessionState::Paused);
}

#[test]
fn stop_failure_and_session_start_are_idle() {
    for name in ["StopFailure", "SessionStart"] {
        let mut r = new();
        r.apply_registry(vec![reg(1, "a", "busy", 1)], 1);
        r.apply_hook(&hook("a", name, 10), 10);
        assert_eq!(state(&mut r, 10), SessionState::Done, "{name}");
    }
}

#[test]
fn idle_with_running_subagents_is_waiting_subagent() {
    let mut r = new();
    r.apply_registry(vec![reg(1, "a", "busy", 1)], 1);
    r.apply_hook(&hook("a", "UserPromptSubmit", 10), 10);
    r.apply_hook(&hook("a", "SubagentStart", 20), 20);
    r.apply_hook(&hook("a", "SubagentStart", 21), 21);
    assert_eq!(state(&mut r, 21), SessionState::Working, "main agent still working");
    r.apply_hook(&hook("a", "Stop", 30), 30);
    let v = only(&mut r, 30);
    assert_eq!(v.state, SessionState::WaitingSubagent);
    assert_eq!(v.subagents, 2);
    r.apply_hook(&hook("a", "SubagentStop", 40), 40);
    assert_eq!(state(&mut r, 40), SessionState::WaitingSubagent);
    r.apply_hook(&hook("a", "SubagentStop", 50), 50);
    assert_eq!(state(&mut r, 50), SessionState::Done);
    // Extra stops never underflow.
    r.apply_hook(&hook("a", "SubagentStop", 60), 60);
    assert_eq!(only(&mut r, 60).subagents, 0);
}

#[test]
fn subagent_counter_expires_when_the_session_goes_silent() {
    let mut r = new();
    r.apply_registry(vec![reg(1, "a", "busy", 1)], 1);
    r.apply_hook(&hook("a", "SubagentStart", 20), 20);
    r.apply_hook(&hook("a", "Stop", 30), 30);
    assert_eq!(state(&mut r, 30 + 29 * MIN), SessionState::WaitingSubagent);
    assert_eq!(state(&mut r, 30 + 31 * MIN), SessionState::Paused);
}

#[test]
fn tool_events_from_subagents_do_not_change_the_main_phase() {
    let mut r = new();
    r.apply_registry(vec![reg(1, "a", "busy", 1)], 1);
    r.apply_hook(&hook("a", "SubagentStart", 10), 10);
    r.apply_hook(&hook("a", "Stop", 20), 20);
    let mut sub = tool("a", "PreToolUse", "Read", 30);
    sub.agent_id = Some("agent-1".into());
    r.apply_hook(&sub, 30);
    assert_eq!(state(&mut r, 30), SessionState::WaitingSubagent);
    // A permission prompt raised by a subagent still needs the user.
    let mut perm = tool("a", "PermissionRequest", "Bash", 40);
    perm.agent_id = Some("agent-1".into());
    r.apply_hook(&perm, 40);
    assert_eq!(state(&mut r, 40), SessionState::NeedsYou);
    // The subagent tool completing clears the prompt back to the previous main phase.
    let mut post = tool("a", "PostToolUse", "Bash", 50);
    post.agent_id = Some("agent-1".into());
    r.apply_hook(&post, 50);
    assert_eq!(state(&mut r, 50), SessionState::WaitingSubagent);
}

#[test]
fn background_shell_keeps_an_idle_session_in_waiting_process() {
    let mut r = new();
    r.apply_registry(vec![reg(1, "a", "busy", 1)], 1);
    let mut bg = tool("a", "PreToolUse", "Bash", 10);
    bg.background = true;
    r.apply_hook(&bg, 10);
    assert_eq!(state(&mut r, 10), SessionState::Working);
    r.apply_hook(&hook("a", "Stop", 20), 20);
    let v = only(&mut r, 20);
    assert_eq!(v.state, SessionState::WaitingProcess);
    assert_eq!(v.background, 1);
    // Evidence of completion: the registry reports plain idle after the shell started.
    r.apply_registry(vec![reg(1, "a", "idle", 30)], 30);
    let v = only(&mut r, 30);
    assert_eq!(v.state, SessionState::Done);
    assert_eq!(v.background, 0);
}

#[test]
fn registry_shell_status_marks_a_hook_idle_session_as_waiting_process() {
    let mut r = new();
    r.apply_registry(vec![reg(1, "a", "busy", 1)], 1);
    r.apply_hook(&hook("a", "Stop", 20), 20);
    r.apply_registry(vec![reg(1, "a", "shell", 25)], 25);
    assert_eq!(state(&mut r, 25), SessionState::WaitingProcess);
}

#[test]
fn a_newer_registry_status_change_overrides_a_stale_hook_phase() {
    // Interrupting with Esc fires no Stop hook; the registry flips to idle.
    let mut r = new();
    r.apply_registry(vec![reg(1, "a", "busy", 1)], 1);
    r.apply_hook(&tool("a", "PreToolUse", "Bash", 100), 100);
    assert_eq!(state(&mut r, 100), SessionState::Working);
    r.apply_registry(vec![reg(1, "a", "idle", 200)], 200);
    assert_eq!(state(&mut r, 200), SessionState::Done);
    // And a later hook wins again.
    r.apply_hook(&hook("a", "UserPromptSubmit", 300), 300);
    assert_eq!(state(&mut r, 300), SessionState::Working);
}

#[test]
fn an_unknown_registry_status_never_overrides_hooks() {
    let mut r = new();
    r.apply_registry(vec![reg(1, "a", "busy", 1)], 1);
    r.apply_hook(&tool("a", "PermissionRequest", "Bash", 100), 100);
    r.apply_registry(vec![reg(1, "a", "mystery", 200)], 200);
    assert_eq!(state(&mut r, 200), SessionState::NeedsYou);
}

#[test]
fn session_end_forgets_hook_state() {
    let mut r = new();
    r.apply_registry(vec![reg(1, "a", "busy", 1)], 1);
    r.apply_hook(&tool("a", "PermissionRequest", "Bash", 100), 100);
    r.apply_hook(&hook("a", "SessionEnd", 200), 200);
    let v = only(&mut r, 200);
    assert_eq!(v.state, SessionState::Working, "falls back to the registry until it drops the entry");
}

#[test]
fn since_tracks_the_moment_the_state_was_entered() {
    let mut r = new();
    r.apply_registry(vec![reg(1, "a", "busy", 500)], 1000);
    assert_eq!(only(&mut r, 1000).since, 500, "first sight uses the best known timestamp");
    assert_eq!(only(&mut r, 5000).since, 500);
    r.apply_hook(&hook("a", "Stop", 6000), 6000);
    assert_eq!(only(&mut r, 6100).since, 6000);
    assert_eq!(only(&mut r, 6000 + 10 * MIN).since, 6000, "paused keeps the idle start");
}

#[test]
fn hook_state_for_unlisted_sessions_is_pruned() {
    let mut r = new();
    r.apply_hook(&hook("ghost", "UserPromptSubmit", 5), 5);
    assert_eq!(r.tracked_hook_sessions(), 1);
    r.apply_registry(vec![], 5 + 61 * MIN);
    assert_eq!(r.tracked_hook_sessions(), 0);
}

#[test]
fn hook_only_sessions_are_capped_and_the_oldest_dropped() {
    let mut r = Reducer::new(ReducerConfig { max_hook_only_sessions: 3, ..ReducerConfig::default() });
    for i in 1..=5u64 {
        r.apply_hook(&hook(&format!("ghost-{i}"), "UserPromptSubmit", i), i);
    }
    assert_eq!(r.tracked_hook_sessions(), 3);
    // The newest survive, the oldest are gone.
    r.apply_registry(vec![reg(1, "ghost-1", "idle", 0), reg(5, "ghost-5", "idle", 0)], 6);
    let v = r.snapshot(6);
    assert!(!v[0].hooks_seen, "ghost-1 was evicted");
    assert!(v[1].hooks_seen, "ghost-5 was kept");
}

#[test]
fn sessions_listed_in_the_registry_never_count_towards_the_cap() {
    let mut r = Reducer::new(ReducerConfig { max_hook_only_sessions: 2, ..ReducerConfig::default() });
    let live: Vec<_> = (1..=5u32).map(|i| reg(i, &format!("live-{i}"), "busy", 1)).collect();
    r.apply_registry(live, 1);
    for i in 1..=5u64 {
        r.apply_hook(&hook(&format!("live-{i}"), "Stop", 10 + i), 10 + i);
    }
    r.apply_hook(&hook("ghost", "Stop", 20), 20);
    assert_eq!(r.tracked_hook_sessions(), 6);
    assert!(r.snapshot(30).iter().all(|v| v.hooks_seen));
}
