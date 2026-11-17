# Answering approvals from outside the pane

cameronsjo/herdr-app#17. A blocked agent's prompt can be answered from a
shell, a mod, another client or a phone, without opening its pane and
without pressing keys blind.

```sh
herdr-app approval show t3
# {"terminal":"t3","kind":"permission","context":["Bash command","rm -rf build/"],
#  "question":"Do you want to proceed?",
#  "options":[{"n":1,"label":"Yes"},{"n":2,"label":"Yes, and don't ask again …"},
#             {"n":3,"label":"No, and tell Claude what to do differently (esc)"}],
#  "token":"9f2c4e…"}
herdr-app approval answer t3 approve --token 9f2c4e…
# {"terminal":"t3","answered":1,"label":"Yes"}
```

- `show` works only while the terminal's program reports `blocked`. It reads
  the screen (`terminal.read`) and finds the numbered menu nearest the
  bottom: options `1.` to `N.` (or `1)`), two to nine of them, with at most
  six lines under them. A line indented deeper than the options (a wrapped
  label, a description) joins the option above it. Boxes and selection
  markers (`❯`, `›`) are ignored, and control and bidi characters are
  dropped. The line above the options is the question. The lines above
  that, up to the panel's top edge (at most 12), are its context: for
  Claude Code's Bash prompt, the command.
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
  `approve` is the one option that starts with yes, allow or approve and
  grants nothing beyond this once: an option that says always, don't ask
  again, session, all, bypass or auto-accept is never `approve`. When
  several plain options match, only a bare `Yes` counts; otherwise give the
  number. `deny` is the one that starts with no, deny or reject.
- It uses the core the window uses (the configured one, else the usual
  install places), and starts it if it isn't running.
- Output is JSON on stdout, errors on stderr. Exit codes: 0 done; 1 the core
  failed; 2 usage, or an answer that names no option; 3 nothing to answer
  or a stale token (run `show` again); 4 the key was pressed but the agent
  had moved on (see below).

## Limits

- The menu is read from the screen, so this works for agents that number
  their choices (Claude Code, Codex). A prompt without numbers has to be
  answered in its pane.
- Key delivery goes through the core's quiet gate: if a person typed into
  that terminal in the last ~3 s, the key waits (up to 60 s here) until
  they stop. The check runs before that wait, so a person who answers in
  the pane meanwhile moves the agent on, and the key lands on whatever
  comes next. `answer` sees this afterwards (the turn changed) and exits
  with 4, but it can't take the key back.
- Any process running as you can call this, as it already can call
  `hc keys`. An agent told to unblock another agent could approve that
  agent's prompt this way; only answer prompts you've read.
- Holding approvals in an agent's permission hook (Claude Code's
  `PermissionRequest`) until `answer` replies is not built; see the
  follow-up issue.
