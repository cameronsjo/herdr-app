//! The agents' loopback websocket (plan Task 5): `ws://127.0.0.1:<port>/<token>`
//! for one browser, passed through `agent::Link`'s allowlist to that page's
//! in-process DevTools session (runtime.rs: `SendDevToolsMessage` and an
//! observer). No CEF remote-debugging port exists; this is the only way in.
//!
//! Threads: one accepts, one serves each connection. CEF is called only on
//! the main thread: messages for a page queue here and the CEF pump
//! (runtime.rs) drains them. Tokens and ports are never logged.

use std::collections::{HashMap, VecDeque};
use std::io::{ErrorKind, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::{Mutex, MutexGuard, OnceLock, mpsc};
use std::time::{Duration, Instant};

use serde_json::Value;
use tungstenite::Message;
use tungstenite::protocol::WebSocketConfig;

use super::agent::{self, FromAgent, Link, Tokens, Upgrade};
use super::runtime;

enum Out {
    Text(String),
    Close,
}

struct Conn {
    id: u64,
    link: Link,
    out: mpsc::Sender<Out>,
}

enum Job {
    /// An allowed message for the page, under the proxy's id.
    ToPage { browser: String, id: i64, message: String },
    /// An agent's connection ended: the page reopens, so nothing it set stays.
    Gone { browser: String },
}

#[derive(Default)]
struct Hub {
    port: Option<u16>,
    tokens: Tokens,
    /// One live connection per browser (and so per token).
    conns: HashMap<String, Conn>,
    next_conn: u64,
    /// Request ids sent to pages: one space for every browser.
    next_id: i64,
    /// Each browser's committed main-frame url (the proxy's URL gate).
    urls: HashMap<String, String>,
    jobs: VecDeque<Job>,
}

static HUB: OnceLock<Mutex<Hub>> = OnceLock::new();

fn hub() -> MutexGuard<'static, Hub> {
    HUB.get_or_init(Mutex::default).lock().unwrap_or_else(|e| e.into_inner())
}

/// A new token url for `browser` (its access is on: the window checked).
/// An earlier token for it dies and its connection closes (the new agent
/// takes over; the old one sees its socket close).
pub fn attach(browser: &str) -> Result<String, String> {
    let port = listen()?;
    let mut random = [0u8; 32];
    // SAFETY: arc4random_buf fills exactly `len` bytes of a valid buffer
    // from the OS CSPRNG; it cannot fail.
    unsafe { libc::arc4random_buf(random.as_mut_ptr().cast(), random.len()) };
    let mut h = hub();
    let token = h.tokens.issue(browser, random);
    if let Some(old) = h.conns.remove(browser) {
        let _ = old.out.send(Out::Close);
        tracing::info!("browser: agent session for {browser} replaced by a new attach");
    }
    Ok(format!("ws://127.0.0.1:{port}/{token}"))
}

/// Ends `browser`'s token and connection (access off, browser close, quit,
/// CEF's agent host gone). True when something was live.
pub fn revoke(browser: &str) -> bool {
    let mut h = hub();
    let token = h.tokens.revoke(browser);
    let conn = h.conns.remove(browser).map(|c| c.out.send(Out::Close)).is_some();
    token || conn
}

/// Quit: every token and connection ends. Returns the browsers that had one.
pub fn revoke_all() -> Vec<String> {
    let mut live = hub().tokens.browsers();
    live.extend(hub().conns.keys().cloned());
    live.sort();
    live.dedup();
    live.retain(|b| revoke(b));
    live
}

/// The page's committed main-frame url changed (runtime.rs, `on_address_change`).
pub fn set_url(browser: &str, url: &str) {
    hub().urls.insert(browser.to_owned(), url.to_owned());
}

/// One DevTools message from `browser`'s page (main thread): to its agent,
/// if it has one and the message is its own.
pub fn from_page(browser: &str, bytes: &[u8]) {
    let mut h = hub();
    if let Some(c) = h.conns.get_mut(browser)
        && let Some(text) = c.link.from_page(bytes)
    {
        let _ = c.out.send(Out::Text(text));
    }
}

/// Runs queued work for pages; the CEF pump calls it on the main thread.
pub fn drain() {
    loop {
        let Some(job) = hub().jobs.pop_front() else { return };
        match job {
            Job::ToPage { browser, id, message } => {
                if !runtime::send_devtools(&browser, message.as_bytes()) {
                    let mut h = hub();
                    if let Some(c) = h.conns.get_mut(&browser)
                        && let Some(reply) = c.link.fail(id, "herdr: the page isn't there right now (opening or reopening): try again")
                    {
                        let _ = c.out.send(Out::Text(reply));
                    }
                }
            }
            Job::Gone { browser } => runtime::agent_gone(&browser),
        }
    }
}

/// The listener, started on the first attach: 127.0.0.1 only, a port the OS picks.
fn listen() -> Result<u16, String> {
    if let Some(port) = hub().port {
        return Ok(port);
    }
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| format!("cannot open the agent socket: {e}"))?;
    let port = listener.local_addr().map_err(|e| format!("cannot open the agent socket: {e}"))?.port();
    std::thread::Builder::new()
        .name("herdr-agent-proxy".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = std::thread::Builder::new().name("herdr-agent-conn".into()).spawn(move || serve(stream, port));
            }
        })
        .map_err(|e| format!("cannot start the agent socket: {e}"))?;
    hub().port = Some(port);
    tracing::info!("browser: agent proxy listening on loopback");
    Ok(port)
}

