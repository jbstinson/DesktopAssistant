//! The tray icon and its menu, show/hide, and the guarded logic behind "Connect to Claude Code".
//!
//! The settings path is always a parameter of the logic functions so tests use temp files;
//! only `claude_settings_path` knows where the real file lives.

use crate::delivery::Queue;
use crate::installer::{self, Change};
use crate::{lock, overlay};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, ShowWindow, IDYES, MB_DEFBUTTON2, MB_ICONERROR, MB_ICONQUESTION, MB_OK,
    MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO, SW_HIDE, SW_SHOWNOACTIVATE,
};

/// `%USERPROFILE%\.claude\settings.json`, from the USERPROFILE value.
pub fn settings_path_from(userprofile: Option<OsString>) -> Option<PathBuf> {
    let home = userprofile.filter(|p| !p.is_empty())?;
    Some(PathBuf::from(home).join(".claude").join("settings.json"))
}

/// The real settings path. Nothing in the tests calls this.
pub fn claude_settings_path() -> Option<PathBuf> {
    settings_path_from(std::env::var_os("USERPROFILE"))
}

/// `da-hook.exe` beside the running app exe.
pub fn hook_path_beside(exe: &Path) -> PathBuf {
    exe.with_file_name("da-hook.exe")
}

/// Whether our hook entries are all present. Reads only; any failure counts as false.
pub fn detect_hooks_on(settings: &Path, hook: &Path) -> bool {
    std::fs::read_to_string(settings)
        .is_ok_and(|text| installer::is_installed(&text, &hook.to_string_lossy()))
}

/// How our hooks stand in Claude Code's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookStatus {
    /// All nine events point at this hook exe.
    pub on: bool,
    /// Entries of ours exist but not all point here (the exe moved): Connect repairs them.
    pub stale: bool,
}

/// Reads only; any failure counts as not connected and not stale.
pub fn hook_status(settings: &Path, hook: &Path) -> HookStatus {
    let Ok(text) = std::fs::read_to_string(settings) else {
        return HookStatus { on: false, stale: false };
    };
    let on = installer::is_installed(&text, &hook.to_string_lossy());
    HookStatus { on, stale: !on && installer::has_any(&text) }
}

/// Adds (`enable`) or removes our hook entries. Enabling with no hook exe on disk is an
/// error and changes nothing.
pub fn set_hooks(settings: &Path, hook: &Path, enable: bool) -> Result<(), String> {
    if enable {
        if !hook.is_file() {
            return Err(format!("da-hook.exe was not found at {}", hook.display()));
        }
        installer::apply(settings, Change::Add(hook.to_string_lossy().into_owned()))
    } else {
        installer::apply(settings, Change::Remove)
    }
}

/// What the confirm dialog says before anything is changed.
pub fn confirm_text(settings: &Path, enable: bool) -> String {
    let path = settings.display();
    if enable {
        format!(
            "Connect DesktopAssistant to Claude Code?\n\n\
             This adds DesktopAssistant hook entries for nine Claude Code events to:\n{path}\n\n\
             Your existing hooks are kept, and a backup is written beside that file. \
             This can be undone from the same tray menu."
        )
    } else {
        format!(
            "Disconnect DesktopAssistant from Claude Code?\n\n\
             This removes only the DesktopAssistant hook entries from:\n{path}\n\n\
             Your other settings and hooks are kept. You can connect again from the same tray menu."
        )
    }
}

static HOOKS_BUSY: AtomicBool = AtomicBool::new(false);

type Core = Arc<Mutex<crate::app::Core>>;
type Publish = Arc<dyn Fn() + Send + Sync>;

/// Menu items the rest of the app needs to update.
pub struct TrayItems {
    toggle: MenuItem<Wry>,
    quiet: CheckMenuItem<Wry>,
    hooks: CheckMenuItem<Wry>,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn message_box(text: &str, flags: u32) -> i32 {
    let (text, title) = (wide(text), wide("DesktopAssistant"));
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), flags | MB_SETFOREGROUND | MB_TOPMOST) }
}

/// A native yes/no question. The default button is No.
fn ask(text: &str) -> bool {
    message_box(text, MB_YESNO | MB_DEFBUTTON2 | MB_ICONQUESTION) == IDYES
}

fn show_error(text: &str) {
    message_box(text, MB_OK | MB_ICONERROR);
}

fn show_window(window: &tauri::WebviewWindow, cmd: i32) {
    match window.hwnd() {
        Ok(hwnd) => unsafe {
            ShowWindow(hwnd.0 as _, cmd);
        },
        Err(e) => eprintln!("ShowWindow: no hwnd: {e}"),
    }
}

