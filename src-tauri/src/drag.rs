//! Window dragging driven from Rust.
//!
//! The window follows the cursor with a short exponential lag instead of being
//! glued to it, which is what makes the drag feel springy. The loop only exists
//! while the mouse button is held, so it costs nothing at rest.

use crate::AppState;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

const FRAME: Duration = Duration::from_millis(8);
/// Fraction of the remaining distance covered each frame while dragging.
const FOLLOW: f64 = 0.30;
/// After release the window keeps easing to the cursor for at most this long.
const SETTLE_LIMIT: Duration = Duration::from_millis(160);

#[cfg(windows)]
mod os {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetSystemMetrics, SM_SWAPBUTTON};

    pub fn cursor() -> Option<(f64, f64)> {
        let mut point = POINT { x: 0, y: 0 };
        // SAFETY: `point` is a valid out-pointer.
        (unsafe { GetCursorPos(&mut point) } != 0).then_some((f64::from(point.x), f64::from(point.y)))
    }

    /// Whether the primary mouse button is physically held.
    pub fn button_down() -> bool {
        // SAFETY: stateless Win32 queries.
        unsafe {
            let key = if GetSystemMetrics(SM_SWAPBUTTON) != 0 { VK_RBUTTON } else { VK_LBUTTON };
            (GetAsyncKeyState(i32::from(key)) as u16 & 0x8000) != 0
        }
    }
}

#[cfg(not(windows))]
mod os {
    pub fn cursor() -> Option<(f64, f64)> {
        None
    }
    pub fn button_down() -> bool {
        true
    }
}

fn cursor(app: &AppHandle) -> Option<(f64, f64)> {
    os::cursor().or_else(|| app.cursor_position().ok().map(|p| (p.x, p.y)))
}

/// Starts following the cursor until the button is released or `stop` is called.
pub fn start(app: AppHandle, window: WebviewWindow) {
    let state = app.state::<AppState>();
    if state.dragging.swap(true, Ordering::SeqCst) {
        return;
    }
    let (Some(start_cursor), Ok(start_pos)) = (cursor(&app), window.outer_position()) else {
        state.dragging.store(false, Ordering::SeqCst);
        return;
    };
    state.drag_release.store(false, Ordering::SeqCst);
    let offset = (start_cursor.0 - f64::from(start_pos.x), start_cursor.1 - f64::from(start_pos.y));
    let dragging = state.dragging.clone();
    let release = state.drag_release.clone();

    std::thread::spawn(move || {
        let (mut x, mut y) = (f64::from(start_pos.x), f64::from(start_pos.y));
        let mut released_at: Option<Instant> = None;
        loop {
            if released_at.is_none() && (release.load(Ordering::SeqCst) || !os::button_down()) {
                released_at = Some(Instant::now());
            }
            let Some(now) = cursor(&app) else { break };
            let (target_x, target_y) = (now.0 - offset.0, now.1 - offset.1);
            let settled = (target_x - x).abs() < 0.5 && (target_y - y).abs() < 0.5;
            let expired = released_at.is_some_and(|t| t.elapsed() > SETTLE_LIMIT);
            if released_at.is_some() && (settled || expired) {
                x = target_x;
                y = target_y;
            } else {
                x += (target_x - x) * FOLLOW;
                y += (target_y - y) * FOLLOW;
            }
            let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
            if released_at.is_some() && (settled || expired) {
                break;
            }
            std::thread::sleep(FRAME);
        }
        dragging.store(false, Ordering::SeqCst);
        crate::remember_position(&app, x.round() as i32, y.round() as i32);
        let _ = app.emit("drag-ended", ());
    });
}

/// Requests the end of the drag (the frontend saw the pointer go up).
pub fn stop(app: &AppHandle) {
    app.state::<AppState>().drag_release.store(true, Ordering::SeqCst);
}
