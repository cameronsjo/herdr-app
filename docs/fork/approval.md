# Answering approvals from outside the pane

cameronsjo/herdr-app#17. A blocked agent's prompt can be answered from a
shell, a mod, another client or a phone, without opening its pane and
without pressing keys blind.

```sh
herdr-app approval show t3
# {"terminal":"t3","kind":"permission","question":"Do you want to proceed?",
#  "options":[{"n":1,"label":"Yes"},{"n":2,"label":"Yes, and don't ask again …"},
#             {"n":3,"label":"No, and tell Claude what to do differently (esc)"}],
#  "token":"9f2c4e…"}
herdr-app approval answer t3 approve --token 9f2c4e…
# {"terminal":"t3","answered":1,"label":"Yes"}
```

- `show` works only while the terminal's program reports `blocked`. It reads
  the screen (`terminal.read`) and finds the numbered menu nearest the
  bottom: options `1.` to `N.` (or `1)`), at least two, on consecutive lines,
  with at most six lines under them. Boxes and selection markers (`❯`, `›`)
  are ignored. The line above the options is the question.
- The token is a hash of the terminal, the question and the options, so it
  names one prompt on one terminal.
- `answer` reads the status and the screen again and presses the option's
  number key (`terminal.keys`) only if the terminal is still blocked and
  the token still matches. Otherwise it refuses and says to run `show` again.
  The answer is a number, `approve` (the first option that starts with yes,
  allow or approve) or `deny` (the first that starts with no, deny or
  reject).
- Output is JSON on stdout. Errors go to stderr with exit code 1, and usage
  errors exit with 2.

## Limits

- The menu is read from the screen, so this works for agents that number
  their choices (Claude Code, Codex). A prompt without numbers has to be
  answered in its pane.
- Key delivery goes through the core's usual quiet gate: if a person typed
  into that terminal in the last ~3 s, the key waits until they stop. The
  check runs before that wait, so a person who answers in the pane during
  those seconds can change the prompt the key lands on.
- Two prompts with the same question and options on the same terminal share
  a token. They ask the same thing, so the answer means the same.
- Holding approvals in an agent's permission hook (Claude Code's
  `PermissionRequest`) until `answer` replies is not built; see the
  follow-up issue.
