# Weekly upstream sync

A Claude cloud routine on the alt account keeps `fork/master` current with `herdrdev/herdr-app`. It opens one draft PR per week on `cameronsjo/herdr-app`; Cameron merges it after CI passes and, for a browser change, after `scripts/browser-daily` on a Mac.

## What the routine does

1. Runs `scripts/fork/sync-upstream` (exit 0: merged or already current; 1: conflicts remain; 2: any other failure).
2. On exit 1, resolves the conflicts by `CLAUDE.md` § Syncing upstream and re-runs the script until it exits 0.
3. Records each conflict decision in `docs/fork/CHANGES.md` under the sync's dated heading, adds the footprint row from `scripts/fork/seams origin/master`, and commits.
4. Pushes the sync branch to `fork` and opens a draft PR against `master` with `gh pr create -R cameronsjo/herdr-app`.

CI (`.github/workflows/ci.yml`, Linux, macOS and Windows) is the build and test gate; the routine does not build. It never pushes `master`, never merges, and never pushes to, opens a PR on, files an issue on, or comments on any `herdrdev/*` repo.

## Routine prompt

```text
Weekly upstream sync for cameronsjo/herdr-app (a personal fork of herdrdev/herdr-app).

Read CLAUDE.md and docs/fork/CHANGES.md first; CLAUDE.md § Syncing upstream is the resolution procedure.

Setup: clone cameronsjo/herdr-app; add remote `fork` = git@github.com:cameronsjo/herdr-app.git (or the https URL) and set remote `origin` = https://github.com/herdrdev/herdr-app.git. Install Rust (rustup, for `cargo metadata`), bun, and cargo-about 0.9.3 (`cargo install --locked cargo-about --version 0.9.3 --features cli`).

1. Run scripts/fork/sync-upstream. Exit 0 with "already current": stop and report "no upstream changes". Exit 2: stop and report the error line.
2. Exit 1: for each conflicted file, read the fork's own lines with the `git diff <upstream-base> <pre-sync-tip> -- <file>` command the script printed, keep upstream's change, and re-apply every fork line (marked FORK( or not). A file upstream rewrote (rename check in CLAUDE.md) means re-implementing the seam, not merging it. git add each file, then re-run the script; repeat until it exits 0.
3. Add a dated section to docs/fork/CHANGES.md § Upstream syncs: one table row per conflicted file (what upstream changed, what the fork has, what was kept and why), the log_safe/log_escaped/public_url count before and after, and the footprint row from `scripts/fork/seams origin/master`. Commit.
4. Push the sync branch with `git push --no-follow-tags fork <branch>`. Open a draft PR: `gh pr create -R cameronsjo/herdr-app --base master --head <branch> --draft`, with the seams output and the conflict table in the body.

Never push master, never merge, never touch any herdrdev/* repo (no push, PR, issue, or comment). If a conflict needs a design decision (upstream rewrote a seam file, or a fork feature no longer fits), resolve what you can, leave that file for Cameron, and say so at the top of the PR body.
```

## Setting it up

Created from the alt account (`claude-as alt`) with `/schedule`, weekly, against `cameronsjo/herdr-app`, once `fork/master` has this file (PR #7 merged). Fork Actions must be enabled first, or the PRs get no CI.
