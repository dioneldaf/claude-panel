//! Claude Panel: always-on-top widget showing the state of Claude Code sessions.

// Release builds are GUI applications; debug builds keep a console for diagnostics.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod autostart;
mod cli;
mod drag;
mod engine;
mod hooks;
mod persist;
mod policy;
mod probe;

use cpanel_core::presence::Phase;
use engine::Engine;
use persist::UiState;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewWindow, Wry};

/// Distance kept from the screen edge when no position was saved yet.
const DEFAULT_MARGIN: i32 = 24;

pub struct AppState {
    pub engine: Mutex<Engine>,
    pub ui: Mutex<UiState>,
    pub ui_path: PathBuf,
    pub demo: bool,
    /// Started by Windows at sign-in: no "waiting for a session" notice.
    pub autostart_launch: bool,
    /// Absolute path of the hook executable, when it sits next to the application.
    pub hook_exe: Option<String>,
    /// Explicit config directories from the command line (empty: auto-discovery).
    pub config_dirs: Vec<PathBuf>,
    /// True while the drag thread is alive.
    pub dragging: Arc<AtomicBool>,
    pub drag_release: Arc<AtomicBool>,
    pub mute_item: Mutex<Option<CheckMenuItem<Wry>>>,
    pub autostart_item: Mutex<Option<CheckMenuItem<Wry>>>,
}

#[derive(Serialize, Clone)]
struct UiPayload {
    mini: bool,
    muted: bool,
}

#[derive(Serialize)]
struct InitialState {
    snapshot: cpanel_core::model::Snapshot,
    ui: UiPayload,
    hook_exe: Option<String>,
    phase: Phase,
}

fn save_ui(app: &AppHandle, change: impl FnOnce(&mut UiState)) -> UiPayload {
    let state = app.state::<AppState>();
    let mut ui = state.ui.lock().unwrap();
    change(&mut ui);
    ui.save(&state.ui_path);
    UiPayload { mini: ui.mini, muted: ui.muted }
}

/// Stores the window position (called when a drag ends).
pub fn remember_position(app: &AppHandle, x: i32, y: i32) {
    save_ui(app, |ui| {
        ui.x = Some(x);
        ui.y = Some(y);
    });
}

fn set_muted_everywhere(app: &AppHandle, muted: bool) {
    let payload = save_ui(app, |ui| ui.muted = muted);
    if let Some(item) = app.state::<AppState>().mute_item.lock().unwrap().as_ref() {
        let _ = item.set_checked(muted);
    }
    let _ = app.emit("ui", payload);
}

/// Tray tooltip: says why nothing is on screen while there are no sessions.
pub fn set_tray_tooltip(app: &AppHandle, phase: Phase) {
    let text = match phase {
        Phase::Hidden => format!("{} - waiting for a Claude Code session", cpanel_core::PRODUCT_NAME),
        _ => cpanel_core::PRODUCT_NAME.to_string(),
    };
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(text));
    }
}

fn toggle_autostart(app: &AppHandle) {
    let preference = autostart::set(!autostart::enabled());
    save_ui(app, |ui| ui.autostart = preference);
    // Show what the registry really says, not what was requested.
    if let Some(item) = app.state::<AppState>().autostart_item.lock().unwrap().as_ref() {
        let _ = item.set_checked(autostart::enabled());
    }
}

fn current_phase(app: &AppHandle) -> Phase {
    app.state::<AppState>().engine.lock().unwrap().phase()
}

/// Tray "Show / Hide". With no sessions there is nothing to show; the panel
/// returns by itself when one opens.
fn toggle_visibility(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    match policy::tray_visibility_toggle(current_phase(app), window.is_visible().unwrap_or(false)) {
        policy::VisibilityToggle::Show => {
            let _ = window.show();
        }
        policy::VisibilityToggle::Hide => {
            let _ = window.hide();
        }
        policy::VisibilityToggle::Ignore => {}
    }
}

/// Tray "Toggle mini mode". While nothing is on screen only the stored mode
/// changes; the window is never shown from here in that case.
fn toggle_mini(app: &AppHandle) {
    let window = app.get_webview_window("main");
    let visible = window.as_ref().is_some_and(|w| w.is_visible().unwrap_or(false));
    match policy::tray_mini_toggle(current_phase(app), visible) {
        policy::MiniToggle::PersistOnly => {
            let payload = save_ui(app, |ui| ui.mini = !ui.mini);
            let _ = app.emit("ui", payload);
        }
        policy::MiniToggle::Animate { show_first } => {
            if let (true, Some(window)) = (show_first, &window) {
                let _ = window.show();
            }
            let _ = app.emit("toggle-mini", ());
        }
    }
}

// ---------- commands ----------

#[tauri::command]
fn get_state(app: AppHandle) -> InitialState {
    let state = app.state::<AppState>();
    let (snapshot, phase) = {
        let engine = state.engine.lock().unwrap();
        (engine.snapshot(), engine.phase())
    };
    let ui = state.ui.lock().unwrap().clone();
    InitialState { snapshot, ui: UiPayload { mini: ui.mini, muted: ui.muted }, hook_exe: state.hook_exe.clone(), phase }
}

/// Fits the window to the content. Sizes are logical pixels.
#[tauri::command]
fn resize_window(window: WebviewWindow, width: f64, height: f64) {
    let (width, height) = (width.clamp(24.0, 1200.0), height.clamp(24.0, 2000.0));
    let _ = window.set_size(LogicalSize::new(width, height));
}

#[tauri::command]
fn set_mini(app: AppHandle, mini: bool) {
    save_ui(&app, |ui| ui.mini = mini);
}

#[tauri::command]
fn set_muted(app: AppHandle, muted: bool) {
    set_muted_everywhere(&app, muted);
}

