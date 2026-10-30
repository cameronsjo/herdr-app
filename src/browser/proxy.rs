//! The agents' loopback websocket (plan Task 5): `ws://127.0.0.1:<port>/<token>`
//! for one browser, passed through `agent::Link`'s allowlist to that page's
//! in-process DevTools session (runtime.rs: `SendDevToolsMessage` and an
//! observer). No CEF remote-debugging port exists; this is the only way in.
//!
//! Threads: one accepts, one serves each connection. CEF is called only on
//! the main thread: messages for a page queue here and the CEF pump
//! (runtime.rs) drains them. Tokens and ports are never logged.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{ErrorKind, Write as _};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, mpsc};
use std::time::{Duration, Instant};

use serde_json::Value;
use tungstenite::Message;
use tungstenite::protocol::WebSocketConfig;

use super::agent::{self, FromAgent, Link, Tokens, Upgrade};
use super::runtime;

/// Messages from a page waiting for an agent that isn't reading: past
/// either limit its connection ends, so an agent can't grow the app's memory.
const MAX_QUEUED: usize = 1024;
const MAX_QUEUED_BYTES: usize = 32 << 20;
/// A write the agent doesn't take within this ends its connection.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

struct Conn {
    id: u64,
    link: Link,
    /// Text for the agent; the connection's thread writes it.
    out: mpsc::SyncSender<String>,
    /// Bytes in `out` not yet written.
    queued: Arc<AtomicUsize>,
    /// The socket: shutting it ends the connection even while its thread
    /// is stuck writing to an agent that stopped reading.
    socket: TcpStream,
}

impl Conn {
    /// Queues `text` for the agent. False: the queue is past its limit.
    fn push(&self, text: String) -> bool {
        let bytes = text.len();
        if self.queued.fetch_add(bytes, Ordering::SeqCst) + bytes > MAX_QUEUED_BYTES {
            return false;
        }
        self.out.try_send(text).is_ok()
    }
}

enum Job {
    /// An allowed message for the page, under the proxy's id, from connection `conn`.
    ToPage { browser: String, conn: u64, id: i64, message: String },
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
    /// Connections whose thread still runs.
    serving: HashSet<u64>,
}

static HUB: OnceLock<Mutex<Hub>> = OnceLock::new();

impl Hub {
    /// Ends `browser`'s connection from this side: its socket shuts and
    /// the page's reset is queued now, so neither waits for the agent to
    /// read. True when there was one. The caller wakes the pump.
    fn end_conn(&mut self, browser: &str) -> bool {
        let Some(c) = self.conns.remove(browser) else { return false };
        let _ = c.socket.shutdown(Shutdown::Both);
        self.jobs.push_back(Job::Gone { browser: browser.to_owned() });
        true
    }

    /// The agent's token and connection end (the connection's end ends the
    /// token too: agent-browser asks the provider for a new one).
    fn end_agent(&mut self, browser: &str) -> bool {
        let token = self.tokens.revoke(browser);
        self.end_conn(browser) || token
    }
}

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
    if h.end_conn(browser) {
        drop(h);
        runtime::wake();
        tracing::info!("browser: agent session for {browser} replaced by a new attach");
    }
    Ok(format!("ws://127.0.0.1:{port}/{token}"))
}

/// Ends `browser`'s token and connection (access off, browser close, quit,
/// CEF's agent host gone). True when something was live.
pub fn revoke(browser: &str) -> bool {
    let live = hub().end_agent(browser);
    runtime::wake();
    live
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
    let Some(c) = h.conns.get_mut(browser) else { return };
    let Some(text) = c.link.from_page(bytes) else { return };
    if !c.push(text) {
        h.end_agent(browser);
        drop(h);
        runtime::wake();
        tracing::warn!("browser: the agent for {browser} stopped reading; its connection and token ended");
    }
}

