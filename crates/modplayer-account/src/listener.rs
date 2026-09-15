// SPDX-License-Identifier: MIT OR Apache-2.0

//! The loopback `TcpListener` thread, request-line parser, HTML response
//! page, and callback routing table (contracts/authorization-service.md
//! "Loopback callback HTTP contract"; research R8). `en-US` is the only
//! locale this product ships (FR-023), so the two response pages are plain
//! constants here rather than routed through Fluent — `modplayer-account`
//! deliberately does not depend on `modplayer-core` (plan.md "Structure
//! Decision"), and with a single locale a Fluent round-trip would add
//! nothing a user could ever observe.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use time::OffsetDateTime;

use crate::clock::Clock;

/// How often the accept-poll loop checks the cancel flag and the deadline
/// while no connection is pending (research R8).
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Longest a single request line may be before the connection is refused
/// (contracts/authorization-service.md: "Only the request line (first
/// line, ≤ 8 KiB) is parsed").
const MAX_REQUEST_LINE_BYTES: usize = 8 * 1024;

const SUCCESS_PAGE: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>ModPlayer</title></head><body>You can close this tab and return to ModPlayer.</body></html>";
const ERROR_PAGE: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>ModPlayer</title></head><body>Sign-in was not completed. You can close this tab and return to ModPlayer.</body></html>";

/// What the listener thread resolved to (contracts/authorization-
/// service.md "PKCE flow" step 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenerOutcome {
    /// `GET {redirect_path}?code=…&state=<match>`.
    Code(String),
    /// `GET {redirect_path}?error=<e>&state=<match>`.
    Error(String),
    /// The cancel flag was set (`cancel_sign_in()`, sign-out, or
    /// `AccountService` drop).
    Cancelled,
    /// The 5-minute browser-wait budget expired with no matching callback.
    TimedOut,
    /// `accept()` failed for a reason other than "would block" (the bound
    /// port was lost — e.g. another process took it after a resume).
    BindLost,
}

/// What one attempt's listener needs to know (contracts/authorization-
/// service.md).
#[derive(Debug, Clone)]
pub struct ListenerConfig {
    pub redirect_path: &'static str,
    /// Must exactly match the callback's `state` query parameter
    /// (data-model.md §1.5; equality need not be constant-time — it is a
    /// public nonce, not a secret).
    pub expected_state: String,
    /// The attempt expires at this instant (`started_at + 5 min`).
    pub deadline: OffsetDateTime,
}

/// Bind a fresh ephemeral loopback port for a new attempt
/// (contracts/authorization-service.md "PKCE flow" step 2).
pub fn bind_ephemeral() -> std::io::Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", 0))
}

/// Re-bind the specific port recorded for a resumed attempt (research R2:
/// "attempt to complete" on relaunch = re-bind the recorded port).
pub fn bind_port(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", port))
}

/// Spawn the listener thread (contracts/authorization-service.md "PKCE
/// flow" steps 2-4; contracts/account-session.md "Threading model" —
/// thread name `account-listener`). Detached; the single resolved outcome
/// is sent once on the returned channel. The thread observes `cancel` and
/// exits within `POLL_INTERVAL` of it being set (contracts/account-
/// session.md: "exits within 50 ms of cancel").
pub fn spawn(
    listener: TcpListener,
    config: ListenerConfig,
    cancel: Arc<AtomicBool>,
    clock: Arc<dyn Clock>,
) -> mpsc::Receiver<ListenerOutcome> {
    let (tx, rx) = mpsc::channel();
    let Ok(()) = listener.set_nonblocking(true) else {
        let _ = tx.send(ListenerOutcome::BindLost);
        return rx;
    };
    let builder = thread::Builder::new().name("account-listener".to_string());
    // Spawning a plain OS thread failing at all is exceptionally rare
    // (resource exhaustion); if it does, `tx` is dropped unsent along with
    // the closure, and the caller sees a disconnected channel — which it
    // must already treat the same as "nothing will ever arrive" while
    // waiting for a `ListenerOutcome`.
    let _ = builder.spawn(move || {
        let outcome = run(&listener, &config, cancel.as_ref(), clock.as_ref());
        let _ = tx.send(outcome);
    });
    rx
}

