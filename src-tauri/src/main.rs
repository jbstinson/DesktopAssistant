#![windows_subsystem = "windows"]

// Dead code is allowed on these until Task 8 wires them into the app.
#[allow(dead_code)]
mod events;
#[allow(dead_code)]
mod sessions;
#[allow(dead_code)]
mod rules;
mod overlay;
#[allow(dead_code)]
mod listener;
#[allow(dead_code)]
mod paths;
#[allow(dead_code)]
mod installer;

use std::sync::{Arc, Mutex};
use tauri::Manager;

#[tauri::command]
fn clicked() {}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![clicked])
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            let size = window.outer_size()?;
            let scale = window.scale_factor()?;
            // Window-relative physical pixels: the character circle sits in the middle.
            let region = Arc::new(Mutex::new(overlay::HitRegion::Circle {
                cx: size.width as f64 / 2.0,
                cy: size.height as f64 / 2.0,
                r: 70.0 * scale,
            }));
            overlay::start(window, region);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run DesktopAssistant");
}
