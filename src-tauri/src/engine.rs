//! Adapters around the pure core: registry polling, the hook listener, snapshot
//! publication and OS notifications.

use crate::hooks;
use crate::probe::OsProbe;
use crate::AppState;
use cpanel_core::demo::{demo_cycle_sessions, demo_sessions};
use cpanel_core::presence::{Phase, Presence, PresenceConfig};
use cpanel_core::eventlog::EventLog;
use cpanel_core::hook_event::{accept, MAX_DATAGRAM};
use cpanel_core::model::{ProfileView, SessionView, Snapshot};
use cpanel_core::notify::{Notice, NoticeKind, Notifier, NotifierConfig};
use cpanel_core::reducer::{Reducer, ReducerConfig};
use cpanel_core::sound::{Cue, SoundGate};
use cpanel_core::registry::{scan_sessions_dir, ConfigDir};
use std::net::UdpSocket;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

/// Registry poll period. The registry is tiny, so one scan per second is cheap.
const POLL_INTERVAL: Duration = Duration::from_millis(1000);
/// Config directories and hook status are re-checked every this many polls.
const DISCOVERY_EVERY: u32 = 10;
const DEMO_INTERVAL: Duration = Duration::from_millis(500);
const EVENT_LOG_MAX_BYTES: u64 = 1024 * 1024;
/// Minimum time between two notification sounds across all sessions.
const SOUND_GAP_MS: u64 = 2000;

pub struct Engine {
    reducer: Reducer,
    notifier: Notifier,
    sound_gate: SoundGate,
    profiles: Vec<ProfileView>,
    sessions: Vec<SessionView>,
    listener_ok: bool,
    demo: bool,
    demo_cycle: bool,
    presence: Presence,
    last_emitted: Option<Snapshot>,
}

impl Engine {
    pub fn new(demo: bool, demo_cycle: bool) -> Self {
        Self {
            reducer: Reducer::new(ReducerConfig::default()),
            notifier: Notifier::new(NotifierConfig::default()),
            sound_gate: SoundGate::new(SOUND_GAP_MS),
            profiles: Vec::new(),
            sessions: Vec::new(),
            // Demo mode has no listener; do not show a port warning for it.
            listener_ok: true,
            demo,
            demo_cycle,
            presence: Presence::new(PresenceConfig::default()),
            last_emitted: None,
        }
    }

