# Browser panes (prototype, macOS)

A browser pane shows a real Chromium page in a block: CEF (the Chromium
Embedded Framework) in windowed mode, one native view per page over the
window. It's behind the Cargo feature `browser`, off by default; other
builds show a placeholder where a browser block is.

## Build and run

```sh
just browser-build   # target/browser-dev/Herdr Dev.app: the app, CEF's framework, 5 helper apps
just browser-dev     # the same, then runs it on a scratch home and its own core
```

Both need `cmake` and `ninja` (`brew install cmake ninja`). CEF on macOS only
starts from such a bundle, so `cargo run` can't show a page.

`CEF_PATH` is where CEF's binary distribution (about 600 MB) is downloaded
once and kept. It must be outside the repo; the recipes default to
`~/.cache/herdr-app/cef`. A bare `cargo build --features browser` without it
is refused: `build.rs` says why, and `.cargo/config.toml` points CEF's
download at an address that doesn't resolve, so nothing lands in `target/`.
To build by hand:

```sh
CEF_PATH=~/.cache/herdr-app/cef CEF_DOWNLOAD_URL=https://cef-builds.spotifycdn.com \
  cargo build --features browser --bin herdr-helper --bin herdr-app
```

## Try it

`just browser-dev` prints its scratch folder (`/tmp/hbd.*`) and writes
`<scratch>/herdr-app`, which talks to that window only. The scratch home
starts with no space; make one, then open a page beside it. Pages open
over http, https or `about:blank` only (no `file:`), so serve the test
pages first with `just browser-fixture` (in another terminal; it serves
`scripts/` on `http://127.0.0.1:8765`):

```sh
herdr-app do space.new
herdr-app do browser.open --url http://127.0.0.1:8765/browser-test-page.html
herdr-app do browser.open --url example.com --place tab
herdr-app ls
herdr-app do browser.navigate --browser b1 --url https://example.com/
herdr-app do browser.devtools --browser b1
herdr-app do browser.close --browser b2
```

Run those through `<scratch>/herdr-app`, not your installed `herdr-app`,
which drives your real window.

`scripts/browser-test-page.html` has what to try by hand: typing with dead
keys and an IME, a `<select>`, a long list to scroll, a right-click menu and
a new-window link.

## How it behaves

- A browser block is `{ "browser": "b1", "url": "..." }`, as a pane in a tab
  (saved with the tab, so it reopens at its last page after a restart) or as
  a block in a layout. Floating blocks refuse it: a native view can't float
  over GPUI's drawing.
- Each frame the page's view takes its block's bounds; a block that isn't
  painted (another tab, a zoomed pane, a hidden column) hides its page.