/// The request line and headers, read without consuming them (tungstenite
/// reads them again for the handshake). None: not HTTP, too big, too slow.
fn peek_head(stream: &TcpStream) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut buf = vec![0u8; 8192];
    loop {
        match stream.peek(&mut buf) {
            Ok(0) => return None,
            Ok(n) => {
                if let Some(end) = buf[..n].windows(4).position(|w| w == b"\r\n\r\n") {
                    return String::from_utf8(buf[..end].to_vec()).ok();
                }
                if n == buf.len() {
                    return None;
                }
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => return None,
        }
        if Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// What `agent::check_upgrade` needs from a request head.
fn upgrade(head: &str) -> Upgrade<'_> {
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or_default().split(' ');
    let (method, path) = (first.next().unwrap_or_default(), first.next().unwrap_or_default());
    let mut header = HashMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            header.entry(k.trim().to_ascii_lowercase()).or_insert(v.trim());
        }
    }
    let has = |k: &str, word: &str| header.get(k).is_some_and(|v| v.to_ascii_lowercase().contains(word));
    Upgrade {
        websocket: method == "GET" && has("upgrade", "websocket") && has("connection", "upgrade"),
        path,
        host: header.get("host").copied(),
        origin: header.get("origin").copied(),
    }
}

/// A bare status: no body, no CORS headers, nothing a page can read.
fn refuse(mut stream: TcpStream, code: u16) {
    let reason = match code {
        403 => "Forbidden",
        409 => "Conflict",
        _ => "Not Found",
    };
    let _ = stream.write_all(format!("HTTP/1.1 {code} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes());
    tracing::debug!("browser: refused an agent connection ({code})");
}

fn serve(stream: TcpStream, port: u16) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
    let Some(head) = peek_head(&stream) else { return };
    let (browser, id, rx) = {
        let mut h = hub();
        let checked = agent::check_upgrade(&upgrade(&head), port, &h.tokens, |b| h.conns.contains_key(b));
        let browser = match checked {
            Ok(b) => b,
            Err(code) => {
                drop(h);
                return refuse(stream, code);
            }
        };
        h.next_conn += 1;
        let id = h.next_conn;
        let (tx, rx) = mpsc::channel();
        h.conns.insert(browser.clone(), Conn { id, link: Link::default(), out: tx });
        (browser, id, rx)
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let config = WebSocketConfig::default().max_message_size(Some(agent::MAX_FRAME)).max_frame_size(Some(agent::MAX_FRAME));
    let mut ws = match tungstenite::accept_with_config(stream, Some(config)) {
        Ok(ws) => ws,
        Err(_) => {
            let mut h = hub();
            if h.conns.get(&browser).is_some_and(|c| c.id == id) {
                h.conns.remove(&browser);
            }
            return;
        }
    };
    let _ = ws.get_mut().set_read_timeout(Some(Duration::from_millis(15)));
    tracing::info!("browser: an agent connected to {browser}");
    'conn: loop {
        loop {
            match rx.try_recv() {
                Ok(Out::Text(t)) => {
                    if ws.send(Message::text(t)).is_err() {
                        break 'conn;
                    }
                }
                Ok(Out::Close) => {
                    let _ = ws.close(None);
                    let _ = ws.flush();
                    break 'conn;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break 'conn,
            }
        }
        let text = match ws.read() {
            Ok(Message::Text(t)) => t.to_string(),
            Ok(Message::Binary(b)) => match String::from_utf8(b.to_vec()) {
                Ok(t) => t,
                Err(_) => continue,
            },
            Ok(Message::Close(_)) => break,
            Ok(_) => continue,
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => continue,
            Err(_) => break,
        };
        if let Some(reply) = from_agent(&browser, id, &text)
            && ws.send(Message::text(reply)).is_err()
        {
            break;
        }
    }
    {
        let mut h = hub();
        if h.conns.get(&browser).is_some_and(|c| c.id == id) {
            h.conns.remove(&browser);
        }
        h.jobs.push_back(Job::Gone { browser: browser.clone() });
    }
    runtime::wake();
    tracing::info!("browser: the agent connection to {browser} ended");
}

/// One agent message: queued for the page, or an answer straight back.
fn from_agent(browser: &str, conn: u64, text: &str) -> Option<String> {
    let method = tracing::enabled!(tracing::Level::DEBUG)
        .then(|| serde_json::from_str::<Value>(text).ok()?.get("method")?.as_str().map(str::to_owned))
        .flatten();
    let mut h = hub();
    let url = h.urls.get(browser).cloned().unwrap_or_default();
    let mut next = h.next_id;
    let c = h.conns.get_mut(browser).filter(|c| c.id == conn)?;
    let r = c.link.from_agent(text, &mut next, &url);
    h.next_id = next;
    match r {
        FromAgent::Forward { id, message } => {
            // Method names only (the recon log): never params, never the token.
            if let Some(m) = &method {
                tracing::debug!("browser: cdp {browser} forward {m}");
            }
            h.jobs.push_back(Job::ToPage { browser: browser.to_owned(), id, message });
            drop(h);
            runtime::wake();
            None
        }
        FromAgent::Reply(reply) => {
            if let Some(m) = &method {
                tracing::debug!("browser: cdp {browser} refused {m}");
            }
            Some(reply)
        }
    }
}
