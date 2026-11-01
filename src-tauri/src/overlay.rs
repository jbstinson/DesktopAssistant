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
        GetCursorPos, GetWindowLongPtrW, GetWindowRect, SetWindowLongPtrW, SetWindowPos,
        GWL_EXSTYLE, SWP_NOACTIVATE, SWP_NOZORDER, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
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

/// Top-left of the panel window so the robot stays at the same screen position and the
/// whole window stays on its monitor. Everything is in physical pixels; `monitor` is
/// `(x, y, width, height)`. The window grows right and down from the robot when there is room,
/// otherwise left or up so the robot ends up on the matching edge.
pub fn panel_rect(
    robot: (i32, i32),
    monitor: (i32, i32, i32, i32),
    panel: (i32, i32),
    robot_size: (i32, i32),
) -> (i32, i32) {
    let (mx, my, mw, mh) = monitor;
    let x = if robot.0 + panel.0 <= mx + mw { robot.0 } else { robot.0 + robot_size.0 - panel.0 };
    let y = if robot.1 + panel.1 <= my + mh { robot.1 } else { robot.1 + robot_size.1 - panel.1 };
    // Keep it on the monitor; if the panel is bigger than the monitor, top-left wins.
    (x.min(mx + mw - panel.0).max(mx), y.min(my + mh - panel.1).max(my))
}

/// Moves and resizes in one step without activating or changing z-order, so the window
/// keeps WS_EX_NOACTIVATE and its topmost state. Physical pixels.
pub fn set_bounds(window: &WebviewWindow, x: i32, y: i32, w: i32, h: i32) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    let ok = unsafe { SetWindowPos(hwnd as _, std::ptr::null_mut(), x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE) };
    if ok == 0 { Err("SetWindowPos failed".into()) } else { Ok(()) }
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

    const PANEL: (i32, i32) = (360, 480);
    const ROBOT: (i32, i32) = (140, 140);

    #[test]
    fn panel_grows_right_and_down_from_top_left_robot() {
        let mon = (0, 0, 1920, 1080);
        assert_eq!(panel_rect((10, 10), mon, PANEL, ROBOT), (10, 10));
    }

    #[test]
    fn panel_grows_left_from_top_right_robot() {
        let mon = (0, 0, 1920, 1080);
        // Robot spans x 1770..1910, so the window's right edge lines up with it.
        assert_eq!(panel_rect((1770, 10), mon, PANEL, ROBOT), (1770 + 140 - 360, 10));
    }

    #[test]
    fn panel_grows_up_from_bottom_left_robot() {
        let mon = (0, 0, 1920, 1080);
        assert_eq!(panel_rect((10, 930), mon, PANEL, ROBOT), (10, 930 + 140 - 480));
    }

    #[test]
    fn panel_grows_up_and_left_from_bottom_right_robot() {
        let mon = (0, 0, 1920, 1080);
        assert_eq!(
            panel_rect((1770, 930), mon, PANEL, ROBOT),
            (1770 + 140 - 360, 930 + 140 - 480)
        );
    }

    #[test]
    fn panel_fits_exactly_at_the_far_edge_without_flipping() {
        let mon = (0, 0, 1920, 1080);
        assert_eq!(panel_rect((1920 - 360, 1080 - 480), mon, PANEL, ROBOT), (1560, 600));
        assert_eq!(panel_rect((1920 - 359, 1080 - 479), mon, PANEL, ROBOT), (1561 + 140 - 360, 601 + 140 - 480));
    }

    #[test]
    fn panel_handles_a_monitor_at_negative_x() {
        let mon = (-1920, 0, 1920, 1080);
        assert_eq!(panel_rect((-1900, 20), mon, PANEL, ROBOT), (-1900, 20));
        // Right edge of that monitor is x=0.
        assert_eq!(panel_rect((-150, 20), mon, PANEL, ROBOT), (-150 + 140 - 360, 20));
    }

    #[test]
    fn panel_handles_a_portrait_monitor_at_negative_y() {
        // Portrait monitor above the primary: y from -1920 to 0.
        let mon = (0, -1920, 1080, 1920);
        assert_eq!(panel_rect((20, -1900), mon, PANEL, ROBOT), (20, -1900));
        assert_eq!(panel_rect((20, -150), mon, PANEL, ROBOT), (20, -150 + 140 - 480));
        assert_eq!(
            panel_rect((930, -150), mon, PANEL, ROBOT),
            (930 + 140 - 360, -150 + 140 - 480)
        );
    }

    #[test]
    fn panel_is_clamped_when_the_robot_is_partly_off_the_monitor() {
        let mon = (0, 0, 1920, 1080);
        assert_eq!(panel_rect((-50, -50), mon, PANEL, ROBOT), (0, 0));
        assert_eq!(panel_rect((1900, 1060), mon, PANEL, ROBOT), (1560, 600));
    }

    #[test]
    fn full_is_always_inside() {
        assert!(is_inside(&HitRegion::Full, 0.0, 0.0));
        assert!(is_inside(&HitRegion::Full, -5.0, 9999.0));
    }
}
