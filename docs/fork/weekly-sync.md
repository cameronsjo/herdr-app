# Weekly upstream sync

A Claude cloud routine on the alt account keeps `fork/master` current with `herdrdev/herdr-app`. It opens one PR per week on `cameronsjo/herdr-app`. A sync with no conflicts gets auto-merge, so it merges itself (a merge commit) once CI passes; a sync where the routine resolved conflicts stays a draft for Cameron, because CI did not catch last time's dropped fork lines. CI here is upstream's `ci.yml` (Linux, macOS, Windows; required check `test`) and the fork's `fork-browser.yml` (the WebKit browser build on macOS; required check `browser-webkit`). A PR the routine could not finish stays a draft for Cameron. After a merge, `scripts/fork/daily-update` on Cameron's Mac builds the new `fork/master` and installs it as the everyday browser build (`scripts/browser-daily`).

## What the routine does

1. Runs `scripts/fork/sync-upstream` (exit 0: merged or already current; 1: conflicts remain; 2: any other failure).
2. On exit 1, resolves the conflicts by `CLAUDE.md` § Syncing upstream and re-runs the script until it exits 0.
3. Records each conflict decision in `docs/fork/CHANGES.md` under the sync's dated heading, adds the footprint row from `scripts/fork/seams origin/master`, and commits.
4. Pushes the sync branch to `fork`, opens a PR against `master` with `gh pr create -R cameronsjo/herdr-app`, and turns on auto-merge with `gh pr merge -R cameronsjo/herdr-app --auto --merge` (a merge commit: squash or rebase would rewrite upstream's history and conflict at the next sync).

CI is the build and test gate; the routine does not build. It never pushes `master`, never merges by hand, never uses `--admin`, and never pushes to, opens a PR on, files an issue on, or comments on any `herdrdev/*` repo.

## Routine prompt

```text
Weekly upstream sync for cameronsjo/herdr-app (a personal fork of herdrdev/herdr-app).

Read CLAUDE.md and docs/fork/CHANGES.md first; CLAUDE.md § Syncing upstream is the resolution procedure.

Setup: clone cameronsjo/herdr-app; add remote `fork` = git@github.com:cameronsjo/herdr-app.git (or the https URL) and set remote `origin` = https://github.com/herdrdev/herdr-app.git. Install Rust (rustup, for `cargo metadata`), bun, and cargo-about 0.9.3 (`cargo install --locked cargo-about --version 0.9.3 --features cli`).

1. Run scripts/fork/sync-upstream. Exit 0 with "already current": stop and report "no upstream changes". Exit 2: stop and report the error line.
2. Exit 1: for each conflicted file, read the fork's own lines with the `git diff <upstream-base> <pre-sync-tip> -- <file>` command the script printed, keep upstream's change, and re-apply every fork line (marked FORK( or not). A file upstream rewrote (rename check in CLAUDE.md) means re-implementing the seam, not merging it. git add each file, then re-run the script; repeat until it exits 0.
3. Add a dated section to docs/fork/CHANGES.md § Upstream syncs: one table row per conflicted file (what upstream changed, what the fork has, what was kept and why), the log_safe/log_escaped/public_url count before and after, and the footprint row from `scripts/fork/seams origin/master`. If upstream changed an action pin in .github/workflows/ci.yml, copy the same pin into .github/workflows/fork-browser.yml. Commit.
4. Push the sync branch with `git push --no-follow-tags fork <branch>`. Open a PR: `gh pr create -R cameronsjo/herdr-app --base master --head <branch>`, with the seams output and the conflict table in the body. Then, only if step 1 exited 0 on its first run (no conflicts to resolve): `gh pr merge -R cameronsjo/herdr-app <number> --auto --merge` (merge commit only; never --squash, --rebase or --admin). If you resolved any conflict, or a file was left for Cameron (below), open the PR with --draft, skip auto-merge, and say at the top of the body which files need a person's look. If `--auto` is refused, leave the PR open and report it.

Never push master, never merge by hand or with --admin, never touch any herdrdev/* repo (no push, PR, issue, or comment). If a conflict needs a design decision (upstream rewrote a seam file, or a fork feature no longer fits), resolve what you can, leave that file for Cameron, and say so at the top of the PR body.
```

## Setting it up

Once, in this order:

1. Enable Actions on the fork (Actions tab, the button for forked repositories). Until then no PR gets CI.
2. Merge PR #7, so `fork/master` has these files and both workflows have run once.
3. Repo settings (done 2026-11-15): `gh api -X PATCH repos/cameronsjo/herdr-app -F allow_auto_merge=true -F allow_merge_commit=true -F allow_squash_merge=false -F allow_rebase_merge=false -F delete_branch_on_merge=true`, and ruleset `master: CI before merge` (id 27338965) on the default branch: required checks `test` and `browser-webkit`, no force-push, no deletion, repository admins may bypass. Without the ruleset, auto-merge merges at once.
4. The routine: from the alt account (`claude-as alt`) with `/schedule`, weekly, against `cameronsjo/herdr-app`.
5. The Mac side: `bash scripts/fork/daily-update install`, then `bash ~/.local/share/herdr-fork/daily-update check` for the first build. It builds only a `fork/master` whose `test` and `browser-webkit` checks passed, so what it compiles and installs is what CI built: the same trust as installing upstream's release, plus the fork's reviewed code.

## Dependency updates

Dependabot (`.github/dependabot.yml`) bumps only `tungstenite`, the one fork-only crate that stands alone (`cef` is pinned to its binary; `objc2-web-kit` and `block2` move with upstream's `objc2` crates), weekly. `allow` may also limit Dependabot security PRs to that list; the repo's Dependabot alerts still show every advisory. Upstream's GitHub Actions entry stays in the file with its version PRs off on the fork: upstream bumps `ci.yml`'s pins and each sync brings them. `fork-browser.yml` reuses `ci.yml`'s actions, so each sync copies their pins over (routine step 3). Dependabot PRs get CI like any other, but merge by hand.

## Core updates

Release builds of the app offer `herdr-core` updates; source builds don't. The fork's browser builds offer them too (`src/update.rs` `checks_core`, one `FORK(browser)` line), within the core's protocol generation, as upstream does.
