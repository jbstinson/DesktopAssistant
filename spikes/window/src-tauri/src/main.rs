#![windows_subsystem = "windows"]

use std::{fs::OpenOptions, io::Write, thread, time::Duration};
use tauri::Manager;
use windows_sys::Win32::{
    Foundation::POINT,
    UI::WindowsAndMessaging::{
        GetCursorPos, GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    },
};

const POLL_MS: u64 = 100;

fn log(line: &str) {
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open("spike.log") {
        let _ = writeln!(file, "{line}");
    }
}

#[tauri::command]
fn clicked() {
    log("robot clicked");
}

#[tauri::command]
fn should_animate() -> bool {
    std::env::var("SPIKE_ANIMATE").is_ok()
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![clicked, should_animate])
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            let hwnd = window.hwnd()?.0 as _;
            unsafe {
                let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                SetWindowLongPtrW(
                    hwnd,
                    GWL_EXSTYLE,
                    style | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize,
                );
            }

            let pos = window.outer_position()?;
            let size = window.outer_size()?;
            let scale = window.scale_factor()?;
            log(&format!(
                "ready pos={},{} size={}x{} scale={scale} hwnd={}",
                pos.x, pos.y, size.width, size.height, hwnd as isize
            ));

            // Clicks pass through everywhere except the character's circle.
            let radius = 70.0 * scale;
            let center_x = pos.x as f64 + size.width as f64 / 2.0;
            let center_y = pos.y as f64 + size.height as f64 / 2.0;
            let hwnd_value = hwnd as isize;
            thread::spawn(move || {
                let mut ignoring = None;
                loop {
                    let mut point = POINT { x: 0, y: 0 };
                    unsafe { GetCursorPos(&mut point) };
                    let dx = point.x as f64 - center_x;
                    let dy = point.y as f64 - center_y;
                    let ignore = dx * dx + dy * dy > radius * radius;
                    if ignoring != Some(ignore) {
                        // Tauri's set_ignore_cursor_events drops WS_EX_NOACTIVATE, so toggle directly.
                        let hwnd = hwnd_value as _;
                        unsafe {
                            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                            let through = (WS_EX_TRANSPARENT | WS_EX_LAYERED) as isize;
                            let style = if ignore { style | through } else { style & !through };
                            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style);
                        }
                        ignoring = Some(ignore);
                    }
                    thread::sleep(Duration::from_millis(POLL_MS));
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run window spike");
}
