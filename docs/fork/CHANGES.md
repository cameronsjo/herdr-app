# Fork divergence ledger

Every way `cameronsjo/herdr-app` differs from `herdrdev/herdr-app`, newest first within each section. A sync resolver reads this before resolving; a feature PR adds its entry in the same PR. Rules for keeping the footprint small: `CLAUDE.md` § Merge-friendly by construction.

## Footprint

Output of `scripts/fork/seams` at each sync, so growth is visible.

| Date | Upstream base | Lines in upstream files | Files | Fork-only files | `FORK(` markers |
|---|---|---:|---:|---:|---:|
| 2026-11-15 | `b6546ce1` (before revival) | 1506 | 23 | 37 | 0 |
| 2026-11-15 | `b6546ce1` (seams extracted, `06f37e1`) | 769 | 23 | 45 | 46 |

## Upstream syncs

Conflict decisions per sync, under a dated heading: file, what each side changed, what was kept and why.

### 2026-11-15: `b6546ce1` → `8e1208d` (262 commits, first sync)

Gate (macOS, load average 460 to 780 from other builds): `browser-webkit` 632 of 635 and default 624 of 625 passed. The two `files::tests` watcher timeouts pass when run alone. `macos_check::tests::checker_job_waits_for_demand_and_runs_once` (new upstream code, untouched by the fork) fails alone too, with `launchctl` itself past its 10s deadline; rerun it on an idle machine before merging.

No upstream renames or deletions of fork-touched files. 18 conflict hunks in 10 files; `log_safe`, `log_escaped` and `public_url` call sites: 104 before, 104 after.

| File | Upstream | Fork | Kept |
|---|---|---|---|
| `src/window.rs` | `share_seen` after sync; `space.new` machine default; `terminal.select*` and `terminal.scroll` commands | `fork_after_sync`, `fork_browser::structural`, the `browser.*` dispatch arm | both; the structural check re-applied to upstream's new line |
| `src/hc.rs` | `run` gained `size` and `display`, builds `params` | agent-browser env for this machine's terminals | upstream's `run`, the fork's env lines re-applied after its `env` |
| `src/mods.rs` | `publish` and `reader` take a revision key | `publish_browsers`, the `browsers()` function | both, the fork's two lines rewritten to the keyed API (`"browsers"`) |
| `src/main.rs` | `macos_check::startup` | agent-browser provider verb | both |
| `Cargo.toml` | `objc2`, `objc2-app-kit`, `objc2-foundation` as plain macOS dependencies | the same crates as optional browser dependencies | upstream's three lines verbatim; the fork's extra objc2 features moved into the `browser-pane` feature list (`"objc2-app-kit/NSAlert"`, …), so upstream's dependency lines carry no fork text |
| `defaults/mods/pane/main.js` | header border follows the frame's position (`paneFrame`, `layout`) | `BrowserBar`, header split into `head` + `terminal()` | both; upstream's changes are all in the header chain the fork keeps, none in the part moved to `terminal()` |
| `defaults/AGENTS.md` | header doc gained `scroll` | browser-pane note on `reveal: "hover"` | upstream's text plus the fork's note |
| `defaults/herdr.d.ts` | `terminal.scroll` | browser commands | both |
| `Cargo.lock` | | | upstream's, then `cargo metadata` |
| `THIRD-PARTY-NOTICES` | | | upstream's, then `bun scripts/notices.ts` (cargo-about 0.9.3 needs `--features cli` to install the binary) |

## Browser pane

Built 2026-10-30 to 2026-11-01 on `browser-prototype`. Design and security model: `docs/browser-prototype.md`; dev loop: `docs/browser-dev.md`; moving to upstream's official browser mod when it ships: `docs/browser-transition.md`.

- Engines behind cargo features: `browser` (CEF) and `browser-webkit` (WKWebView), both on the macOS-only `browser-pane`.
- Panes save as a `browser-prototype` mod block, so a stock app shows one unloaded pane instead of failing the layout, and upstream's future browser mod never reads this fork's blocks.
- Seams (upstream-owned files touched): see the footprint table. The window's browser code is in `src/window/fork_browser.rs`, the saved form of a browser block in `src/layout/fork_browser.rs`, and the control socket's owner-only checks in `src/browser/socket.rs`; `src/window.rs` (33 lines), `src/layout.rs` (6) and `src/control.rs` (2) keep only `FORK(browser)` call lines.
- `Content::Browser` still widens upstream's `Content` enum, and `Content` (de)serializes through the fork's `Saved`, which must mirror upstream's serde attributes. cameronsjo/herdr-app#18 tracks removing the variant.

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
