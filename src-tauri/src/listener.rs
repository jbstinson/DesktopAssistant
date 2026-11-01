//! Localhost listener receiving hook events from da-hook.

use crate::events::HookEvent;
use std::io::Read;
use std::path::Path;
use tiny_http::{Method, Request, Response, Server};

const MAX_BODY: u64 = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub port: u16,
}

pub fn start(on_event: impl Fn(HookEvent) + Send + 'static) -> std::io::Result<Endpoint> {
    start_in(&crate::paths::data_dir(), on_event)
}

pub fn start_in(
    dir: &Path,
    on_event: impl Fn(HookEvent) + Send + 'static,
) -> std::io::Result<Endpoint> {
    std::fs::create_dir_all(dir)?;
    let server = Server::http("127.0.0.1:0").map_err(std::io::Error::other)?;
    let port = server
        .server_addr()
        .to_ip()
        .map(|a| a.port())
        .ok_or_else(|| std::io::Error::other("no ip address"))?;
    let token = new_token()?;

    let json = serde_json::json!({ "port": port, "token": token, "pid": std::process::id() }).to_string();
    let tmp = dir.join("endpoint.json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, dir.join("endpoint.json"))?;

    let expected = format!("Bearer {token}");
    std::thread::spawn(move || {
        for req in server.incoming_requests() {
            handle(req, &expected, &on_event);
        }
    });
    Ok(Endpoint { port })
}

fn new_token() -> std::io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(std::io::Error::other)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Compares without returning at the first differing byte.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = usize::from(a.len() != b.len());
    for i in 0..a.len().max(b.len()) {
        diff |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
    }
    diff == 0
}

fn respond(req: Request, code: u16) {
    let _ = req.respond(Response::empty(code));
}

fn handle(mut req: Request, expected: &str, on_event: &dyn Fn(HookEvent)) {
    if req.url() != "/event" {
        return respond(req, 404);
    }
    if *req.method() != Method::Post {
        return respond(req, 405);
    }
    let auth = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .map(|h| h.value.as_str().to_owned())
        .unwrap_or_default();
    if !ct_eq(auth.as_bytes(), expected.as_bytes()) {
        return respond(req, 401);
    }
    if req.body_length().is_some_and(|n| n as u64 > MAX_BODY) {
        return respond(req, 413);
    }
    let mut body = Vec::new();
    if req
        .as_reader()
        .take(MAX_BODY + 1)
        .read_to_end(&mut body)
        .is_err()
    {
        return respond(req, 400);
    }
    if body.len() as u64 > MAX_BODY {
        return respond(req, 413);
    }
    match serde_json::from_slice::<HookEvent>(&body) {
        Ok(ev) => {
            on_event(ev);
            respond(req, 204);
        }
        Err(_) => respond(req, 400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpStream;
    use std::sync::mpsc;
    use std::time::Duration;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("da-listener-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// Returns the response status code.
    fn request(port: u16, method: &str, path: &str, token: Option<&str>, body: &[u8]) -> u16 {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut head = format!(
            "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        if let Some(t) = token {
            head.push_str(&format!("Authorization: Bearer {t}\r\n"));
        }
        head.push_str("\r\n");
        // The server may answer before reading a large body; ignore write errors.
        let _ = s.write_all(head.as_bytes());
        let _ = s.write_all(body);
        let mut out = String::new();
        let mut buf = [0u8; 256];
        while let Ok(n) = s.read(&mut buf) {
            if n == 0 {
                break;
            }
            out.push_str(&String::from_utf8_lossy(&buf[..n]));
            if out.contains("\r\n") {
                break;
            }
        }
        out.split_whitespace().nth(1).unwrap().parse().unwrap()
    }

    /// What a client learns from endpoint.json.
    struct Ep {
        port: u16,
        token: String,
    }

    fn setup(tag: &str) -> (Ep, mpsc::Receiver<HookEvent>, std::path::PathBuf) {
        let dir = temp_dir(tag);
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let ep = start_in(&dir, move |e| {
            tx.lock().unwrap().send(e).unwrap();
        })
        .unwrap();
        let file: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("endpoint.json")).unwrap()).unwrap();
        let token = file["token"].as_str().unwrap().to_string();
        assert_eq!(file["port"], ep.port);
        (Ep { port: ep.port, token }, rx, dir)
    }

    const EV: &[u8] = br#"{"name":"Stop","session_id":"s1","cwd":"C:/x"}"#;

    #[test]
    fn valid_token_returns_204_and_calls_once() {
        let (ep, rx, _) = setup("ok");
        assert_eq!(request(ep.port, "POST", "/event", Some(&ep.token), EV), 204);
        let e = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(e.name, "Stop");
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn wrong_or_missing_token_returns_401_without_call() {
        let (ep, rx, _) = setup("auth");
        assert_eq!(request(ep.port, "POST", "/event", Some("nope"), EV), 401);
        assert_eq!(request(ep.port, "POST", "/event", None, EV), 401);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn malformed_json_returns_400() {
        let (ep, rx, _) = setup("bad");
        assert_eq!(request(ep.port, "POST", "/event", Some(&ep.token), b"{nope"), 400);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn get_returns_405() {
        let (ep, _rx, _) = setup("get");
        assert_eq!(request(ep.port, "GET", "/event", Some(&ep.token), b""), 405);
    }

    #[test]
    fn oversized_body_returns_413_without_call() {
        let (ep, rx, _) = setup("big");
        let body = vec![b' '; 1024 * 1024 + 1];
        assert_eq!(request(ep.port, "POST", "/event", Some(&ep.token), &body), 413);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn writes_endpoint_json_and_creates_dir() {
        let (ep, _rx, dir) = setup("file");
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("endpoint.json")).unwrap())
                .unwrap();
        assert_eq!(v["port"], ep.port);
        assert_eq!(v["token"], ep.token);
        assert_eq!(v["pid"], std::process::id());
        assert_eq!(ep.token.len(), 64);
        assert!(!dir.join("endpoint.json.tmp").exists());
    }

    #[test]
    fn ct_eq_works() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"abcd"));
        // Lengths differing by exactly 256 with a zero-padded tail must still differ.
        let long = vec![0u8; 256 + 3];
        assert!(!ct_eq(&long[..3], &long));
    }
}
