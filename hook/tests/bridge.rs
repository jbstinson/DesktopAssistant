use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn temp_appdata(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("da-bridge-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("DesktopAssistant")).unwrap();
    d
}

/// Runs da-hook; asserts exit 0, empty stdout, under 2 seconds.
fn run(appdata: &std::path::Path, stdin: &str, extra_env: &[(&str, &str)]) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_da-hook"));
    cmd.env("APPDATA", appdata)
        .env_remove("CLAUDE_CODE_ENTRYPOINT")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let start = Instant::now();
    let mut child = cmd.spawn().unwrap();
    let _ = child.stdin.take().unwrap().write_all(stdin.as_bytes());
    let mut out = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut out).unwrap();
    let status = child.wait().unwrap();
    assert!(start.elapsed() < Duration::from_secs(2), "too slow: {:?}", start.elapsed());
    assert!(status.success());
    assert!(out.is_empty());
}

const EV: &str = r#"{"hook_event_name":"PreToolUse","session_id":"s1","cwd":"C:\\x","tool_name":"Read","tool_input":{"secret":"x"}}"#;

#[test]
fn no_endpoint_file_exits_zero() {
    run(&temp_appdata("none"), EV, &[]);
}

#[test]
fn malformed_endpoint_file_exits_zero() {
    let d = temp_appdata("bad");
    std::fs::write(d.join("DesktopAssistant/endpoint.json"), "{half").unwrap();
    run(&d, EV, &[]);
}

