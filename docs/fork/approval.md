# Answering approvals from outside the pane

cameronsjo/herdr-app#17. A blocked agent's prompt can be answered from a
shell, a mod, another client or a phone, without opening its pane and
without pressing keys blind.

```sh
herdr-app approval show t3
# {"terminal":"t3","kind":"permission","context":["Bash command","rm -rf build/"],
#  "complete":true,"question":"Do you want to proceed?",
#  "options":[{"n":1,"label":"Yes"},{"n":2,"label":"Yes, and don't ask again …"},
#             {"n":3,"label":"No, and tell Claude what to do differently (esc)"}],
#  "token":"9f2c4e…"}
herdr-app approval answer t3 approve --token 9f2c4e…
# {"terminal":"t3","answered":1,"label":"Yes"}
```

- `show` works only while the terminal's program reports `blocked`, and,
  when the program says what it is blocked on (OSC 7501 `kind`), only for
  `permission` or `question`: a sign-in (`auth`) or a kind this doesn't
  know is answered in its pane (exit 3). A program that says no kind is
  read from its screen alone. It reads
  the screen (`terminal.read`) and finds the numbered menu nearest the
  bottom: options `1.` to `N.` (or `1)`), two to nine of them, with at most
  six lines under them. A line indented deeper than the options (a wrapped
  label, a description) joins the option above it. Boxes and selection
  markers (`❯`, `›`) are ignored, and control, bidi and zero-width
  characters are dropped. The line above the options is the question. All
  the lines above that, up to the panel's top edge, are its context: for
  Claude Code's Bash prompt, the whole command. A top edge is a box's top
  (`╭───╮`, `┌───┐`) or a `───` rule that starts in column 0 and spans the
  pane (its width from `terminal.get`; without one, a rule at least as wide
  as everything under it). Claude Code draws its prompt's rule that way and
  indents what it shows, so a `---` line, a box's bottom, or a rule or box
  inside the command is never the edge. `complete` is false when no top
  edge was found on screen, so part of what is asked may be scrolled off.
- The token is a hash of the terminal, its turn (the core counts one each
  time the agent stops working, so the next prompt after an answer is a
  new turn), the context, the question and the options. A token names one
  prompt: an answer meant for `git status` can't approve a `git push
  --force` that shows the same question and options. The token is a
  freshness check, not a password: anything that can read the terminal can
  work it out.
- `answer` reads the status and the screen again and presses the option's
  number key (`terminal.keys`) only if the terminal is still blocked and
  the token still matches. The answer is a number, `approve` or `deny`.
  `approve` is the one option whose whole label is a one-time yes: `Yes`,
  `Allow`, `Approve`, `Proceed`, `Allow once`, `Yes, proceed`, `Yes, just
  this once` and a few more (`ONCE` in the code), ignoring case, a trailing
  key hint such as `(y)` and a full stop. Any other label, a grant this list
  doesn't know or one cut short, needs its number, so `approve` fails
  closed. When several match, only a bare `Yes`, `Allow` or `Approve`
  counts; otherwise give the number. `deny` is the one that starts with no,
  deny or reject and sets nothing for later (not "No, and block this host
  in the future"). On a prompt
  that isn't `complete`, `approve` and `deny` refuse: read it in its pane
  and give the number. A `question` takes only a number.
- `answer --wait <secs>` stops waiting for the key after that long and
  exits 4: the key stays queued in the core (see Limits). Without it,
  `answer` waits as long as someone keeps typing in the pane.
- It uses the core the window uses (the configured one, else the usual
  install places), and starts it if it isn't running.
- Output is JSON on stdout, errors on stderr. Exit codes: 0 done; 1 the core
  failed; 2 usage, or an answer that names no option (or `approve`/`deny`
  on a question); 3 nothing to answer, a kind this doesn't answer, or a
  stale token (run `show` again); 4 the key was pressed but the agent had
  moved on, or `--wait` ran out with the key still queued (see below).

## Limits

- The menu is read from the screen, so this works for agents that number
  their choices (Claude Code, Codex). A prompt without numbers has to be
  answered in its pane. The tests carry live captures of Claude Code
  2.1.328's Bash and Edit prompts (`src/fork/fixtures/`).
- Codex draws its approval panel with no top edge (its source,
  `codex-rs/tui/src/bottom_pane/approval_overlay.rs`, renders a title, the
  command and the options with no rule or box), so its prompts are never
  `complete`: `approve` and `deny` refuse, and a number works. Codex wasn't
  captured live.
- Key delivery goes through the core's quiet gate: if a person typed into
  that terminal in the last ~3 s, the key waits until they stop, however
  long they keep typing. `answer` waits with it so it can check the result.
  Stopping `answer` (Ctrl-C, a lost connection) does not cancel the key: the
  core still presses it 3 s after typing stops, on whatever prompt is
  showing then. The check runs before that wait, so a person who answers in
  the pane meanwhile moves the agent on, and the key lands on whatever
  comes next. `answer` catches some of these afterwards and exits with 4:
  when the core says the agent was no longer blocked, or its turn (the core
  counts one each time the program's status leaves `working`) had moved
  on. It can't catch an agent that goes from one prompt straight to the
  next without reporting `working` in between (Claude Code does with
  parallel tool calls: if a person answers the first prompt in the pane
  while the key waits, the key answers the second, and `answer` still
  exits 0 with the first prompt's label), and it can't take a key
  back. herdr-core would close this by cancelling a `terminal.keys` call
  when its connection closes (as it does for `terminal.send`) and by
  pressing only if the turn still matches; it does neither today, and that
  change belongs upstream.
- A program that reports no `kind` is read from its screen alone, so
  `approve` and `deny` work on any numbered menu it draws, including a
  question the model wrote to look like a permission prompt.
- Any process running as you can call this, as it already can call
  `hc keys`. An agent told to unblock another agent could approve that
  agent's prompt this way; only answer prompts you've read.
- Holding approvals in an agent's permission hook (Claude Code's
  `PermissionRequest`) until `answer` replies is not built:
  cameronsjo/herdr-app#37.
