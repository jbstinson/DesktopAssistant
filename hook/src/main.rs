#![windows_subsystem = "windows"]
//! da-hook: reads a Claude Code hook event on stdin and forwards it to the
//! desktop app over localhost. Always exits 0 with empty stdout.

use serde_json::{json, Value};
use std::io::Read;
use std::time::Duration;

fn main() {
    // Release builds use panic = "abort"; make any panic a clean exit 0.
    std::panic::set_hook(Box::new(|_| std::process::exit(0)));
    // The whole job runs on a worker so a stuck stdin or network call cannot
    // hold the session past the 2 second budget.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        run();
        let _ = tx.send(());
    });
    let _ = rx.recv_timeout(Duration::from_millis(1800));
    std::process::exit(0);
}

fn run() {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return;
    }
    let Ok(v) = serde_json::from_str::<Value>(&input) else { return };
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);

    let Some((port, token, app_pid)) = read_endpoint() else { return };
    // The app is gone: skip the connect probe and everything after it.
    if app_pid.is_some_and(|pid| !process_exists(pid)) {
        return;
    }
    let source = std::env::var("CLAUDE_CODE_ENTRYPOINT").unwrap_or_default();

    // tool_input is deliberately never read or forwarded.
    let mut event = json!({
        "name": s("hook_event_name").unwrap_or_default(),
        "session_id": s("session_id").unwrap_or_default(),
        "cwd": s("cwd").unwrap_or_default(),
        "tool": s("tool_name"),
        "notification_type": s("notification_type"),
        "source": source,
    });
    // Present only for events from a subagent.
    if let Some(agent) = s("agent_id") {
        event["agent_id"] = json!(agent);
    }
    if source == "cli" {
        event["terminal_pid"] = json!(terminal::terminal_pid());
    }

    // Windows takes about 2 seconds to report a refused loopback connect. A live app on
    // loopback accepts almost at once, so give up fast when nothing is listening.
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(150)).is_err() {
        return;
    }

    // The URL is built only from the port, so the target is always loopback;
    // redirects are disabled so a stale port cannot send us elsewhere.
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(1))
        .build();
    let _ = agent
        .post(&format!("http://127.0.0.1:{port}/event"))
        .set("Authorization", &format!("Bearer {token}"))
        .send_string(&event.to_string());
}

/// Whether a process with this id is running. A cheap handle check, well under a millisecond.
/// An access-denied answer still means the process exists.
fn process_exists(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ACCESS_DENIED};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    const STILL_ACTIVE: u32 = 259;
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return GetLastError() == ERROR_ACCESS_DENIED;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code);
        CloseHandle(h);
        ok == 0 || code == STILL_ACTIVE
    }
}

/// Port, token and (when the app wrote one) the app's process id.
fn read_endpoint() -> Option<(u16, String, Option<u32>)> {
    let dir = std::path::PathBuf::from(std::env::var_os("APPDATA")?).join("DesktopAssistant");
    let text = std::fs::read_to_string(dir.join("endpoint.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let port = u16::try_from(v.get("port")?.as_u64()?).ok()?;
    let token = v.get("token")?.as_str()?.to_owned();
    let pid = v.get("pid").and_then(Value::as_u64).and_then(|p| u32::try_from(p).ok());
    Some((port, token, pid))
}

mod terminal {
    use std::collections::HashMap;
    use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, INVALID_HANDLE_VALUE, LPARAM};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible,
    };

    struct Search {
        pid: u32,
        found: bool,
    }

    unsafe extern "system" fn each_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let s = &mut *(lparam as *mut Search);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == s.pid && IsWindowVisible(hwnd) != 0 {
            s.found = true;
            return 0;
        }
        1
    }

    fn has_visible_window(pid: u32) -> bool {
        let mut s = Search { pid, found: false };
        unsafe { EnumWindows(Some(each_window), &mut s as *mut Search as LPARAM) };
        s.found
    }

    /// Parent pid and executable name of every process.
    type Procs = HashMap<u32, (u32, String)>;

    fn processes() -> Procs {
        let mut map = HashMap::new();
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return map;
            }
            let mut e: PROCESSENTRY32 = std::mem::zeroed();
            e.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;
            let mut ok = Process32First(snap, &mut e);
            while ok != 0 {
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                let bytes: Vec<u8> = e.szExeFile[..len].iter().map(|&c| c as u8).collect();
                let name = String::from_utf8_lossy(&bytes).into_owned();
                map.insert(e.th32ProcessID, (e.th32ParentProcessID, name));
                ok = Process32Next(snap, &mut e);
            }
            CloseHandle(snap);
        }
        map
    }

    /// First ancestor of `start` that owns a visible window. Reaching explorer.exe first means
    /// the session was not started from a terminal window (explorer owns the desktop and
    /// taskbar), so there is nothing to jump to.
    fn find_terminal(start: u32, procs: &Procs, has_window: impl Fn(u32) -> bool) -> Option<u32> {
        let mut pid = start;
        for _ in 0..32 {
            pid = procs.get(&pid)?.0;
            if pid == 0 {
                return None;
            }
            if procs.get(&pid).is_some_and(|(_, name)| name.eq_ignore_ascii_case("explorer.exe")) {
                return None;
            }
            if has_window(pid) {
                return Some(pid);
            }
        }
        None
    }

    /// First ancestor of this process that owns a visible top-level window, never explorer.
    pub fn terminal_pid() -> Option<u32> {
        find_terminal(std::process::id(), &processes(), has_visible_window)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn procs(list: &[(u32, u32, &str)]) -> Procs {
            list.iter().map(|&(pid, parent, name)| (pid, (parent, name.to_string()))).collect()
        }

        #[test]
        fn finds_the_first_ancestor_with_a_window() {
            let p = procs(&[(10, 20, "da-hook.exe"), (20, 30, "bash.exe"), (30, 40, "WindowsTerminal.exe"), (40, 0, "explorer.exe")]);
            assert_eq!(find_terminal(10, &p, |pid| pid == 30 || pid == 40), Some(30));
        }

        #[test]
        fn explorer_means_no_terminal() {
            let p = procs(&[(10, 20, "da-hook.exe"), (20, 30, "claude.exe"), (30, 40, "Explorer.EXE"), (40, 0, "winlogon.exe")]);
            // Explorer owns visible windows, but it is never a terminal.
            assert_eq!(find_terminal(10, &p, |pid| pid == 30 || pid == 40), None);
        }

        #[test]
        fn no_window_or_missing_parent_is_none() {
            let p = procs(&[(10, 20, "a.exe"), (20, 0, "b.exe")]);
            assert_eq!(find_terminal(10, &p, |_| false), None);
            assert_eq!(find_terminal(99, &p, |_| true), None);
        }
    }
}
