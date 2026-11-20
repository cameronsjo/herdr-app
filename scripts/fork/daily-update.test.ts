// FORK(browser): tests for scripts/fork/daily-update's `check`, run against
// stand-ins for gh, pmset and osascript in a temporary HOME, and a fixture repo
// whose fork/master commit carries a changed copy of the script. Nothing here
// touches the network, ~/Applications or the real agent.
//
// On macOS the script runs under /bin/bash (3.2), as launchd runs it: bash 3.2
// fires the ERR trap for a failing `command grep` inside an `if`, which bash 5
// does not, and that turned every check into "check failed" for five days.
import { test, expect, beforeEach, afterEach } from "bun:test";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, rmSync, chmodSync, copyFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFileSync, spawnSync } from "node:child_process";

const SCRIPT = join(import.meta.dir, "daily-update");
const BASH = existsSync("/bin/bash") ? "/bin/bash" : "bash";
const skip = process.platform === "win32";

let dir = "", home = "", root = "", state = "", self = "", gatedSha = "";

function git(cwd: string, ...args: string[]) {
  return execFileSync("git", args, { cwd, encoding: "utf8", env: { ...process.env, GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_NOSYSTEM: "1" } }).trim();
}

function stub(name: string, body: string) {
  const p = join(home, ".cargo/bin", name);
  writeFileSync(p, `#!/bin/bash\n${body}\n`);
  chmodSync(p, 0o755);
}

// gh answers the four calls check makes; STUB_PR empty means "not a PR merge".
function stubGh(runs: string, pr = "57") {
  stub("gh", `
case "$*" in
  *"/commits/"*"/pulls"*) [ -n ${JSON.stringify(pr)} ] && echo ${JSON.stringify(pr)}; exit 0 ;;
  *"actions/runs?head_sha="*) printf '%b\\n' ${JSON.stringify(runs)} ;;
  *"workflows/fork-mac-build.yml/runs"*) echo completed ;;
  "release view"*) exit 1 ;;
  *) echo "gh stub: unexpected: $*" >&2; exit 3 ;;
esac`);
}

beforeEach(() => {
  if (skip) return;
  dir = mkdtempSync(join(tmpdir(), "daily-update-test-"));
  home = join(dir, "home");
  root = join(dir, "herdr-app");
  state = join(home, ".local/share/herdr-fork");
  self = join(state, "daily-update");
  mkdirSync(join(home, ".cargo/bin"), { recursive: true });
  mkdirSync(join(home, "Library/Logs"), { recursive: true });
  mkdirSync(state, { recursive: true });
  // The fixture repo: fork/master is a commit whose script differs from the agent's.
  mkdirSync(join(root, "scripts/fork"), { recursive: true });
  git(dir, "init", "-q", "-b", "master", root);
  git(root, "remote", "add", "fork", "git@github.com:cameronsjo/herdr-app.git");
  const newer = readFileSync(SCRIPT, "utf8") + "\n# the gated commit's version\n";
  writeFileSync(join(root, "scripts/fork/daily-update"), newer);
  git(root, "add", ".");
  git(root, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "gated");
  gatedSha = git(root, "rev-parse", "HEAD");
  git(root, "update-ref", "refs/remotes/fork/master", gatedSha);
  // The agent's copy runs from outside any checkout, so it finds the repo through state.
  copyFileSync(SCRIPT, self);
  chmodSync(self, 0o755);
  writeFileSync(join(state, "root"), root + "\n");
  // git: real, except fetch (no network); the script puts $HOME/.cargo/bin first on PATH.
  const realGit = execFileSync("sh", ["-c", "command -v git"], { encoding: "utf8" }).trim();
  stub("git", `for a in "$@"; do [ "$a" = fetch ] && exit 0; done\nexport GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1\nexec ${realGit} "$@"`);
  stub("pmset", `echo "Now drawing from 'Battery Power'"`);
  stub("osascript", `echo "$*" >> "$HOME/notified.log"`);
});

afterEach(() => {
  if (dir) rmSync(dir, { recursive: true, force: true });
});

function check() {
  const r = spawnSync(BASH, [self, "check"], { encoding: "utf8", env: { HOME: home, PATH: process.env.PATH ?? "/usr/bin:/bin" } });
  return { code: r.status, out: (r.stdout ?? "") + (r.stderr ?? "") };
}

const GREEN = [".github/workflows/ci.yml success", ".github/workflows/fork-browser.yml success", ".github/workflows/fork-mods.yml success"].join("\n");

test.skipIf(skip)("a commit whose CI failed leaves the agent's script byte-identical", () => {
  const before = readFileSync(self);
  stubGh([".github/workflows/ci.yml failure", ".github/workflows/ci.yml success", ".github/workflows/fork-browser.yml success"].join("\n"));
  const r = check();
  expect(r.out).toContain("a run of .github/workflows/ci.yml failed");
  expect(r.out).toContain("not building");
  expect(r.code).toBe(0);
  expect(readFileSync(self).equals(before)).toBe(true);
  expect(existsSync(join(state, "script-from"))).toBe(false);
});

test.skipIf(skip)("a commit that is not a PR merge leaves the agent's script byte-identical", () => {
  const before = readFileSync(self);
  stubGh(GREEN, "");
  const r = check();
  expect(r.out).toContain("not the merge commit of a PR merged into master");
  expect(r.code).toBe(0);
  expect(readFileSync(self).equals(before)).toBe(true);
});

test.skipIf(skip)("green CI with no failed run passes the check (bash 3.2 ERR trap)", () => {
  stubGh(GREEN);
  const r = check();
  expect(r.out).not.toContain("check failed");
  expect(r.code).toBe(0);
});

test.skipIf(skip)("a gated commit refreshes the script before the download, even when no build follows", () => {
  stubGh(GREEN);
  const r = check();
  expect(r.code).toBe(0);
  // No release and on battery: nothing was built, but the gated script is in place.
  expect(r.out).toContain("local build skipped");
  expect(existsSync(join(state, "built"))).toBe(false);
  expect(readFileSync(self, "utf8")).toBe(readFileSync(join(root, "scripts/fork/daily-update"), "utf8"));
  expect(readFileSync(join(state, "script-from"), "utf8").trim()).toBe(`fork/master ${gatedSha}`);
});
