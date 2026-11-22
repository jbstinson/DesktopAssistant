//! Checks GitHub for a newer published release and installs it when the user says yes. Nothing
//! is contacted unless the user clicks "Check for updates", or ticked "Check automatically" and
//! a check is due (updates::is_due). The plugin verifies every download against the public key
//! in tauri.conf.json before running it.

use crate::updates::{self, UpdateSettings};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

/// Only one check (and install) at a time, however often the menu is clicked.
static BUSY: AtomicBool = AtomicBool::new(false);

/// How often the automatic-check thread looks at whether a check is due.
const POLL: Duration = Duration::from_secs(60 * 60);

pub fn settings_path() -> PathBuf {
    crate::paths::data_dir().join("updates.json")
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Starts a check in the background and returns at once. A manual check reports every outcome;
/// an automatic one speaks up only when there is an update to offer.
pub fn check(app: &AppHandle, manual: bool) {
    if BUSY.swap(true, Ordering::SeqCst) {
        if manual {
            std::thread::spawn(|| crate::tray::info("A check for updates is already running."));
        }
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = run(&app, manual).await;
        BUSY.store(false, Ordering::SeqCst);
        if let Err(e) = result {
            eprintln!("update check failed: {e}");
            if manual {
                let text = format!("Could not check for updates: {e}");
                let _ = tauri::async_runtime::spawn_blocking(move || crate::tray::show_error(&text)).await;
            }
        }
    });
}

async fn run(app: &AppHandle, manual: bool) -> Result<(), String> {
    let updater = app
        .updater_builder()
        // Runs just before the plugin starts the installer and exits: like Quit, so hooks stop
        // trying this app at once instead of waiting on a closed port.
        .on_before_exit(|| {
            let _ = std::fs::remove_file(crate::paths::data_dir().join("endpoint.json"));
        })
        .build()
        .map_err(|e| e.to_string())?;
    let found = updater.check().await.map_err(|e| e.to_string())?;
    record_check();

    let Some(update) = found else {
        if manual {
            let text = format!("You have the latest version ({}).", app.package_info().version);
            let _ = tauri::async_runtime::spawn_blocking(move || crate::tray::info(&text)).await;
        }
        return Ok(());
    };
    let question = format!(
        "DesktopAssistant {} is available (you have {}). Install it now?\n\n\
         The character closes, updates and starts again.",
        update.version, update.current_version
    );
    let yes = tauri::async_runtime::spawn_blocking(move || crate::tray::ask(&question))
        .await
        .unwrap_or(false);
    if !yes {
        return Ok(());
    }
    let bytes = update.download(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    // Verifies the signature again, starts the installer and exits the app on success.
    update.install(bytes).map_err(|e| e.to_string())
}

/// Remembers when a check last succeeded, so automatic checks wait a day.
fn record_check() {
    let path = settings_path();
    let s = UpdateSettings { last_check: Some(now_secs()), ..updates::load(&path) };
    if let Err(e) = updates::save(&path, &s) {
        eprintln!("updates: cannot save the check time: {e}");
    }
}

/// Watches for automatic checks for the app's lifetime: one at startup when due, then whenever
/// a day has passed. Reads updates.json each time, so the tray tick takes effect at once.
pub fn start_auto(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || loop {
        if updates::is_due(&updates::load(&settings_path()), now_secs()) {
            check(&app, false);
        }
        std::thread::sleep(POLL);
    });
}

/// Flips "Check automatically" and saves it. Returns the new state. Turning it on checks now if
/// a check is due.
pub fn toggle_auto(app: &AppHandle) -> bool {
    let path = settings_path();
    let s = updates::load(&path);
    let s = UpdateSettings { auto: !s.auto, ..s };
    if let Err(e) = updates::save(&path, &s) {
        eprintln!("updates: cannot save the choice: {e}");
    }
    if updates::is_due(&s, now_secs()) {
        check(app, false);
    }
    s.auto
}
