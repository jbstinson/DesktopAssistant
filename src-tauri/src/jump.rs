//! Brings a session's window forward. The choice of target is pure; the Windows calls are
//! in `win` and only ever touch windows of the target. No keystrokes are sent to anything.

//!
//! Finding the right VS Code window is a tray option ("Jump to VS Code window"), off by default
//! and saved as `vscode-jump.txt` in the data folder. Off, VS Code sessions jump as they always
//! did: the extension's sessions not at all, its built-in terminal to VS Code's first window.

use std::path::Path;

/// Where a session lives. `project` is the session's folder name, used to tell VS Code
/// windows apart: VS Code runs every window from one process, so the process alone is not enough.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The terminal window owned by this process. With a `project` and that process being VS Code
    /// (Claude Code in its built-in terminal), the window showing `project` is picked from its
    /// windows; without one, its first window.
    Terminal { pid: u32, project: Option<String> },
    /// The VS Code window showing `project` (the Claude Code extension's panel).
    VsCode { project: String },
    /// The Claude desktop app's main window.
    DesktopApp,
    /// Nothing to bring forward (background sessions, unknown sources).
    None,
}

/// `vscode` is the tray option: whether VS Code sessions are matched to their window.
pub fn target_for(source: &str, terminal_pid: Option<u32>, project: &str, vscode: bool) -> Target {
    let project = project.to_string();
    match (source, terminal_pid) {
        ("cli", Some(pid)) if pid != 0 => Target::Terminal { pid, project: vscode.then_some(project) },
        ("claude-vscode", _) if vscode => Target::VsCode { project },
        ("claude-desktop", _) => Target::DesktopApp,
        _ => Target::None,
    }
}

/// Reads the "Jump to VS Code window" option. Only a file saying `on` turns it on.
pub fn load_vscode_option(path: &Path) -> bool {
    std::fs::read_to_string(path).unwrap_or_default().trim() == "on"
}

/// Writes the option atomically: temp file, then rename.
pub fn save_vscode_option(path: &Path, on: bool) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("txt.tmp");
    std::fs::write(&tmp, if on { "on" } else { "off" })?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// True for VS Code and the editors built from it, which all host the Claude Code extension.
pub fn is_vscode_exe(path: &str) -> bool {
    let p = path.replace('/', "\\").to_lowercase();
    let name = p.rsplit('\\').next().unwrap_or("");
    matches!(name, "code.exe" | "code - insiders.exe" | "vscodium.exe" | "cursor.exe" | "windsurf.exe")
}

/// True if a VS Code window title names `project` as its folder. Titles are parts joined by
/// " - " (or " \u{2014} "), such as "main.rs - DesktopAssistant - Visual Studio Code"; the folder
/// part may carry " (Workspace)" or a remote tag like " [WSL: Ubuntu]".
pub fn title_shows_project(title: &str, project: &str) -> bool {
    let project = project.trim().to_lowercase();
    if project.is_empty() {
        return false;
    }
    title.split(" - ").flat_map(|part| part.split(" \u{2014} ")).any(|part| {
        // A leading dot marks unsaved changes.
        let part = part.trim().trim_start_matches('\u{25cf}').trim().to_lowercase();
        part == project
            || part.strip_prefix(&project).is_some_and(|rest| rest.starts_with(" (") || rest.starts_with(" ["))
    })
}

/// Which of an editor's windows (by title) to bring forward for `project`: the first that shows
/// it, or the only window there is. With several windows and none showing it, none: guessing
/// would bring the wrong project forward.
pub fn pick_window(titles: &[String], project: &str) -> Option<usize> {
    titles
        .iter()
        .position(|t| title_shows_project(t, project))
        .or((titles.len() == 1).then_some(0))
}

/// What it took to bring the window forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The window is now the foreground window.
    Foreground,
    /// Windows refused foreground rights; the taskbar button was flashed instead.
    Flashed,
}

