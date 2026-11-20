# Fork divergence ledger

Every way `cameronsjo/herdr-app` differs from `herdrdev/herdr-app`, newest first within each section. A sync resolver reads this before resolving; a feature PR adds its entry in the same PR. Rules for keeping the footprint small: `CLAUDE.md` § Merge-friendly by construction.

## Footprint

Output of `scripts/fork/seams` at each sync, so growth is visible.

| Date | Upstream base | Lines in upstream files | Files | Fork-only files | `FORK(` markers |
|---|---|---:|---:|---:|---:|
| 2026-11-15 | `b6546ce1` (before revival) | 1506 | 23 | 37 | 0 |
| 2026-11-15 | `b6546ce1` (seams extracted, `06f37e1`) | 769 | 23 | 45 | 46 |
| 2026-11-15 | `8e1208d` (synced, `bf9b67c`) | 761 | 23 | 45 | 51 |
| 2026-11-18 | `5b32e12` (synced) | 951 | 30 | 85 | 148 |

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

### 2026-11-18: `8e1208d` → `5b32e12` (8 commits)

Run by hand from a cloud thread (Linear CAM-945), not the weekly routine. Supply-chain review: CLEAN. Upstream added the `regex` crate (already in the lock as a transitive dependency; no new download), one Bun test to `justfile`'s `test` recipe, and terminal-link handlers (`link.js` mods, `src/links.rs`, `ShellRuntime::check_module` and `call_module` in vendored gpui-kit) that open URLs and files through the existing `links::open`. No workflow, build-script or installer changes.

No upstream renames or deletions of fork-touched files. 5 conflicted files, one hunk each; `log_safe`, `log_escaped` and `public_url` call sites: 106 before, 106 after. Every fork line in each upstream file both sides touched was checked against the merged tree; the only fork lines no longer present verbatim are the three below that were edited to carry upstream's change.

