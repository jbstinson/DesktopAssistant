#![windows_subsystem = "windows"]

mod app;
mod delivery;
mod events;
mod jump;
mod listener;
mod overlay;
mod paths;
mod position;
mod rules;
mod rules_watch;
mod sessions;
mod tray;
mod installer;

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{Emitter, Manager, PhysicalPosition, State, WebviewWindow, WindowEvent};

/// How often silent sessions are expired.
const TICK: Duration = Duration::from_secs(30);

const HOTKEY_WARNING: &str = "The Ctrl+Alt+D hotkey is taken by another app";
const HOOKS_WARNING: &str = "Hooks cannot connect: the local listener failed to start";

type SharedCore = Arc<Mutex<app::Core>>;
type Publish = Arc<dyn Fn() + Send + Sync>;
type SharedRegion = Arc<Mutex<overlay::HitRegion>>;

/// Sizes in CSS pixels; the window is scaled by the monitor's scale factor.
const CLOSED_SIZE: f64 = 200.0;
const ROBOT_SIZE: f64 = 140.0;
const PANEL_W: f64 = 360.0;
const PANEL_H: f64 = 480.0;
const ROBOT_HIT_RADIUS: f64 = 70.0;

/// Where the robot sits inside the window while the panel is open (physical pixels), or
/// None while closed.
#[derive(Default)]
struct PanelState {
    robot_offset: Option<(i32, i32)>,
}

/// Robot position inside the open window, in CSS pixels.
#[derive(Serialize)]
struct PanelLayout {
    robot_x: f64,
    robot_y: f64,
}

/// A poisoned lock must not take a thread down with it.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Opens or closes the panel by resizing the single window. The robot keeps its screen
/// position and the window grows toward the side of its monitor that has room.
#[tauri::command]
fn set_panel(
    open: bool,
    window: WebviewWindow,
    region: State<'_, SharedRegion>,
    state: State<'_, Mutex<PanelState>>,
) -> Result<PanelLayout, String> {
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let px = |css: f64| (css * scale).round() as i32;
    let pos = window.outer_position().map_err(|e| e.to_string())?;
    let mut state = lock(&state);
    let closed_inset = px((CLOSED_SIZE - ROBOT_SIZE) / 2.0);
    let layout = |off: (i32, i32)| PanelLayout {
        robot_x: off.0 as f64 / scale,
        robot_y: off.1 as f64 / scale,
    };

    if open {
        if let Some(off) = state.robot_offset {
            return Ok(layout(off));
        }
        let robot = (pos.x + closed_inset, pos.y + closed_inset);
        let monitor = match window.current_monitor() {
            Ok(Some(m)) => (m.position().x, m.position().y, m.size().width as i32, m.size().height as i32),
            _ => (i32::MIN / 2, i32::MIN / 2, i32::MAX, i32::MAX),
        };
        let (w, h) = (px(PANEL_W), px(PANEL_H));
        let (x, y) = overlay::panel_rect(robot, monitor, (w, h), (px(ROBOT_SIZE), px(ROBOT_SIZE)));
        overlay::set_bounds(&window, x, y, w, h)?;
        *lock(&region) = overlay::HitRegion::Full;
        let off = (robot.0 - x, robot.1 - y);
        state.robot_offset = Some(off);
        Ok(layout(off))
    } else {
        if let Some(off) = state.robot_offset.take() {
            let (x, y) = (pos.x + off.0 - closed_inset, pos.y + off.1 - closed_inset);
            let side = px(CLOSED_SIZE);
            *lock(&region) = closed_region(scale);
            overlay::set_bounds(&window, x, y, side, side)?;
        }
        Ok(layout((closed_inset, closed_inset)))
    }
}

/// The robot's click circle while the panel is closed: the middle of the small window.
fn closed_region(scale: f64) -> overlay::HitRegion {
    let c = CLOSED_SIZE / 2.0 * scale;
    overlay::HitRegion::Circle { cx: c, cy: c, r: ROBOT_HIT_RADIUS * scale }
}

#[tauri::command]
fn dismiss_session(id: String, core: State<'_, SharedCore>, publish: State<'_, Publish>) {
    lock(&core).dismiss(&id);
    publish();
}

/// Brings the session's window forward. A session with nothing to bring forward does
/// nothing; a window that has gone is reported as an error for the panel to show.
#[tauri::command]
fn jump_to(id: String, core: State<'_, SharedCore>) -> Result<(), String> {
    let target = lock(&core).jump_target(&id).ok_or("session not found")?;
    if target == jump::Target::None {
        return Ok(());
    }
    // The core lock is released above; the Windows calls can take a moment.
    let outcome = jump::focus(target)?;
    eprintln!("jump to {target:?}: {outcome:?}");
    Ok(())
}

/// The UI asks once on load so it cannot miss events emitted before the page was ready.
#[tauri::command]
fn get_snapshot(core: State<'_, SharedCore>) -> app::Snapshot {
    lock(&core).snapshot()
}