/// Runs queued work for pages; the CEF pump calls it on the main thread.
pub fn drain() {
    loop {
        let Some(job) = hub().jobs.pop_front() else { return };
        match job {
            Job::ToPage { browser, conn, id, message } => {
                // A message from a connection that has ended since never reaches the page.
                if hub().conns.get(&browser).map(|c| c.id) != Some(conn) {
                    continue;
                }
                if !runtime::send_devtools(&browser, message.as_bytes()) {
                    let mut h = hub();
                    if let Some(c) = h.conns.get_mut(&browser).filter(|c| c.id == conn)
                        && let Some(reply) = c.link.fail(id, "herdr: the page isn't there right now (opening or reopening): try again")
                        && !c.push(reply)
                    {
                        h.end_agent(&browser);
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
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let Some(head) = peek_head(&stream) else { return };
    let Ok(socket) = stream.try_clone() else { return };
    let (browser, id, rx, queued) = {
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
        let (tx, rx) = mpsc::sync_channel(MAX_QUEUED);
        let queued = Arc::new(AtomicUsize::new(0));
        h.conns.insert(browser.clone(), Conn { id, link: Link::default(), out: tx, queued: queued.clone(), socket });
        h.serving.insert(id);
        (browser, id, rx, queued)
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
            h.serving.remove(&id);
            return;
        }
    };
    let _ = ws.get_mut().set_read_timeout(Some(Duration::from_millis(15)));
    tracing::info!("browser: an agent connected to {browser}");
    'conn: loop {
        loop {
            match rx.try_recv() {
                Ok(t) => {
                    queued.fetch_sub(t.len(), Ordering::SeqCst);
                    // A write the agent doesn't take in WRITE_TIMEOUT fails here too.
                    if ws.send(Message::text(t)).is_err() {
                        break 'conn;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                // The hub dropped this connection (revoke, replace, a full queue).
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
    // The agent's own end (it closed, or stopped reading): its token ends
    // too and the page resets. Any other end (revoke, replace, a full
    // queue) did both already, on the side that ended it.
    let own = {
        let mut h = hub();
        h.serving.remove(&id);
        h.conns.get(&browser).is_some_and(|c| c.id == id) && h.end_agent(&browser)
    };
    if own {
        runtime::wake();
    }
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
            h.jobs.push_back(Job::ToPage { browser: browser.to_owned(), conn, id, message });
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

#[cfg(test)]
mod tests {
    //! Real sockets against the real hub (no CEF: jobs only queue, nothing
    //! drains them here). Each test uses its own browser id.
    use super::*;
    use tungstenite::WebSocket;
    use tungstenite::handshake::HandshakeError;

    fn connect(url: &str) -> Result<WebSocket<TcpStream>, u16> {
        let addr = url.trim_start_matches("ws://").split('/').next().unwrap();
        let stream = TcpStream::connect(addr).unwrap();
        match tungstenite::client::client(url, stream) {
            Ok((ws, _)) => Ok(ws),
            Err(HandshakeError::Failure(tungstenite::Error::Http(r))) => Err(r.status().as_u16()),
            Err(e) => panic!("handshake: {e}"),
        }
    }

    fn wait(limit: Duration, done: impl Fn() -> bool) -> bool {
        let end = Instant::now() + limit;
        while Instant::now() < end {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        done()
    }

    fn connected(b: &str) -> bool {
        hub().conns.contains_key(b)
    }

    fn resets(b: &str) -> usize {
        hub().jobs.iter().filter(|j| matches!(j, Job::Gone { browser } if browser == b)).count()
    }

    /// An event as big as `kb` KiB (the page's events reach the agent unasked).
    fn event(kb: usize) -> Vec<u8> {
        format!(r#"{{"method":"Page.frameNavigated","params":{{"x":"{}"}}}}"#, "a".repeat(kb << 10)).into_bytes()
    }

    /// Fills the socket until the connection's thread is stuck writing.
    fn stall(b: &str) {
        for _ in 0..24 {
            from_page(b, &event(512));
        }
        std::thread::sleep(Duration::from_millis(300));
    }

    #[test]
    fn an_agents_own_disconnect_ends_its_token_and_resets_the_page() {
        let url = attach("t-disc").unwrap();
        let ws = connect(&url).unwrap();
        assert!(connected("t-disc"));
        drop(ws);
        assert!(wait(Duration::from_secs(3), || !connected("t-disc")));
        assert_eq!(connect(&url).err(), Some(403), "the same token works again after its connection ended");
        assert_eq!(resets("t-disc"), 1);
    }

    #[test]
    fn an_agent_that_stops_reading_is_cut_off_after_the_write_timeout() {
        let url = attach("t-stall").unwrap();
        let _ws = connect(&url).unwrap();
        stall("t-stall");
        assert!(connected("t-stall"));
        // Each write that moves nothing for WRITE_TIMEOUT fails; a partial one
        // can take one more round.
        assert!(wait(WRITE_TIMEOUT * 4, || !connected("t-stall")), "a blocked write never ends");
        assert_eq!(resets("t-stall"), 1);
        assert_eq!(connect(&url).err(), Some(403));
    }

    #[test]
    fn a_revoke_ends_a_stuck_connection_and_resets_the_page_at_once() {
        let url = attach("t-revoke").unwrap();
        let _ws = connect(&url).unwrap();
        stall("t-revoke");
        let id = hub().conns.get("t-revoke").unwrap().id;
        assert!(revoke("t-revoke"));
        // The reset is queued by the revoke, not by the thread's exit.
        assert_eq!(resets("t-revoke"), 1);
        assert!(wait(Duration::from_secs(1), || !hub().serving.contains(&id)), "the connection's thread is still stuck writing");
        assert_eq!(resets("t-revoke"), 1, "one reset per connection");
        assert_eq!(connect(&url).err(), Some(403));
    }

    #[test]
    fn a_page_flood_past_the_queue_limit_ends_the_connection() {
        let url = attach("t-flood").unwrap();
        let _ws = connect(&url).unwrap();
        stall("t-flood");
        for _ in 0..=MAX_QUEUED {
            from_page("t-flood", br#"{"method":"Page.loadEventFired","params":{}}"#);
        }
        assert!(!connected("t-flood"), "the queue grew past its limit");
        assert_eq!(resets("t-flood"), 1);
        assert_eq!(connect(&url).err(), Some(403));
    }
}
