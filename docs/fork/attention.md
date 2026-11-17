# The attention queue

With many agents running, the agents list is where you look to see who needs you. The fork makes it say why, and catches two cases it used to miss (cameronsjo/herdr-app#16).

## Why an agent needs you

The "Needs you" badge now says why, when the agent said: **Approval**, **Question** or **Sign in** for a blocked agent that reported `kind=permission`, `question` or `auth` over OSC 7501, and **Error** for an agent that reported `state=error`. herdr-core already keeps both facts; the app used to show them all as "Needs you". Mods read the same thing from `agents()`'s `reason`.

## The attention label

Some turns end waiting on you in prose: a question, "please run X", a usage limit. The agent reports `done` or `idle`, so the turn sinks among finished agents. A program that reads the end of a turn (a classifier, a hook) can flag it:

```sh
hc label t7 herdr.attention="asks you to run make deploy"
```

While the label is set and the agent isn't working, the app shows it as blocked: it sorts with blocked agents, its dot and its space's roll-up say so, the badge shows the label's text (at most 40 characters), and the needs-you sound and toast fire once. A label written just after a turn ends rings twice: Done for the finish, then the needs-you alert when the label lands. herdr-core's own status doesn't change, so `hc wait`, `hc list` and other automation never see it. The app removes the label when the agent starts working again; write it empty (`herdr.attention=`) to remove it yourself, which counts as dealt with: no Done alert, and the agent isn't shown as an unread finish.

Labels are shared, so every Herdr client sees it, but only this fork acts on it.

## Stalls

An agent can stay `working` forever when its integration misses the end of a turn. Turn on the stall check in `<app home>/fork.json` (the app home is `~/.config/herdr-app` by default):

```json
{ "stall_minutes": 10 }
```

Then a working agent whose screen and status report both stay the same for that long shows **Stalled** in place of its spinner, until either changes. The app reads each working agent's screen once a minute for this (`terminal.read`). A screen that can't be read counts as a sign of life, never as a stall. Off by default; the file is read again within 30 seconds of a change. Mods read it as `agents()`'s `stalled`.

The check sees only change. A spinner or an elapsed-time counter on screen keeps a hung agent from ever stalling, and a quiet command that runs longer than `stall_minutes` (a silent test suite) shows as stalled while it is fine.