/// Hides or shows the overlay. Hiding also pauses the click-through poll, and the UI is told
/// so it can stop its animations. Showing never activates the window.
pub fn set_visible(app: &AppHandle, visible: bool) {
    let Some(window) = app.get_webview_window("main") else { return };
    if visible {
        overlay::pause(false);
        show_window(&window, SW_SHOWNOACTIVATE);
        let _ = app.emit("visibility", true);
    } else {
        // Tell the UI first so animations stop before the window disappears.
        let _ = app.emit("visibility", false);
        // Raw ShowWindow both ways: Tauri keeps its own visible flag, which this path bypasses.
        show_window(&window, SW_HIDE);
        overlay::pause(true);
    }
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.toggle.set_text(if visible { "Hide" } else { "Show" });
    }
}

pub fn toggle_visible(app: &AppHandle) {
    let visible = app.get_webview_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(true);
    set_visible(app, !visible);
}

/// Removes endpoint.json so a stopped app does not leave a stale port, then exits.
pub fn quit(app: &AppHandle) {
    let _ = std::fs::remove_file(crate::paths::data_dir().join("endpoint.json"));
    app.exit(0);
}

fn toggle_quiet(items: &TrayItems, core: &Core, publish: &Publish, queue: &Queue) {
    let quiet = !lock(core).snapshot().quiet;
    lock(core).set_quiet(quiet);
    queue.set_quiet(quiet);
    let _ = items.quiet.set_checked(quiet);
    publish();
}

/// The Connect / Disconnect flow. Blocks on message boxes, so call it off the event thread.
fn toggle_hooks(items: &TrayItems, core: &Core, publish: &Publish) {
    let settings_hook = claude_settings_path().zip(std::env::current_exe().ok().map(|e| hook_path_beside(&e)));
    let current = lock(core).snapshot().hooks_on;
    let finish = |status: HookStatus| {
        lock(core).set_hooks_status(status);
        let _ = items.hooks.set_checked(status.on);
        publish();
    };
    let Some((settings, hook)) = settings_hook else {
        show_error("Cannot find your Claude Code settings folder (USERPROFILE is not set).");
        finish(lock(core).hooks_status());
        return;
    };
    let enable = !current;
    if !ask(&confirm_text(&settings, enable)) {
        finish(lock(core).hooks_status());
        return;
    }
    if let Err(e) = set_hooks(&settings, &hook, enable) {
        show_error(&format!("Nothing was changed.\n\n{e}"));
    }
    // Trust the file, not the click.
    finish(hook_status(&settings, &hook));
}

/// Opens rules.toml in Notepad, which is always there and shows plain text.
fn open_rules() {
    let path = crate::paths::data_dir().join("rules.toml");
    if let Err(e) = std::process::Command::new("notepad.exe").arg(&path).spawn() {
        show_error(&format!("Cannot open {}: {e}", path.display()));
    }
}

