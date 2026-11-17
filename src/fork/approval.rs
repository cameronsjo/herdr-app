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
    /// The line above the options (often the question), "" if none.
    pub question: String,
    /// The options, numbered from 1, in order.
    pub options: Vec<String>,
}

/// How many lines may sit below the menu (a hint such as "Esc to cancel",
/// a box's bottom edge, the input line).
const BELOW: usize = 6;

/// One screen line without a box's edges and the padding inside them.
fn unboxed(line: &str) -> &str {
    let edges: &[char] = &['│', '┃', '║', '|', '╭', '╮', '╰', '╯', '─', '━', '═', '┌', '┐', '└', '┘'];
    line.trim().trim_matches(edges).trim()
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
    let mut first = last;
    while first > 0 && option(lines[first - 1]).is_some() {
        first -= 1;
    }
    let options: Vec<(usize, String)> = lines[first..=last].iter().filter_map(|l| option(l)).collect();
    if options.len() < 2 || options.iter().enumerate().any(|(i, (n, _))| *n != i + 1) {
        return None;
    }
    let question = lines[..first].iter().rev().map(|l| unboxed(l)).find(|l| !l.is_empty()).unwrap_or("").to_owned();
    Some(Prompt { question, options: options.into_iter().map(|(_, l)| l).collect() })
}

