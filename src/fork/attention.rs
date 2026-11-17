//! FORK(attention): the agents list as an attention queue
//! (cameronsjo/herdr-app#16). Pure rules here; the window's part is
//! `src/window/fork_attention.rs`.
//!
//! - **Why an agent needs you.** herdr-core keeps the program's own 7501
//!   report: `state=error`, and `kind=permission|question|auth` on
//!   `state=blocked`. The app already shows both as blocked; [`reason`] says
//!   which, for the mods (`agents()`'s `reason`).
//! - **The attention label.** A program outside the agent (a classifier that
//!   reads the end of a turn) writes `herdr.attention=<short text>` on a
//!   terminal whose turn ended waiting on the person in prose ("please run
//!   X", a usage limit). While it is set and the agent isn't working, the
//!   app shows the agent as blocked: it sorts with blocked agents, its dot
//!   and its space's roll-up say so, and the needs-you sound and toast fire
//!   once. The core's own status is unchanged, so automation (`hc wait`,
//!   `hc list`) never sees it. The app removes the label when the agent
//!   starts working again.
//! - **Stalls.** With `stall_minutes` in `<app home>/fork.json`, an agent
//!   that reports `working` while neither its screen nor its report changes
//!   for that long is flagged `stalled` until either does. Off by default.

use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;

/// The terminal label an outside classifier sets (docs/fork/attention.md).
pub const LABEL: &str = "herdr.attention";

/// The longest attention text shown; a longer label is cut.
const MAX_TEXT: usize = 40;

/// How often a working agent's screen is read for the stall check, in ms.
pub const READ_EVERY_MS: u64 = 60_000;

/// The attention label's text, if set (trimmed, at most [`MAX_TEXT`] characters).
pub fn attention(rec: &Value) -> Option<String> {
    let text = rec.pointer("/labels").and_then(|l| l.get(LABEL)).and_then(Value::as_str)?.trim();
    if text.is_empty() {
        return None;
    }
    let cut: String = text.chars().filter(|c| !c.is_control()).take(MAX_TEXT).collect();
    Some(if text.chars().count() > MAX_TEXT { format!("{cut}…") } else { cut })
}

/// The word the app shows for a terminal (`rec`: a listed record, `status`
/// already the root report's state): the core's, except that an agent with
/// the attention label that isn't working, blocked or in error is blocked.
pub fn word(rec: &Value) -> Option<String> {
    let w = rec.get("status").and_then(Value::as_str)?;
    if !matches!(w, "working" | "blocked" | "error") && attention(rec).is_some() {
        return Some("blocked".to_owned());
    }
    Some(w.to_owned())
}

/// Why an agent needs the person: `error`, the program's blocked `kind`
/// (`permission`, `question`, `auth`), or `attention` (the label). None when
/// it doesn't, or a blocked program didn't say why.
pub fn reason(rec: &Value) -> Option<&'static str> {
    match rec.get("status").and_then(Value::as_str)? {
        "error" => Some("error"),
        "blocked" => match rec.pointer("/report/kind").and_then(Value::as_str)? {
            "permission" => Some("permission"),
            "question" => Some("question"),
            "auth" => Some("auth"),
            _ => None,
        },
        "working" => None,
        _ => attention(rec).map(|_| "attention"),
    }
}

/// Terminals whose attention label should go: set while the agent works
/// again (a new turn started). Each is returned once until the core echoes
/// the label gone; `cleared_failed` lets one be tried again.
pub fn labels_to_clear(live: &HashMap<String, Value>) -> Vec<String> {
    let mut clearing = CLEARING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    clearing.retain(|a| live.get(a).is_some_and(|r| attention(r).is_some()));
    let mut out = Vec::new();
    for (addr, rec) in live {
        if rec.get("status").and_then(Value::as_str) == Some("working") && attention(rec).is_some() && clearing.insert(addr.clone()) {
            out.push(addr.clone());
        }
    }
    out.sort();
    out
}

/// A label removal that failed: try it on a later tick.
pub fn cleared_failed(addr: &str) {
    CLEARING.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(addr);
}

static CLEARING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

// ---- stalls ----------------------------------------------------------------

/// One working agent's last sign of life.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Watch {
    /// A hash of the screen as last read.
    screen: u64,
    /// When the screen or the report last changed, ms.
    changed: u64,
    /// When the screen was last read (or asked for), ms.
    read: u64,
}