/// The accept-poll loop (research R8): non-blocking `accept()`, polling
/// every [`POLL_INTERVAL`] while checking `cancel` and `config.deadline`.
/// A connection whose request does not "consume" the attempt (wrong path,
/// state mismatch, non-`GET`, unparsable) is answered and the loop keeps
/// waiting.
fn run(
    listener: &TcpListener,
    config: &ListenerConfig,
    cancel: &AtomicBool,
    clock: &dyn Clock,
) -> ListenerOutcome {
    loop {
        if cancel.load(Ordering::SeqCst) {
            return ListenerOutcome::Cancelled;
        }
        if clock.now() >= config.deadline {
            return ListenerOutcome::TimedOut;
        }
        match listener.accept() {
            Ok((stream, _addr)) => {
                if let Some(outcome) = handle_connection(stream, config) {
                    return outcome;
                }
                // Stray/mismatched request: already answered, keep waiting.
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                thread::sleep(POLL_INTERVAL);
            }
            Err(_) => return ListenerOutcome::BindLost,
        }
    }
}

/// One parsed HTTP request line (headers and body are ignored per the
/// contract).
struct ParsedRequest {
    method: String,
    path: String,
    query: Vec<(String, String)>,
}

impl ParsedRequest {
    fn query_value(&self, key: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Handle one accepted connection: parse, route, respond, and report
/// `Some(outcome)` only when the request consumes the attempt (a matching
/// `code`/`error` callback); `None` for every other case, having already
/// sent the appropriate response (contracts/authorization-service.md
/// "Loopback callback HTTP contract").
fn handle_connection(mut stream: TcpStream, config: &ListenerConfig) -> Option<ListenerOutcome> {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));

    let Some(request_line) = read_request_line(&mut stream) else {
        respond_plain(&mut stream, 400, "Bad Request");
        return None;
    };
    let Some(parsed) = parse_request_line(&request_line) else {
        respond_plain(&mut stream, 400, "Bad Request");
        return None;
    };
    if parsed.method != "GET" {
        respond_plain(&mut stream, 400, "Bad Request");
        return None;
    }
    if parsed.path != config.redirect_path {
        respond_plain(&mut stream, 404, "Not Found");
        return None;
    }

    let state_matches = parsed
        .query_value("state")
        .is_some_and(|state| state == config.expected_state);
    if !state_matches {
        // "state mismatch or missing state: 400, listener keeps waiting"
        // (contracts/authorization-service.md).
        respond_plain(&mut stream, 400, "Bad Request");
        return None;
    }

    if let Some(code) = parsed.query_value("code") {
        let code = code.to_string();
        respond_html(&mut stream, SUCCESS_PAGE);
        return Some(ListenerOutcome::Code(code));
    }
    if let Some(error) = parsed.query_value("error") {
        let error = error.to_string();
        respond_html(&mut stream, ERROR_PAGE);
        return Some(ListenerOutcome::Error(error));
    }

    // Matching state but neither `code` nor `error`: malformed, treated as
    // a stray request that must not consume the attempt.
    respond_plain(&mut stream, 400, "Bad Request");
    None
}

/// Read only the first line of the request (≤ [`MAX_REQUEST_LINE_BYTES`]),
/// ignoring headers and body.
fn read_request_line(stream: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => {
                if byte[0] == b'\n' {
                    break;
                }
                buf.push(byte[0]);
                if buf.len() > MAX_REQUEST_LINE_BYTES {
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
    String::from_utf8(buf).ok()
}

/// Parse `"METHOD /path?query HTTP/1.1"` (trailing `\r` already stripped by
/// the caller's line reader treating `\n` as the terminator, but a `\r`
/// left on the target/method is trimmed defensively).
fn parse_request_line(line: &str) -> Option<ParsedRequest> {
    let line = line.trim_end_matches(['\r', '\n']);
    let mut parts = line.splitn(3, ' ');
    let method = parts.next()?.trim().to_string();
    let target = parts.next()?.trim();
    let _version = parts.next()?;
    if method.is_empty() || target.is_empty() {
        return None;
    }
    let (path, query_str) = target.split_once('?').unwrap_or((target, ""));
    Some(ParsedRequest {
        method,
        path: path.to_string(),
        query: parse_query(query_str),
    })
}

/// Parse `a=b&c=d` into ordered key/value pairs, percent-decoding each.
fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (percent_decode(key), percent_decode(value))
        })
        .collect()
}