/// The token for one terminal's prompt: the same question and options on the
/// same terminal give the same token.
pub fn token(terminal: &str, p: &Prompt) -> String {
    let mut h = Sha256::new();
    for part in std::iter::once(terminal).chain(std::iter::once(p.question.as_str())).chain(p.options.iter().map(String::as_str)) {
        h.update(part.as_bytes());
        h.update([0]);
    }
    h.finalize().iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// The option an answer means: a number, `approve` (the first option that
/// starts with yes, allow or approve) or `deny` (the first that starts with
/// no, deny or reject).
pub fn choose(p: &Prompt, answer: &str) -> Result<usize, String> {
    let starts = |words: &[&str]| {
        p.options.iter().position(|o| {
            let o = o.to_lowercase();
            words.iter().any(|w| o == *w || o.starts_with(&format!("{w} ")) || o.starts_with(&format!("{w},")))
        })
    };
    let n = match answer {
        "approve" | "yes" | "allow" => starts(&["yes", "allow", "approve"]).map(|i| i + 1).ok_or("no option here approves (none starts with yes, allow or approve): answer with its number")?,
        "deny" | "no" | "reject" => starts(&["no", "deny", "reject"]).map(|i| i + 1).ok_or("no option here denies (none starts with no, deny or reject): answer with its number")?,
        n => n.parse::<usize>().map_err(|_| format!("answer {n:?}: give an option's number, approve or deny"))?,
    };
    if n == 0 || n > p.options.len() || n > 9 {
        return Err(format!("there is no option {n} here (1 to {})", p.options.len().min(9)));
    }
    Ok(n)
}

/// What `show` prints for a blocked terminal's prompt.
pub fn shown(terminal: &str, kind: Option<&str>, p: &Prompt) -> Value {
    let options: Vec<Value> = p.options.iter().enumerate().map(|(i, l)| json!({ "n": i + 1, "label": l })).collect();
    json!({ "terminal": terminal, "kind": kind, "question": p.question, "options": options, "token": token(terminal, p) })
}

/// The terminal's word (its program's 7501 state) and blocked kind, from a
/// `terminal.get` reply.
fn state(rec: &Value) -> (Option<&str>, Option<&str>) {
    let status = rec.get("status");
    let word = status.and_then(|s| s.get("state")).and_then(Value::as_str).or_else(|| status.and_then(Value::as_str));
    (word, status.and_then(|s| s.get("kind")).and_then(Value::as_str))
}

/// The prompt a terminal is blocked on now, with its kind.
fn current(core: &crate::hc::Core, terminal: &str) -> Result<(Option<String>, Prompt), String> {
    let rec = core.call("terminal.get", json!({ "terminal": terminal })).map_err(|e| format!("{e:#}"))?;
    let (word, kind) = state(&rec);
    if word != Some("blocked") {
        return Err(format!("terminal {terminal} is not blocked (it is {}): nothing to answer", word.unwrap_or("not an agent")));
    }
    let kind = kind.map(str::to_owned);
    let read = core.call("terminal.read", json!({ "terminal": terminal, "lines": 0 })).map_err(|e| format!("{e:#}"))?;
    let text = read.get("text").and_then(Value::as_str).unwrap_or_default();
    let p = parse(text).ok_or_else(|| format!("terminal {terminal} is blocked, but its screen shows no numbered options: answer it in its pane"))?;
    Ok((kind, p))
}

/// `herdr-app approval show|answer ...`: prints JSON on stdout, an error on
/// stderr. 0 on success, 1 when the core or the prompt says no, 2 on usage.
pub fn main(args: &[String]) -> i32 {
    const USAGE: &str = "usage: herdr-app approval show <terminal> | herdr-app approval answer <terminal> <number|approve|deny> --token <token>";
    let run = |f: &dyn Fn(&crate::hc::Core) -> Result<Value, String>| -> i32 {
        let out = crate::hc::Core::connect().map_err(|e| format!("{e:#}")).and_then(|core| f(&core));
        match out {
            Ok(v) => {
                println!("{v}");
                0
            }
            Err(e) => {
                eprintln!("herdr-app: {e}");
                1
            }
        }
    };
    match args {
        [verb, terminal] if verb == "show" => run(&|core| current(core, terminal).map(|(kind, p)| shown(terminal, kind.as_deref(), &p))),
        [verb, terminal, answer, flag, want] if verb == "answer" && flag == "--token" => run(&|core| {
            let (_, p) = current(core, terminal)?;
            if token(terminal, &p) != *want {
                return Err(format!("terminal {terminal}'s prompt changed since that token: run herdr-app approval show {terminal} again"));
            }
            let n = choose(&p, answer)?;
            core.call("terminal.keys", json!({ "terminal": terminal, "keys": [n.to_string()] })).map_err(|e| format!("{e:#}"))?;
            Ok(json!({ "terminal": terminal, "answered": n, "label": p.options[n - 1] }))
        }),
        _ => {
            eprintln!("herdr-app: {USAGE}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE: &str = "\
 Bash command

   rm -rf build/

 Do you want to proceed?
 ❯ 1. Yes
   2. Yes, and don't ask again for rm commands in /src/app
   3. No, and tell Claude what to do differently (esc)

";

    #[test]
    fn a_numbered_menu_at_the_bottom_is_the_prompt() {
        let p = parse(CLAUDE).unwrap();
        assert_eq!(p.question, "Do you want to proceed?");
        assert_eq!(p.options.len(), 3);
        assert_eq!(p.options[1], "Yes, and don't ask again for rm commands in /src/app");
        // In a box, with a hint below.
        let boxed = "╭────────╮\n│ Allow this edit?        │\n│ › 1. Allow              │\n│   2) Deny               │\n╰────────╯\n  Esc to cancel\n";
        assert_eq!(parse(boxed), Some(Prompt { question: "Allow this edit?".into(), options: vec!["Allow".into(), "Deny".into()] }));
    }

    #[test]
    fn a_screen_without_a_whole_menu_has_no_prompt() {
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
        ] {
            assert_eq!(parse(screen), None, "{screen:?}");
        }
    }

    #[test]
    fn a_token_names_one_prompt_on_one_terminal() {
        let p = parse(CLAUDE).unwrap();
        let t = token("t3", &p);
        assert_eq!(t.len(), 16);
        assert_eq!(token("t3", &parse(CLAUDE).unwrap()), t);
        assert_ne!(token("t4", &p), t);
        let other = Prompt { question: "Do you want to proceed?".into(), options: vec!["Yes".into(), "No".into()] };
        assert_ne!(token("t3", &other), t);
        let shown = shown("t3", Some("permission"), &p);
        assert_eq!(shown["token"], t);
        assert_eq!(shown["options"][2]["n"], 3);
    }

    #[test]
    fn answers_name_an_option() {
        let p = parse(CLAUDE).unwrap();
        assert_eq!(choose(&p, "approve"), Ok(1));
        assert_eq!(choose(&p, "deny"), Ok(3));
        assert_eq!(choose(&p, "2"), Ok(2));
        assert!(choose(&p, "4").is_err());
        assert!(choose(&p, "0").is_err());
        assert!(choose(&p, "always").is_err());
        let odd = Prompt { question: String::new(), options: vec!["Run it".into(), "Skip".into()] };
        assert!(choose(&odd, "approve").is_err(), "no option says yes");
        assert!(choose(&odd, "deny").is_err());
        // "Nothing" doesn't start with the word "no".
        let tricky = Prompt { question: String::new(), options: vec!["Yes".into(), "Nothing else".into()] };
        assert!(choose(&tricky, "deny").is_err());
    }

    #[test]
    fn only_a_blocked_terminal_has_something_to_answer() {
        assert_eq!(state(&json!({ "status": { "state": "blocked", "kind": "permission" } })), (Some("blocked"), Some("permission")));
        assert_eq!(state(&json!({ "status": null })), (None, None));
        assert_eq!(state(&json!({ "status": "working" })), (Some("working"), None));
    }
}