    pub fn phase(&self) -> Phase {
        self.presence.phase()
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            sessions: self.sessions.clone(),
            profiles: self.profiles.clone(),
            demo: self.demo,
            listener_ok: self.listener_ok,
        }
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Recomputes the view, emits it when it changed and shows due notifications.
pub fn publish(app: &AppHandle) {
    let state = app.state::<AppState>();
    let ui = state.ui.lock().unwrap().clone();
    let alerts = crate::policy::alert_outputs(ui.muted, ui.sound);
    let (changed, notices, phase, cue) = {
        let mut engine = state.engine.lock().unwrap();
        let now = now_ms();
        engine.sessions = match (engine.demo, engine.demo_cycle) {
            (true, true) => demo_cycle_sessions(now),
            (true, false) => demo_sessions(now),
            _ => engine.reducer.snapshot(now),
        };
        let count = engine.sessions.len();
        let phase = engine.presence.observe(count, now);
        let sessions = engine.sessions.clone();
        // Demo transitions are synthetic and must not raise real notifications.
        let notices = if engine.demo { Vec::new() } else { engine.notifier.observe(&sessions, now) };
        let snapshot = engine.snapshot();
        let changed = (engine.last_emitted.as_ref() != Some(&snapshot)).then(|| {
            engine.last_emitted = Some(snapshot.clone());
            snapshot
        });
        // One cue per batch, the most urgent, and rate-limited across sessions.
        let cue = Cue::for_notices(&notices).filter(|_| alerts.sound && engine.sound_gate.allow(now));
        (changed, notices, phase, cue)
    };
    if let Some(snapshot) = changed {
        let _ = app.emit("snapshot", snapshot);
    }
    if let Some(phase) = phase {
        apply_phase(app, phase);
    }
    if alerts.toast {
        for notice in notices {
            show_notice(app, &notice);
        }
    }
    if let Some(cue) = cue {
        crate::sound::play(cue);
    }
}

/// Carries out a phase change: tells the frontend, and owns the two transitions
/// that must not depend on it (hiding the window, and showing it after a timeout).
fn apply_phase(app: &AppHandle, phase: Phase) {
    let window = app.get_webview_window("main");
    match phase {
        // The frontend sizes the window to its content and then shows it itself.
        Phase::Entering | Phase::Exiting => {}
        Phase::Visible => {
            if let Some(window) = &window {
                let _ = window.show();
            }
        }
        Phase::Hidden => {
            if let Some(window) = &window {
                let _ = window.hide();
            }
        }
    }
    crate::set_tray_tooltip(app, phase);
    let _ = app.emit("presence", phase);
}

/// The frontend finished an entrance or exit animation.
pub fn animation_finished(app: &AppHandle, finished: Phase) {
    let phase = app.state::<AppState>().engine.lock().unwrap().presence.animation_finished(finished, now_ms());
    if let Some(phase) = phase {
        apply_phase(app, phase);
    }
}

/// One-off notice for a manual start with no sessions, so an invisible panel does
/// not look like a failed launch.
fn notify_waiting(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.autostart_launch || state.demo || state.ui.lock().unwrap().muted {
        return;
    }
    if state.engine.lock().unwrap().phase() != Phase::Hidden {
        return;
    }
    let _ = app
        .notification()
        .builder()
        .title(format!("{} is running", cpanel_core::PRODUCT_NAME))
        .body("The panel appears when a Claude Code session opens. It stays in the tray until then.")
        .show();
}

/// Toasts never set a sound name: notify-rust then hands `None` to
/// tauri-winrt-notification, which writes `<audio silent="true"/>`. The panel plays
/// its own cue instead (`crate::sound`), so the Windows default never plays on top.
fn show_notice(app: &AppHandle, notice: &Notice) {
    let (title, body) = match notice.kind {
        NoticeKind::NeedsYou => (
            format!("Action required: {}", notice.name),
            format!("The session in \"{}\" is waiting for a permission or an answer.", notice.folder),
        ),
        NoticeKind::Done => (
            format!("Turn finished: {}", notice.name),
            format!("The session in \"{}\" is awaiting the next prompt.", notice.folder),
        ),
    };
    let _ = app.notification().builder().title(title).body(body).show();
}

/// Re-reads the hook installation state of every profile (read-only).
pub fn refresh_profiles(app: &AppHandle) -> Vec<ConfigDir> {
    let state = app.state::<AppState>();
    let dirs = hooks::config_dirs(&state.config_dirs);
    let profiles = hooks::profile_views(&dirs, state.hook_exe.as_deref());
    state.engine.lock().unwrap().profiles = profiles;
    dirs
}

/// Starts the background threads. Returns immediately.
pub fn start(app: AppHandle) {
    let demo = app.state::<AppState>().demo;
    if demo {
        std::thread::spawn(move || loop {
            publish(&app);
            std::thread::sleep(DEMO_INTERVAL);
        });
        return;
    }
    start_listener(app.clone());
    std::thread::spawn(move || {
        // Restore hook entries that an upgrade or a cancelled uninstall removed,
        // or that dangle after the installation moved. A no-op when all is well.
        if let (Some(state_file), Some(exe)) = (hooks::state_path(), app.state::<AppState>().hook_exe.clone()) {
            hooks::startup_repair(&state_file, &exe);
        }
        let probe = OsProbe;
        let mut dirs = Vec::new();
        let mut tick = 0u32;
        loop {
            if tick % DISCOVERY_EVERY == 0 {
                dirs = refresh_profiles(&app);
            }
            tick = tick.wrapping_add(1);
            let entries = dirs
                .iter()
                .flat_map(|dir| scan_sessions_dir(&dir.path.join("sessions"), &dir.tag, &probe))
                .collect();
            app.state::<AppState>().engine.lock().unwrap().reducer.apply_registry(entries, now_ms());
            publish(&app);
            if tick == 1 {
                notify_waiting(&app);
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    });
}

/// Receives hook summaries on the loopback UDP port. When the port is taken the
/// panel keeps working from the registry alone and reports it in the snapshot.
fn start_listener(app: AppHandle) {
    let port = cpanel_core::resolve_port(std::env::var(cpanel_core::PORT_ENV).ok().as_deref());
    let socket = match UdpSocket::bind(("127.0.0.1", port)) {
        Ok(socket) => socket,
        Err(_) => {
            app.state::<AppState>().engine.lock().unwrap().listener_ok = false;
            return;
        }
    };
    let log = app.path().app_log_dir().ok().map(|dir| EventLog::new(dir.join("hook-events.jsonl"), EVENT_LOG_MAX_BYTES));
    std::thread::spawn(move || {
        let mut buffer = [0u8; MAX_DATAGRAM * 2];
        loop {
            let Ok((len, _)) = socket.recv_from(&mut buffer) else {
                // Windows reports ICMP resets of earlier sends as errors; keep listening.
                std::thread::sleep(Duration::from_millis(50));
                continue;
            };
            // Any local process can send to this port: validate first, and log only
            // the decoded compact summary, never the bytes as received.
            let Some((event, line)) = accept(&buffer[..len]) else {
                continue;
            };
            if let Some(log) = &log {
                log.append(&line);
            }
            app.state::<AppState>().engine.lock().unwrap().reducer.apply_hook(&event, now_ms());
            publish(&app);
        }
    });
}
