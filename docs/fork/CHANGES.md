# Fork divergence ledger

Every way `cameronsjo/herdr-app` differs from `herdrdev/herdr-app`, newest first within each section. A sync resolver reads this before resolving; a feature PR adds its entry in the same PR. Rules for keeping the footprint small: `CLAUDE.md` § Merge-friendly by construction.

## Footprint

Output of `scripts/fork/seams` at each sync, so growth is visible.

| Date | Upstream base | Lines in upstream files | Files | Fork-only files | `FORK(` markers |
|---|---|---:|---:|---:|---:|
| 2026-11-15 | `b6546ce1` (before revival) | 1506 | 23 | 37 | 0 |

## Upstream syncs

Conflict decisions per sync, under a dated heading: file, what each side changed, what was kept and why.

## Browser pane

Built 2026-10-30 to 2026-11-01 on `browser-prototype`. Design and security model: `docs/browser-prototype.md`; dev loop: `docs/browser-dev.md`; moving to upstream's official browser mod when it ships: `docs/browser-transition.md`.

- Engines behind cargo features: `browser` (CEF) and `browser-webkit` (WKWebView), both on the macOS-only `browser-pane`.
- Panes save as a `browser-prototype` mod block, so a stock app shows one unloaded pane instead of failing the layout, and upstream's future browser mod never reads this fork's blocks.
- Seams (upstream-owned files touched): see the footprint table; `src/window.rs` carries most of them.

## Ported from the TUI fork

Features carried over from `cameronsjo/herdr` (archived). One entry per feature when it lands, naming the original ledger section and whether it shipped as a mod or Rust.

Gap check 2026-11-15 against herdr-app `ce9a3cf` and herdr-core. To port: cameronsjo/herdr-app#8 to #15 (seven mods, one app-Rust). Not ported, because upstream already covers them:

| TUI fork feature | Upstream equivalent |
|---|---|
| Pin an agent (cameronsjo/herdr#96) | `terminal.pin` (`src/commands.rs:48`), Pinned group in the agents mod |
| Workers under their launcher (`--parent`, cameronsjo/herdr#98) | `hc run --parent` (herdr-core `docs/commands.md:498`), nested sub-agent rows |
| Peek a pane (cameronsjo/herdr#95) | `terminal.peek` (`src/commands.rs:63`) |
| `pane focus <id>` (cameronsjo/herdr#94) | `pane.focus {pane\|terminal}` (`src/commands.rs:60`) |
| Drag a pane to swap | grip drag, `pane.swap {target}` (`src/commands.rs:62`) |
| `send-text --paste` (cameronsjo/herdr#121) | `hc send` pastes, then presses Enter |
| Prompt delivery report (cameronsjo/herdr#102) | `SendResult.started` and `status` (herdr-core `schema/hc.openrpc.json`) |
| `agent start --exec` (cameronsjo/herdr#110) | status comes from the agent's own OSC 7501 report, not its process name |
| Palette recents and keywords, confirm destructive actions, type-submit, skill directory | built in |

TUI-only (server, renderer, or fork tooling): cold-restore labels, `rows_by_agent` warning, Codex hooks, sync and release scripts.