| File | Upstream | Fork | Kept |
|---|---|---|---|
| `src/window.rs` | `pane.rename` and `pane.resize` are structural commands | `fork_browser::structural(name)` on the same line | upstream's list plus the fork's check |
| `src/main.rs` | `herdr-app check` checks a `link.js`-only mod first | `main_with_components(browser::element::components())` | upstream's link check, then the fork's components call in place of upstream's default registry |
| `defaults/mods/pane/main.js` | a pane's `name` comes first in its header | header split into `head` + `terminal()` | upstream's `p.name \|\|` moved into the fork's `terminal()` |
| `defaults/herdr.d.ts` | `PaneInfo` doc lists `name` | browser-pane note on the same comment | both |
| `justfile` | `scripts/changes.test.ts` in the Bun line | the `fork-mods.test.ts` line | both |
| `src/window/fork_browser.rs` (fork-only, no conflict) | every `panes()` entry gains `name` (the pane's own name, from `pane.rename`) | the browser pane's entry, built before upstream's | `"name": p.title` added, so a renamed browser pane reports its name like any other |
| `Cargo.lock`, `THIRD-PARTY-NOTICES` | | | upstream's, then regenerated |

Not a regression: 34 of upstream's Bun tests in `scripts/agents.test.ts` fail before and after this sync, because the harness evaluates the agents mod's source with `new Function`, which can't parse the fork's `import * as` line (cameronsjo/herdr-app#39). Upstream's CI doesn't run them.

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

## README

Added 2026-11-18 (Linear CAM-945). `README.md` opens with a fork section: what the fork is, how to run it, and a plain-English list of what it changes, ending in a `---` rule. Everything after the rule is upstream's README, untouched, so upstream's edits there merge cleanly.

- Seam: the block from the `FORK(readme)` HTML comment down to the `---` rule above upstream's `# Herdr` heading. A sync that conflicts at the top of the file keeps the fork block and upstream's new text below it.
- A feature PR that changes what the fork does updates its line in that list.

## Font picker

Added 2026-11-17 (Linear CAM-923). Upstream's settings font picker lists only the first 200 font families (`.slice(0, 200)`), so on a Mac with more families than that, fonts late in the alphabet never show unless you search for them. The fork lists every family through a `uniform_list`, which draws only the rows on screen; each row is drawn in its own font, which is why a plain scroll area of every font would be slow to open. Rows live in the fork-only `defaults/frame/settings/fork-font-list.js`.

- Seams: `defaults/frame/settings/main.js` the `fontRows` import, the `matches` line (no cap) and the `fontRows(...)` call that replaces upstream's scroll area (upstream's `item` helper, used only there, is removed); `src/mods/render_tests.rs` `font_pickers_contain_scroll_on_the_wrapper` expects `uniform_list` in place of `overflow_y_scrollbar`, plus the fork test `font_picker_lists_every_installed_font`. All marked `FORK(font-picker)`.
- If upstream lifts the cap or virtualizes its list, drop the fork file and take upstream's.

## Attention queue

Added 2026-11-17 (cameronsjo/herdr-app#16, Linear CAM-921). What it does: `docs/fork/attention.md`. Rules in `src/fork/attention.rs`, the window's part in `src/window/fork_attention.rs`.

- Seams: `src/main.rs` `mod fork`; `src/window.rs` `mod fork_attention`, the shown word in `tend_terminals` (`Queue::word`), the `looking` line after it (a label cleared while the agent rested raises no Done alert), the tick call there, and `fork_agent_fields` on each `agents()` entry; `defaults/mods/agents/main.js` the badge text (`needsYou`) and the Stalled badge; `defaults/herdr.d.ts` the three `AgentInfo` fields. All marked `FORK(attention)`.
- The label is `herdr.attention`, in Herdr's shared label namespace on purpose, so other clients can learn it. If upstream ever gives that label another meaning, rename the fork's.
- `stall_minutes` lives in `<app home>/fork.json`, not `settings.json`, so upstream's settings struct stays as it is.
- Follow-ups (cameronsjo/herdr-app#35): the priority view shows the full Stalled pill (the "…" glyph is for the narrow rail only); a failed label removal waits 5 s before it is tried again; and a label also goes once the terminal's `turn` moves past the one it had when the label was first seen, so a turn shorter than a tick doesn't leave it behind (that turn's Done alert still shows: a label cleared because the agent worked is not one the person dealt with, and that mark goes with the label).

## Answering approvals

Added 2026-11-17 (cameronsjo/herdr-app#17, Linear CAM-921). What it does: `docs/fork/approval.md`. All of it is in `src/fork/approval.rs`.

- Seams: `src/main.rs` routes `herdr-app approval ...` to `fork::approval::main` before the shell commands (`FORK(approval)`). It is not listed in upstream's `HELP`, so that text stays upstream's. `defaults/AGENTS.md` names the command in one `FORK(approval)` paragraph after the `herdr-app do` shell block.
- It talks to the core directly (`terminal.get`, `terminal.read`, `terminal.keys`), not to the window, so it works with the app closed. The calls go through a small `Rpc` trait, so the tests answer them with a fake core.
- Follow-ups (cameronsjo/herdr-app#38, 2026-11-19): a panel's top edge must start in column 0 and span the pane's width (`cols` from `terminal.get`), so a rule or box inside a command no longer hides the command; a blocked `kind` other than `permission` or `question` (a sign-in) is refused, and a `question` takes only a number; `answer --wait <secs>` bounds the wait for the key; `approve` takes only a whole one-time label (`ONCE`), so an unknown grant fails closed, and `deny` never picks an option that sets something for later; a box side comes off a screen line only when both ends have one, so a command line ending in `|` keeps it. Fixtures in `src/fork/fixtures/` are live Claude Code 2.1.328 Bash and Edit dialogs; Codex was read from its source, not captured (no Codex login in the cloud).

## Browser pane

Built 2026-10-30 to 2026-11-01 on `browser-prototype`. Design and security model: `docs/browser-prototype.md`; dev loop: `docs/browser-dev.md`; moving to upstream's official browser mod when it ships: `docs/browser-transition.md`.

- Engines behind cargo features: `browser` (CEF) and `browser-webkit` (WKWebView), both on the macOS-only `browser-pane`.
- Panes save as a `browser-prototype` mod block, so a stock app shows one unloaded pane instead of failing the layout, and upstream's future browser mod never reads this fork's blocks.
- Seams (upstream-owned files touched): see the footprint table. The window's browser code is in `src/window/fork_browser.rs`, reading a browser block in `src/browser/block.rs`, and the control socket's owner-only checks in `src/browser/socket.rs`; `src/window.rs` and `src/control.rs` keep only `FORK(browser)` call lines.
- A browser block is a `Content::Mod` named `browser-prototype` in memory too (cameronsjo/herdr-app#18, 2026-11-17), so upstream's `Content` enum and its serde attributes are untouched and `src/layout.rs` has no fork lines. The fork reads one through `browser::block::block`. Since cameronsjo/herdr-app#4 (below) a browser block runs the shipped `browser-prototype` mod like any mod block, so upstream's `visible`, `owned_mods` and `missing_mods` no longer skip it. Two `FORK(browser)` seams in `src/window.rs` still check for one: `render_block` (the window draws the block's frame around the mod) and the mods' `panes` list (`fork_pane_json` runs first). After a sync, grep upstream's new `Content::Mod` sites for another place that draws or lists mods.
- The browser pane is an ordinary mod (cameronsjo/herdr-app#4, 2026-11-17). `defaults/mods/browser-prototype/` draws the placeholder (the page's title and address while it opens or while an overlay covers it) and `nativeView("browser", id)` from `herdr-native`, a fork component (`src/browser/element.rs`) registered through gpui-shell's public `ComponentRegistry`. The element only says where the page goes. The window still owns the rest: `render_block` wraps the mod copy in the block's frame, which keeps the agent header (so only the person can turn agent access on) and paints a canvas after the mod every frame that places the page (`browser::place`). That canvas exists because gpui-shell caches a mod's view: on a frame where the mod didn't change, its paint is replayed and the element's own paint doesn't run, so the frame reuses the mod's last report, kept relative to the block, and ignores any report outside the block that owns the page. The window draws the block itself when the page can't show there (the browser didn't start, another block owns the page, an id that can't open, and every build without browsers). Seams: the mods' VM is created with the catalog (`ShellRuntime::new_with_components` in `with_file_events`, `replace_default_with_components` in `restart_mods`, `src/window.rs`), `mods.rs` declares it in `gpui-kit.d.ts`, and `vendor/gpui-kit/crates/shell/src/engine/quickjs/mod.rs` gains `replace_default_with_components` (a new function beside upstream's `HERDR:` `replace_default`, so `restart_mods` keeps the catalog). Removed: the three skips in `visible`, `owned_mods` and `missing_mods`. Net footprint: +18 lines in upstream files (906/93 against 888/85 before, measured with `scripts/fork/seams upstream/master`), most of it the 9-line vendor function; `src/mods/render_tests.rs` gives its shipped-mod VM the same catalog. `herdr-app check` and `types` get it too (`src/main.rs`), so a mod that imports `herdr-native` checks. The mod copy's props are `{ browser, profile? }` (`browser::block::mod_props`, a `FORK(browser)` call wherever `visible` and `owned_mods` build a copy's instance, `float_mod` included): the url follows every navigation, so with it each page would be a new mod instance and the copy would reload, and the full url (sign-in codes) would reach mod JS. `browser-prototype` now counts in `visible()`, which telemetry reads, as any shown pane mod does.
- agent-browser's provider asks the app that holds the caller's terminal now, not the one that started it (cameronsjo/herdr-app#6, 2026-11-17): the app labels its local terminals `herdr-app-fork.socket` (one `FORK(browser)` call in `tend_terminals`), and the provider tries that socket, then the pinned one, then the default home's (`docs/browser-dev.md`).
- Review follow-ups (2026-11-19, cameronsjo/herdr-app#29, #31, #43, #5; Linear CAM-998). Source-order tests now guard `render_block`'s browser early return and the panes-list seam, each scoped to its function and its needles required exactly once. The provider builds "no terminal" in one place (`browser::no_terminal`), waits 6 s per app instead of 10 (a live app answers in 5 s), and no longer puts a labeled socket path in the agent's errors. An app labels terminals only while it serves its control socket (`browser::socket::bind_control`, the same `FORK(browser)` line in `src/control.rs`), retries a failed label every 30 s instead of every tick, and takes its terminals back when the app that took them quits (its socket file is gone, or it is this user's private socket and refuses connections; a label pointing anywhere else is never connected to). A floating `browser-prototype` copy has a fixed width, since a content-sized float collapses `size_full()`. The window draws the mod's root itself instead of through `render_slot`, so a click on a browser pane focuses it once instead of publishing and saving twice. An edited `browser-prototype` (files differ from `installed.json`) no longer draws over pages: the window draws the block and places the page whole until the next start moves the edit aside. Seam: one `FORK(browser)` line in `files_changed`'s mod-edit path (`fork_mods_edited`), +1 line in upstream files. The duplicate "Script error." from #5 needs a live WebKit page and moved to the hands-on checks in cameronsjo/herdr-app#1.

## Updates and automation

Added 2026-11-15; setup order and the routine's prompt: `docs/fork/weekly-sync.md`.

- Weekly upstream sync PRs from a cloud routine, which first reviews upstream's delta for supply-chain risk (Cameron's choice: that review stands in for his). A conflict-free sync with a clean review auto-merges (a merge commit) once CI passes; conflicts, a workflow change or a held review stay a draft; a security hold stops the sync and opens an issue. Trust model: `docs/fork/weekly-sync.md` § Trust.
- `.github/workflows/fork-browser.yml` (fork-only): the WebKit browser build's tests on macOS, the check upstream's CI can't run.
- `.github/dependabot.yml`: `tungstenite` only (the fork's one stand-alone crate); upstream's Actions entry kept, its version PRs off on the fork (one `FORK(browser)` line). Syncs copy `ci.yml`'s action pins into `fork-browser.yml`.
- `src/update.rs` `checks_core`: the fork's browser builds offer `herdr-core` updates as release builds do, except in an isolated dev run with its own app home (one `FORK(browser)` line).
- `.github/workflows/fork-mac-build.yml` (fork-only, added 2026-11-17, Linear CAM-913): each push to master builds the WebKit dev bundle on macOS with a read-only token, and a separate job attests it (`actions/attest-build-provenance`) and publishes it as the prerelease `browser-<sha>`, keeping the newest 10. Upstream's `release.yml` never publishes on the fork: it publishes only upstream's own tags, to upstream's storage.
- `scripts/fork/daily-update`: a launchd agent on Cameron's Mac checks hourly for a new `fork/master` that is a PR merge with `ci.yml` and `fork-browser.yml` green, downloads its `browser-<sha>` release (checksum and `gh attestation verify` against `fork-mac-build.yml` on master), and installs it as the everyday browser build, never quitting a running one. With no release and no build running for it, or a download that fails a check, it builds the commit locally instead (on AC power). Fixed 2026-11-20: the CI check matched workflow runs with `command grep` inside an `if`, which under launchd's `/bin/bash` 3.2 fires the ERR trap, so every check from 2026-11-16 failed and the agent stayed on `a1330f9`; it now matches with `case`. Under the same bash, a command that fails inside `$(...)` runs the ERR trap in that subshell, so a good check could still notify "check failed" (a missing state file, no release yet, a transient `gh` error); state files are read with `state_of` and those substitutions end in `|| true`. The agent's copy now refreshes right after the gated checkout instead of after a successful build, the attestation-failure notification comes once per commit, and `status` prints the script's sha256 and source. Tests: `scripts/fork/daily-update.test.ts` (stand-ins for `gh`, `pmset`, `osascript`; `/bin/bash` on macOS).
- Developer ID signing of the everyday WebKit build (added 2026-11-19, Linear CAM-543). `fork-mac-build.yml`'s `build` job builds cold (no `target/` cache, and `setup-zig`'s cache off) and uploads an unsigned zip; a `sign` job, the only one on the `apple-signing` environment (master only), checks the zip's shape, sets the bundle id `dev.herdr.app.browser-daily`, signs with hardened runtime, notarizes and staples, and checks team `5H883FMW6H`, the entitlements, the stapled ticket and (where the runner has it on) Gatekeeper before `publish` attests it. Without a certificate in the environment it signs ad hoc, unless the repo variable `FORK_REQUIRE_DEVID` is `true`. `daily-update` and `browser-daily on` install a build as is when it passes `codesign --verify --deep --strict` against a Developer ID Application certificate of that team and that bundle id, `spctl` reports a notarized Developer ID app, and `stapler validate` passes. A build with the fork's signature that fails the `spctl` or `stapler` check is held by `daily-update check` (installed app unchanged, retried at the next check, one notification per build) and re-signed ad hoc by `browser-daily on`; any other build is re-signed ad hoc as before. Fork-owned copies of upstream's signing files, so a sync can't change what runs with the certificate: `scripts/fork/mac-sign` (upstream's `scripts/mac-sign` at `b153ad6` plus a header), `scripts/fork/macos-entitlements.plist` (identical); `scripts/fork/mac-notarize` is new. The weekly sync holds any change to them or to upstream's originals.

## Artificer theme

Added 2026-11-16 (Linear CAM-875). A layout without its own theme shows Artificer: `artificer-dark` in dark appearance, `artificer-light` in light (upstream: `graphite` / `paper`).

- `defaults/themes/artificer-{dark,light}.json` (fork-only files): copied verbatim from `cameronsjo/artificer-design-system` `themes/herdr-app/` at `79d8170c`. That repo stays the source. When it changes the theme, copy both files again and add the old text to `PREVIOUS_THEMES`, or existing installs keep the old colors; `shipped_theme_upgrades_preserve_personal_edits` then needs an edit that changes an Artificer file too (it only replaces `#abb2bf` and `#c0c0c0`). A shipped theme file that someone edited is never overwritten (`install_theme`), and neither is a copy its `install.sh` already placed, so this only fills a missing file.
- `src/settings.rs`: `DEFAULT_DARK` / `DEFAULT_LIGHT`, the two `THEMES` entries and the two `SHIPPED_THEMES` names (one line), the `mod fork_artificer` line, and in `palette()` one call to `fork_artificer::theme_text`, which reads a shipped theme's embedded copy when its file is missing or can't be read, so the default pair never depends on `themes/` being written (six `FORK(artificer)` markers). The helper and its tests are in the fork-only `src/settings/fork_artificer.rs`. If upstream adds a shipped theme, keep both sides' entries.
- `defaults/frame/palette/main.js` and `defaults/mods/menu/main.js`: the palette's and the menu's theme lists offer the Artificer pair (one and two `FORK(artificer)` lines). `defaults/AGENTS.md` names the fork's default in its `theme` note.
- Not changed: the in-memory theme starts as graphite until settings load (`resolve(&GRAPHITE, ..)` in `src/settings.rs`), which is never drawn. In `artificer-light.json`, ANSI black equals the background, so black text is invisible; artificer-design-system defers that on purpose until light mode has a real user (`themes/_palette.json` `lightElevationDeferral`).
- Layouts that name a theme keep it: the shipped `terminals` layout still says `graphite`, and `win98` says `win98`.

## Decisions pane

Added 2026-11-16 (Linear CAM-877). A native pane for cadence:ask-questions slates, so several decisions open beside Claude's terminal instead of in a browser. Shipped as a mod; no Rust and no upstream-owned file changes.

- `defaults/mods/decisions/` (fork-only): `main.js` draws the cards (pick one or many, compare candidates, typed answers, notes, a review page, Send), and `slate.js` is a port of cadence's `plugins/ask/hooks/slate.ts`, which mirrors `serve-decisions.py` `validate_spec`. Keep the three in step: a slate the dashboard takes must open here.
- Opened by cadence's `serve-decisions.py` when it runs in a Herdr terminal: `herdr-app do pane.split '{"direction":"right","mod":"decisions","props":{"dir":"<slate folder>","run":"<nonce>"}}'`, where `run` is the nonce that waiting `serve-decisions.py` run made (a pane without one refuses to open the slate). The pane reads `<dir>/slate.json` and only for a folder of the form `<absolute path>/.claude/decisions/<slug>`. Send writes `<dir>/response.json` (create-only, never replacing one). Close without sending, closing the pane, or a slate the pane can't show writes `<dir>/closed.json` (the last with an `error`, so the script opens the browser instead). Both files carry `props.run`, the script's per-run nonce, so a pane left from an earlier run can't answer this one. The script appends the ledger line and closes the pane. A draft survives a reload of the pane, kept per run in the mod's own storage. No attachments.
- Keys work only when no field has the keyboard (`a`-`h` pick, `n`/`p` move, `s` skip, `r` take the recommended options on the review page). Send has no key.
- `scripts/fork/decisions.test.ts` (fork-only) runs the slate logic and the mod against stand-ins; `.github/workflows/fork-mods.yml` (fork-only) runs it, since upstream's CI runs only its own Bun tests. The shipped-mod render test in `src/mods/render_tests.rs` loads the mod with no `dir`, which draws the empty pane and reads nothing.

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

- Seams: `src/window.rs`, the `mod fork_mods` line plus one line each in `sync()` (where copies load), `publish_shown_copies()`, `owned_mods()` and `drop_layout_copies()` (five `FORK(background-mods)` markers; the last keeps background copies through a layout switch), and `window::tests::open_window` is `pub(super)` for the fork's window test (one more). Not in `visible()`, which the daily telemetry report reads: these mods are never on screen. `justfile`'s `test` recipe runs `scripts/fork-mods.test.ts` (one marked line). `defaults/AGENTS.md` lists the background mods' commands in one `FORK(background-mods)` block.
- Upstream CI runs no mod tests (`ci.yml` runs only `scripts/docs.test.ts`), so the fork-only `.github/workflows/fork-mods.yml` runs `scripts/fork-mods.test.ts`.

### Open a folder's existing space (cameronsjo/herdr-app#9)

TUI: cameronsjo/herdr PR #64 (`workspace open` focused the workspace already in a folder). Shipped as the background mod `defaults/mods/open-folder/`: `herdr-app do open-folder.open:<folder>` (or `run("open-folder.open:<folder>")` from a mod, `~` expanded) selects this computer's first space made in that folder or whose first terminal is there now, else makes a space there; `open-folder.pick` asks with the folder picker first. No key is bound: `keys.json` is upstream's file. Upstream's worktree dialog already goes to a shown checkout; plain `space.new {folder}` still always makes a new space.

- Follow-ups (cameronsjo/herdr-app#28): a folder's name keeps its spaces (only a blank one is refused); a home of `/` makes `~/x` `/x`; a space with no folder of its own counts as made at home; and the folder's symlink-resolved path (`/bin/pwd -P`, not on Windows) matches too, so `/tmp/x` finds a space whose terminal is in `/private/tmp/x`.

### Move a worktree group as one block (cameronsjo/herdr-app#13)

TUI: cameronsjo/herdr PR #79 (issue cameronsjo/herdr#48). Upstream's spaces list already keeps each repo's worktree spaces under its main checkout after a drag (`ordered()` in `defaults/mods/spaces/main.js`), but has no key or command to move a space by one place. Shipped as the background mod `defaults/mods/space-groups/`: `space-groups.up` and `space-groups.down` move the shown space's group (its repo's main checkout and the worktree spaces after it, found with `git rev-parse`) past one neighbouring group or lone space on the same machine. Unlike the TUI, on a worktree space the whole group moves instead of nothing.

- Follow-ups (cameronsjo/herdr-app#28): one `git rev-parse --git-dir --git-common-dir` per folder (spaces sharing a folder share it), and a linked worktree is one whose two differ, so a submodule no longer counts as one; a lookup that failed or timed out stops the move with a message instead of reading as "no repo" and splitting a group, while git's exit 128 (no repo in any language, or a deleted folder, since git now gets `-C <folder>`) still reads as "no repo" (so does git refusing a repo it doesn't trust, "dubious ownership"); and a space made while git answered is looked up before anything moves (three rounds at most).

### One name follows one agent onto its tab (cameronsjo/herdr-app#10)

TUI: cameronsjo/herdr PR #149 (issue cameronsjo/herdr#97). Shipped as the background mod `defaults/mods/name-sync/`: a live terminal with a core name (`hc run --name`) names its tab when it is the tab's only named terminal and nobody named the tab; when its name changes, a tab still called by the old name follows. A tab renamed to anything else is left alone. It looks every two seconds (background copies aren't redrawn) and keeps each terminal's name and tab in its `localStorage`, so a change while a machine was offline still follows. It decides only once the terminal's tab is shown and the terminal is the tab's only named one, so a tab still loading, or a second named terminal that later ends, is named once it can be; a terminal moved to another tab counts as new there. Not ported: the TUI's pane border label, because upstream's pane header already shows the agent's title. The sidebar `name` token (cameronsjo/herdr PR #146) is part of cameronsjo/herdr-app#8.

- Follow-ups (cameronsjo/herdr-app#34): the tab comes from the pane showing the terminal (`panes()`), not its `herdr.tab` label, which an agent can rewrite; a rename is recorded only once the tab shows it, so a rejected one is asked for again; the first build's bare-name records are kept as records with no tab, which match the terminal's tab once.

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
- Follow-ups (cameronsjo/herdr-app#40): ZWJ, ZWNJ and variation selectors stay in label text (emoji, Persian), and more invisible characters go (the set outside-agents drops, tag characters included); only the first `max * 8 + 16` code units of a label are looked at; a cut keeps grapheme clusters whole (`Intl.Segmenter` where it exists, else a fallback for joiners, marks, variation selectors, skin tones and flags); the task line hovers and clicks like its row (focusing the agent) and starts under the name with or without an icon; a tab the person named stays the row's detail when the agent is alone in it, else the terminal's name shows; a project group is keyed by its whole label, so long labels sharing their first 80 characters stay apart. "Needs you" is a filter: it hides a pinned agent that doesn't need you, on purpose.

### Agents running outside Herdr's terminals (cameronsjo/herdr-app#14)

TUI: cameronsjo/herdr PR #91 (`agent register`, a heartbeat TTL). Shipped as the background mod `defaults/mods/outside-agents/` plus rows in the agents mod's `fork.js`, with no core verb: each outside agent keeps one `<id>.json` in the app home's `outside-agents/` folder (`{"name", "status", "title", "task", "ttl", "at"}`, only `status` needed) and rewrites it as its heartbeat. The mod writes the folder's `README.md`, lists it at start, follows it with `watchFolder`, and publishes the live ones as shared `fork.outside-agents`. A record drops out `ttl` seconds (1 to 86400, default 60) after its last write, counted from its `at` when it gives one and otherwise from when the mod saw the file change, or as soon as the file is deleted. At start the list keeps only rows still within their ttl, so the last run's rows never outlive it. The folder is listed with `find` (`dir` on Windows); a file over 4 KiB or with an unknown `status` is ignored, and at most 128 records are kept. At most 64 show; text is cleaned as the agents list cleans labels (control, bidi and invisible characters dropped), with names cut to 80 code points and the rest to 200.

- The agents list draws them last, under "Elsewhere", with upstream's status marks and the task line; "Needs you" keeps the blocked ones. Rows do nothing on click (no pane), and the narrow rail leaves them out.
- The TUI's `source`/`name` key becomes the file name. Not ported: a CLI verb (`echo … > file` is the API), and the multi-machine list (only this computer's folder is read).
- Files present at start without `at` show for one `ttl` before they expire. The first look waits two seconds after start, which also keeps its file reads, on real threads, out of the window tests' scheduler.
- Seams: one more `FORK(outside-agents)` line in `defaults/mods/agents/main.js` (the rows after the list) and the shared list passed to its empty message; `BACKGROUND` gains `outside-agents`.
- Follow-ups (cameronsjo/herdr-app#45), 2026-11-19:
  - Security: on Linux, upstream's `watchFolder` runs `git ls-files` and `git rev-parse` in the watched folder, and git ran a `core.fsmonitor` command from a `.git/config` planted there (checked with git 2.43). An agent allowed to write only `outside-agents/` could run code as the app. Both calls in `src/files.rs` now pass `-c core.fsmonitor=false`, neither runs at all in the app's own `outside-agents/` folder (`src/fork/untrusted.rs`), where a planted `.gitignore`, `core.excludesFile` or `include.path` pipe would hang git while upstream's folder-watch lock is held, and both are stopped after 5 s (`output_within`), since a repo holding the app home (dotfiles) walks into that folder too; a stopped `ls-files` means no ignore pruning for that watch. `src/files.rs` has four `FORK(outside-agents)` lines plus the test module `src/fork/files_tests.rs` behind a fifth. The same calls are upstream's, so upstream has the bug too; reporting it there is Cameron's call.
  - The folder is listed with `find -maxdepth 1 -type f -name '*.json' -print0` (`dir /b /a-d-l` on Windows), so only plain files right in it are listed, and a name with a newline, slash or backslash is no agent's; a folder event is matched to those names, and an event naming the folder itself (the watch overflowed) counts every file as changed. A file can still be swapped for another kind between the listing and the read, so `readTextFile` itself (`src/mod_files.rs`, one `FORK(outside-agents)` line) now opens without waiting (`O_NONBLOCK` on Unix) and reads plain files only, never a pipe or device; a link swapped in shows what it points at, to the person only. A linked `outside-agents` folder is not read and gets no README. On Windows an app-home path with a character cmd reads as syntax (`&|<>^%!"()`) is not listed, and exit 1 with no output counts as empty, in any language.
  - The watch starts before the first read, and is re-armed before each batch is handled. Only valid ids count toward the 128-record cap, and a full map makes room by dropping the record that expires soonest (if it expires before the new one); a dropped record's text is remembered, so a relist doesn't read its unchanged file as a fresh heartbeat. The 4 KiB limit counts bytes.
  - Display: more invisible characters are dropped (soft hyphen, CGJ, Hangul fillers, U+180E, U+2060-206F, U+FFF9-FFFB, tags), the same set the agents list now drops from labels; ZWJ, ZWNJ and variation selectors stay; an `error` record keeps `reason: "error"`, so its pill says Error; no blank line above "Elsewhere" when only outside agents show; records drop out within a second of their ttl; a hot reload keeps the rows still within their ttl (published records carry `until`), and keeps publishing those (as their ttls run out) until its first read of the folder, so they no longer blink. The 2 s start delay stays: it keeps the first file reads, on real threads, out of the window tests' scheduler.

### Mod commands and a direction chooser in the palette (cameronsjo/herdr-app#12)

TUI: the palette listed plugin commands and chose a direction in one step. Shipped in upstream's frame palette, `defaults/frame/palette/`, as a fork-only `fork.js` behind five `FORK(palette)` seams in its `main.js` (the import, the "Mods" group, the read of mods' commands when it opens, the entries it ranks, and the chooser in `go`), plus the fork-only `defaults/mods/pick-direction/`. The issue suggested a separate palette mod rebound in `keys.json`. The shipped frame is the fork's to change, though, and a copy would stop getting upstream's palette fixes, so this is a seam in place.

- Mod commands: a mod lists its own commands in its `mod.json`, `"commands": [{"command", "title", "words"}]`, read from every folder under the app home's `mods/` each time the palette opens. A command must be `<that folder>.<command>` (one plain word after the dot), at most 20 per mod and 200 folders; a folder named like an app command namespace (`app`, `pane`, ..., or the `project` and `context` aliases) lists none, so a row can never run an app command. Titles drop control and format characters. Rows show under "Mods" with their keys from `keys.json`. `open-folder` lists `open-folder.pick`, `space-groups` lists `up` and `down`. Herdr ignores the extra field.
- Direction chooser: with nothing typed, the four "Focus pane …" rows, the four "Swap pane …" rows and the two "Split …" rows become "Focus pane…", "Swap pane…" and "Split pane…". Choosing one opens `pick-direction`, where an arrow key or a tile runs the command that way (only `pane.split`, `pane.go` or `pane.swap`, whoever opened it); Esc closes. Typing still finds each direction's own row ("split right").
- A direction row used before this change ("Split right") stays in Recent while it is among the first five; choosing a family row puts that row there too.
- The mods folder is listed with `fs.readdir`, falling back to `ls` or `dir` when the app home sits outside the home folder (as `outside-agents` does). Each direction tile shows its own key from `keys.json`, as the palette's rows did.
- Seams: `src/mods/render_tests.rs` gets two `FORK(palette)` lines, the `mod fork_palette` line for `src/mods/render_tests/fork_palette.rs` and an `ls` reply in the fixture host. Sync: an upstream rewrite of the palette's `entries()`, `rows()` or `go()` means re-placing each marked line; `scripts/fork-mods.test.ts` and `fork_palette` fail if one goes missing.
- Follow-ups (cameronsjo/herdr-app#47): a direction run from the chooser now reaches Recent: `pick-direction` leaves its row's id in shared `fork.palette.picked`, and the palette's next open puts it first in place of the family row, once per pick (the palette's init seam line gains `fork.withPicked`); with nothing typed, a direction row among the first five Recent ids stays listed so Recent can show it (the entries seam passes `this.recent`). An arrow picks on release, so a held arrow's repeats stay in the chooser instead of reaching the terminal after it closes. Only the newest of overlapping `load()`s sets the list. A command name over 80 characters is refused instead of cut to a shorter name; titles are still cut. `load()` has tests of its own (the `ls` fallback, the 64 KB cap, app namespaces).

### Move a tab to another space, and merge spaces (cameronsjo/herdr-app#15)

TUI: cameronsjo/herdr#107 (a tab moves to another space; moving the last one closes it) and cameronsjo/herdr#53 (`workspace.merge`). Shipped as app Rust, because a mod can't write the layout document (`defaults/herdr.d.ts` has `layout.set` for the person's layout tree, nothing for spaces or tabs). Upstream's `tab.move` still orders a tab inside its space only.

- `tab.move_to_space {space, id?, index?}` moves a tab with its panes, splits and terminals; `space.merge {into, id?}` moves every tab of a space to the end of another, in order. Both refuse a space on another machine (its terminals run there). The emptied space goes away without joining the recent spaces: nothing in it closed. A shown tab stays shown in its new space. The tab keeps its id, so the moved terminals get only a new `herdr.space` label, which puts one that loses its pane back in the right space; terminals labeled for a moved tab that have no pane yet are relabeled too, so they never bring the old space back. Until its label is written, a moved terminal counts as changing (as during `terminal.park`): a park, a second move or an adoption waits, and a command that would move it refuses.
- Follow-ups (cameronsjo/herdr-app#49, 2026-11-19): the destination comes first in each command's arguments, so the `keys.json` shorthand `tab.move_to_space:<space>` and `space.merge:<into>` names it. A move or merge waits (refuses, "try again") while a `terminal.keep` is still giving a terminal a pane in a moving tab, or in the space the move would empty: the keep would otherwise land under a stale `herdr.space` label, or park its terminal. A tab put ahead of the destination's shown tab no longer changes which tab that space shows when it had none selected.
- Code: `src/fork/spaces.rs` (the model change, with tests) and `src/window/fork_spaces.rs` (the commands). Seams, each marked `FORK(spaces)`: `src/fork/mod.rs` and `src/window.rs` (`mod` lines), one `dispatch` arm and two lines at the top of `adopt()` in `src/window.rs` (a terminal still labeled for a moved tab, because its label write failed or a quit cut it off, joins the tab where it is instead of making a second tab with that id in a new copy of the old space), the two command rows in `src/commands.rs`, one line in `defaults/AGENTS.md`, and the test-only `hc::scripted` core in `src/hc.rs` (a core whose replies a test picks, for a label write that fails). `docs/commands.md` is regenerated.
