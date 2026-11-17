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

## Attention queue

Added 2026-11-17 (cameronsjo/herdr-app#16, Linear CAM-921). What it does: `docs/fork/attention.md`. Rules in `src/fork/attention.rs`, the window's part in `src/window/fork_attention.rs`.

- Seams: `src/main.rs` `mod fork`; `src/window.rs` `mod fork_attention`, the shown word in `tend_terminals` (`Queue::word`), the `looking` line after it (a label cleared while the agent rested raises no Done alert), the tick call there, and `fork_agent_fields` on each `agents()` entry; `defaults/mods/agents/main.js` the badge text (`needsYou`) and the Stalled badge; `defaults/herdr.d.ts` the three `AgentInfo` fields. All marked `FORK(attention)`.
- The label is `herdr.attention`, in Herdr's shared label namespace on purpose, so other clients can learn it. If upstream ever gives that label another meaning, rename the fork's.
- `stall_minutes` lives in `<app home>/fork.json`, not `settings.json`, so upstream's settings struct stays as it is.

## Answering approvals

Added 2026-11-17 (cameronsjo/herdr-app#17, Linear CAM-921). What it does: `docs/fork/approval.md`. All of it is in `src/fork/approval.rs`.

- Seam: `src/main.rs` routes `herdr-app approval ...` to `fork::approval::main` before the shell commands (`FORK(approval)`). It is not listed in upstream's `HELP`, so that text stays upstream's.
- It talks to the core directly (`terminal.get`, `terminal.read`, `terminal.keys`), not to the window, so it works with the app closed.

## Browser pane

Built 2026-10-30 to 2026-11-01 on `browser-prototype`. Design and security model: `docs/browser-prototype.md`; dev loop: `docs/browser-dev.md`; moving to upstream's official browser mod when it ships: `docs/browser-transition.md`.

- Engines behind cargo features: `browser` (CEF) and `browser-webkit` (WKWebView), both on the macOS-only `browser-pane`.
- Panes save as a `browser-prototype` mod block, so a stock app shows one unloaded pane instead of failing the layout, and upstream's future browser mod never reads this fork's blocks.
- Seams (upstream-owned files touched): see the footprint table. The window's browser code is in `src/window/fork_browser.rs`, reading a browser block in `src/browser/block.rs`, and the control socket's owner-only checks in `src/browser/socket.rs`; `src/window.rs` and `src/control.rs` keep only `FORK(browser)` call lines.
- A browser block is a `Content::Mod` named `browser-prototype` in memory too (cameronsjo/herdr-app#18, 2026-11-17), so upstream's `Content` enum and its serde attributes are untouched and `src/layout.rs` has no fork lines. The fork reads one through `browser::block::block`. Upstream code that treats a mod block as a mod skips browser blocks at four `FORK(browser)` seams in `src/window.rs`: `visible` and `owned_mods` (no mod copy starts), `render_block` (the fork draws it), and the mods' `panes` list (`fork_pane_json` runs first). `missing_mods` already skipped the name. After a sync, grep upstream's new `Content::Mod` sites for another place that needs the check.
- agent-browser's provider asks the app that holds the caller's terminal now, not the one that started it (cameronsjo/herdr-app#6, 2026-11-17): the app labels its local terminals `herdr-app-fork.socket` (one `FORK(browser)` call in `tend_terminals`), and the provider tries that socket, then the pinned one, then the default home's (`docs/browser-dev.md`).

## Updates and automation

Added 2026-11-15; setup order and the routine's prompt: `docs/fork/weekly-sync.md`.

- Weekly upstream sync PRs from a cloud routine, which first reviews upstream's delta for supply-chain risk (Cameron's choice: that review stands in for his). A conflict-free sync with a clean review auto-merges (a merge commit) once CI passes; conflicts, a workflow change or a held review stay a draft; a security hold stops the sync and opens an issue. Trust model: `docs/fork/weekly-sync.md` § Trust.
- `.github/workflows/fork-browser.yml` (fork-only): the WebKit browser build's tests on macOS, the check upstream's CI can't run.
- `.github/dependabot.yml`: `tungstenite` only (the fork's one stand-alone crate); upstream's Actions entry kept, its version PRs off on the fork (one `FORK(browser)` line). Syncs copy `ci.yml`'s action pins into `fork-browser.yml`.
- `src/update.rs` `checks_core`: the fork's browser builds offer `herdr-core` updates as release builds do, except in an isolated dev run with its own app home (one `FORK(browser)` line).
- `.github/workflows/fork-mac-build.yml` (fork-only, added 2026-11-17, Linear CAM-913): each push to master builds the WebKit dev bundle on macOS with a read-only token, and a separate job attests it (`actions/attest-build-provenance`) and publishes it as the prerelease `browser-<sha>`, keeping the newest 10. Upstream's `release.yml` never publishes on the fork: it publishes only upstream's own tags, to upstream's storage.
- `scripts/fork/daily-update`: a launchd agent on Cameron's Mac checks hourly for a new `fork/master` that is a PR merge with `ci.yml` and `fork-browser.yml` green, downloads its `browser-<sha>` release (checksum and `gh attestation verify` against `fork-mac-build.yml` on master), and installs it as the everyday browser build, never quitting a running one. With no release and no build running for it, or a download that fails a check, it builds the commit locally instead (on AC power).

## Artificer theme

Added 2026-11-16 (Linear CAM-875). A layout without its own theme shows Artificer: `artificer-dark` in dark appearance, `artificer-light` in light (upstream: `graphite` / `paper`).

- `defaults/themes/artificer-{dark,light}.json` (fork-only files): copied verbatim from `cameronsjo/artificer-design-system` `themes/herdr-app/` at `79d8170c`. That repo stays the source. When it changes the theme, copy both files again and add the old text to `PREVIOUS_THEMES`, or existing installs keep the old colors; `shipped_theme_upgrades_preserve_personal_edits` then needs an edit that changes an Artificer file too (it only replaces `#abb2bf` and `#c0c0c0`). A shipped theme file that someone edited is never overwritten (`install_theme`), and neither is a copy its `install.sh` already placed, so this only fills a missing file.
- `src/settings.rs`: `DEFAULT_DARK` / `DEFAULT_LIGHT`, the two `THEMES` entries and the two `SHIPPED_THEMES` names (one line), the `mod fork_artificer` line, and in `palette()` one call to `fork_artificer::theme_text`, which reads a shipped theme's embedded copy when its file is missing or can't be read, so the default pair never depends on `themes/` being written (six `FORK(artificer)` markers). The helper and its tests are in the fork-only `src/settings/fork_artificer.rs`. If upstream adds a shipped theme, keep both sides' entries.
- `defaults/frame/palette/main.js` and `defaults/mods/menu/main.js`: the palette's and the menu's theme lists offer the Artificer pair (one and two `FORK(artificer)` lines). `defaults/AGENTS.md` names the fork's default in its `theme` note.
- Not changed: the in-memory theme starts as graphite until settings load (`resolve(&GRAPHITE, ..)` in `src/settings.rs`), which is never drawn. In `artificer-light.json`, ANSI black equals the background, so black text is invisible; artificer-design-system defers that on purpose until light mode has a real user (`themes/_palette.json` `lightElevationDeferral`).
- Layouts that name a theme keep it: the shipped `terminals` layout still says `graphite`, and `win98` says `win98`.

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

### Background mods (the host for the ports below)

Added 2026-11-17 (Linear CAM-922). A mod copy normally lives only while a layout block, pane, float or overlay names it, and `<mod>.<command>` reaches only shown copies, so a command or watcher with no screen had nowhere to run, and the shipped layouts are upstream's files. The window now loads one copy of each mod in `BACKGROUND` (`src/window/fork_mods.rs`, fork-only) on every layout, counts it as shown, and never draws it: `init()`, `cx.spawn`, `cx.timer` and `nextCommand()` work; `render()` is not called. There is no off switch: Herdr restores a deleted shipped mod folder at start (`sync_defaults`), and a copy whose folder is missing while the app runs is skipped. open-folder and space-groups act only when called; name-sync acts on its own every two seconds. In tests, background copies are off except in `fork_mods`'s own, so upstream's window tests keep their copy counts.

- Seams: `src/window.rs`, the `mod fork_mods` line plus one line each in `sync()` (where copies load), `publish_shown_copies()` and `owned_mods()` (four `FORK(background-mods)` markers), and `window::tests::open_window` is `pub(super)` for the fork's window test (one more). Not in `visible()`, which the daily telemetry report reads: these mods are never on screen. `justfile`'s `test` recipe runs `scripts/fork-mods.test.ts` (one marked line). `defaults/AGENTS.md` lists the background mods' commands in one `FORK(background-mods)` block.
- Upstream CI runs no mod tests (`ci.yml` runs only `scripts/docs.test.ts`), so the fork-only `.github/workflows/fork-mods.yml` runs `scripts/fork-mods.test.ts`.

### Open a folder's existing space (cameronsjo/herdr-app#9)

TUI: cameronsjo/herdr PR #64 (`workspace open` focused the workspace already in a folder). Shipped as the background mod `defaults/mods/open-folder/`: `herdr-app do open-folder.open:<folder>` (or `run("open-folder.open:<folder>")` from a mod, `~` expanded) selects this computer's first space made in that folder or whose first terminal is there now, else makes a space there; `open-folder.pick` asks with the folder picker first. No key is bound: `keys.json` is upstream's file. Upstream's worktree dialog already goes to a shown checkout; plain `space.new {folder}` still always makes a new space.

### Move a worktree group as one block (cameronsjo/herdr-app#13)

TUI: cameronsjo/herdr PR #79 (issue cameronsjo/herdr#48). Upstream's spaces list already keeps each repo's worktree spaces under its main checkout after a drag (`ordered()` in `defaults/mods/spaces/main.js`), but has no key or command to move a space by one place. Shipped as the background mod `defaults/mods/space-groups/`: `space-groups.up` and `space-groups.down` move the shown space's group (its repo's main checkout and the worktree spaces after it, found with `git rev-parse`) past one neighbouring group or lone space on the same machine. Unlike the TUI, on a worktree space the whole group moves instead of nothing.

### One name follows one agent onto its tab (cameronsjo/herdr-app#10)

TUI: cameronsjo/herdr PR #149 (issue cameronsjo/herdr#97). Shipped as the background mod `defaults/mods/name-sync/`: a live terminal with a core name (`hc run --name`) names its tab when it is the tab's only named terminal and nobody named the tab; when its name changes, a tab still called by the old name follows. A tab renamed to anything else is left alone. It looks every two seconds (background copies aren't redrawn) and keeps each terminal's name and tab in its `localStorage`, so a change while a machine was offline still follows. It decides only once the terminal's tab is shown and the terminal is the tab's only named one, so a tab still loading, or a second named terminal that later ends, is named once it can be; a terminal moved to another tab counts as new there. Not ported: the TUI's pane border label, because upstream's pane header already shows the agent's title. The sidebar `name` token (cameronsjo/herdr PR #146) is part of cameronsjo/herdr-app#8.

### Extra environment variables for an agent (cameronsjo/herdr-app#11)

TUI: cameronsjo/herdr PR #124 (`agent start --env`). No code: the app passes `argv` to the core as separate arguments (`src/hc.rs` `run`, wrapped by `unix_agent_argv` in `src/window.rs`), so `tab.new {argv: ["env", "K=V", "claude"]}` reaches the agent with no shell quoting to get wrong, which is what most of the TUI port was about. `src/SKILL.md` and `defaults/AGENTS.md` say so in their `FORK(background-mods)` blocks. Limits: Unix only (Windows agents start under PowerShell, which has no `env`); the values are in the terminal's argv for `hc ls` to show; the TUI's per-shell refusals don't apply because nothing is typed into a shell.

### The agents list's task line, name, needs-you sort and project groups (cameronsjo/herdr-app#8)

TUI: cameronsjo/herdr PR #86 (group by a project token), issues cameronsjo/herdr#97 (`name` token) and #98 (task line, location token, needs-me filter). Shipped in upstream's `defaults/mods/agents/` as a fork-only `fork.js` plus thin seams in its `main.js` (ten `FORK(agents)` markers: the import, the two sorts, the needs-you list's priority order, its filter, the task line, the name, the project groups, the sort menu's height, the empty message and the row naming its space). A clone of the mod would need the shipped layouts, upstream's files, to name it, and would stop getting upstream's fixes.

- Task line: `hc label <terminal> task="…"` draws a dim line under the agent's row; an empty value clears it. The TUI's `herdr agent task` verb is not ported: `hc label` already sets it from anywhere, and `$HC_TERMINAL_ID` names the calling agent.
- Name: a terminal's core name (`hc run --name`) is its row's detail (after the space in priority order, the row's name under a space header).
- Needs you: a third sort, priority order with only the agents that need the person (one of their sub-agents counts); the empty list says "No agents need you." when agents run. No key yet: cameronsjo/herdr-app#12 (the palette) adds commands.
- Grouped by project: a fourth sort, one header per `project` label (`hc label <terminal> project=herdr`), else per space, each group gathered behind its first agent. The TUI's workspace-level token fallback has no equivalent: spaces carry no labels.
- Location: not ported, upstream already names a row's space wherever no space header is above it. Under a project header, which can span spaces, each row names its space too (a tenth `FORK(agents)` seam, on the row's `full` check).
- Label text: any process can set a label or a name, so each value drops control, bidi and invisible characters and is cut to 200 characters (task) or 80 (name, project), counted by code point. The keys are plain `task` and `project`, as the TUI's tokens were and as `hc label` users type them, not `herdr.`-prefixed like Herdr's own labels.
- Seams: `defaults/mods/agents/main.js` (above); `src/mods/render_tests.rs`, one `mod fork_agents` line (marked) for the render cases in `src/mods/render_tests/fork_agents.rs`. `src/SKILL.md` and `defaults/AGENTS.md` name the labels in their `FORK(background-mods)` blocks.
- Sync: an upstream rewrite of the agents mod's `render()` means re-placing each marked line by hand; `fork_agents`' render cases and `scripts/fork-mods.test.ts` fail if one goes missing.