#[test]
fn dead_port_exits_zero() {
    let d = temp_appdata("dead");
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    std::fs::write(
        d.join("DesktopAssistant/endpoint.json"),
        format!(r#"{{"port":{port},"token":"t"}}"#),
    )
    .unwrap();
    run(&d, EV, &[]);
}

#[test]
fn empty_stdin_exits_zero() {
    run(&temp_appdata("empty"), "", &[]);
}

#[test]
fn forwards_mapped_fields_without_tool_input() {
    let d = temp_appdata("live");
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::fs::write(
        d.join("DesktopAssistant/endpoint.json"),
        format!(r#"{{"port":{port},"token":"tok"}}"#),
    )
    .unwrap();
    let server = std::thread::spawn(move || {
        l.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        // The hook first opens a bare probe connection; skip any connection that sends nothing.
        let (mut s, mut buf) = 'conn: loop {
            let mut s = match l.accept() {
                Ok((s, _)) => s,
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(e) => panic!("hook never connected: {e}"),
            };
            s.set_nonblocking(false).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut chunk = [0u8; 1024];
            let n = s.read(&mut chunk).unwrap_or(0);
            if n > 0 {
                break 'conn (s, chunk[..n].to_vec());
            }
        };
        let mut chunk = [0u8; 1024];
        loop {
            let text = String::from_utf8_lossy(&buf).to_string();
            if let Some(i) = text.find("\r\n\r\n") {
                let len: usize = text
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                if buf.len() >= i + 4 + len {
                    break;
                }
            }
            let n = s.read(&mut chunk).unwrap();
            buf.extend_from_slice(&chunk[..n]);
        }
        s.write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n").unwrap();
        String::from_utf8_lossy(&buf).to_string()
    });
    run(&d, EV, &[("CLAUDE_CODE_ENTRYPOINT", "sdk-ts")]);
    let req = server.join().unwrap();
    assert!(req.starts_with("POST /event "));
    assert!(req.contains("Bearer tok"));
    let body = &req[req.find("\r\n\r\n").unwrap() + 4..];
    let v: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(v["name"], "PreToolUse");
    assert_eq!(v["session_id"], "s1");
    assert_eq!(v["tool"], "Read");
    assert_eq!(v["source"], "sdk-ts");
    assert!(v.get("terminal_pid").is_none());
    assert!(!body.contains("secret"));
}

#[test]
fn server_that_never_replies_still_exits_zero_within_two_seconds() {
    let d = temp_appdata("hang");
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::fs::write(
        d.join("DesktopAssistant/endpoint.json"),
        format!(r#"{{"port":{port},"token":"t"}}"#),
    )
    .unwrap();
    // Accept in the background and hold the connection open without replying.
    let held = std::thread::spawn(move || {
        let conn = l.accept().unwrap();
        std::thread::sleep(Duration::from_secs(4));
        drop(conn);
    });
    run(&d, EV, &[]);
    drop(held);
}

#[test]
fn dead_port_exits_zero_quickly() {
    let d = temp_appdata("deadfast");
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    std::fs::write(
        d.join("DesktopAssistant/endpoint.json"),
        format!(r#"{{"port":{port},"token":"t"}}"#),
    )
    .unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_da-hook"));
    cmd.env("APPDATA", &d)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let start = Instant::now();
    let mut child = cmd.spawn().unwrap();
    let _ = child.stdin.take().unwrap().write_all(EV.as_bytes());
    let mut out = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut out).unwrap();
    assert!(child.wait().unwrap().success());
    assert!(out.is_empty());
    assert!(start.elapsed() < Duration::from_millis(500), "too slow: {:?}", start.elapsed());
}

/// A pid that belonged to a process which has exited.
fn dead_pid() -> u32 {
    let mut child = Command::new("cmd").args(["/c", "exit"]).spawn().unwrap();
    let pid = child.id();
    child.wait().unwrap();
    pid
}

#[test]
fn dead_app_pid_skips_the_port_probe_and_returns_fast() {
    let d = temp_appdata("deadpid");
    // Something is listening, so a probe would connect; the dead pid must stop it trying.
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    l.set_nonblocking(true).unwrap();
    let port = l.local_addr().unwrap().port();
    std::fs::write(
        d.join("DesktopAssistant/endpoint.json"),
        format!(r#"{{"port":{port},"token":"t","pid":{}}}"#, dead_pid()),
    )
    .unwrap();
    let start = Instant::now();
    run(&d, EV, &[]);
    assert!(start.elapsed() < Duration::from_secs(1), "slow: {:?}", start.elapsed());
    assert!(l.accept().is_err(), "the hook connected although the app pid is gone");
}

#[test]
fn live_app_pid_still_delivers() {
    let d = temp_appdata("livepid");
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    l.set_nonblocking(true).unwrap();
    let port = l.local_addr().unwrap().port();
    // The test process itself stands in for the running app.
    std::fs::write(
        d.join("DesktopAssistant/endpoint.json"),
        format!(r#"{{"port":{port},"token":"t","pid":{}}}"#, std::process::id()),
    )
    .unwrap();
    run(&d, EV, &[]);
    assert!(l.accept().is_ok(), "the hook never connected to a live app");
}

#[test]
fn forwards_agent_id_only_when_present() {
    for (extra, want) in [(r#","agent_id":"ag-7""#, Some("ag-7")), ("", None)] {
        let d = temp_appdata("agent");
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::fs::write(
            d.join("DesktopAssistant/endpoint.json"),
            format!(r#"{{"port":{port},"token":"tok"}}"#),
        )
        .unwrap();
        let ev = format!(
            r#"{{"hook_event_name":"PreToolUse","session_id":"s1","cwd":"C:/x","tool_name":"Read"{extra}}}"#
        );
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            l.set_nonblocking(true).unwrap();
            loop {
                let Ok((mut s, _)) = l.accept() else {
                    assert!(Instant::now() < deadline, "hook never connected");
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                };
                s.set_nonblocking(false).unwrap();
                s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut buf = Vec::new();
                let mut chunk = [0u8; 2048];
                // Read until the JSON body is complete (ends with a closing brace).
                while let Ok(n) = s.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.ends_with(b"}") {
                        break;
                    }
                }
                if buf.is_empty() {
                    continue; // the bare probe connection
                }
                let _ = s.write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n");
                return String::from_utf8_lossy(&buf).to_string();
            }
        });
        run(&d, &ev, &[]);
        let req = server.join().unwrap();
        let body = &req[req.find("\r\n\r\n").unwrap() + 4..];
        let v: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(v.get("agent_id").and_then(|a| a.as_str()), want);
        let mut keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        let mut expect = vec!["cwd", "name", "notification_type", "session_id", "source", "tool"];
        if want.is_some() {
            expect.push("agent_id");
        }
        expect.sort();
        assert_eq!(keys, expect);
    }
}