/// Work areas of all monitors in physical pixels, the primary first.
fn monitor_rects(window: &WebviewWindow) -> Vec<position::Rect> {
    let rect = |m: &tauri::Monitor| {
        let a = m.work_area();
        position::Rect { x: a.position.x, y: a.position.y, w: a.size.width as i32, h: a.size.height as i32 }
    };
    let primary = window.primary_monitor().ok().flatten();
    let mut all: Vec<tauri::Monitor> = window.available_monitors().unwrap_or_default();
    if let Some(p) = &primary {
        if let Some(i) = all.iter().position(|m| m.position() == p.position() && m.size() == p.size()) {
            all.swap(0, i);
        }
    }
    all.iter().map(rect).collect()
}

fn state_path() -> std::path::PathBuf {
    paths::data_dir().join("state.json")
}

/// Waits until the window has stopped moving, then saves where it ended up. The panel
/// moves the window too, so nothing is saved while it is open.
const SAVE_DELAY: Duration = Duration::from_millis(400);
static MOVE_GEN: AtomicU64 = AtomicU64::new(0);

fn schedule_position_save(handle: tauri::AppHandle) {
    let gen = MOVE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    std::thread::spawn(move || {
        std::thread::sleep(SAVE_DELAY);
        if MOVE_GEN.load(Ordering::SeqCst) != gen {
            return;
        }
        let panel_open = lock(&handle.state::<Mutex<PanelState>>()).robot_offset.is_some();
        let Some(window) = handle.get_webview_window("main") else { return };
        if panel_open || !window.is_visible().unwrap_or(false) {
            return;
        }
        if let Ok(p) = window.outer_position() {
            if let Err(e) = position::save(&state_path(), (p.x, p.y)) {
                eprintln!("could not save window position: {e}");
            }
        }
    });
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![set_panel, dismiss_session, jump_to, get_snapshot])
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            let scale = window.scale_factor()?;
            // Move to the remembered spot (or the primary's corner) before anything is drawn.
            let side = (CLOSED_SIZE * scale).round() as i32;
            let (x, y) = position::restore(position::load(&state_path()), &monitor_rects(&window), (side, side));
            window.set_position(PhysicalPosition::new(x, y))?;
            window.on_window_event({
                let handle = app.handle().clone();
                move |e| {
                    if matches!(e, WindowEvent::Moved(_)) {
                        schedule_position_save(handle.clone());
                    }
                }
            });
            // Window-relative physical pixels: the character circle sits in the middle.
            let region: SharedRegion = Arc::new(Mutex::new(closed_region(scale)));
            app.manage(region.clone());
            app.manage(Mutex::new(PanelState::default()));
            overlay::start(window, region);

            let rules_path = paths::data_dir().join("rules.toml");
            let (rules, rules_warning) = rules::load_or_create(&rules_path);
            let core: SharedCore = Arc::new(Mutex::new(app::Core::new(rules, rules_warning)));
            // Read-only: a missing or unreadable settings file counts as not connected.
            let status = tray::claude_settings_path()
                .zip(std::env::current_exe().ok().map(|e| tray::hook_path_beside(&e)))
                .map_or(tray::HookStatus { on: false, stale: false }, |(settings, hook)| {
                    tray::hook_status(&settings, &hook)
                });
            let hooks_on = status.on;
            lock(&core).set_hooks_status(status);
            app.manage(core.clone());

            // The emit mutex is held across snapshot and emit so an older snapshot can never
            // arrive after a newer one. The core lock is released before emitting.
            let publish: Publish = {
                let core = core.clone();
                let handle = app.handle().clone();
                let emit_order = Arc::new(Mutex::new(()));
                Arc::new(move || {
                    let _order = lock(&emit_order);
                    let snap = lock(&core).snapshot();
                    let _ = handle.emit("snapshot", snap);
                })
            };
            app.manage(publish.clone());

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
            match started {
                Ok(ep) => eprintln!("listening on 127.0.0.1:{}", ep.port),
                Err(e) => {
                    eprintln!("listener failed to start: {e}");
                    lock(&core).set_connection_warning(Some(HOOKS_WARNING.to_string()));
                }
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
            app.manage(queue.clone());

            tray::install(app.handle(), core.clone(), publish.clone(), queue, hooks_on)?;

            // Ctrl+Alt+D shows or hides the character. If another app owns the key, say so
            // in the warning and carry on.
            {
                use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
                let result = app.global_shortcut().on_shortcut("ctrl+alt+d", |app, _, event| {
                    if event.state == ShortcutState::Pressed {
                        tray::toggle_visible(app);
                    }
                });
                if let Err(e) = result {
                    eprintln!("hotkey failed to register: {e}");
                    lock(&core).set_hotkey_warning(Some(HOTKEY_WARNING.to_string()));
                    publish();
                }
            }
            tray::set_visible(app.handle(), true);

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
