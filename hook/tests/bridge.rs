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

const PERM: &str = r#"{"hook_event_name":"PermissionRequest","session_id":"s1","cwd":"C:/x","tool_name":"Bash","tool_input":{"command":"rm x","description":"Remove x"},"permission_suggestions":[{"type":"addRules"}]}"#;
const HOLD: &str = r#"{"id":"ab12","hold":true}"#;
const NO_HOLD: &str = r#"{"id":"ab12","hold":false}"#;
const ALLOW: &str = r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#;
const DENY: &str = r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny","message":"no"}}}"#;

/// One scripted answer from the fake app.
struct Answer {
    code: u16,
    headers: String,
    body: String,
    /// Read the request, then keep the connection open without answering.
    hang: bool,
}

fn say(code: u16, body: &str) -> Answer {
    Answer { code, headers: String::new(), body: body.to_string(), hang: false }
}

fn waiting() -> Answer {
    say(204, "")
}

fn hang() -> Answer {
    Answer { hang: true, ..waiting() }
}

/// Reads one whole HTTP request; empty for the bare probe connection.
fn read_request(s: &mut std::net::TcpStream) -> String {
    s.set_nonblocking(false).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let text = String::from_utf8_lossy(&buf).to_string();
        if let Some(i) = text.find("\r\n\r\n") {
            let len: usize = text[..i]
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(|v| v.trim().parse().unwrap())
                })
                .unwrap_or(0);
            if buf.len() >= i + 4 + len {
                return text;
            }
        }
        match s.read(&mut chunk) {
            Ok(0) | Err(_) => return String::from_utf8_lossy(&buf).to_string(),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
}