#[tauri::command]
fn drag_start(app: AppHandle, window: WebviewWindow) {
    drag::start(app, window);
}

#[tauri::command]
fn drag_end(app: AppHandle) {
    drag::stop(&app);
}

/// The frontend finished the entrance or exit animation.
#[tauri::command]
fn presence_done(app: AppHandle, phase: Phase) {
    engine::animation_finished(&app, phase);
}

#[tauri::command]
fn hide_window(window: WebviewWindow) {
    let _ = window.hide();
}

/// Installs or removes the hooks in every watched profile. Only ever called from
/// an explicit, confirmed user action in the UI.
#[tauri::command]
fn hooks_action(app: AppHandle, action: String) -> Result<Vec<hooks::ActionResult>, String> {
    let action = match action.as_str() {
        "install" => hooks::Action::Install,
        "remove" => hooks::Action::Remove,
        other => return Err(format!("unknown action: {other}")),
    };
    let state = app.state::<AppState>();
    if state.demo {
        return Err("Hook actions are disabled in demo mode.".into());
    }
    let dirs = hooks::config_dirs(&state.config_dirs);
    let results = hooks::apply_recorded(action, &dirs, state.hook_exe.as_deref(), hooks::state_path().as_deref());
    engine::refresh_profiles(&app);
    engine::publish(&app);
    Ok(results)
}

// ---------- setup ----------

fn restore_position(window: &WebviewWindow, ui: &UiState) {
    let monitors = window.available_monitors().unwrap_or_default();
    let on_screen = |x: i32, y: i32| {
        monitors.iter().any(|m| {
            let (pos, size) = (m.position(), m.size());
            x >= pos.x - 8 && y >= pos.y - 8 && x < pos.x + size.width as i32 - 40 && y < pos.y + size.height as i32 - 40
        })
    };
    let target = match (ui.x, ui.y) {
        (Some(x), Some(y)) if on_screen(x, y) => Some((x, y)),
        _ => window.primary_monitor().ok().flatten().map(|m| {
            let width = window.outer_size().map(|s| s.width as i32).unwrap_or(320);
            (m.position().x + m.size().width as i32 - width - DEFAULT_MARGIN, m.position().y + DEFAULT_MARGIN * 3)
        }),
    };
    if let Some((x, y)) = target {
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

fn build_tray(app: &AppHandle, muted: bool) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "Show / Hide", true, None::<&str>)?;
    let mini = MenuItem::with_id(app, "mini", "Toggle mini mode", true, None::<&str>)?;
    let mute = CheckMenuItem::with_id(app, "mute", "Mute notifications", true, muted, None::<&str>)?;
    let sign_in =
        CheckMenuItem::with_id(app, "autostart", "Launch at sign-in", true, autostart::enabled(), None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&toggle, &mini, &mute, &sign_in, &PredefinedMenuItem::separator(app)?, &quit],
    )?;
    *app.state::<AppState>().mute_item.lock().unwrap() = Some(mute);
    *app.state::<AppState>().autostart_item.lock().unwrap() = Some(sign_in);

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip(format!("{} - waiting for a Claude Code session", cpanel_core::PRODUCT_NAME))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => toggle_visibility(app),
            "mini" => toggle_mini(app),
            "mute" => {
                let muted = !app.state::<AppState>().ui.lock().unwrap().muted;
                set_muted_everywhere(app, muted);
            }
            "autostart" => toggle_autostart(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                toggle_visibility(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn run_app(options: cli::Options) {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            get_state,
            resize_window,
            set_mini,
            set_muted,
            drag_start,
            drag_end,
            hide_window,
            show_window,
            presence_done,
            hooks_action
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let ui_path = app.path().app_config_dir()?.join("ui-state.json");
            let mut ui = UiState::load(&ui_path);
            // Sign-in entry: on by default for an installed build only, corrected
            // when the installation moved, never re-enabled once turned off.
            let preference = autostart::reconcile_at_startup(ui.autostart);
            if preference != ui.autostart {
                ui.autostart = preference;
                ui.save(&ui_path);
            }
            app.manage(AppState {
                engine: Mutex::new(Engine::new(options.demo, options.demo_cycle)),
                ui: Mutex::new(ui.clone()),
                ui_path,
                demo: options.demo,
                autostart_launch: options.autostart,
                hook_exe: hooks::hook_exe(),
                config_dirs: options.config_dirs.clone(),
                dragging: Arc::new(AtomicBool::new(false)),
                drag_release: Arc::new(AtomicBool::new(false)),
                mute_item: Mutex::new(None),
                autostart_item: Mutex::new(None),
            });
            build_tray(&handle, ui.muted)?;
            if let Some(window) = app.get_webview_window("main") {
                restore_position(&window, &ui);
                // The window stays hidden until a session exists; the frontend
                // then sizes it, shows it and plays the entrance.
            }
            engine::refresh_profiles(&handle);
            engine::start(handle);
            Ok(())
        })
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| panic!("error while running {}: {e}", cpanel_core::PRODUCT_NAME));
}

/// Shows the window; called by the frontend after its first layout.
#[tauri::command]
fn show_window(app: AppHandle, window: WebviewWindow) {
    // The frontend asks at the start of an entrance; a stale request that arrives
    // after the panel went back to hidden must not put an empty window on screen.
    if policy::may_show(current_phase(&app)) {
        let _ = window.show();
    }
}

fn main() {
    let options = cli::parse(std::env::args().skip(1), std::env::var("CPANEL_DEMO").ok().as_deref());
    if options.help {
        print!("{}", cli::usage());
        return;
    }
    if let Some(command) = options.hooks {
        std::process::exit(cli::run_hooks(command, &options));
    }
    run_app(options);
}
