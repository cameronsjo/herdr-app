---
status: in-progress
tier: T2
next: "Task 2: extract browser seams from src/window.rs before the upstream merge"
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
- [ ] 2. Shrink the seams before merging: move browser code out of `src/window.rs` (589 lines), `src/layout.rs` (147) and `src/control.rs` (42) into `src/browser/`, leaving `FORK(browser)` call lines. Re-measure with `scripts/fork/seams`. Behavior unchanged; the macOS gate passes on the old base.
- [ ] 3. Merge upstream (260 commits; trial merge conflicts in 10 files) with `scripts/fork/sync-upstream` logic, record decisions in the ledger, pass the Linux and macOS gates, PR into `fork/master`. `fork/master` is 0 commits ahead of upstream, so this fast-forwards it with no rewrite.
- [ ] 4. Gap-check each TUI-fork feature (its `docs/fork/CHANGES.md`) against upstream app and `herdr-core`; file one `cameronsjo/herdr-app` issue per missing feature, marked mod or core.
- [ ] 5. Port features in issue order, mod first.
- [ ] 6. Grep the estate for consumers of the TUI `herdr` binary, deprecate `herdr.rb` in `cameronsjo/homebrew-tap`, archive `cameronsjo/herdr`.
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

## Learnings

- The app drives `herdr-core` (`hc`), not the TUI `herdr` server, so TUI-fork features are ports, not cherry-picks.
- `browser-pane` compiles only on macOS, so a Linux-only routine cannot gate the browser build.
