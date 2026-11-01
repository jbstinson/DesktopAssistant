//! Brings a session's window forward. The choice of target is pure; the Windows calls are
//! in `win` and only ever touch windows of the target. No keystrokes are sent to anything.

/// Where a session lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The terminal window owned by this process.
    Terminal(u32),
    /// The Claude desktop app's main window.
    DesktopApp,
    /// Nothing to bring forward (background sessions, unknown sources).
    None,
}

pub fn target_for(source: &str, terminal_pid: Option<u32>) -> Target {
    match (source, terminal_pid) {
        ("cli", Some(pid)) if pid != 0 => Target::Terminal(pid),
        ("claude-desktop", _) => Target::DesktopApp,
        _ => Target::None,
    }
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
    use super::{is_desktop_app_exe, is_main_window, Outcome, Target};
    use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM};
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    struct Search {
        want: Target,
        found: HWND,
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
            Target::Terminal(p) => pid == p,
            Target::DesktopApp => exe_path(pid).is_some_and(|p| is_desktop_app_exe(&p)),
            Target::None => false,
        };
        if !wanted {
            return 1;
        }
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let has_owner = !GetWindow(hwnd, GW_OWNER).is_null();
        if is_main_window(
            IsWindowVisible(hwnd) != 0,
            has_owner,
            ex & WS_EX_TOOLWINDOW != 0,
            GetWindowTextLengthW(hwnd).max(0) as usize,
        ) {
            s.found = hwnd;
            return 0;
        }
        1
    }

    pub fn focus(target: Target) -> Result<Outcome, String> {
        unsafe {
            let mut s = Search { want: target, found: std::ptr::null_mut() };
            EnumWindows(Some(visit), &mut s as *mut Search as LPARAM);
            let hwnd = s.found;
            if hwnd.is_null() {
                return Err("window not found".into());
            }
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
        assert_eq!(target_for("cli", Some(42)), Target::Terminal(42));
    }

    #[test]
    fn desktop_is_desktop_app() {
        assert_eq!(target_for("claude-desktop", None), Target::DesktopApp);
        assert_eq!(target_for("claude-desktop", Some(5)), Target::DesktopApp);
    }

    #[test]
    fn sdk_cli_and_unknown_are_none() {
        assert_eq!(target_for("sdk-cli", Some(42)), Target::None);
        assert_eq!(target_for("", Some(42)), Target::None);
        assert_eq!(target_for("whatever", None), Target::None);
    }

    #[test]
    fn cli_without_pid_is_none() {
        assert_eq!(target_for("cli", None), Target::None);
        assert_eq!(target_for("cli", Some(0)), Target::None);
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
