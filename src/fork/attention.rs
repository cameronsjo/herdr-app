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

/// How long a failed label removal waits before it is tried again, in ms.
const RETRY_MS: u64 = 5_000;

/// The attention label's text, if set (trimmed, at most [`MAX_TEXT`] characters).
pub fn attention(rec: &Value) -> Option<String> {
    let text = rec.pointer("/labels").and_then(|l| l.get(LABEL)).and_then(Value::as_str)?.trim();
    let mut plain = text.chars().filter(|c| !c.is_control());
    let cut: String = plain.by_ref().take(MAX_TEXT).collect();
    if cut.is_empty() {
        return None;
    }
    Some(if plain.next().is_some() { format!("{cut}…") } else { cut })
}

/// Whether the attention label alone makes a terminal show as blocked: it is
/// set and the program isn't working, blocked or in error.
fn label_blocks(rec: &Value) -> bool {
    !matches!(rec.get("status").and_then(Value::as_str), Some("working" | "blocked" | "error") | None) && attention(rec).is_some()
}

/// The word the app shows for a terminal (`rec`: a listed record, `status`
/// already the root report's state): the core's, except that an agent with
/// the attention label that isn't working, blocked or in error is blocked.
pub fn word(rec: &Value) -> Option<String> {
    let w = rec.get("status").and_then(Value::as_str)?;
    Some(if label_blocks(rec) { "blocked" } else { w }.to_owned())
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

/// One working agent's last sign of life.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Watch {
    /// A hash of the screen as last read.
    screen: u64,
    /// A hash of the program's report as last seen.
    report: u64,
    /// When the screen or the report last changed, ms.
    changed: u64,
    /// When the screen was last read (or asked for), ms.
    read: u64,
    /// Whether the mods were last told it is stalled.
    shown: bool,
}

