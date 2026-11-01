#![windows_subsystem = "windows"]

mod app;
mod delivery;
mod events;
mod listener;
mod overlay;
mod paths;
mod rules;
mod rules_watch;
mod sessions;
// Task 10 wires the installer to the tray; nothing calls it yet.
#[allow(dead_code)]
mod installer;

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager, State};

/// How often silent sessions are expired.
const TICK: Duration = Duration::from_secs(30);

const HOOKS_WARNING: &str = "Hooks cannot connect: the local listener failed to start";

type SharedCore = Arc<Mutex<app::Core>>;
type Publish = Arc<dyn Fn() + Send + Sync>;

/// A poisoned lock must not take a thread down with it.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[tauri::command]
fn clicked() {}

/// The UI asks once on load so it cannot miss events emitted before the page was ready.
#[tauri::command]
fn get_snapshot(core: State<'_, SharedCore>) -> app::Snapshot {
    lock(&core).snapshot()
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![clicked, get_snapshot])
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

            let rules_path = paths::data_dir().join("rules.toml");
            let (rules, rules_warning) = rules::load_or_create(&rules_path);
            let core: SharedCore = Arc::new(Mutex::new(app::Core::new(rules, rules_warning)));
            app.manage(core.clone());

            // Snapshot under the lock, emit after releasing it.
            let publish: Publish = {
                let core = core.clone();
                let handle = app.handle().clone();
                Arc::new(move || {
                    let snap = lock(&core).snapshot();
                    let _ = handle.emit("snapshot", snap);
                })
            };

            let queue = Arc::new(delivery::Queue::new(delivery::WindowsOutput));

            let started = {
                let (core, queue, publish) = (core.clone(), queue.clone(), publish.clone());
                listener::start(move |event| {
                    let alert = lock(&core).handle(event, now_secs());
                    if let Some(alert) = alert {
                        queue.push(alert);
                    }
                    publish();
                })
            };
            if let Err(e) = started {
                eprintln!("listener failed to start: {e}");
                lock(&core).set_connection_warning(Some(HOOKS_WARNING.to_string()));
            }

            // Kept alive for the app's lifetime; dropping it would stop the watch.
            let watcher = {
                let (core, publish) = (core.clone(), publish.clone());
                rules_watch::watch(rules_path, move |result| {
                    lock(&core).replace_rules(result);
                    publish();
                })
            };
            app.manage(Mutex::new(watcher));
            app.manage(queue);

            std::thread::spawn({
                let (core, publish) = (core.clone(), publish.clone());
                move || loop {
                    std::thread::sleep(TICK);
                    let changed = lock(&core).tick(now_secs());
                    if changed {
                        publish();
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run DesktopAssistant");
}
