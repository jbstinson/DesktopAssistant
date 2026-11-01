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

    let Some((port, token)) = read_endpoint() else { return };
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

fn read_endpoint() -> Option<(u16, String)> {
    let dir = std::path::PathBuf::from(std::env::var_os("APPDATA")?).join("DesktopAssistant");
    let text = std::fs::read_to_string(dir.join("endpoint.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let port = u16::try_from(v.get("port")?.as_u64()?).ok()?;
    let token = v.get("token")?.as_str()?.to_owned();
    Some((port, token))
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

    fn parents() -> HashMap<u32, u32> {
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
                map.insert(e.th32ProcessID, e.th32ParentProcessID);
                ok = Process32Next(snap, &mut e);
            }
            CloseHandle(snap);
        }
        map
    }

    /// First ancestor of this process that owns a visible top-level window.
    pub fn terminal_pid() -> Option<u32> {
        let map = parents();
        let mut pid = std::process::id();
        for _ in 0..32 {
            pid = *map.get(&pid)?;
            if pid == 0 {
                return None;
            }
            if has_visible_window(pid) {
                return Some(pid);
            }
        }
        None
    }
}