/// True for the Claude desktop app's executable and nothing else named claude.exe. The Claude
/// Code CLI is also claude.exe (under .local\bin or ...\claude-code\...) and has no window.
pub fn is_desktop_app_exe(path: &str) -> bool {
    let p = path.replace('/', "\\").to_lowercase();
    let name = p.rsplit('\\').next().unwrap_or("");
    if name != "claude.exe" || p.contains(r"\claude-code\") || p.contains(r"\.local\bin\") {
        return false;
    }
    p.contains(r"\windowsapps\claude_")
        || p.contains(r"\anthropicclaude\")
        || p.contains(r"\programs\claude\")
}

/// A window worth bringing forward: visible, titled, and neither a tool window nor owned by
/// another window (popups, tooltips and helper windows are).
pub fn is_main_window(visible: bool, has_owner: bool, tool_window: bool, title_len: usize) -> bool {
    visible && !has_owner && !tool_window && title_len > 0
}

pub fn focus(target: Target) -> Result<Outcome, String> {
    match target {
        Target::None => Ok(Outcome::Foreground),
        _ => win::focus(target),
    }
}

#[cfg(windows)]
mod win {
    use super::{is_desktop_app_exe, is_main_window, is_vscode_exe, pick_window, Outcome, Target};
    use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM};
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    /// Which processes' windows to collect.
    enum Want {
        Pid(u32),
        DesktopApp,
        VsCode,
    }

    struct Search {
        want: Want,
        /// Every matching main window, topmost first, with its title.
        found: Vec<(HWND, String)>,
        /// Process ids already checked against `want`, with the answer.
        checked: Vec<(u32, bool)>,
    }

    unsafe fn exe_path(pid: u32) -> Option<String> {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }

    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let s = &mut *(lparam as *mut Search);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let wanted = match s.want {
            Want::Pid(p) => pid == p,
            _ => match s.checked.iter().find(|(p, _)| *p == pid) {
                Some(&(_, w)) => w,
                None => {
                    let w = exe_path(pid).is_some_and(|path| match s.want {
                        Want::DesktopApp => is_desktop_app_exe(&path),
                        _ => is_vscode_exe(&path),
                    });
                    s.checked.push((pid, w));
                    w
                }
            },
        };
        if !wanted {
            return 1;
        }
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let has_owner = !GetWindow(hwnd, GW_OWNER).is_null();
        let len = GetWindowTextLengthW(hwnd).max(0) as usize;
        if is_main_window(IsWindowVisible(hwnd) != 0, has_owner, ex & WS_EX_TOOLWINDOW != 0, len) {
            let mut buf = vec![0u16; len + 1];
            let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32).max(0) as usize;
            s.found.push((hwnd, String::from_utf16_lossy(&buf[..n])));
        }
        1
    }

    unsafe fn windows(want: Want) -> Vec<(HWND, String)> {
        let mut s = Search { want, found: Vec::new(), checked: Vec::new() };
        EnumWindows(Some(visit), &mut s as *mut Search as LPARAM);
        s.found
    }

    /// The window to bring forward for `target`, if there is one.
    unsafe fn find(target: &Target) -> Option<HWND> {
        let (found, project) = match target {
            Target::Terminal { pid, project } => {
                let found = windows(Want::Pid(*pid));
                match project {
                    Some(project) if exe_path(*pid).is_some_and(|p| is_vscode_exe(&p)) => (found, project),
                    _ => return found.first().map(|w| w.0),
                }
            }
            Target::VsCode { project } => (windows(Want::VsCode), project),
            Target::DesktopApp => return windows(Want::DesktopApp).first().map(|w| w.0),
            Target::None => return None,
        };
        let titles: Vec<String> = found.iter().map(|w| w.1.clone()).collect();
        pick_window(&titles, project).map(|i| found[i].0)
    }

    pub fn focus(target: Target) -> Result<Outcome, String> {
        unsafe {
            let hwnd = find(&target).ok_or("window not found")?;
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            AllowSetForegroundWindow(GetCurrentProcessId());
            SetForegroundWindow(hwnd);
            // The switch completes on the target's thread, so give it a moment to show.
            for _ in 0..10 {
                if GetForegroundWindow() == hwnd {
                    return Ok(Outcome::Foreground);
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            let info = FLASHWINFO {
                cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
                hwnd,
                dwFlags: FLASHW_ALL | FLASHW_TIMERNOFG,
                uCount: 3,
                dwTimeout: 0,
            };
            FlashWindowEx(&info);
            Ok(Outcome::Flashed)
        }
    }
}

#[cfg(not(windows))]
mod win {
    use super::{Outcome, Target};
    pub fn focus(_: Target) -> Result<Outcome, String> {
        Err("window not found".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_with_pid_is_terminal() {
        assert_eq!(target_for("cli", Some(42), "p", true), Target::Terminal { pid: 42, project: Some("p".into()) });
    }

    #[test]
    fn with_the_option_off_a_terminal_carries_no_project() {
        assert_eq!(target_for("cli", Some(42), "p", false), Target::Terminal { pid: 42, project: None });
    }

    #[test]
    fn vscode_extension_is_vscode_with_or_without_pid() {
        let want = Target::VsCode { project: "DesktopAssistant".into() };
        assert_eq!(target_for("claude-vscode", None, "DesktopAssistant", true), want);
        assert_eq!(target_for("claude-vscode", Some(5), "DesktopAssistant", true), want);
    }

    #[test]
    fn with_the_option_off_the_vscode_extension_is_none() {
        assert_eq!(target_for("claude-vscode", None, "p", false), Target::None);
        assert_eq!(target_for("claude-vscode", Some(5), "p", false), Target::None);
    }

    #[test]
    fn desktop_is_desktop_app() {
        for on in [true, false] {
            assert_eq!(target_for("claude-desktop", None, "p", on), Target::DesktopApp);
            assert_eq!(target_for("claude-desktop", Some(5), "p", on), Target::DesktopApp);
        }
    }

    #[test]
    fn sdk_cli_and_unknown_are_none() {
        for on in [true, false] {
            assert_eq!(target_for("sdk-cli", Some(42), "p", on), Target::None);
            assert_eq!(target_for("", Some(42), "p", on), Target::None);
            assert_eq!(target_for("whatever", None, "p", on), Target::None);
        }
    }

    #[test]
    fn cli_without_pid_is_none() {
        assert_eq!(target_for("cli", None, "p", true), Target::None);
        assert_eq!(target_for("cli", Some(0), "p", true), Target::None);
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("da-jump-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_vscode_option_is_off_unless_saved_on() {
        let p = temp_dir("option").join("vscode-jump.txt");
        assert!(!load_vscode_option(&p));
        for (text, want) in [("on", true), ("on\n", true), (" on ", true), ("off", false), ("ON", false), ("yes", false), ("", false)] {
            std::fs::write(&p, text).unwrap();
            assert_eq!(load_vscode_option(&p), want, "{text:?}");
        }
    }

    #[test]
    fn the_vscode_option_saves_and_loads_including_a_new_folder() {
        let p = temp_dir("save").join("nested").join("vscode-jump.txt");
        save_vscode_option(&p, true).unwrap();
        assert!(load_vscode_option(&p));
        save_vscode_option(&p, false).unwrap();
        assert!(!load_vscode_option(&p));
        assert!(!p.with_extension("txt.tmp").exists());
    }

    #[test]
    fn vscode_exe_names() {
        assert!(is_vscode_exe(r"C:\Users\A\AppData\Local\Programs\Microsoft VS Code\Code.exe"));
        assert!(is_vscode_exe(r"C:\Program Files\Microsoft VS Code Insiders\Code - Insiders.exe"));
        assert!(is_vscode_exe("C:/Users/A/AppData/Local/Programs/cursor/Cursor.exe"));
        assert!(!is_vscode_exe(r"C:\Windows\System32\cmd.exe"));
        assert!(!is_vscode_exe(r"C:\tools\mycode.exe"));
        assert!(!is_vscode_exe(""));
    }

    #[test]
    fn title_names_the_project_folder() {
        let p = "DesktopAssistant";
        assert!(title_shows_project("main.rs - DesktopAssistant - Visual Studio Code", p));
        assert!(title_shows_project("\u{25cf} main.rs - desktopassistant - Visual Studio Code", p));
        assert!(title_shows_project("DesktopAssistant - Visual Studio Code", p));
        assert!(title_shows_project("main.rs \u{2014} DesktopAssistant \u{2014} Cursor", p));
        assert!(title_shows_project("a.rs - DesktopAssistant (Workspace) - Visual Studio Code", p));
        assert!(title_shows_project("a.rs - DesktopAssistant [WSL: Ubuntu] - Visual Studio Code", p));
    }

    #[test]
    fn title_needs_the_whole_folder_name() {
        assert!(!title_shows_project("main.rs - DesktopAssistant2 - Visual Studio Code", "DesktopAssistant"));
        assert!(!title_shows_project("main.rs - Desktop - Visual Studio Code", "DesktopAssistant"));
        assert!(!title_shows_project("Welcome - Visual Studio Code", ""));
    }

    #[test]
    fn picks_the_window_showing_the_project() {
        let titles: Vec<String> = ["a.rs - other - Visual Studio Code", "b.rs - proj - Visual Studio Code"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(pick_window(&titles, "proj"), Some(1));
        // Several windows and none showing the project: no guess.
        assert_eq!(pick_window(&titles, "missing"), None);
        assert_eq!(pick_window(&[], "proj"), None);
    }

    #[test]
    fn a_lone_window_is_used_even_without_a_match() {
        let titles = vec!["Welcome - Visual Studio Code".to_string()];
        assert_eq!(pick_window(&titles, "proj"), Some(0));
    }

    #[test]
    fn desktop_exe_matches_install_locations_only() {
        assert!(is_desktop_app_exe(r"C:\Program Files\WindowsApps\Claude_4.4212.0.0_x64__pzs8sxrjxfjjc\app\claude.exe"));
        assert!(is_desktop_app_exe(r"C:\Users\A\AppData\Local\AnthropicClaude\app-1.0\Claude.exe"));
        assert!(!is_desktop_app_exe(r"C:\Users\A\.local\bin\claude.exe"));
        assert!(!is_desktop_app_exe(r"C:\Users\A\AppData\Roaming\Claude\claude-code\2.1.309\abc\claude.exe"));
        assert!(!is_desktop_app_exe(r"C:\Program Files\WindowsApps\Claude_4.4\app\other.exe"));
        assert!(!is_desktop_app_exe(r"C:\tools\claude.exe"));
        assert!(!is_desktop_app_exe(""));
    }

    #[test]
    fn main_window_rules() {
        assert!(is_main_window(true, false, false, 6));
        assert!(!is_main_window(false, false, false, 6));
        assert!(!is_main_window(true, true, false, 6));
        assert!(!is_main_window(true, false, true, 6));
        assert!(!is_main_window(true, false, false, 0));
    }
}
