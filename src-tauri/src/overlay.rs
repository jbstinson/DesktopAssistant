//! Overlay window behaviour: never takes focus, clicks pass through except over the character.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use tauri::WebviewWindow;
use windows_sys::Win32::{
    Foundation::{POINT, RECT},
    UI::WindowsAndMessaging::{
        GetCursorPos, GetWindowLongPtrW, GetWindowRect, SetWindowLongPtrW, GWL_EXSTYLE,
        WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    },
};

const POLL_MS: u64 = 100;
const PAUSED_SLEEP_MS: u64 = 500;

static PAUSED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, PartialEq)]
pub enum HitRegion {
    /// Circle in window-relative physical pixels.
    Circle { cx: f64, cy: f64, r: f64 },
    /// The whole window takes clicks.
    #[allow(dead_code)]
    Full,
}

/// `x`, `y` are window-relative.
pub fn is_inside(region: &HitRegion, x: f64, y: f64) -> bool {
    match region {
        HitRegion::Full => true,
        HitRegion::Circle { cx, cy, r } => {
            let (dx, dy) = (x - cx, y - cy);
            dx * dx + dy * dy <= r * r
        }
    }
}

/// While paused the poll thread does no work beyond a slow sleep.
#[allow(dead_code)]
pub fn pause(paused: bool) {
    PAUSED.store(paused, Ordering::Relaxed);
}

/// Sets WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW, then polls the cursor to toggle click-through.
pub fn start(window: WebviewWindow, region: Arc<Mutex<HitRegion>>) {
    let hwnd_value = window.hwnd().expect("overlay window has no hwnd").0 as isize;
    unsafe {
        let hwnd = hwnd_value as _;
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize);
    }
    thread::spawn(move || {
        let hwnd = hwnd_value as _;
        let mut ignoring = None;
        loop {
            if PAUSED.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(PAUSED_SLEEP_MS));
                continue;
            }
            let mut point = POINT { x: 0, y: 0 };
            let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            // The window moves, so read its rect on every poll.
            let ok = unsafe { GetCursorPos(&mut point) != 0 && GetWindowRect(hwnd, &mut rect) != 0 };
            if ok {
                let x = (point.x - rect.left) as f64;
                let y = (point.y - rect.top) as f64;
                let ignore = !is_inside(&region.lock().unwrap(), x, y);
                if ignoring != Some(ignore) {
                    // Tauri's set_ignore_cursor_events drops WS_EX_NOACTIVATE, so toggle directly.
                    unsafe {
                        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                        let through = (WS_EX_TRANSPARENT | WS_EX_LAYERED) as isize;
                        let style = if ignore { style | through } else { style & !through };
                        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style);
                    }
                    ignoring = Some(ignore);
                }
            }
            thread::sleep(Duration::from_millis(POLL_MS));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle() -> HitRegion {
        HitRegion::Circle { cx: 100.0, cy: 100.0, r: 70.0 }
    }

    #[test]
    fn circle_center_is_inside() {
        assert!(is_inside(&circle(), 100.0, 100.0));
    }

    #[test]
    fn circle_edge_is_inside() {
        assert!(is_inside(&circle(), 170.0, 100.0));
    }

    #[test]
    fn just_outside_circle_is_outside() {
        assert!(!is_inside(&circle(), 170.5, 100.0));
    }

    #[test]
    fn window_corner_is_outside() {
        assert!(!is_inside(&circle(), 0.0, 0.0));
        assert!(!is_inside(&circle(), 200.0, 200.0));
    }

    #[test]
    fn full_is_always_inside() {
        assert!(is_inside(&HitRegion::Full, 0.0, 0.0));
        assert!(is_inside(&HitRegion::Full, -5.0, 9999.0));
    }
}
