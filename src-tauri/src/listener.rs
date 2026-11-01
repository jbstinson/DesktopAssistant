//! Localhost listener receiving hook events from da-hook.

use crate::events::{HookEvent, PermissionAsk, Polled};
use std::io::Read;
use std::path::Path;
use serde::de::DeserializeOwned;
use tiny_http::{Header, Method, Request, Response, Server};

const MAX_BODY: u64 = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub port: u16,
}

/// What each route does. All three run on the one listener thread, so each must return fast
/// and none may wait for a decision.
pub struct Routes {
    pub event: Box<dyn Fn(HookEvent) + Send>,
    /// Returns the request id and whether the request is being held.
    pub open: Box<dyn Fn(PermissionAsk) -> (String, bool) + Send>,
    pub poll: Box<dyn Fn(&str) -> Polled + Send>,
}

pub fn start(routes: Routes) -> std::io::Result<Endpoint> {
    start_in(&crate::paths::data_dir(), routes)
}

pub fn start_in(dir: &Path, routes: Routes) -> std::io::Result<Endpoint> {
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
            handle(req, &expected, &routes);
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

fn respond_json(req: Request, code: u16, body: String) {
    let header = Header::from_bytes("Content-Type", "application/json")
        .expect("a fixed header is valid");
    let _ = req.respond(Response::from_string(body).with_status_code(code).with_header(header));
}

/// Reads a JSON body of at most `MAX_BODY` bytes, or gives the status to refuse it with.
fn read_json<T: DeserializeOwned>(req: &mut Request) -> Result<T, u16> {
    if req.body_length().is_some_and(|n| n as u64 > MAX_BODY) {
        return Err(413);
    }
    let mut body = Vec::new();
    if req
        .as_reader()
        .take(MAX_BODY + 1)
        .read_to_end(&mut body)
        .is_err()
    {
        return Err(400);
    }
    if body.len() as u64 > MAX_BODY {
        return Err(413);
    }
    serde_json::from_slice(&body).map_err(|_| 400)
}

enum Route {
    Event,
    Open,
    Poll(String),
}

fn handle(mut req: Request, expected: &str, routes: &Routes) {
    let (route, method) = match req.url() {
        "/event" => (Route::Event, Method::Post),
        "/permission" => (Route::Open, Method::Post),
        url => match url.strip_prefix("/permission/") {
            Some(id) => (Route::Poll(id.to_string()), Method::Get),
            None => return respond(req, 404),
        },
    };
    if *req.method() != method {
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
    match route {
        Route::Event => match read_json::<HookEvent>(&mut req) {
            Ok(ev) => {
                (routes.event)(ev);
                respond(req, 204);
            }
            Err(code) => respond(req, code),
        },
        Route::Open => match read_json::<PermissionAsk>(&mut req) {
            Ok(ask) => {
                let (id, hold) = (routes.open)(ask);
                respond_json(req, 200, serde_json::json!({ "id": id, "hold": hold }).to_string());
            }
            Err(code) => respond(req, code),
        },
        Route::Poll(id) => match (routes.poll)(&id) {
            Polled::Waiting => respond(req, 204),
            Polled::Reply(reply) => respond_json(req, 200, reply),
            Polled::Gone => respond(req, 410),
        },
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

    /// Status and body of a request that gets its whole answer read.
    fn fetch(port: u16, method: &str, path: &str, token: Option<&str>, body: &[u8]) -> (u16, String) {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let auth = token.map_or(String::new(), |t| format!("Authorization: Bearer {t}\r\n"));
        let head = format!(
            "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n{auth}\r\n",
            body.len()
        );
        s.write_all(head.as_bytes()).unwrap();
        s.write_all(body).unwrap();
        let mut out = String::new();
        let _ = s.read_to_string(&mut out);
        let code = out.split_whitespace().nth(1).unwrap().parse().unwrap();
        let text = out.split_once("\r\n\r\n").map_or("", |(_, b)| b).to_string();
        (code, text)
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
        let routes = Routes {
            event: Box::new(move |e| tx.lock().unwrap().send(e).unwrap()),
            open: Box::new(|_| (String::new(), false)),
            poll: Box::new(|_| Polled::Gone),
        };
        let ep = start_in(&dir, routes).unwrap();
        let file: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("endpoint.json")).unwrap()).unwrap();
        let token = file["token"].as_str().unwrap().to_string();
        assert_eq!(file["port"], ep.port);
        (Ep { port: ep.port, token }, rx, dir)
    }

    const EV: &[u8] = br#"{"name":"Stop","session_id":"s1","cwd":"C:/x"}"#;

    /// A listener whose poll route gives `polled`, and which reports every ask and poll id.
    fn setup_permission(
        tag: &str,
        polled: Polled,
    ) -> (Ep, mpsc::Receiver<PermissionAsk>, mpsc::Receiver<String>) {
        let (asks, ask_rx) = mpsc::channel();
        let (polls, poll_rx) = mpsc::channel();
        let (asks, polls) = (std::sync::Mutex::new(asks), std::sync::Mutex::new(polls));
        let dir = temp_dir(tag);
        let ep = start_in(
            &dir,
            Routes {
                event: Box::new(|_| {}),
                open: Box::new(move |a| {
                    asks.lock().unwrap().send(a).unwrap();
                    ("ab12".to_string(), true)
                }),
                poll: Box::new(move |id| {
                    let _ = polls.lock().unwrap().send(id.to_string());
                    polled.clone()
                }),
            },
        )
        .unwrap();
        let file: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("endpoint.json")).unwrap()).unwrap();
        (Ep { port: ep.port, token: file["token"].as_str().unwrap().to_string() }, ask_rx, poll_rx)
    }

    const ASK: &[u8] = br#"{"session_id":"s1","tool_name":"Bash","tool_input":{"command":"ls"}}"#;

    #[test]
    fn open_returns_id_and_hold_and_passes_the_ask_on() {
        let (ep, asks, _) = setup_permission("open", Polled::Waiting);
        let (code, body) = fetch(ep.port, "POST", "/permission", Some(&ep.token), ASK);
        assert_eq!(code, 200);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&body).unwrap(),
            serde_json::json!({"id": "ab12", "hold": true})
        );
        let a = asks.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(a.tool_input["command"], "ls");
        assert!(asks.try_recv().is_err());
    }

    #[test]
    fn poll_maps_to_204_200_and_410() {
        let (ep, _, polls) = setup_permission("wait", Polled::Waiting);
        let got = fetch(ep.port, "GET", "/permission/ab12", Some(&ep.token), b"");
        assert_eq!(got, (204, String::new()));
        assert_eq!(polls.recv_timeout(Duration::from_secs(2)).unwrap(), "ab12");

        let reply = r#"{"verbatim": [1,  2]}"#;
        let (ep, _, _) = setup_permission("reply", Polled::Reply(reply.to_string()));
        let got = fetch(ep.port, "GET", "/permission/ab12", Some(&ep.token), b"");
        assert_eq!(got, (200, reply.to_string()));

        let (ep, _, _) = setup_permission("gone", Polled::Gone);
        assert_eq!(fetch(ep.port, "GET", "/permission/ab12", Some(&ep.token), b"").0, 410);
    }

    #[test]
    fn permission_routes_need_the_token() {
        let (ep, asks, polls) = setup_permission("perm-auth", Polled::Reply("{}".into()));
        for token in [Some("nope"), None] {
            assert_eq!(request(ep.port, "POST", "/permission", token, ASK), 401);
            assert_eq!(fetch(ep.port, "GET", "/permission/ab12", token, b""), (401, String::new()));
        }
        assert!(asks.try_recv().is_err());
        assert!(polls.try_recv().is_err());
    }

    #[test]
    fn oversized_ask_returns_413_without_call() {
        let (ep, asks, _) = setup_permission("perm-big", Polled::Gone);
        let body = vec![b' '; 1024 * 1024 + 1];
        assert_eq!(request(ep.port, "POST", "/permission", Some(&ep.token), &body), 413);
        assert!(asks.try_recv().is_err());
    }

    #[test]
    fn bad_asks_and_wrong_routes_are_refused() {
        let (ep, asks, polls) = setup_permission("perm-bad", Polled::Gone);
        let t = Some(ep.token.as_str());
        assert_eq!(request(ep.port, "POST", "/permission", t, b"{nope"), 400);
        assert_eq!(request(ep.port, "POST", "/permission", t, br#"{"tool_name":7}"#), 400);
        assert_eq!(request(ep.port, "GET", "/permission", t, b""), 405);
        assert_eq!(request(ep.port, "POST", "/permission/ab12", t, b""), 405);
        assert_eq!(request(ep.port, "GET", "/nope", t, b""), 404);
        assert_eq!(request(ep.port, "GET", "/permissions/ab12", t, b""), 404);
        assert!(asks.try_recv().is_err());
        assert!(polls.try_recv().is_err());
    }

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
