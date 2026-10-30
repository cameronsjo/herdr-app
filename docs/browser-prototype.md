# Browser panes: a prototype to discuss

This branch puts a real Chromium page in a Herdr pane on macOS, using CEF
(the Chromium Embedded Framework) as a native view over the window, and lets
a coding agent in a Herdr terminal drive that page with
[`agent-browser`](https://github.com/vercel-labs/agent-browser) once the
person allows it. It is a prototype to start a conversation, not a pull
request. Everything sits behind a Cargo feature (`browser`) that is off by
default; a default build has no CEF in it.

The demo, in six lines:

1. `herdr-app do browser.open --profile agent --url http://127.0.0.1:8765/browser-fixture.html` opens a local test page beside a terminal.
2. The pane's header says "Test profile: don't sign into real accounts." and "Agent access off · b1".
3. The person clicks **Allow agent…**; a macOS dialog asks "Let agents drive this browser?"; the person clicks **Allow**.
4. In the terminal, `agent-browser snapshot -i` lists the page's fields and buttons.
5. `agent-browser fill @e2 "Ada"` and `agent-browser click @e7` fill and submit the form while the person watches.
6. **Stop agent** ends the agent's connection and reloads the page.

Who built it and how: a coding agent (Claude) wrote the code and these docs
under Cameron's direction. Cameron chose the approach and approved the plan.
Separate agent reviewers attacked the agent-access design over several
rounds, and each round's findings were fixed or written down as limits.
Cameron has not yet done the manual checks that need a person at the
keyboard (typing, IME, menus, the dialog's buttons; listed in
[Manual checks](#manual-checks-pending)). Nothing here has run outside this
one Mac.

## Can's three questions

From `AGENTS.md`, "Before you grow the app's core: ask Can".

**1. Does the app itself act on it?** Yes. A browser pane is a native
`NSView` that the app has to create, place on its block every frame, hide
when the block isn't shown, and close in order on quit (CEF closes pages
asynchronously, then shuts down). The app also has to decide who gets the
keyboard: a page that has focus takes keys before GPUI does, so the app
watches AppKit's first responder and sends its own global keys to itself
first.

**2. Can only the app see it?** Mostly. Where each block lands this frame,
which overlays (palette, menus, dialogs, toasts, floating blocks, the drag
grip) cover it this frame, and which view AppKit has as first responder are
known only inside the app's render pass. A mod sees none of them.

**3. Can mods not do it correctly, even with care?** A mod can't host an
`NSView` or start CEF: CEF needs a hook in the app's `NSApplication` class
and its own helper apps in the `.app` bundle. For agent access there is a
second reason: agents can write mods, layouts, keys and themes and can reach
the control socket, so a consent switch reachable from any of those is not
consent. In this prototype only an app-modal macOS dialog turns access on.

## Smallest core asks

If browser panes are wanted at all, these three are what the core would
need. The rest could live elsewhere.

1. **CEF's app-class hook.** CEF requires `NSApp` to report whether
   `sendEvent:` is running (`CefAppProtocol` in `cef_application_mac.h`).
   The prototype adds three methods to GPUI's `GPUIApplication`
   (`vendor/gpui-pre-macos/src/platform.rs`, 34 lines, `HERDR:` marked):
   two report and set a flag, and `sendEvent:` sets it around the call.
   Without CEF nothing reads the flag. The protocol itself is added at
   run time, after CEF's framework loads.
2. **A block that hosts a native view.** Today this is a layout block,
   `{ "browser": "b1", "url": "…", "profile"?: "agent" }`, with bounds and
   visibility reported each frame, and key and focus routing. The shape is
   open: a `browser` block as here, or something more general (a mod pane
   with props, or a "native view" block that a browser is one kind of).
   See the compatibility gap below before choosing.
3. **An overlay hook.** GPUI draws in one Metal layer and can't draw over a
   native view, so Herdr's overlays report their rectangles each frame and a
   page under one is hidden (its block shows the title and url meanwhile).
   Overlays a mod draws inside its own view don't report, and still draw
   under a page.

Agent access, if wanted, is a fourth piece that has to sit in the app for
the consent reason above: the native dialog, the per-page token proxy
(`src/browser/proxy.rs`, `src/browser/agent.rs`) and the provider that
`agent-browser` calls. It can be left out; the three asks stand without it.

Optional, and mod-shaped or later work: a URL bar, browser tabs, docked
DevTools (today `browser.devtools` opens CEF's own window), design mode
(pick an element and send it to the agent), pixel snapshots for overlays,
a clear-data command, Linux and Windows.

Size of the change against `origin/master` (`git diff --shortstat
origin/master...HEAD -- . ':!docs'`): 34 files, about 6,800 lines added, 5,353
of them in `src/browser/` (tests included). The other 24 files (`Cargo.lock`
included) gain about 1,400 lines, 565 of them in `src/window.rs`.

## Facts, and how each was measured

| Fact | Value | How measured |
|---|---|---|
| App size, plain release | 62 MB `.app`, 25.8 MB `.tar.gz` | `scripts/package` (default features), then `du -sh Herdr.app` and the tarball's size |
| App size, browser release | 387 MB `.app`, 172 MB `.tar.gz` | `scripts/browser-dev-bundle --release`, then `du -sh "Herdr Dev.app"`; `tar -czf` of it |
| Where the browser size goes | CEF framework 325 MB, app binary 59 MB, 5 helper apps 468 KB each | `du -sh` on each bundle part |
| CEF pin | `cef` crate `=156.2.0` (`156.2.0+156.0.14` in `Cargo.lock`), CEF 156.0.14 | `Cargo.toml`, `Cargo.lock` |
| Chromium version | 156.0.8078.48 | `CEF_VERSION` in the distribution's `include/cef_version.h`; the framework's `Info.plist` carries only `156.0.14.0` and branch 8078 |
| Signing | ad hoc, no hardened runtime, no team | `codesign -dv` on the app and a helper: `flags=0x2(adhoc)`, `Signature=adhoc`, `TeamIdentifier=not set`; `codesign --verify --deep --strict` exits 0 |
| Notarization | not attempted | — |
| Sandbox | renderer, GPU and utility processes run sandboxed with ad hoc signing | checked once, in the first spike (each helper process reported sandboxed); not re-checked on this build |
| Notices | not covered | `just notices` exits 0 and leaves `THIRD-PARTY-NOTICES` unchanged; it has no `cef` or `cef-dll-sys` entry and nothing from Chromium's `CREDITS.html`; `licenses/inventory.json` has no CEF entry |

Signing notes. With ad hoc signing Chromium logs, once per launch,
`process_requirement.cc … Signature validation of current process failed …
(-67030)`. Nothing visible breaks; whether Chromium then skips checks
between its own processes is unverified. Library validation is off without
the hardened runtime, so `DYLD_INSERT_LIBRARIES` works on the browser
process (same user only). A Developer ID build needs per-helper entitlements
(JIT for the Renderer and GPU helpers), which are untested. The repo's
existing signing covers the app, not CEF's helpers.

Packaging notes. CEF on macOS only starts from an `.app` bundle with its
framework and helpers, so `cargo run` can't show a page; `just
browser-build` builds `target/browser-dev/Herdr Dev.app`. CEF's binary
distribution (about 600 MB) downloads once into `CEF_PATH`, outside the repo.

## Security model, in short

Detail, including the full residual statement: [Agents drive a
page](browser-dev.md#agents-drive-a-page-agent-access) in
`docs/browser-dev.md`.

- **Agent profile only.** Agents can drive only a browser opened with
  `profile: "agent"`, a separate Chromium profile under `<app
  home>/browser/agent-profile`. Its header always says "Test profile: don't
  sign into real accounts." Its cookies are encrypted with Chromium's public
  test key (`--use-mock-keychain`), so any process running as you can read
  them. Don't sign into real accounts in this profile.
- **Native consent.** Access is off by default and never saved. Only
  **Allow** in an app-modal `NSAlert` turns it on; the control socket, mods
  and keys can only turn it off. Cancel is the default button, and Allow
  stays disabled for half a second.
- **No remote-debugging port.** Agents get `ws://127.0.0.1:<port>/<token>`
  with a 256-bit token for one page. Herdr's proxy passes CDP messages to
  that page's in-process DevTools session through a method allowlist: no
  browser-wide targets, no CDP cookie methods, no file inputs, no
  downloads, and cookie and auth headers stripped from network events. It
  refuses any upgrade with an `Origin` header or a foreign `Host`.
- **Revocation.** Turning access off, closing the browser, or quitting ends
  the token and closes the socket from Herdr's side; the page then reopens
  at its url so nothing the agent set stays.
- **Residuals.** While access is on, any process running as you can get the
  token through the control socket (same user, socket `0600`). A process
  with macOS Accessibility permission can click **Allow**, and any app can
  draw its own window over the dialog. agent-browser's own stream server (a
  loopback websocket with no token, in agent-browser's code) lets any local
  user, and any `localhost` or `file://` page in any browser, watch and
  drive an attached pane; checked against agent-browser 0.39.1, later
  versions may differ. The in-window markers (header, bottom strip) are
  information, not the control: a mod or theme can cover or recolor them.

## Known gaps

- **Compatibility of saved layouts.** `Content` in `src/layout.rs` is an
  untagged enum, so a tab document that holds a `{ "browser": … }` block
  can't be read by a released herdr-app on the same core
  (`mirror_sync::read` gives "Cannot read this machine's layout: …"). Found
  by reading the code, not reproduced. Ways out: store a browser pane as a
  mod pane with props, or ship a `Content` that tolerates unknown blocks
  first. This needs Can's call before anything ships.
- **Global keys win over a focused page.** While a page has the keyboard,
  keys bound in keys.json's `global` section (and the shown layout's) still
  go to Herdr, so on macOS `cmd-w` closes the tab and `cmd-1`…`cmd-9`
  switch tabs instead of reaching the page. `send:` keys stay with the page.
- **CEF logs "Timeout of new browser info response"** after a page fails to
  load (seen in the logs of two test runs, right after each load error).
  Its effect was not investigated.
- **Takeover's first call fails.** When a second agent attaches to the same
  page, the first agent's connection ends and the page reopens; the new
  agent's first request gets `herdr: the page isn't there right now (opening
  or reopening): try again`. A retry half a second later works.
- **The dialog's answer-time re-check is weak.** It reads Herdr's record of
  the page, and page events from while the dialog was open are applied only
  after the answer, so a page that moved to another site during the dialog
  gets access at its new site.
- **CEF's message pump is untested during menu tracking** and live window
  resizing. The pump runs on a 33 ms tick on GPUI's foreground executor.
- **Mod tooltips and in-mod popups draw under a page.** Only overlays Herdr
  registers hide a page.
- **The id check is shape only.** An agent-profile block needs an id shaped
  like Herdr's (`b` and 1-9 digits), which keeps agent text out of the
  dialog, but any such id opens, including one Herdr never gave out.
- **A frame of lag while resizing.** The view follows its block every frame;
  GPUI has no layout-change event or hosted native view.
- **`herdr-app shot` can't see pages.** It renders GPUI only, so a shot shows
  the placeholder where a page is. The demo needs a screen recording.
- **macOS only.** Other platforms show a placeholder in a browser block.
- **Notices.** As above: no CEF or Chromium notices yet.
- **Terminals started before this build, or by `hc run`,** don't get the
  agent-browser environment.

## Try it

Needs macOS on Apple silicon, `cmake` and `ninja` (`brew install cmake
ninja`), and for the agent part `agent-browser` on `PATH` (tested with
0.39.1). Everything runs on a scratch home and its own herdr-core session,
not your installed Herdr.

```sh
just browser-fixture            # terminal 1: serves scripts/ on http://127.0.0.1:8765
just browser-dev --release      # terminal 2: builds, prints its scratch folder, runs the app
```

Then, with `<scratch>` the folder it printed:

```sh
<scratch>/herdr-app do space.new
<scratch>/herdr-app do browser.open --profile agent --url http://127.0.0.1:8765/browser-fixture.html
```

Click **Allow agent…** in the browser pane's header, wait half a second,
click **Allow** in the dialog. Then, in the terminal pane Herdr opened in
that space:

```sh
agent-browser snapshot -i          # lists the page; on the fixture: Name e2, Save e7, Add item e5
agent-browser fill @e2 "Ada"
agent-browser click @e7            # the page shows "Saved Ada (red)"
agent-browser click @e5            # a fourth list item; the title becomes "Items: 4"
```

Use the refs `snapshot` printed if they differ. If a command prints only
`Plugin 'herdr' returned success=false`, run `agent-browser plugin run herdr
herdr.check` for the reason. Quit the app with `cmd-q`; the scratch folder
keeps `app.log`.

## Demo recording (60–90 s)

Setup before recording: run the two commands in [Try it](#try-it), then
`<scratch>/herdr-app do space.new`. Make the window wide enough for a
terminal and a page side by side, and clear the terminal.

1. Press `cmd-shift-5`, choose "Record Selected Window", click the Herdr Dev
   window.
2. In the Herdr terminal, run `<scratch>/herdr-app do browser.open --profile
   agent --url http://127.0.0.1:8765/browser-fixture.html`. The page opens
   on the right.
3. Click the page's Name field and type a few letters, to show it is a
   real page. Clear it.
4. Point at the header: "Test profile: don't sign into real accounts." and
   "Agent access off · b1".
5. In the terminal, run `agent-browser plugin run herdr herdr.check`. It
   says access is off for b1.
6. Click **Allow agent…**. The dialog names b1, the agent profile and
   `http://127.0.0.1:8765`. Wait a moment, click **Allow**. The header turns
   to "Agent access ON" and a strip appears along the bottom.
7. Run `agent-browser snapshot -i`, then `agent-browser fill @e2 "Ada"`,
   then `agent-browser click @e7`. The page shows "Saved Ada (red)".
8. Run `agent-browser click @e5`. A fourth item appears.
9. Click **Stop agent**. The page reloads and the strip goes.
10. Run `agent-browser plugin run herdr herdr.check` again: access is off.
11. Stop the recording from the menu bar.

## Manual checks pending

These need a person and have not been done yet:

- Typing in a page, including a dead key (`option-e e`) and a Japanese IME
  commit; trackpad momentum scrolling; the right-click menu; a `<select>`
  popup (`scripts/browser-test-page.html` has each).
- `ctrl-alt-shift-escape` and a global key while a page has the keyboard.
- The palette, a right-click menu, a dialog, a toast and a floating block
  each drawing fully over a page, with no flash.
- The dialog: it names the same `bN` as the header; Return, Esc and Cancel
  leave access off; Allow stays disabled for half a second; Allow turns
  access on; Stop agent turns it off.
- That the app's other work waits while the dialog is open (read from the
  code, not measured).
