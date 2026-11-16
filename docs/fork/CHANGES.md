# Fork divergence ledger

Every way `cameronsjo/herdr-app` differs from `herdrdev/herdr-app`, newest first within each section. A sync resolver reads this before resolving; a feature PR adds its entry in the same PR. Rules for keeping the footprint small: `CLAUDE.md` § Merge-friendly by construction.

## Footprint

Output of `scripts/fork/seams` at each sync, so growth is visible.

| Date | Upstream base | Lines in upstream files | Files | Fork-only files | `FORK(` markers |
|---|---|---:|---:|---:|---:|
| 2026-11-15 | `b6546ce1` (before revival) | 1506 | 23 | 37 | 0 |
| 2026-11-15 | `b6546ce1` (seams extracted, `06f37e1`) | 769 | 23 | 45 | 46 |
| 2026-11-15 | `8e1208d` (synced, `bf9b67c`) | 761 | 23 | 45 | 51 |

## Unmarked seams

Upstream files the fork changes that can't carry a `FORK(` comment. A sync resolver re-applies these from the fork's diff.

- `defaults/keys.macos.json` (JSON): the `browser` key section (`cmd-=`, `cmd-+`, `cmd--`, `cmd-0`).
- `Cargo.lock`, `THIRD-PARTY-NOTICES`: regenerated (`cargo metadata`, `bun scripts/notices.ts`).
- `docs/commands.md`: generated from `src/commands.rs` (`bun scripts/docs.ts`).

## Widened upstream types

Exceptions to `CLAUDE.md` rule 4, each with its reason.

- `Content::Browser` (`src/layout.rs`): the browser block. cameronsjo/herdr-app#18 tracks saving it as a mod block instead.
- `Snapshot.browsers` (`src/mods.rs`): the mods' `browsers()` list. `Snapshot` is the one store every mod read goes through, and a parallel fork store would duplicate its revision tracking.

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

## Markdown viewer

Added 2026-11-16, as a mod (no Rust).

- `defaults/mods/markdown/` (fork-only): a pane that renders a markdown file with `TextView.markdown`, follows its saves (a folder watch plus a 10-second poll on this computer, a 5-second poll on another machine) and toggles to the raw source. Open it with `pane.split {direction, mod: "markdown", props: {file, on?}}`; `file` must be absolute.
- `defaults/mods/files/main.js`: `.md` and `.markdown` previews render instead of showing raw text, with an **Open** button that splits the markdown pane beside the focused terminal (`FORK(markdown)` lines).
- `defaults/AGENTS.md`: one `FORK(markdown)` sentence naming the mod in the list of shipped mods.

## Updates and automation

Added 2026-11-15; setup order and the routine's prompt: `docs/fork/weekly-sync.md`.

- Weekly upstream sync PRs from a cloud routine, which first reviews upstream's delta for supply-chain risk (Cameron's choice: that review stands in for his). A conflict-free sync with a clean review auto-merges (a merge commit) once CI passes; conflicts, a workflow change or a held review stay a draft; a security hold stops the sync and opens an issue. Trust model: `docs/fork/weekly-sync.md` § Trust.
- `.github/workflows/fork-browser.yml` (fork-only): the WebKit browser build's tests on macOS, the check upstream's CI can't run.
- `.github/dependabot.yml`: `tungstenite` only (the fork's one stand-alone crate); upstream's Actions entry kept, its version PRs off on the fork (one `FORK(browser)` line). Syncs copy `ci.yml`'s action pins into `fork-browser.yml`.
- `src/update.rs` `checks_core`: the fork's browser builds offer `herdr-core` updates as release builds do, except in an isolated dev run with its own app home (one `FORK(browser)` line).
- `scripts/fork/daily-update`: a launchd agent on Cameron's Mac builds each new `fork/master` that is a PR merge with `ci.yml` and `fork-browser.yml` green and installs it as the everyday browser build, never quitting a running one.

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
