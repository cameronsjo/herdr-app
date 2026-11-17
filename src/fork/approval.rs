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

/// Screen text as shown to a caller: no control characters and no bidi
/// formatting, which could make a label read as something else.
fn clean(s: &str) -> String {
    s.chars().filter(|c| !c.is_control() && !matches!(*c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{2069}' | '\u{feff}')).collect()
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

/// The panel's top edge above the options at `first`. A bare rule counts
/// only when it is at least as wide as every line under it: a panel's rule
/// spans it, and a short rule is part of what is shown (a table in a
/// heredoc), so the lines above it still belong to the prompt.
fn top_edge(lines: &[&str], first: usize) -> Option<usize> {
    (0..first).rev().find(|&i| {
        let t = lines[i].trim();
        edge(t) && (!t.chars().all(|c| matches!(c, '─' | '━' | '═')) || lines[i + 1..first].iter().all(|l| l.trim_end().chars().count() <= lines[i].trim_end().chars().count()))
    })
}

/// One screen line without a box's edges and the padding inside them.
fn unboxed(line: &str) -> &str {
    line.trim().trim_matches(EDGES).trim()
}

const EDGES: &[char] = &['│', '┃', '║', '|', '╭', '╮', '╰', '╯', '─', '━', '═', '┌', '┐', '└', '┘', '├', '┤', '╞', '╡'];

/// How far in a line's text starts, inside a box's left edge.
fn indent(line: &str) -> usize {
    let inside = line.trim_start().strip_prefix(EDGES).unwrap_or(line);
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
/// them. None when the screen shows no such menu.
pub fn parse(screen: &str) -> Option<Prompt> {
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
    let top = top_edge(&lines, first);
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
const STANDING: &[&str] = &["always", "don't ask", "dont ask", "do not ask", "again", "session", "all ", "every", "remember", "bypass", "auto-accept", "auto accept", "without asking", "permanent", "trust", "for this project"];

/// The option an answer means: a number, `approve` (the one option that
/// starts with yes, allow or approve and grants nothing beyond this once)
/// or `deny` (the one that starts with no, deny or reject). More than one
/// match asks for a number.
pub fn choose(p: &Prompt, answer: &str) -> Result<usize, String> {
    let one = |words: &[&str], what: &str| -> Result<usize, String> {
        let found: Vec<usize> = (0..p.options.len())
            .filter(|&i| {
                let o = p.options[i].to_lowercase().replace('’', "'");
                words.iter().any(|w| o == *w || o.starts_with(&format!("{w} ")) || o.starts_with(&format!("{w},")) || o.starts_with(&format!("{w}.")))
                    && (what == "denies" || !STANDING.iter().any(|s| o.contains(s)))
            })
            .collect();
        // Several: only a bare "Yes" (or "Yes (y)") is clearly the plain one.
        let bare = |i: &usize| words.iter().any(|w| p.options[*i].to_lowercase().trim_end_matches("(y)").trim_end_matches("(n)").trim() == *w);
        let found = if found.len() > 1 { found.iter().copied().filter(bare).collect() } else { found };
        match found[..] {
            [i] => Ok(i + 1),
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

/// The prompt a terminal is blocked on now, with its kind and turn. Exit 3
/// when there is nothing to answer.
fn current(core: &crate::hc::Core, terminal: &str) -> Result<(Option<String>, u64, Prompt), Stop> {
    let rec = core.call("terminal.get", json!({ "terminal": terminal })).map_err(|e| failed(format!("{e:#}")))?;
    let (word, kind) = state(&rec);
    if word != Some("blocked") {
        return Err((3, format!("terminal {terminal} is not blocked (it is {}): nothing to answer", word.unwrap_or("not an agent"))));
    }
    let kind = kind.map(str::to_owned);
    let turn = rec.get("turn").and_then(Value::as_u64).ok_or_else(|| failed(format!("the core did not say terminal {terminal}'s turn: this core is too old to answer prompts safely")))?;
    let read = core.call("terminal.read", json!({ "terminal": terminal, "lines": 0 })).map_err(|e| failed(format!("{e:#}")))?;
    let text = read.get("text").and_then(Value::as_str).unwrap_or_default();
    let p = parse(text).ok_or_else(|| (3, format!("terminal {terminal} is blocked, but its screen shows no numbered options: answer it in its pane")))?;
    Ok((kind, turn, p))
}

/// How long `answer` waits for the key to go in. The core holds it for as
/// long as a person keeps typing in that pane and can't take it back, so
/// `answer` waits for the reply and checks it; only a lost core ends this
/// sooner.
const KEYS_WAIT: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// `answer`'s arguments, `--token` anywhere: terminal, answer, token.
fn answer_args(rest: &[String]) -> Option<(&str, &str, &str)> {
    let mut token = None;
    let mut plain = Vec::new();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        if a == "--token" {
            token = Some(it.next()?.as_str());
        } else if let Some(t) = a.strip_prefix("--token=") {
            token = Some(t);
        } else {
            plain.push(a.as_str());
        }
    }
    match (plain.as_slice(), token) {
        ([terminal, answer], Some(t)) => Some((*terminal, *answer, t)),
        _ => None,
    }
}

fn answer(core: &crate::hc::Core, terminal: &str, answer: &str, want: &str) -> Result<Value, Stop> {
    let (_, turn, p) = current(core, terminal)?;
    if token(terminal, turn, &p) != want {
        return Err((3, format!("terminal {terminal}'s prompt changed since that token: run herdr-app approval show {terminal} again")));
    }
    let n = choose(&p, answer).map_err(|e| (2, e))?;
    // The core holds keys while a person types in the pane, and a person
    // answering there moves the agent on: its turn then differs, and this
    // key went to whatever came next.
    let sent = core.call_timeout("terminal.keys", json!({ "terminal": terminal, "keys": [n.to_string()] }), KEYS_WAIT).map_err(|e| {
        failed(format!("{e:#}: the key {n} may still be queued for terminal {terminal}; the core presses it 3 s after typing in that pane stops and it can't be cancelled, so it may answer a later prompt: check the pane"))
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
    const USAGE: &str = "usage: herdr-app approval show <terminal> | herdr-app approval answer <terminal> <number|approve|deny> --token <token>";
    let run = |f: &dyn Fn(&crate::hc::Core) -> Result<Value, Stop>| -> i32 {
        // The core the window uses: the person's configured one, else the
        // usual places, as the window finds it.
        let person = crate::settings::load_person();
        let Some(bin) = crate::core_bin::find(person.as_ref().ok().and_then(|p| p.core.as_deref())) else {
            eprintln!("herdr-app: herdr-core is not installed: open Herdr to set it up");
            return 1;
        };
        crate::core_bin::use_core(bin);
        match crate::hc::Core::connect().map_err(|e| failed(format!("{e:#}"))).and_then(|core| f(&core)) {
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
            Some((terminal, a, t)) => run(&|core| answer(core, terminal, a, t)),
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
        let p = parse(&claude("rm -rf build/")).unwrap();
        assert_eq!(p.question, "Do you want to proceed?");
        assert_eq!(p.context, ["Bash command", "rm -rf build/"]);
        assert_eq!(p.options.len(), 3);
        assert_eq!(p.options[1], "Yes, and don't ask again for rm commands in /src/app");
        // In a box, with a hint below: the box's top edge ends the context.
        let boxed = "earlier output\n╭────────╮\n│ Edit src/a.rs           │\n│ Allow this edit?        │\n│ › 1. Allow              │\n│   2) Deny               │\n╰────────╯\n  Esc to cancel\n";
        assert_eq!(parse(boxed), Some(Prompt { context: vec!["Edit src/a.rs".into()], complete: true, question: "Allow this edit?".into(), options: vec!["Allow".into(), "Deny".into()] }));
        // A wrapped label and a description join the option above.
        let wrapped = "Pick one\n 1. Yes\n      the plain one\n 2. Yes, and don't ask again for git\n    commands in /src/app\n 3. No\n";
        assert_eq!(parse(wrapped).unwrap().options, ["Yes the plain one", "Yes, and don't ask again for git commands in /src/app", "No"]);
        let boxed_wrap = "╭──╮\n│ Go?            │\n│ ❯ 1. Yes, and don't ask │\n│      again          │\n│   2. No             │\n╰──╯\n";
        assert_eq!(parse(boxed_wrap).unwrap().options, ["Yes, and don't ask again", "No"]);
        // The whole command is context, however long; `---` inside it is
        // text, and a box's empty row is no edge.
        let long = format!("{}\n Bash command\n   rm -rf ~/proj/data\n{}   ---\n│        │\n Do you want to proceed?\n 1. Yes\n 2. No\n", "─".repeat(40), "   echo ok\n".repeat(15));
        let p = parse(&long).unwrap();
        assert!(p.complete);
        assert_eq!(p.context[1], "rm -rf ~/proj/data");
        assert_eq!(p.context.len(), 18);
        // A short rule or a box's bottom inside the command is content: the
        // lines above it still belong to the prompt.
        let heredoc = format!("{}\n Bash command\n   rm -rf ~/proj/data\n   cat <<EOF\n   ────────\n   EOF\n Do you want to proceed?\n 1. Yes\n 2. No\n", "─".repeat(40));
        assert!(parse(&heredoc).unwrap().context.contains(&"rm -rf ~/proj/data".to_string()));
        let bottom = "──────────────────────────────\n Bash command\n   rm -rf ~/proj/data\n   ╰──────╯\n Proceed?\n 1. Yes\n 2. No\n";
        assert!(parse(bottom).unwrap().context.contains(&"rm -rf ~/proj/data".to_string()));
        // No top edge above it: maybe cut off.
        let cut = parse("   rm -rf x\n Proceed?\n 1. Yes\n 2. No\n").unwrap();
        assert!(!cut.complete);
        // Control and bidi characters never reach a caller.
        let sly = "Proceed?\n 1. Yes\u{202e}oN\n 2. No\u{9b}2J don\u{200b}'t\n";
        assert_eq!(parse(sly).unwrap().options, ["YesoN", "No2J don't"]);
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
            assert_eq!(parse(screen), None, "{screen:?}");
        }
    }

    #[test]
    fn a_token_names_one_prompt_on_one_terminal_in_one_turn() {
        let p = parse(&claude("git status")).unwrap();
        let t = token("t3", 4, &p);
        assert_eq!(t.len(), 16);
        assert_eq!(token("t3", 4, &parse(&claude("git status")).unwrap()), t);
        assert_ne!(token("t4", 4, &p), t);
        // The same question and options about another command.
        assert_ne!(token("t3", 4, &parse(&claude("git push --force origin main")).unwrap()), t);
        // The next prompt after an answer is another turn.
        assert_ne!(token("t3", 5, &p), t);
        let shown = shown("t3", Some("permission"), 4, &p);
        assert_eq!(shown["token"], t);
        assert_eq!(shown["context"][1], "git status");
        assert_eq!(shown["options"][2]["n"], 3);
    }

    #[test]
    fn answers_name_one_plain_option() {
        let p = parse(&claude("ls")).unwrap();
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
    fn answer_takes_its_token_anywhere() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(answer_args(&args(&["t3", "approve", "--token", "ab"])), Some(("t3", "approve", "ab")));
        assert_eq!(answer_args(&args(&["t3", "--token", "ab", "1"])), Some(("t3", "1", "ab")));
        assert_eq!(answer_args(&args(&["--token=ab", "t3", "deny"])), Some(("t3", "deny", "ab")));
        for bad in [&["t3", "approve"][..], &["t3", "approve", "--token"], &["t3", "--token", "ab"], &["t3", "1", "2", "--token", "ab"]] {
            assert_eq!(answer_args(&args(bad)), None, "{bad:?}");
        }
    }

    #[test]
    fn only_a_blocked_terminal_has_something_to_answer() {
        assert_eq!(state(&json!({ "status": { "state": "blocked", "kind": "permission" } })), (Some("blocked"), Some("permission")));
        assert_eq!(state(&json!({ "status": null })), (None, None));
        assert_eq!(state(&json!({ "status": "working" })), (Some("working"), None));
    }
}
