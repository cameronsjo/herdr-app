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
starts with no space; make one, then open a page beside it:

```sh
herdr-app do space.new
herdr-app do browser.open --url "file://$PWD/scripts/browser-test-page.html"
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
  it: it uses a mock keychain.