/// Answers each request with the next scripted answer, then stops listening, so anything
/// after the script finds a dead server. Returns the requests it saw.
fn fake_app(tag: &str, script: Vec<Answer>) -> (std::path::PathBuf, std::thread::JoinHandle<Vec<String>>) {
    let d = temp_appdata(tag);
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::fs::write(
        d.join("DesktopAssistant/endpoint.json"),
        format!(r#"{{"port":{port},"token":"tok"}}"#),
    )
    .unwrap();
    let server = std::thread::spawn(move || {
        l.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut seen = Vec::new();
        for answer in script {
            loop {
                let Ok((mut s, _)) = l.accept() else {
                    if Instant::now() > deadline {
                        return seen;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                let req = read_request(&mut s);
                if req.is_empty() {
                    continue;
                }
                seen.push(req);
                if answer.hang {
                    std::thread::sleep(Duration::from_millis(2500));
                    break;
                }
                let length = if answer.code == 204 {
                    String::new()
                } else {
                    format!("Content-Length: {}\r\n", answer.body.len())
                };
                let reply = format!(
                    "HTTP/1.1 {} X\r\n{}{length}Connection: close\r\n\r\n{}",
                    answer.code, answer.headers, answer.body
                );
                let _ = s.write_all(reply.as_bytes());
                break;
            }
        }
        seen
    });
    (d, server)
}

/// Runs da-hook to the end; asserts exit 0 and returns stdout and how long it took.
fn run_capture(appdata: &std::path::Path, stdin: &str) -> (String, Duration) {
    let start = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_da-hook"))
        .env("APPDATA", appdata)
        .env_remove("CLAUDE_CODE_ENTRYPOINT")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(stdin.as_bytes());
    let mut out = String::new();
    child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    assert!(child.wait().unwrap().success());
    (out, start.elapsed())
}

fn body_of(req: &str) -> serde_json::Value {
    serde_json::from_str(&req[req.find("\r\n\r\n").unwrap() + 4..]).unwrap()
}

#[test]
fn hold_false_exits_at_once_with_nothing() {
    let (d, server) = fake_app("nohold", vec![say(200, NO_HOLD)]);
    let (out, took) = run_capture(&d, PERM);
    assert_eq!(out, "");
    assert!(took < Duration::from_secs(2), "too slow: {took:?}");
    assert_eq!(server.join().unwrap().len(), 1, "polled although hold was false");
}

#[test]
fn decided_allow_prints_exactly_the_reply() {
    let (d, server) = fake_app("allow", vec![say(200, HOLD), waiting(), say(200, ALLOW)]);
    let (out, _) = run_capture(&d, PERM);
    assert_eq!(out, ALLOW);
    let seen = server.join().unwrap();
    assert!(seen[0].starts_with("POST /permission "), "{}", seen[0]);
    assert!(seen[1].starts_with("GET /permission/ab12 "), "{}", seen[1]);
    assert!(seen[2].starts_with("GET /permission/ab12 "), "{}", seen[2]);
    assert!(seen.iter().all(|r| r.contains("Bearer tok")));
}

#[test]
fn decided_deny_prints_exactly_the_reply() {
    let (d, server) = fake_app("deny", vec![say(200, HOLD), say(200, DENY)]);
    assert_eq!(run_capture(&d, PERM).0, DENY);
    server.join().unwrap();
}

#[test]
fn a_hold_outlasts_the_two_second_limit() {
    let mut script = vec![say(200, HOLD)];
    script.extend((0..12).map(|_| waiting()));
    script.push(say(200, ALLOW));
    let (d, server) = fake_app("long", script);
    let (out, took) = run_capture(&d, PERM);
    assert_eq!(out, ALLOW);
    assert!(took > Duration::from_millis(2500), "polled too fast: {took:?}");
    assert_eq!(server.join().unwrap().len(), 14);
}

#[test]
fn permission_body_carries_the_request_and_never_touches_event() {
    let (d, server) = fake_app("body", vec![say(200, NO_HOLD)]);
    run_capture(&d, PERM);
    let seen = server.join().unwrap();
    assert!(seen.iter().all(|r| !r.starts_with("POST /event")));
    let v = body_of(&seen[0]);
    assert_eq!(v["session_id"], "s1");
    assert_eq!(v["cwd"], "C:/x");
    assert_eq!(v["tool_name"], "Bash");
    assert_eq!(v["tool_input"]["command"], "rm x");
    assert_eq!(v["permission_suggestions"][0]["type"], "addRules");
    assert_eq!(v["description"], "Remove x");
    assert_eq!(v["source"], "");
    assert!(v.get("terminal_pid").is_none());
}

#[test]
fn only_a_string_description_is_passed_on() {
    let ev = PERM.replace(r#""description":"Remove x""#, r#""description":7"#);
    let (d, server) = fake_app("desc", vec![say(200, NO_HOLD)]);
    run_capture(&d, &ev);
    assert!(body_of(&server.join().unwrap()[0])["description"].is_null());
}

#[test]
fn gone_prints_nothing() {
    let (d, server) = fake_app("gone", vec![say(200, HOLD), waiting(), say(410, "")]);
    assert_eq!(run_capture(&d, PERM).0, "");
    server.join().unwrap();
}

#[test]
fn replies_that_are_not_a_decision_print_nothing() {
    let bad = [
        "",
        "null",
        "allow",
        "{half",
        "{}",
        r#"{"behavior":"allow"}"#,
        r#"{"decision":{"behavior":"allow"}}"#,
        r#"{"hookSpecificOutput":{"decision":{}}}"#,
        r#"{"hookSpecificOutput":{"decision":{"behavior":"ask"}}}"#,
        r#"{"hookSpecificOutput":{"decision":{"behavior":"Allow"}}}"#,
        r#"{"hookSpecificOutput":{"decision":{"behavior":"allow "}}}"#,
        r#"{"hookSpecificOutput":{"decision":{"behavior":true}}}"#,
    ];
    for (i, reply) in bad.iter().enumerate() {
        let (d, server) = fake_app(&format!("notdec{i}"), vec![say(200, HOLD), say(200, reply)]);
        assert_eq!(run_capture(&d, PERM).0, "", "{reply}");
        server.join().unwrap();
    }
}

#[test]
fn a_decision_with_any_other_status_prints_nothing() {
    for code in [201, 202, 400, 401, 404, 500] {
        let (d, server) = fake_app(&format!("code{code}"), vec![say(200, HOLD), say(code, ALLOW)]);
        assert_eq!(run_capture(&d, PERM).0, "", "{code}");
        server.join().unwrap();
    }
}

#[test]
fn a_bad_open_reply_prints_nothing_and_never_polls() {
    let bad = [
        say(401, ""),
        say(404, ""),
        say(413, ""),
        say(500, HOLD),
        say(201, HOLD),
        say(200, ""),
        say(200, "{half"),
        say(200, r#"{"hold":true}"#),
        say(200, r#"{"id":"ab12"}"#),
        say(200, r#"{"id":"","hold":true}"#),
        say(200, r#"{"id":"../event","hold":true}"#),
        say(200, r#"{"id":"ab12","hold":"true"}"#),
    ];
    for (i, answer) in bad.into_iter().enumerate() {
        let (d, server) = fake_app(&format!("badopen{i}"), vec![answer]);
        let (out, took) = run_capture(&d, PERM);
        assert_eq!(out, "");
        assert!(took < Duration::from_secs(2), "case {i} polled on: {took:?}");
        server.join().unwrap();
    }
}

#[test]
fn a_server_dying_mid_hold_prints_nothing() {
    let (d, server) = fake_app("dying", vec![say(200, HOLD), waiting(), waiting()]);
    assert_eq!(run_capture(&d, PERM).0, "");
    server.join().unwrap();
}

#[test]
fn a_killed_intermediate_process_ends_the_hold() {
    // cmd stands in for bash: test > cmd > da-hook. Killing cmd leaves da-hook running, as
    // killing a session does on Windows, so the hook must notice by itself.
    let waits: Vec<Answer> = (0..60).map(|_| waiting()).collect();
    let mut script = vec![say(200, HOLD)];
    script.extend(waits);
    let (d, _server) = fake_app("chainkill", script);
    let start = Instant::now();
    let mut cmd = Command::new("cmd")
        .arg("/c")
        .arg(env!("CARGO_BIN_EXE_da-hook"))
        .env("APPDATA", &d)
        .env_remove("CLAUDE_CODE_ENTRYPOINT")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = cmd.stdin.take().unwrap();
    let mut stdout = cmd.stdout.take().unwrap();
    stdin.write_all(PERM.as_bytes()).unwrap();
    drop(stdin);
    std::thread::sleep(Duration::from_millis(1200));
    cmd.kill().unwrap();
    let killed = Instant::now();
    let mut out = String::new();
    stdout.read_to_string(&mut out).unwrap();
    assert_eq!(out, "");
    assert!(killed.elapsed() < Duration::from_millis(1500), "kept holding: {:?}", killed.elapsed());
    assert!(start.elapsed() < Duration::from_secs(5));
    let _ = cmd.wait();
}

#[test]
fn a_stuck_poll_ends_the_wait_after_its_one_second_limit() {
    let (d, server) = fake_app("stuck", vec![say(200, HOLD), hang()]);
    let (out, took) = run_capture(&d, PERM);
    assert_eq!(out, "");
    assert!(took < Duration::from_millis(2400), "too slow: {took:?}");
    server.join().unwrap();
}

#[test]
fn redirects_are_not_followed() {
    let elsewhere = TcpListener::bind("127.0.0.1:0").unwrap();
    elsewhere.set_nonblocking(true).unwrap();
    let port = elsewhere.local_addr().unwrap().port();
    let moved = || Answer {
        headers: format!("Location: http://127.0.0.1:{port}/permission/ab12\r\n"),
        ..say(302, ALLOW)
    };
    let (d, server) = fake_app("redir", vec![say(200, HOLD), moved()]);
    assert_eq!(run_capture(&d, PERM).0, "");
    server.join().unwrap();
    let (d, server) = fake_app("redir-open", vec![moved()]);
    assert_eq!(run_capture(&d, PERM).0, "");
    server.join().unwrap();
    assert!(elsewhere.accept().is_err(), "the hook followed a redirect");
}

#[test]
fn no_app_at_all_prints_nothing_for_a_permission_request() {
    let (out, took) = run_capture(&temp_appdata("noapp"), PERM);
    assert_eq!(out, "");
    assert!(took < Duration::from_secs(1), "too slow: {took:?}");
}

#[test]
fn a_stuck_stdin_keeps_the_short_limit() {
    let d = temp_appdata("stdin");
    let start = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_da-hook"))
        .env("APPDATA", &d)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(&PERM.as_bytes()[..20]).unwrap();
    let mut out = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut out).unwrap();
    assert!(child.wait().unwrap().success());
    assert!(out.is_empty());
    assert!(start.elapsed() < Duration::from_millis(2500), "too slow: {:?}", start.elapsed());
    drop(stdin);
}

#[test]
fn other_events_still_go_to_event_without_the_input_fields() {
    let ev = PERM.replace("PermissionRequest", "PreToolUse");
    let (d, server) = fake_app("otherev", vec![waiting()]);
    assert_eq!(run_capture(&d, &ev).0, "");
    let seen = server.join().unwrap();
    assert!(seen[0].starts_with("POST /event "));
    assert!(!seen[0].contains("rm x") && !seen[0].contains("Remove x"));
    assert!(!seen[0].contains("addRules") && !seen[0].contains("tool_input"));
}