- GPUI can't draw over a page, so a page hides while any overlay overlaps
  its block: a dialog (its dimmed backdrop covers every page), a popup (the
  palette, a menu), a toast, a mod window, the update pill, a floating
  block, the pane grip and the drop preview while you drag a pane. Its block
  shows the page's title and url on the theme's `surface` meanwhile; the
  page keeps its scroll position and comes back when the overlay goes. A
  hidden page can't have the keyboard: Herdr takes it, and the page gets it
  back only if it should still have it then. The log names each change
  (`b1 hidden under overlay popup frame-palette`). Popups and tooltips a mod
  draws inside its own view (not through Herdr's overlays) still draw under
  a page.
- Clicking a page gives it the keyboard and makes its pane the focused one.
  A layout block's page keeps the keyboard until a click elsewhere in Herdr
  or a focus change. A dialog or popup (the palette, a menu, a rename
  prompt) takes the keyboard while it's open. So does a control you click
  (the Changes panel's commit box, a floating terminal), until it loses
  focus, the focused pane changes, or you click a page. A page that asks for
  focus on its own (CEF does when it creates one, and on loads) is refused
  unless it's the page that should have the keyboard. Closing the page that
  has the keyboard gives it back to Herdr.
  While a page has the keyboard, `ctrl-alt-shift-escape` and the keys bound in
  keys.json's `global` section (and the shown layout's) still go to Herdr,
  from the last keys.json that loaded. `send:` keys stay with the page;
  `app.hide` and friends hide the app as usual.
- Browser ids (`b1`, `b2`, ...) are never reused while the app runs, so an id
  you hold never names another page.
- A popup or a `target="_blank"` link opens in the same pane.
- No remote-debugging port is opened. `browser.devtools` opens CEF's own
  DevTools window.
- The profile (cookies, cache) is `<app home>/browser/profile`; delete it
  while the app is closed to start fresh. Don't sign into real accounts in
  it: it uses a mock keychain. The app home, `browser/` and both profiles
  are created `0700`; the control socket is `0600` and drops a peer running
  as another user.
- One URL gate covers `browser.open`, `browser.navigate`, a restored
  block's url, popups and an agent's `Page.navigate`: `http`, `https` and
  `about:blank` only. A popup from a frame of another origin needs a click.
  Logs keep a url's scheme, host and path only.

## Agents drive a page (agent access)

An agent in a Herdr terminal can drive a browser pane with
[`agent-browser`](https://github.com/vercel-labs/agent-browser), once the
person allows it for that pane:

1. `just browser-fixture` (another terminal), then
   `herdr-app do browser.open --profile agent --url http://127.0.0.1:8765/browser-fixture.html`.
   `profile: "agent"` is a separate Chromium profile
   (`<app home>/browser/agent-profile`); only such a browser can be driven.
   Its header says "Test profile: don't sign into real accounts." Downloads
   are refused in it.
2. Click **Allow agent** in that header. Nothing else turns agent access on:
   `herdr-app do browser.agent_access --on true` and a mod's `run()` are
   refused, because an agent can reach both. Turning it off works from
   anywhere (the header's **Stop agent**, or
   `herdr-app do browser.agent_access --browser b1 --on false`). Access is
   never saved; closing the browser or quitting turns it off. While any
   browser has access on, a strip along the bottom of the window names it,
   with its own **Stop agent**. Herdr draws the header and the strip from the
   open browser, not the layout, so hiding or editing a block, or switching
   tab or space, doesn't hide them.
3. In a terminal Herdr started (any agent pane), `agent-browser snapshot -i`,
   `agent-browser click @e5`, `agent-browser fill @e2 "Ada"`. The header
   names the terminal that last attached, as it claimed itself.

How it works: Herdr gives its local terminals `AGENT_BROWSER_PROVIDER=herdr`,
`AGENT_BROWSER_PLUGINS` (Herdr's own provider, `herdr-app
__agent-browser-provider`, first) and an `AGENT_BROWSER_SESSION` of their
own. The provider asks the window over the control socket for the caller's
browser (`HERDR_BROWSER=<id>` picks one in the caller's space; else the
browser in its tab, else the one in its space the person focused last) and
gets `ws://127.0.0.1:<port>/<token>`: a 256-bit token for that one page.
Herdr's proxy passes CDP messages from that socket to the page's in-process
DevTools session through an allowlist (`src/browser/agent.rs`): no
`Target.attachToBrowserTarget`, no cookie reads, no file inputs, no
downloads; cookie headers are stripped from network events. It refuses any
upgrade with an `Origin` header or a `Host` other than `127.0.0.1:<port>`,
and answers anything but an upgrade with a bare 404. Turning access off, a
new attach for the same page, closing the browser and quitting end the
token and the connection: Herdr shuts the socket itself, so an agent that
stopped reading can't hold it open. A connection's own end (the agent
closed it, a write it didn't take in 5 s, or more than 1024 messages or
32 MiB waiting for it) ends its token too; agent-browser asks the provider
for a new one. Whenever a connection ends, the page reopens at its url so
nothing the agent set (headers, overrides) stays. The reopen keeps the
profile's storage (cookies, local storage).
Every on, off and attach is in the log, never the token or port.

agent-browser hides a provider's error text (`Plugin 'herdr' returned
success=false`); `agent-browser plugin run herdr herdr.check` says why
(for example `agent access is off for b1: ask the person to turn on agent
access for b1`). Terminals started before this build, or by `hc run`, don't
have the environment; `AGENT_BROWSER_PROVIDER=` opts a terminal out.
agent-browser's daemon reads the provider's environment once, when its
session starts, so to pick another browser run
`export HERDR_BROWSER=<id> AGENT_BROWSER_SESSION=$AGENT_BROWSER_SESSION-<id>`
before the next agent-browser command.

What this does and doesn't protect (state this wherever the prototype is
shown):

> Agent access is a consent switch for a cooperating agent, not a security
> boundary against software running as you. While it is on, any process
> running as your user can get the page's CDP token through Herdr's control
> socket and act as you on whatever site that pane shows: run script in the
> page, click, type, and read what the page shows. Any process running as
> you can also read this profile's cookies from disk, because the prototype
> encrypts them with Chromium's public test key (`--use-mock-keychain`).
> Other users on the machine and web pages cannot reach the agent
> connection: it needs a 256-bit token, accepts only loopback connections
> without an `Origin` header, and ends when you turn access off, close the
> browser, or quit. Use this browser only with test accounts, never your own.

Signing: the dev bundle is ad hoc signed without the hardened runtime, so
library validation is off (`DYLD_INSERT_LIBRARIES` works on the browser
process at its next launch, same user only), and Chromium logs a failed
signature check of its own process (`process_requirement.cc … -67030`);
whether Chromium then skips checks between its processes is unverified.

### Dev-only switch (automated tests only)

A build with the switch drops the consent property: an agent that can
relaunch the app (`herdr-app do app.quit`, then the app with the variable
set) can turn its own access on. So only an explicit test build has it:
`scripts/browser-dev-bundle --dev-agent-access` builds a debug bundle with
the `dev-agent-access` Cargo feature. `just browser-dev` and
`just browser-build` leave it out, and release builds ignore it (the code is
behind `debug_assertions` and the feature; the test
`builds_without_the_dev_feature_have_no_dev_switch` pins it).

Started with `HERDR_DEV_AGENT_ACCESS=1` in its own environment, such a
build lets `herdr-app do browser.agent_access --on true` turn access on, so
an agent can test the whole path without a person's click. It is read once
at launch and removed from the environment (terminals never inherit it),
logged as `DEV SWITCH ON` at startup and on every use, and every agent
header and the window's agent strip say "DEV SWITCH: the control socket can
turn agent access on". Show the prototype on a bundle without it
(`just browser-dev --release`).