#[derive(Default)]
struct Stalls {
    watches: HashMap<String, Watch>,
}

static STALLS: Mutex<Option<Stalls>> = Mutex::new(None);

fn with_stalls<R>(f: impl FnOnce(&mut Stalls) -> R) -> R {
    let mut guard = STALLS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    f(guard.get_or_insert_with(Stalls::default))
}

/// The terminals to read now for the stall check: working agents not read
/// in the last [`READ_EVERY_MS`]. Forgets every other terminal, so an agent
/// that stops working starts fresh. `limit_ms`: the stall time (0: off).
pub fn screens_to_read(live: &HashMap<String, Value>, now: u64, limit_ms: u64) -> Vec<String> {
    with_stalls(|s| {
        if limit_ms == 0 {
            s.watches.clear();
            return Vec::new();
        }
        s.watches.retain(|a, _| live.get(a).is_some_and(|r| r.get("status").and_then(Value::as_str) == Some("working")));
        let mut out = Vec::new();
        for (addr, rec) in live {
            if rec.get("status").and_then(Value::as_str) != Some("working") {
                continue;
            }
            let reported = rec.get("status_ms").and_then(Value::as_u64).unwrap_or(0);
            let w = s.watches.entry(addr.clone()).or_insert(Watch { screen: 0, changed: now.max(reported), read: 0 });
            w.changed = w.changed.max(reported);
            if now.saturating_sub(w.read) >= READ_EVERY_MS {
                w.read = now;
                out.push(addr.clone());
            }
        }
        out.sort();
        out
    })
}

/// A screen read for the stall check came back.
pub fn saw_screen(addr: &str, text: &str, now: u64) {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    let screen = h.finish();
    with_stalls(|s| {
        if let Some(w) = s.watches.get_mut(addr)
            && w.screen != screen
        {
            w.screen = screen;
            w.changed = now;
        }
    });
}

/// Whether a working agent has shown no sign of life for `limit_ms` (0: off).
pub fn stalled(addr: &str, now: u64, limit_ms: u64) -> bool {
    limit_ms > 0 && with_stalls(|s| s.watches.get(addr).is_some_and(|w| now.saturating_sub(w.changed) >= limit_ms))
}

/// `stall_minutes` from `<app home>/fork.json`, in ms (0: off, the default),
/// read again at most every 30 s.
pub fn stall_limit_ms(now: u64) -> u64 {
    static CACHE: Mutex<(u64, u64)> = Mutex::new((0, 0));
    let mut c = CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if c.0 == 0 || now.saturating_sub(c.0) >= 30_000 {
        let path = crate::mods::app_dir().join("fork.json");
        let text = std::fs::read_to_string(path).unwrap_or_default();
        *c = (now.max(1), limit_from(&text));
    }
    c.1
}

