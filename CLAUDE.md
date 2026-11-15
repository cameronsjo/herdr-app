# cameronsjo/herdr-app: personal fork

@AGENTS.md

Upstream's `AGENTS.md` (above) governs the code. These fork rules win where they differ; upstream's "ask Can" gate does not apply, because nothing here goes upstream.

## What this fork is

Upstream `herdrdev/herdr-app` plus the browser pane (`src/browser/`) and features ported from the archived TUI fork `cameronsjo/herdr`. Every divergence is recorded in `docs/fork/CHANGES.md`.

- Remotes: `origin` = `herdrdev/herdr-app` (fetch only), `fork` = `cameronsjo/herdr-app`.
- **MUST NOT** push, open PRs, file issues, or comment on any `herdrdev/*` repo. Fork issues live on `cameronsjo/herdr-app`.
- **MUST** pass `-R cameronsjo/herdr-app` to every `gh pr` and `gh issue` command: in a fork, `gh` defaults to the parent repo.
- `fork/master` is the mainline. Merge upstream into it; never rebase it.

## Merge-friendly by construction

Fork lines inside upstream-owned files cause every sync conflict; new files never conflict. In priority order:

1. **Mod first.** Ship a feature as a mod (`defaults/mods/<fork-mod>/`) when the mod API allows; change Rust only when it cannot.
2. **Own files, thin seams.** Fork Rust lives in fork-only modules (`src/browser/`, `src/fork/`).
3. **Mark every seam.** An upstream-owned file gets only the call, registration, or `mod` line that reaches fork code, marked `FORK(<feature>):` in a comment on or above it. This mirrors upstream's `HERDR:` patch marker, so `git grep 'FORK('` lists every seam to re-apply.
4. **Never widen an upstream signature, struct, or enum.** Add a function, wrapper, or fork-side table instead; a widened signature merges clean and breaks only at clippy.
5. **Docs in fork files.** Fork docs live in `docs/fork/` and `docs/browser-*.md`. `defaults/AGENTS.md` must still name every fork command (a `cargo test` checks it): keep them in one `FORK(browser)` block.
6. **Regenerate generated files.** `Cargo.lock`: take upstream's, then `cargo metadata --format-version 1 >/dev/null` adds the fork's deps. `THIRD-PARTY-NOTICES`: `just notices`.
7. **Measure the footprint.** `scripts/fork/seams` prints fork lines inside upstream-owned files. Every sync PR reports it; growth needs a ledger reason.

## Syncing upstream

Run `scripts/fork/sync-upstream`; it is resumable, so re-run it after resolving conflicts. The gate mirrors upstream CI, which runs `just test` and no fmt or clippy; a stricter gate fails on upstream's own code every sync. Linux: `just test`. macOS, required before merging a sync because `browser-pane` compiles only there: `cargo test --locked --features browser-webkit` too. Compiler warnings must not rise against the pre-sync tip; a widened signature shows up there first. A clean merge is not a pass:

- Before resolving, run `git diff --find-renames=30% --name-status <base> origin/master` over the seam files. A rewritten file means re-implementing its seam, not merging it.
- After taking `--theirs` on a seam file, re-apply its seams from `git grep -n 'FORK(' <base> -- <file>`.
- Count `log_safe` and escape call sites in `src/browser/` and seam files before and after; a drop is a finding.
- Record each conflict decision under the sync's dated heading in `docs/fork/CHANGES.md`.

## Building

`scripts/browser-dev-run` builds and runs the browser build in an isolated home; `scripts/browser-daily` switches the everyday app to it and back. Never stop or restart the running app or `herdr-core` from a session.
