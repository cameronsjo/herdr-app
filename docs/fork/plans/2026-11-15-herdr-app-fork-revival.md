---
status: in-progress
tier: T2
next: "Task 3: Linux CI on PR #7 (operator enables fork Actions), then Cameron merges; then task 7 per docs/fork/weekly-sync.md"
approved_session_id: 07e87b2f-6fb2-46e2-afa6-dec605969fac
branch: fork/revival-20261115
---

# Revive the herdr-app fork with the browser pane

## Goal

Make `cameronsjo/herdr-app` Cameron's everyday Herdr build: upstream plus the browser pane plus the features worth keeping from the TUI fork `cameronsjo/herdr`, which gets archived. The fork is built so that merging upstream stays cheap, and a weekly agent run keeps it current.

## Decisions (operator, 2026-11-15)

- Server-side TUI features go to app mods first; fork `herdrdev/herdr-core` only for a feature a mod cannot do.
- The weekly sync is a Claude cloud routine owned by the alt account.
- The fork must be designed and built so upstream merges stay as simple as possible (rules: `CLAUDE.md` § Merge-friendly by construction).

## Tasks

- [x] 1. Seed the project: `CLAUDE.md`, `docs/fork/CHANGES.md`, `scripts/fork/seams`, `scripts/fork/sync-upstream`, this plan.
- [x] 2. (done `06f37e1`: 1506 → 769 lines; window.rs 589 → 33, layout.rs 147 → 6, control.rs 42 → 2) Shrink the seams before merging: move browser code out of `src/window.rs` (589 lines), `src/layout.rs` (147) and `src/control.rs` (42) into `src/browser/`, leaving `FORK(browser)` call lines. Re-measure with `scripts/fork/seams`. Behavior unchanged; the macOS gate passes on the old base.
- [ ] 3. Merge upstream (260 commits; trial merge conflicts in 10 files) with `scripts/fork/sync-upstream` logic, record decisions in the ledger, pass the Linux and macOS gates, PR into `fork/master`. `fork/master` is 0 commits ahead of upstream, so this fast-forwards it with no rewrite.
- [x] 4. (done: cameronsjo/herdr-app#8 to #15; drops in the ledger) Gap-check each TUI-fork feature (its `docs/fork/CHANGES.md`) against upstream app and `herdr-core`; file one `cameronsjo/herdr-app` issue per missing feature, marked mod or core.
- [ ] 5. Port features in issue order, mod first.
- [x] 6. Grep the estate for consumers of the TUI `herdr` binary, deprecate `herdr.rb` in `cameronsjo/homebrew-tap`, archive `cameronsjo/herdr`. (Archived 2026-11-15 with a README notice; open issues #156/#157 transferred to cameronsjo/herdr-app#16/#17, three TUI-only issues closed. Tap deprecation held: see Deviations.)
- [ ] 7. Create the weekly sync routine on the alt account: runs `scripts/fork/sync-upstream`, resolves per `CLAUDE.md`, runs the Linux gate, opens a PR with the seams table. Never pushes `master`.

## Loop

- Sync PRs: closed by Cameron merging after the macOS gate (`scripts/browser-daily` build), or closing it with a reason.
- The routine: ends when upstream ships an official browser mod that replaces the fork's (`docs/browser-transition.md`), at which point the fork re-evaluates whether it still needs to exist.
- The TUI fork: closed by task 6.

## Alternatives declined

- Plain GitHub Actions cron sync: the TUI fork ran one and it failed 5 of 5 weeks on conflicts (runs 2026-08-10 to 2026-09-07) before it was removed.
- Forking `herdr-core` up front: a third fork to sync before any feature has shown it needs one.
- Local LaunchAgent sync on a Mac: would gate on macOS, but needs a machine awake and changes a workload machine's role.

## Panel

Panel: none (T2 shape chosen by the operator in-session; the revival PR gets a fresh-context review before merge)

## Deviations

- Plan lives at `docs/fork/plans/`, not `docs/plans/`: upstream's `.gitignore` ignores `docs/plans`, and un-ignoring it would add a seam.
- Tap formula not deprecated yet: the TUI is still a supported host in forgectl (`internal/herdr`, `HostHerdr`), cadence, herdr-orchestrator, and dotfiles (`Brewfile.darwin`, `bin/herdr-upgrade`). Archiving leaves releases installable, so nothing breaks; retiring the TUI host everywhere is the operator's call.
- Task 7 (routine) waits for this PR to merge: the routine syncs `fork/master`, which has no browser until then.
- Task 3 merged upstream at `8e1208d` (262 commits, `bf9b67c`); the macOS gate passes on `e723802`. Two fresh reviews (code, security) found two keyboard-ownership bugs, unmarked seams and sync-script gaps, fixed in `5ea3735` and `e723802`.
- Operator added auto-updates (2026-11-15): sync PRs auto-merge after CI, a fork-only macOS browser workflow, Dependabot for the fork's own crates, core updates from the fork's browser builds, and a launchd agent that builds and installs each new `fork/master` on the Mac (`docs/fork/weekly-sync.md`). Repo auto-merge settings and the ruleset wait for Actions to be enabled; the agent's install waits for this PR's merge.
- Task 7's gate is the fork's own CI on each sync PR, not a build inside the routine (`docs/fork/weekly-sync.md`): CI already runs Linux, macOS and Windows on every PR, so the routine only merges, resolves and opens the PR. That needs Actions enabled on the fork, which it has never been.
- Task 2 moved the code into child modules (`src/window/fork_browser.rs`, `src/layout/fork_browser.rs`) and `src/browser/socket.rs`, not one `src/browser/` tree: a child module sees `AppWindow`'s and `Content`'s private items, so no upstream field or method had to become public. `Content::Browser` stays an upstream-enum variant for now (cameronsjo/herdr-app#18).
- Task 2's gate on the pre-sync base skipped `files::tests::a_watched_folder_reports_changes_and_misses_none_between_waits`, which hung there before and after the extraction. FSEvents and `notify` 7.0.0 worked on the machine; pure upstream at `8e1208d` and the merged fork both pass it, after upstream's watcher rewrite (`17eac39`, `d8a0ba5`). The post-sync gate runs it.
- The gate mirrors upstream CI (`cargo test --locked`, bun tests); `cargo fmt --check` failed on upstream files the fork never touched, so fmt and clippy are not gates.

## Learnings

- The app drives `herdr-core` (`hc`), not the TUI `herdr` server, so TUI-fork features are ports, not cherry-picks.
- `browser-pane` compiles only on macOS, so a Linux-only routine cannot gate the browser build.