/// `stall_minutes` in a fork.json's text, in ms.
fn limit_from(text: &str) -> u64 {
    serde_json::from_str::<Value>(text).ok().and_then(|v| v.get("stall_minutes").and_then(Value::as_f64)).filter(|m| m.is_finite() && *m > 0.0).map_or(0, |m| (m * 60_000.0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rec(status: &str, kind: Option<&str>, label: Option<&str>) -> Value {
        let mut labels = json!({});
        if let Some(l) = label {
            labels[LABEL] = json!(l);
        }
        json!({ "status": status, "report": { "state": status, "kind": kind }, "labels": labels })
    }

    #[test]
    fn the_attention_label_shows_a_finished_agent_as_blocked() {
        assert_eq!(word(&rec("done", None, Some("asks you to run make deploy"))).as_deref(), Some("blocked"));
        assert_eq!(word(&rec("idle", None, Some("usage limit"))).as_deref(), Some("blocked"));
        // Working, blocked and error stay what the program said.
        for w in ["working", "blocked", "error"] {
            assert_eq!(word(&rec(w, None, Some("x"))).as_deref(), Some(w));
        }
        // An empty or blank label is no label.
        assert_eq!(word(&rec("done", None, Some(" "))).as_deref(), Some("done"));
        assert_eq!(word(&rec("done", None, None)).as_deref(), Some("done"));
        // A terminal that never reported is no agent, label or not.
        assert_eq!(word(&json!({ "status": null, "labels": { LABEL: "x" } })), None);
    }

    #[test]
    fn reason_says_why_an_agent_needs_you() {
        assert_eq!(reason(&rec("error", None, None)), Some("error"));
        assert_eq!(reason(&rec("blocked", Some("permission"), None)), Some("permission"));
        assert_eq!(reason(&rec("blocked", Some("question"), None)), Some("question"));
        assert_eq!(reason(&rec("blocked", Some("auth"), None)), Some("auth"));
        assert_eq!(reason(&rec("blocked", Some("other"), None)), None);
        assert_eq!(reason(&rec("blocked", None, None)), None);
        assert_eq!(reason(&rec("done", None, Some("run X"))), Some("attention"));
        assert_eq!(reason(&rec("working", None, Some("run X"))), None);
        assert_eq!(reason(&rec("idle", None, None)), None);
    }

    #[test]
    fn attention_text_is_short_and_plain() {
        let long = "a".repeat(60);
        assert_eq!(attention(&rec("done", None, Some(&long))).unwrap(), format!("{}…", "a".repeat(40)));
        assert_eq!(attention(&rec("done", None, Some("run\u{1b}[31m X\n"))).unwrap(), "run[31m X");
    }

    #[test]
    fn a_label_is_cleared_once_when_the_agent_works_again() {
        let mut live = HashMap::new();
        live.insert("t1".to_owned(), rec("working", None, Some("run X")));
        live.insert("t2".to_owned(), rec("done", None, Some("run Y")));
        assert_eq!(labels_to_clear(&live), vec!["t1".to_owned()]);
        // Not again while the removal is on its way.
        assert!(labels_to_clear(&live).is_empty());
        // A failed removal is tried again.
        cleared_failed("t1");
        assert_eq!(labels_to_clear(&live), vec!["t1".to_owned()]);
        // Gone: forgotten, so a later label on t1 clears again.
        live.insert("t1".to_owned(), rec("working", None, None));
        assert!(labels_to_clear(&live).is_empty());
        live.insert("t1".to_owned(), rec("working", None, Some("again")));
        assert_eq!(labels_to_clear(&live), vec!["t1".to_owned()]);
    }

    #[test]
    fn a_working_agent_with_a_still_screen_stalls() {
        let limit = 10 * 60_000;
        let mut live = HashMap::new();
        live.insert("s1".to_owned(), json!({ "status": "working", "status_ms": 1_000 }));
        live.insert("s2".to_owned(), json!({ "status": "idle" }));
        let t0 = 1_000_000;
        assert_eq!(screens_to_read(&live, t0, limit), vec!["s1".to_owned()]);
        saw_screen("s1", "thinking", t0);
        // Not read again within a minute.
        assert!(screens_to_read(&live, t0 + 1_000, limit).is_empty());
        assert!(!stalled("s1", t0 + limit - 1, limit));
        // The same screen a minute later is no sign of life.
        assert_eq!(screens_to_read(&live, t0 + READ_EVERY_MS, limit), vec!["s1".to_owned()]);
        saw_screen("s1", "thinking", t0 + READ_EVERY_MS);
        assert!(stalled("s1", t0 + limit, limit));
        // A new screen is.
        saw_screen("s1", "thinking.", t0 + limit);
        assert!(!stalled("s1", t0 + limit + 1, limit));
        // So is a new report.
        live.insert("s1".to_owned(), json!({ "status": "working", "status_ms": t0 + 2 * limit }));
        screens_to_read(&live, t0 + 2 * limit, limit);
        assert!(!stalled("s1", t0 + 2 * limit + 1, limit));
        // Stopping forgets it; off forgets everything and never stalls.
        live.insert("s1".to_owned(), json!({ "status": "done" }));
        screens_to_read(&live, t0 + 3 * limit, limit);
        assert!(!stalled("s1", t0 + 9 * limit, limit));
        assert!(!stalled("s1", t0 + 9 * limit, 0));
        assert!(screens_to_read(&live, t0, 0).is_empty());
    }

    #[test]
    fn stall_minutes_come_from_fork_json() {
        assert_eq!(limit_from(r#"{ "stall_minutes": 10 }"#), 600_000);
        assert_eq!(limit_from(r#"{ "stall_minutes": 0.5 }"#), 30_000);
        for off in ["", "{}", r#"{ "stall_minutes": 0 }"#, r#"{ "stall_minutes": -3 }"#, r#"{ "stall_minutes": "10" }"#, "not json"] {
            assert_eq!(limit_from(off), 0, "{off:?}");
        }
    }
}
