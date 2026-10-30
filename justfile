set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

notes-clip scene version="":
    bun scripts/notes-media.ts clip "{{scene}}" "{{version}}"

notes-upload version:
    bun scripts/notes-media.ts upload "{{version}}"

notes-check version:
    bun scripts/notes-media.ts check "{{version}}"

check: test

notices:
    bun scripts/notices.ts

# Before a release: tests, docs, the next version's notes and clips, then what
# changed since the last release, to review by hand.
[unix]
pre-release: test docs-check
    #!/usr/bin/env bash
    set -euo pipefail
    version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
    last=$(git describe --tags --abbrev=0 --match 'v*')
    if grep -q "^## \[$version\]" CHANGELOG.md; then
      bun scripts/notes-media.ts check "$version"
    else
      echo "note: CHANGELOG.md has no [$version] entry yet: write the release notes before tagging."
    fi
    echo; echo "Since $last:"; git log --oneline "$last"..HEAD
    echo
    if git diff --quiet "$last"..HEAD -- src/SKILL.md; then
      echo "review: the agent manual (herdr-app --skill, src/SKILL.md) is unchanged since $last. Does it still match the changes above?"
    else
      echo "review: the agent manual (herdr-app --skill) changed since $last. Read it once more."
    fi
    echo "review: the mod guide (defaults/AGENTS.md), README.md and docs/ describe the changes above."

build:
    cargo build --locked

test:
    cargo test --locked

docs-generate: build
    bun scripts/docs.ts generate

docs-check: build
    bun test scripts/docs.test.ts
    bun scripts/docs.ts check

# Builds target/browser-dev/Herdr Dev.app (scripts/browser-dev-bundle): the
# `browser` feature, CEF's framework and helpers. CEF_PATH (default
# ~/.cache/herdr-app/cef) keeps CEF's ~600 MB download outside the repo; a
# bare `cargo build --features browser` without it is refused (build.rs).
# Herdr with Chromium browser panes, built but not started (macOS)
[macos]
browser-build *args:
    bash scripts/browser-dev-bundle {{args}}

# Runs the dev bundle on a scratch home and its own core, never your real
# ones. `$root/herdr-app` talks to it, e.g. `$root/herdr-app do space.new`,
# then `$root/herdr-app do browser.open --url <url>` (test pages: run
# `just browser-fixture`, then http://127.0.0.1:8765/browser-fixture.html).
# Quit it (cmd-q) to end; the scratch folder
# stays for its logs.
# Herdr with Chromium browser panes, on a scratch home (macOS)
[macos]
browser-dev *args:
    #!/usr/bin/env bash
    set -euo pipefail
    bash scripts/browser-dev-bundle {{args}}
    # Short: hc's socket path inside it has a 103-byte limit.
    root=$(mktemp -d /tmp/hbd.XXXXXX)
    export HERDR_APP_HOME=$root/home XDG_STATE_HOME=$root/state HC_SESSION=hbd-${root##*.}
    export HERDR_APP_ID=herdr-browser-dev HERDR_APP_NO_SOUND=1 HERDR_TELEMETRY=0
    app="$PWD/target/browser-dev/Herdr Dev.app/Contents/MacOS/Herdr"
    printf '#!/bin/bash\nHERDR_APP_HOME=%q exec %q "$@"\n' "$HERDR_APP_HOME" "$app" > "$root/herdr-app"
    chmod +x "$root/herdr-app"
    echo "scratch: $root (log: $root/app.log)"
    echo "drive it: just browser-fixture (another terminal); $root/herdr-app do space.new; $root/herdr-app do browser.open --profile agent --url http://127.0.0.1:8765/browser-fixture.html"
    # Stops only this run's core (HC_SESSION, XDG_STATE_HOME above).
    trap 'hc server stop >/dev/null 2>&1 || true' EXIT
    "$app" 2>&1 | tee "$root/app.log"

# Serves scripts/ on http://127.0.0.1:8765 for browser panes (they open
# http, https and about:blank only): open
# http://127.0.0.1:8765/browser-fixture.html with --profile agent.
# Serves scripts/ over loopback http for browser panes
browser-fixture port="8765":
    python3 -m http.server {{port}} --bind 127.0.0.1 --directory scripts

[linux]
install:
    bash scripts/install-linux

[macos]
install:
    bash scripts/install-macos

[windows]
install:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/install-windows.ps1