fn hash_of(v: &impl std::hash::Hash) -> u64 {
    use std::hash::Hasher as _;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

/// The queue's memory between ticks: one per window ([`queue`]); tests make
/// their own.
#[derive(Default)]
pub struct Queue {
    /// Label removals on their way.
    clearing: BTreeSet<String>,
    /// Terminals shown blocked by the label alone, last time.
    labeled: BTreeSet<String>,
    /// Terminals whose label went while the agent wasn't working.
    handled: BTreeSet<String>,
    watches: HashMap<String, Watch>,
    /// Failed label removals: not tried again before this time, ms.
    retry_at: HashMap<String, u64>,
    /// The terminal's turn when its label was first seen. A later turn means
    /// the agent worked since, even if no tick saw it working.
    label_turn: HashMap<String, u64>,
    /// Terminals whose label [`Queue::labels_to_clear`] took off because the
    /// agent worked: that label's going is not [`Queue::handled`].
    worked_off: BTreeSet<String>,
}

static QUEUE: Mutex<Option<Queue>> = Mutex::new(None);

/// The window's queue.
pub fn queue<R>(f: impl FnOnce(&mut Queue) -> R) -> R {
    let mut guard = QUEUE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    f(guard.get_or_insert_with(Queue::default))
}

impl Queue {
    /// [`word`], remembering which terminals the label alone shows blocked,
    /// so one whose label goes while it isn't working is [`Queue::handled`].
    pub fn word(&mut self, addr: &str, rec: &Value) -> Option<String> {
        if label_blocks(rec) {
            self.labeled.insert(addr.to_owned());
        } else {
            // A label taken off because the agent worked is never dealt with;
            // forgotten here either way, so it can't outlive its label.
            let worked = self.worked_off.remove(addr);
            if self.labeled.remove(addr) && !worked && !matches!(rec.get("status").and_then(Value::as_str), Some("working" | "blocked" | "error")) {
                self.handled.insert(addr.to_owned());
            }
        }
        word(rec)
    }

    /// Whether this terminal's label just went without the agent working: the
    /// person (or the classifier) dealt with it, so its finish is not news.
    /// Once.
    pub fn handled(&mut self, addr: &str) -> bool {
        self.handled.remove(addr)
    }

    /// Terminals whose attention label should go: set while the agent works
    /// again, or the terminal's `turn` moved past the one it had when the label
    /// was first seen (a turn shorter than a tick: done, working, done). Each
    /// is returned once until the core echoes the label gone;
    /// [`Queue::cleared_failed`] lets one be tried again after [`RETRY_MS`].
    pub fn labels_to_clear(&mut self, live: &HashMap<String, Value>, now: u64) -> Vec<String> {
        self.clearing.retain(|a| live.get(a).is_some_and(|r| attention(r).is_some()));
        self.labeled.retain(|a| live.contains_key(a));
        self.handled.retain(|a| live.contains_key(a));
        self.worked_off.retain(|a| live.contains_key(a));
        self.retry_at.retain(|a, at| *at > now && live.contains_key(a));
        self.label_turn.retain(|a, _| live.get(a).is_some_and(|r| attention(r).is_some()));
        let mut out = Vec::new();
        for (addr, rec) in live {
            if attention(rec).is_none() {
                continue;
            }
            let turn = rec.get("turn").and_then(Value::as_u64);
            let first = *self.label_turn.entry(addr.clone()).or_insert(turn.unwrap_or(0));
            let worked = rec.get("status").and_then(Value::as_str) == Some("working") || turn.is_some_and(|t| t > first);
            if worked && !self.retry_at.contains_key(addr) && self.clearing.insert(addr.clone()) {
                if self.labeled.contains(addr) {
                    self.worked_off.insert(addr.clone());
                }
                out.push(addr.clone());
            }
        }
        out.sort();
        out
    }

    /// A label removal that failed: try it again once [`RETRY_MS`] have passed.
    pub fn cleared_failed(&mut self, addr: &str, now: u64) {
        self.clearing.remove(addr);
        self.retry_at.insert(addr.to_owned(), now + RETRY_MS);
    }

    /// The terminals to read now for the stall check: working agents not
    /// read in the last [`READ_EVERY_MS`]. Forgets every other terminal, so
    /// an agent that stops working starts fresh. A new report counts as a
    /// sign of life. `limit_ms`: the stall time (0: off).
    pub fn screens_to_read(&mut self, live: &HashMap<String, Value>, now: u64, limit_ms: u64) -> Vec<String> {
        let working = |r: &Value| r.get("status").and_then(Value::as_str) == Some("working");
        if limit_ms == 0 {
            self.watches.clear();
            return Vec::new();
        }
        self.watches.retain(|a, _| live.get(a).is_some_and(working));
        let mut out = Vec::new();
        for (addr, rec) in live {
            if !working(rec) {
                continue;
            }
            let report = hash_of(&rec.get("report").map(Value::to_string));
            let w = self.watches.entry(addr.clone()).or_insert(Watch { screen: 0, report, changed: now, read: 0, shown: false });
            if w.report != report {
                w.report = report;
                w.changed = now;
            }
            if now.saturating_sub(w.read) >= READ_EVERY_MS {
                w.read = now;
                out.push(addr.clone());
            }
        }
        out.sort();
        out
    }

    /// A screen read for the stall check came back (`text`), or failed
    /// (None, or an empty screen: no evidence of a stall, so it counts as a
    /// sign of life). True when the stall the mods were last shown changes:
    /// one started since, or this read ended one.
    pub fn saw_screen(&mut self, addr: &str, text: Option<&str>, now: u64, limit_ms: u64) -> bool {
        let screen = text.filter(|t| !t.trim().is_empty()).map(|t| hash_of(&t));
        let Some(w) = self.watches.get_mut(addr) else { return false };
        if screen.is_none() || screen != Some(w.screen) {
            w.screen = screen.unwrap_or(0);
            w.changed = now;
        }
        let stalled = limit_ms > 0 && now.saturating_sub(w.changed) >= limit_ms;
        std::mem::replace(&mut w.shown, stalled) != stalled
    }

    /// Whether a working agent has shown no sign of life for `limit_ms` (0: off).
    pub fn stalled(&self, addr: &str, now: u64, limit_ms: u64) -> bool {
        limit_ms > 0 && self.watches.get(addr).is_some_and(|w| now.saturating_sub(w.changed) >= limit_ms)
    }
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
        // Control characters don't count toward the cut.
        assert_eq!(attention(&rec("done", None, Some(&format!("{}\u{7}", "a".repeat(40))))).unwrap(), "a".repeat(40));
        assert_eq!(attention(&rec("done", None, Some("\u{7}"))), None);
    }

    #[test]
    fn a_label_is_cleared_once_when_the_agent_works_again() {
        let mut q = Queue::default();
        let mut live = HashMap::new();
        live.insert("t1".to_owned(), rec("working", None, Some("run X")));
        live.insert("t2".to_owned(), rec("done", None, Some("run Y")));
        assert_eq!(q.labels_to_clear(&live, 0), vec!["t1".to_owned()]);
        // Not again while the removal is on its way.
        assert!(q.labels_to_clear(&live, 500).is_empty());
        // A failed removal is tried again, but not on the very next tick.
        q.cleared_failed("t1", 1_000);
        assert!(q.labels_to_clear(&live, 1_500).is_empty());
        assert!(q.labels_to_clear(&live, 5_999).is_empty());
        assert_eq!(q.labels_to_clear(&live, 6_000), vec!["t1".to_owned()]);
        // Gone: forgotten, so a later label on t1 clears again.
        live.insert("t1".to_owned(), rec("working", None, None));
        assert!(q.labels_to_clear(&live, 7_000).is_empty());
        live.insert("t1".to_owned(), rec("working", None, Some("again")));
        assert_eq!(q.labels_to_clear(&live, 7_500), vec!["t1".to_owned()]);
    }

    #[test]
    fn a_turn_shorter_than_a_tick_still_clears_the_label() {
        let with_turn = |status: &str, turn: u64| {
            let mut r = rec(status, None, Some("run X"));
            r["turn"] = json!(turn);
            r
        };
        let mut q = Queue::default();
        let mut live = HashMap::new();
        live.insert("t1".to_owned(), with_turn("done", 4));
        assert!(q.labels_to_clear(&live, 0).is_empty());
        // Same turn: the label stays.
        assert!(q.labels_to_clear(&live, 500).is_empty());
        // Done, working, done between two ticks: the turn moved on, so it goes.
        live.insert("t1".to_owned(), with_turn("done", 5));
        q.word("t1", &live["t1"]);
        assert_eq!(q.labels_to_clear(&live, 1_000), vec!["t1".to_owned()]);
        // The label gone, the agent at rest: that turn's finish is still news.
        q.word("t1", &rec("done", None, None));
        assert!(!q.handled("t1"), "a label cleared because the agent worked is not dealt with");
        // A later label the person deals with while the agent rests is handled.
        q.word("t1", &rec("done", None, Some("run Y")));
        q.word("t1", &rec("done", None, None));
        assert!(q.handled("t1"), "nothing from the last label lingers");
    }

    #[test]
    fn a_label_cleared_while_working_leaves_nothing_behind() {
        // word() runs before labels_to_clear in a tick: a working agent's label
        // no longer blocks, so it was never one the queue showed.
        let mut q = Queue::default();
        let mut live = HashMap::new();
        q.word("t1", &rec("done", None, Some("run X")));
        live.insert("t1".to_owned(), rec("working", None, Some("run X")));
        q.word("t1", &live["t1"]);
        assert_eq!(q.labels_to_clear(&live, 0), vec!["t1".to_owned()]);
        q.word("t1", &rec("done", None, None));
        // The next label, dealt with while the agent rests: no Done alert.
        q.word("t1", &rec("done", None, Some("run Y")));
        q.word("t1", &rec("done", None, None));
        assert!(q.handled("t1"));
    }

    #[test]
    fn a_label_that_goes_while_the_agent_rests_is_handled_not_news() {
        let mut q = Queue::default();
        assert_eq!(q.word("t1", &rec("done", None, Some("run X"))).as_deref(), Some("blocked"));
        assert!(!q.handled("t1"));
        // Cleared without a new turn: the finish isn't news.
        assert_eq!(q.word("t1", &rec("done", None, None)).as_deref(), Some("done"));
        assert!(q.handled("t1"));
        assert!(!q.handled("t1"), "once");
        // Cleared because the agent works again: an ordinary turn.
        q.word("t2", &rec("idle", None, Some("usage limit")));
        q.word("t2", &rec("working", None, Some("usage limit")));
        q.word("t2", &rec("working", None, None));
        assert!(!q.handled("t2"));
        // A program's own blocked word is not the label's.
        q.word("t3", &rec("blocked", Some("permission"), Some("x")));
        q.word("t3", &rec("done", None, None));
        assert!(!q.handled("t3"));
    }

    #[test]
    fn a_working_agent_with_a_still_screen_stalls() {
        let mut q = Queue::default();
        let limit = 10 * 60_000;
        let mut live = HashMap::new();
        let working = |n: u64| json!({ "status": "working", "report": { "state": "working", "msg": n.to_string() } });
        live.insert("s1".to_owned(), working(1));
        live.insert("s2".to_owned(), json!({ "status": "idle" }));
        let t0 = 1_000_000;
        assert_eq!(q.screens_to_read(&live, t0, limit), vec!["s1".to_owned()]);
        assert!(!q.saw_screen("s1", Some("thinking"), t0, limit));
        // Not read again within a minute.
        assert!(q.screens_to_read(&live, t0 + 1_000, limit).is_empty());
        assert!(!q.stalled("s1", t0 + limit - 1, limit));
        // The same screen later is no sign of life.
        assert_eq!(q.screens_to_read(&live, t0 + READ_EVERY_MS, limit), vec!["s1".to_owned()]);
        assert!(!q.saw_screen("s1", Some("thinking"), t0 + READ_EVERY_MS, limit));
        assert!(q.stalled("s1", t0 + limit, limit));
        // The read that finds the stall says so, once.
        assert!(q.saw_screen("s1", Some("thinking"), t0 + limit, limit), "a stall starts");
        assert!(!q.saw_screen("s1", Some("thinking"), t0 + limit, limit), "already shown");
        // A new screen is, and ends the stall.
        assert!(q.saw_screen("s1", Some("thinking."), t0 + limit, limit));
        assert!(!q.stalled("s1", t0 + limit + 1, limit));
        // So is a new report.
        assert!(q.stalled("s1", t0 + 2 * limit, limit));
        live.insert("s1".to_owned(), working(2));
        q.screens_to_read(&live, t0 + 2 * limit, limit);
        assert!(!q.stalled("s1", t0 + 2 * limit + 1, limit));
        // A screen that can't be read is no evidence of a stall.
        assert!(q.stalled("s1", t0 + 3 * limit, limit));
        assert!(q.saw_screen("s1", Some("thinking."), t0 + 3 * limit, limit), "shown stalled");
        assert!(q.saw_screen("s1", None, t0 + 3 * limit, limit));
        assert!(!q.stalled("s1", t0 + 3 * limit, limit));
        // Nor is an empty one.
        assert!(q.stalled("s1", t0 + 4 * limit, limit));
        q.saw_screen("s1", Some(" \n"), t0 + 4 * limit, limit);
        assert!(!q.stalled("s1", t0 + 4 * limit, limit));
        // Stopping forgets it; off forgets everything and never stalls.
        live.insert("s1".to_owned(), json!({ "status": "done" }));
        q.screens_to_read(&live, t0 + 5 * limit, limit);
        assert!(!q.stalled("s1", t0 + 9 * limit, limit));
        assert!(!q.stalled("s1", t0 + 9 * limit, 0));
        assert!(q.screens_to_read(&live, t0, 0).is_empty());
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