/// Builds the tray icon and its menu. `hooks_on` is the checked state at startup.
pub fn install(app: &AppHandle, core: Core, publish: Publish, queue: Arc<Queue>, hooks_on: bool) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "Hide", true, None::<&str>)?;
    let quiet = CheckMenuItem::with_id(app, "quiet", "Do not disturb", true, false, None::<&str>)?;
    let hooks = CheckMenuItem::with_id(app, "hooks", "Connect to Claude Code", true, hooks_on, None::<&str>)?;
    let rules = MenuItem::with_id(app, "rules", "Open rules file", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&toggle, &quiet, &hooks, &rules, &sep, &quit_item])?;
    app.manage(TrayItems { toggle, quiet, hooks });

    let mut builder = TrayIconBuilder::new().tooltip("DesktopAssistant").menu(&menu);
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "toggle" => toggle_visible(app),
            "quiet" => toggle_quiet(&app.state::<TrayItems>(), &core, &publish, &queue),
            "hooks" => {
                // One dialog at a time, even if the item is clicked again meanwhile.
                if HOOKS_BUSY.swap(true, Ordering::SeqCst) {
                    return;
                }
                let (app, core, publish) = (app.clone(), core.clone(), publish.clone());
                std::thread::spawn(move || {
                    toggle_hooks(&app.state::<TrayItems>(), &core, &publish);
                    HOOKS_BUSY.store(false, Ordering::SeqCst);
                });
            }
            "rules" => open_rules(),
            "quit" => quit(app),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("da-tray-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A fake hook exe next to a settings path inside a fresh temp dir.
    fn fixture(name: &str, with_hook: bool) -> (PathBuf, PathBuf) {
        let dir = temp_dir(name);
        let hook = dir.join("da-hook.exe");
        if with_hook {
            std::fs::write(&hook, b"").unwrap();
        }
        (dir.join(".claude").join("settings.json"), hook)
    }

    #[test]
    fn settings_path_is_under_userprofile() {
        assert_eq!(
            settings_path_from(Some(r"C:\Users\x".into())),
            Some(PathBuf::from(r"C:\Users\x").join(".claude").join("settings.json"))
        );
        assert_eq!(settings_path_from(None), None);
        assert_eq!(settings_path_from(Some("".into())), None);
    }

    #[test]
    fn hook_path_is_beside_the_exe() {
        assert_eq!(
            hook_path_beside(Path::new(r"C:\apps\DA\desktop-assistant.exe")),
            PathBuf::from(r"C:\apps\DA\da-hook.exe")
        );
    }

    #[test]
    fn detect_is_false_for_missing_or_broken_files_and_never_writes() {
        let (settings, hook) = fixture("detect-false", true);
        assert!(!detect_hooks_on(&settings, &hook));
        assert!(!settings.exists());
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        std::fs::write(&settings, "{ not json").unwrap();
        assert!(!detect_hooks_on(&settings, &hook));
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), "{ not json");
    }

    #[test]
    fn moved_exe_is_stale_and_connect_repairs_it() {
        let (settings, hook) = fixture("stale", true);
        set_hooks(&settings, &hook, true).unwrap();
        assert_eq!(hook_status(&settings, &hook), HookStatus { on: true, stale: false });
        // The app folder moves: same settings, different hook path.
        let (_, moved) = fixture("stale-moved", true);
        let before = std::fs::read_to_string(&settings).unwrap();
        assert_eq!(hook_status(&settings, &moved), HookStatus { on: false, stale: true });
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), before, "status never writes");
        set_hooks(&settings, &moved, true).unwrap();
        assert_eq!(hook_status(&settings, &moved), HookStatus { on: true, stale: false });
        assert_eq!(hook_status(&settings, &hook), HookStatus { on: false, stale: true });
        let text = std::fs::read_to_string(&settings).unwrap();
        assert_eq!(text.matches("da-hook.exe").count(), 9);
        // Disconnect from the new location also leaves nothing behind.
        set_hooks(&settings, &moved, false).unwrap();
        assert_eq!(hook_status(&settings, &moved), HookStatus { on: false, stale: false });
    }

    #[test]
    fn missing_settings_is_neither_on_nor_stale() {
        let (settings, hook) = fixture("status-none", true);
        assert_eq!(hook_status(&settings, &hook), HookStatus { on: false, stale: false });
    }

    #[test]
    fn enable_then_detect_then_disable() {
        let (settings, hook) = fixture("roundtrip", true);
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        let original = "{\n  \"theme\": \"dark\"\n}\n";
        std::fs::write(&settings, original).unwrap();

        set_hooks(&settings, &hook, true).unwrap();
        assert!(detect_hooks_on(&settings, &hook));
        let backup = settings.with_file_name("settings.json.desktop-assistant.bak");
        assert_eq!(std::fs::read_to_string(backup).unwrap(), original);

        set_hooks(&settings, &hook, false).unwrap();
        assert!(!detect_hooks_on(&settings, &hook));
        let text = std::fs::read_to_string(&settings).unwrap();
        assert!(text.contains("\"theme\""));
        assert!(!text.contains("da-hook"));
    }

    #[test]
    fn enable_without_the_hook_exe_errors_and_changes_nothing() {
        let (settings, hook) = fixture("nohook", false);
        let err = set_hooks(&settings, &hook, true).unwrap_err();
        assert!(err.contains("da-hook.exe"), "{err}");
        assert!(!settings.exists());

        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        std::fs::write(&settings, "{}").unwrap();
        assert!(set_hooks(&settings, &hook, true).is_err());
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), "{}");
        assert!(!settings.with_file_name("settings.json.desktop-assistant.bak").exists());
    }

    #[test]
    fn invalid_settings_error_and_stay_untouched() {
        let (settings, hook) = fixture("invalid", true);
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        std::fs::write(&settings, "{ not json").unwrap();
        assert!(set_hooks(&settings, &hook, true).is_err());
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), "{ not json");
    }

    #[test]
    fn disabling_works_even_if_the_hook_exe_is_gone() {
        let (settings, hook) = fixture("gone", true);
        set_hooks(&settings, &hook, true).unwrap();
        std::fs::remove_file(&hook).unwrap();
        set_hooks(&settings, &hook, false).unwrap();
        assert!(!detect_hooks_on(&settings, &hook));
    }

    #[test]
    fn confirm_text_names_the_path_and_the_promises() {
        let p = Path::new(r"C:\Users\x\.claude\settings.json");
        let on = confirm_text(p, true);
        assert!(on.contains(r"C:\Users\x\.claude\settings.json"));
        assert!(on.contains("nine"));
        assert!(on.contains("existing hooks"));
        assert!(on.contains("backup"));
        assert!(on.contains("undone"));
        let off = confirm_text(p, false);
        assert!(off.contains(r"C:\Users\x\.claude\settings.json"));
        assert!(off.contains("removes"));
    }
}