/// Minimal percent-decoding (`%XX` -> byte); `+` is left literal (this is a
/// URL query string, not a form body). Malformed escapes are left as-is
/// rather than rejecting the whole request.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            let decoded = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok());
            match decoded {
                Some(byte) => {
                    out.push(byte);
                    i += 3;
                    continue;
                }
                None => {
                    out.push(bytes[i]);
                    i += 1;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| value.to_string())
}

/// `Content-Length` + `Connection: close`, always (contracts/authorization-
/// service.md).
fn respond_plain(stream: &mut TcpStream, status: u16, reason: &str) {
    let body = format!("{status} {reason}");
    write_response(stream, status, reason, "text/plain; charset=utf-8", &body);
}

fn respond_html(stream: &mut TcpStream, body: &str) {
    write_response(stream, 200, "OK", "text/html; charset=utf-8", body);
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &str,
) {
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use crate::clock::FakeClock;
    use std::io::BufRead;
    use std::io::BufReader;
    use std::net::SocketAddr;

    fn config(state: &str, deadline: OffsetDateTime) -> ListenerConfig {
        ListenerConfig {
            redirect_path: "/login",
            expected_state: state.to_string(),
            deadline,
        }
    }

    fn far_future() -> OffsetDateTime {
        OffsetDateTime::now_utc() + time::Duration::hours(1)
    }

    fn connect(addr: SocketAddr) -> TcpStream {
        TcpStream::connect(addr).expect("connect to loopback listener")
    }

    fn send_request(stream: &mut TcpStream, request_line: &str) -> (u16, String) {
        stream
            .write_all(format!("{request_line}\r\n\r\n").as_bytes())
            .expect("write request");
        let mut reader = BufReader::new(stream);
        let mut status_line = String::new();
        reader
            .read_line(&mut status_line)
            .expect("read status line");
        let status: u16 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .expect("parse status code");
        let mut rest = String::new();
        use std::io::Read as _;
        let _ = reader.read_to_string(&mut rest);
        (status, rest)
    }

    #[test]
    fn parses_a_well_formed_request_line() {
        let parsed =
            parse_request_line("GET /login?code=abc&state=xyz HTTP/1.1").expect("should parse");
        assert_eq!(parsed.method, "GET");
        assert_eq!(parsed.path, "/login");
        assert_eq!(parsed.query_value("code"), Some("abc"));
        assert_eq!(parsed.query_value("state"), Some("xyz"));
    }

    #[test]
    fn rejects_an_unparsable_request_line() {
        // Fewer than the three space-separated parts a request line needs.
        assert!(parse_request_line("GET /login").is_none());
        assert!(parse_request_line("").is_none());
    }

    #[test]
    fn percent_decode_handles_encoded_bytes() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("plain"), "plain");
        assert_eq!(percent_decode("bad%"), "bad%");
    }

    /// contracts/authorization-service.md callback table: a request whose
    /// `state` does not match (or is missing) gets `400` and must **not**
    /// consume the attempt — the listener keeps waiting for the real
    /// callback (T050).
    #[test]
    fn stray_callback_requests_do_not_consume_attempt() {
        let listener = bind_ephemeral().expect("bind ephemeral loopback port");
        let addr = listener.local_addr().expect("local addr");
        let cancel = Arc::new(AtomicBool::new(false));
        let clock: Arc<dyn Clock> = Arc::new(FakeClock::new(OffsetDateTime::now_utc()));
        let rx = spawn(
            listener,
            config("expected-state", far_future()),
            cancel,
            clock,
        );

        // A mismatched `state` — refused, attempt still alive.
        let mut stray = connect(addr);
        let (status, _) = send_request(&mut stray, "GET /login?code=abc&state=WRONG HTTP/1.1");
        assert_eq!(status, 400);
        drop(stray);

        // A stray path (e.g. a browser favicon probe) — refused, attempt
        // still alive.
        let mut favicon = connect(addr);
        let (status, _) = send_request(&mut favicon, "GET /favicon.ico HTTP/1.1");
        assert_eq!(status, 404);
        drop(favicon);

        // The real callback, sent last, is the one that resolves the
        // attempt — proving neither stray request above consumed it.
        let mut real = connect(addr);
        let (status, _) = send_request(
            &mut real,
            "GET /login?code=real-code&state=expected-state HTTP/1.1",
        );
        assert_eq!(status, 200);
        drop(real);

        let outcome = rx.recv_timeout(Duration::from_secs(5)).expect("outcome");
        assert_eq!(outcome, ListenerOutcome::Code("real-code".to_string()));
    }

    #[test]
    fn error_callback_with_matching_state_resolves_as_error() {
        let listener = bind_ephemeral().expect("bind ephemeral loopback port");
        let addr = listener.local_addr().expect("local addr");
        let cancel = Arc::new(AtomicBool::new(false));
        let clock: Arc<dyn Clock> = Arc::new(FakeClock::new(OffsetDateTime::now_utc()));
        let rx = spawn(listener, config("s", far_future()), cancel, clock);

        let mut stream = connect(addr);
        let (status, _) = send_request(
            &mut stream,
            "GET /login?error=access_denied&state=s HTTP/1.1",
        );
        assert_eq!(status, 200);

        let outcome = rx.recv_timeout(Duration::from_secs(5)).expect("outcome");
        assert_eq!(outcome, ListenerOutcome::Error("access_denied".to_string()));
    }

    #[test]
    fn cancel_flag_stops_the_listener() {
        let listener = bind_ephemeral().expect("bind ephemeral loopback port");
        let cancel = Arc::new(AtomicBool::new(false));
        let clock: Arc<dyn Clock> = Arc::new(FakeClock::new(OffsetDateTime::now_utc()));
        let rx = spawn(
            listener,
            config("s", far_future()),
            Arc::clone(&cancel),
            clock,
        );

        cancel.store(true, Ordering::SeqCst);
        let outcome = rx.recv_timeout(Duration::from_secs(2)).expect("outcome");
        assert_eq!(outcome, ListenerOutcome::Cancelled);
    }

    #[test]
    fn deadline_in_the_past_times_out_immediately() {
        let listener = bind_ephemeral().expect("bind ephemeral loopback port");
        let cancel = Arc::new(AtomicBool::new(false));
        let past = OffsetDateTime::now_utc() - time::Duration::minutes(10);
        let clock: Arc<dyn Clock> = Arc::new(FakeClock::new(OffsetDateTime::now_utc()));
        let rx = spawn(listener, config("s", past), cancel, clock);

        let outcome = rx.recv_timeout(Duration::from_secs(2)).expect("outcome");
        assert_eq!(outcome, ListenerOutcome::TimedOut);
    }

    #[test]
    fn advancing_a_fake_clock_past_the_deadline_times_out() {
        let listener = bind_ephemeral().expect("bind ephemeral loopback port");
        let cancel = Arc::new(AtomicBool::new(false));
        let start = OffsetDateTime::now_utc();
        let fake = Arc::new(FakeClock::new(start));
        let clock: Arc<dyn Clock> = fake.clone();
        let deadline = start + time::Duration::minutes(5);
        let rx = spawn(listener, config("s", deadline), cancel, clock);

        // Not yet timed out.
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());

        fake.advance(time::Duration::minutes(6));
        let outcome = rx.recv_timeout(Duration::from_secs(2)).expect("outcome");
        assert_eq!(outcome, ListenerOutcome::TimedOut);
    }
}
