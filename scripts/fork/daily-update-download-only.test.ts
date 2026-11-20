// FORK(browser): tests for scripts/fork/daily-update's download-only mode (a Mac
// with no checkout), run against stand-ins in a temporary HOME. The gh stand-in
// answers `attestation verify` only when every pin the script must pass is
// there with its exact value, and only for a file whose sha256 a fixture marks
// as attested for that commit, so dropping a pin or flipping a byte fails here.
// codesign, spctl and xcrun stand in for the signature checks (STUB_SIGN), and
// ditto for zip and copy. Nothing here touches the network or the real agent.
// On macOS the script runs under /bin/bash (3.2), as launchd runs it.
import { test, expect, beforeEach, afterEach } from "bun:test";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, rmSync, chmodSync, copyFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";

const SCRIPT = join(import.meta.dir, "daily-update");
const BASH = existsSync("/bin/bash") ? "/bin/bash" : "bash";
const skip = process.platform === "win32";
const REPO = "cameronsjo/herdr-app";
const SHA = "a".repeat(40);
const OLD = "b".repeat(40);
const TAG = `browser-${SHA}`;

let dir = "", home = "", state = "", self = "", fixtures = "", daily = "";

const sha256 = (p: string) => createHash("sha256").update(readFileSync(p)).digest("hex");

function stub(name: string, body: string) {
  const p = join(home, ".cargo/bin", name);
  writeFileSync(p, `#!/bin/bash\n${body}\n`);
  chmodSync(p, 0o755);
}

// What GitHub's compare/<base>...<head> answers for one pair.
function compare(base: string, head: string, status: string, ahead: number, behind: number) {
  mkdirSync(join(fixtures, "compare"), { recursive: true });
  writeFileSync(join(fixtures, "compare", `${base}...${head}`), JSON.stringify({ status, ahead_by: ahead, behind_by: behind }));
}

// Marks a fixture file as attested by fork-mac-build.yml at commit <sha>. Keyed
// by digest, as gh looks attestations up; the statement names the subject <name>.
function attest(name: string, sha = SHA) {
  writeFileSync(join(fixtures, "attested", sha256(join(fixtures, "assets", name))), `${name} ${sha}\n`);
}

beforeEach(() => {
  if (skip) return;
  dir = mkdtempSync(join(tmpdir(), "daily-update-dl-test-"));
  home = join(dir, "home");
  state = join(home, ".local/share/herdr-fork");
  self = join(state, "daily-update");
  fixtures = join(dir, "fixtures");
  daily = join(home, "Applications/Herdr Browser.app");
  for (const d of [join(home, ".cargo/bin"), join(home, "Library/Logs"), state, join(fixtures, "assets"), join(fixtures, "attested")]) mkdirSync(d, { recursive: true });
  copyFileSync(SCRIPT, self);
  chmodSync(self, 0o755);
  writeFileSync(join(state, "mode"), "download-only\n");

  // Release assets: an app zip and a newer daily-update, both attested at SHA.
  const app = join(dir, "build/Herdr Dev.app/Contents/MacOS");
  mkdirSync(app, { recursive: true });
  writeFileSync(join(app, "herdr"), "#!/bin/sh\n");
  execFileSync("zip", ["-qr", join(fixtures, "assets/herdr-browser-webkit.zip"), "Herdr Dev.app"], { cwd: join(dir, "build") });
  writeFileSync(join(fixtures, "assets/daily-update"), readFileSync(SCRIPT, "utf8") + "\n# release version\n");
  attest("herdr-browser-webkit.zip");
  attest("daily-update");

  const pins = [
    `--repo ${REPO}`,
    `--cert-identity https://github.com/${REPO}/.github/workflows/fork-mac-build.yml@refs/heads/master`,
    "--cert-oidc-issuer https://token.actions.githubusercontent.com",
    "--source-ref refs/heads/master",
    "--predicate-type https://slsa.dev/provenance/v1",
    "--deny-self-hosted-runners",
    "--format json",
  ];
  stub("gh", `
F=${JSON.stringify(fixtures)}
echo "gh $*" >> "$HOME/gh.log"
# An empty or default config dir means gh could fall back to its own login.
case "$GH_CONFIG_DIR" in ""|"$HOME/.config/gh") echo "gh stub: GH_CONFIG_DIR is '$GH_CONFIG_DIR'" >&2; exit 9 ;; esac
[ -n "$GH_TOKEN" ] || { echo "gh stub: no GH_TOKEN" >&2; exit 9; }
jqx=''; prev=''; for a in "$@"; do [ "$prev" = --jq ] && jqx=$a; prev=$a; done
answer() { if [ -n "$jqx" ]; then jq -r "$jqx" <<<"$1"; else echo "$1"; fi; }
fail_http() { echo "gh: $1 (HTTP $2)" >&2; exit 1; }
case "$1 $2" in
  "release list")
    case "\${STUB_LIST_HTTP:-}" in
      401) echo "HTTP 401: Bad credentials (https://api.github.com/graphql)" >&2; exit 1 ;;
      403) echo "HTTP 403: Resource not accessible by personal access token (https://api.github.com/graphql)" >&2; exit 1 ;;
      403rl) echo "HTTP 403: API rate limit exceeded for user ID 1. (https://api.github.com/graphql)" >&2; exit 1 ;;
      403long) echo 'gh: Resource not accessible by personal access token. This fine-grained token lacks a permission the endpoint needs; see https://docs.github.com/rest/overview/permissions-required-for-fine-grained-personal-access-tokens for which one, then regenerate it (HTTP 403)' >&2; exit 1 ;;
    esac
    def='[{"tagName":"${TAG}"}]'; answer "\${STUB_RELEASES:-$def}"; exit 0 ;;
  "release download")
    shift 2; tag=$1; shift; dest='' pat=''
    while [ $# -gt 0 ]; do case $1 in -p) pat=$2; shift 2 ;; -D) dest=$2; shift 2 ;; *) shift ;; esac; done
    [ -f "$F/assets/$pat" ] || exit 1
    cp "$F/assets/$pat" "$dest/$pat"; exit 0 ;;
  "attestation verify")
    file=$3 args=" $* " src='' signer='' jqx=''
    for pin in ${pins.map((p) => JSON.stringify(p)).join(" ")}; do
      case "$args" in *" $pin "*) ;; *) echo "gh stub: missing pin: $pin" >> "$HOME/gh.log"; exit 1 ;; esac
    done
    while [ $# -gt 0 ]; do case $1 in --source-digest) src=$2; shift 2 ;; --signer-digest) signer=$2; shift 2 ;; --jq) jqx=$2; shift 2 ;; *) shift ;; esac; done
    [ -z "\${STUB_ATTEST_403:-}" ] || { echo "HTTP 403: Resource not accessible by personal access token" >&2; exit 1; }
    got=$(shasum -a 256 "$file" | cut -d' ' -f1)
    rec="$F/attested/$got"
    [ -f "$rec" ] || { echo "Error: no attestations found for subject" >&2; exit 1; }
    read -r subj_name want_sha < "$rec"
    [ "$src" = "$want_sha" ] && [ "$signer" = "$want_sha" ] || exit 1
    json='[{"verificationResult":{"statement":{"subject":[{"name":"'"$subj_name"'","digest":{"sha256":"'"$got"'"}}]}}}]'
    answer "$json"; exit 0 ;;
esac
case "$*" in
  *"/git/ref/tags/browser-"*)
    case "\${STUB_TAG_HTTP:-}" in 404) fail_http "Not Found" 404 ;; 5*) fail_http "Server Error" "$STUB_TAG_HTTP" ;; esac
    read -r t s <<<"\${STUB_TAG_POINTS:-commit ${SHA}}"
    answer '{"object":{"type":"'"$t"'","sha":"'"$s"'"}}' ;;
  *"/compare/"*)
    [ "\${STUB_COMPARE_HTTP:-}" != 403long ] || { echo 'gh: Resource not accessible by personal access token. This fine-grained token lacks a permission the endpoint needs; see https://docs.github.com/rest/overview/permissions-required-for-fine-grained-personal-access-tokens for which one, then regenerate it (HTTP 403)' >&2; exit 1; }
    [ -z "\${STUB_COMPARE_HTTP:-}" ] || fail_http "Server Error" "$STUB_COMPARE_HTTP"
    path=$2; pair=\${path##*/compare/}
    if [ -f "$F/compare/$pair" ]; then
      body=$(cat "$F/compare/$pair")
      case $body in "HTTP "*) fail_http "Server Error" "\${body#HTTP }" ;; esac
      answer "$body"
    else case $pair in *...master) answer '{"status":"identical","ahead_by":0,"behind_by":0}' ;; *) answer '{"status":"ahead","ahead_by":1,"behind_by":0}' ;; esac; fi ;;
  *"/commits/"*"/pulls"*) echo 57 ;;
  *"actions/runs?head_sha="*) printf '%b\\n' "\${STUB_RUNS:-.github/workflows/ci.yml success\\n.github/workflows/fork-browser.yml success}" ;;
  *) echo "gh stub: unexpected: $*" >&2; exit 3 ;;
esac`);
  stub("ditto", `
if [ "$1" = -x ]; then mkdir -p "$4" && cd "$4" && unzip -q "$3"; else cp -R "$1" "$2"; fi`);
  stub("codesign", `
echo "codesign $*" >> "$HOME/codesign.log"
case " $* " in *" --sign "*) exit 0 ;; esac
case "\${STUB_SIGN:-devid}" in devid|unnotarized) exit 0 ;; *) exit 1 ;; esac`);
  stub("spctl", `[ "\${STUB_SIGN:-devid}" = devid ] && echo "source=Notarized Developer ID" || { echo rejected; exit 3; }`);
  stub("xcrun", `[ "\${STUB_SIGN:-devid}" = devid ]`);
  stub("xattr", `exit 0`);
  stub("pgrep", `[ -n "\${STUB_RUNNING:-}" ]`);
  stub("security", `
echo "security $*" >> "$HOME/security.log"
case "$1" in
  find-generic-password) [ -n "\${STUB_KEYCHAIN:-}" ] || exit 44; case " $* " in *" -w "*) echo "$STUB_KEYCHAIN" ;; esac ;;
  *) exit 0 ;;
esac`);
  stub("launchctl", `echo "launchctl $*" >> "$HOME/launchctl.log"`);
  stub("plutil", `exit 0`);
  // mv: real, except moving the new app in when STUB_MV_FAIL is set.
  const realMv = existsSync("/bin/mv") ? "/bin/mv" : "/usr/bin/mv";
  stub("mv", `case "$1" in *".app.next") [ -n "\${STUB_MV_FAIL:-}" ] && { echo "mv: stub failure" >&2; exit 1; } ;; esac\nexec ${realMv} "$@"`);
  stub("osascript", `echo "$*" >> "$HOME/notified.log"`);
});

afterEach(() => {
  if (dir) rmSync(dir, { recursive: true, force: true });
});

function run(args: string[], env: Record<string, string> = {}) {
  const r = spawnSync(BASH, [self, ...args], { encoding: "utf8", env: { HOME: home, PATH: process.env.PATH ?? "/usr/bin:/bin", GH_TOKEN: "test-token", ...env } });
  return { code: r.status, out: (r.stdout ?? "") + (r.stderr ?? "") };
}
const read = (p: string) => (existsSync(p) ? readFileSync(p, "utf8") : "");
const notified = () => read(join(home, "notified.log"));

test.skipIf(skip)("a verified, notarized release installs as is and stages its newer script", () => {
  const before = readFileSync(self);
  const r = run(["check"]);
  expect(r.code).toBe(0);
  expect(r.out).toContain("(Developer ID signed)");
  expect(read(join(state, "installed")).trim()).toBe(SHA);
  expect(existsSync(join(daily, "Contents/MacOS/herdr"))).toBe(true);
  // Never re-signed, and the running script is untouched until `adopt`.
  expect(read(join(home, "codesign.log"))).not.toContain("--sign");
  expect(readFileSync(self).equals(before)).toBe(true);
  expect(read(join(state, "staged/sha")).trim()).toBe(SHA);
  expect(notified()).toContain(`adopt ${SHA}`);
  expect(notified()).not.toContain("check failed");
});

test.skipIf(skip)("a flipped byte in the app zip is refused and nothing is installed", () => {
  const zip = join(fixtures, "assets/herdr-browser-webkit.zip");
  const b = readFileSync(zip); b[b.length - 1] ^= 1; writeFileSync(zip, b);
  const r = run(["check"]);
  expect(r.out).toContain("did not verify");
  expect(existsSync(daily)).toBe(false);
  expect(existsSync(join(state, "installed"))).toBe(false);
  expect(notified()).toContain("was not installed");
});

test.skipIf(skip)("attested bytes under another asset's name are refused (subject name)", () => {
  // The release's script, attested as daily-update, uploaded as the app zip.
  copyFileSync(join(fixtures, "assets/daily-update"), join(fixtures, "assets/herdr-browser-webkit.zip"));
  const r = run(["check"]);
  expect(r.out).toContain("herdr-browser-webkit.zip did not verify");
  expect(existsSync(daily)).toBe(false);
});

test.skipIf(skip)("a flipped byte in the script asset is refused and nothing is staged", () => {
  const s = join(fixtures, "assets/daily-update");
  writeFileSync(s, readFileSync(s, "utf8") + "x");
  const r = run(["check"]);
  expect(read(join(state, "installed")).trim()).toBe(SHA);
  expect(r.out).toContain("its daily-update did not verify");
  expect(existsSync(join(state, "staged"))).toBe(false);
});

test.skipIf(skip)("a release that is not ahead of the installed commit is refused (downgrade floor)", () => {
  writeFileSync(join(state, "installed"), OLD + "\n");
  compare(OLD, SHA, "behind", 0, 2);
  const r = run(["check"]);
  expect(r.out).toContain("is not ahead of the installed");
  expect(read(join(state, "installed")).trim()).toBe(OLD);
  expect(existsSync(daily)).toBe(false);
});

test.skipIf(skip)("a release whose commit is not an ancestor of master is never chosen", () => {
  compare(SHA, "master", "diverged", 2, 1);
  const r = run(["check"]);
  expect(r.out).toContain("no browser-<sha> release on master yet");
  expect(existsSync(daily)).toBe(false);
});

// I4: a re-run of an old commit publishes later and is listed first.
const RERUN = JSON.stringify([{ tagName: `browser-${OLD}` }, { tagName: TAG }]);

test.skipIf(skip)("a re-run of an old commit does not win a first install", () => {
  compare(OLD, "master", "ahead", 3, 0);
  const r = run(["check"], { STUB_RELEASES: RERUN });
  expect(r.code).toBe(0);
  expect(read(join(state, "installed")).trim()).toBe(SHA);
});

test.skipIf(skip)("a re-run of an old commit does not stall an installed Mac", () => {
  writeFileSync(join(state, "installed"), SHA + "\n");
  compare(OLD, "master", "ahead", 3, 0);
  const r = run(["check"], { STUB_RELEASES: RERUN });
  expect(r.out).toContain(`browser-${SHA.slice(0, 8)} already installed`);
  expect(r.out).not.toContain("refused");
  expect(notified()).not.toContain("was not installed");
});

test.skipIf(skip)("a first install refuses a release more than 5 commits behind master", () => {
  compare(SHA, "master", "ahead", 6, 0);
  const r = run(["check"]);
  expect(r.out).toContain("is 6 commits behind master, more than 5");
  expect(existsSync(daily)).toBe(false);
});

test.skipIf(skip)("a first install takes a release 5 commits behind master", () => {
  compare(SHA, "master", "ahead", 5, 0);
  const r = run(["check"]);
  expect(read(join(state, "installed")).trim()).toBe(SHA);
});

// N3: no answer from the API is a retry, not a refusal.
test.skipIf(skip)("a tag lookup the API does not answer is retried, not refused", () => {
  const r = run(["check"], { STUB_TAG_HTTP: "502" });
  expect(r.out).toContain("the next check tries again");
  expect(r.out).not.toContain("refused");
  expect(notified()).toBe("");
});

test.skipIf(skip)("a tag that does not exist is refused", () => {
  const r = run(["check"], { STUB_TAG_HTTP: "404" });
  expect(r.out).toContain("refused: tag");
});

test.skipIf(skip)("a compare the API does not answer is retried, not refused", () => {
  const r = run(["check"], { STUB_COMPARE_HTTP: "500" });
  expect(r.out).toContain("the next check tries again");
  expect(r.out).not.toContain("refused");
  expect(existsSync(daily)).toBe(false);
});

test.skipIf(skip)("a floor compare the API does not answer is retried, not refused", () => {
  writeFileSync(join(state, "installed"), OLD + "\n");
  mkdirSync(join(fixtures, "compare"), { recursive: true });
  writeFileSync(join(fixtures, "compare", `${OLD}...${SHA}`), "HTTP 503");
  const r = run(["check"]);
  expect(r.out).toContain("(compare: gh: Server Error (HTTP 503)");
  expect(r.out).toContain("the next check tries again");
  expect(r.out).not.toContain("refused");
  expect(read(join(state, "installed")).trim()).toBe(OLD);
});

// I5: gh's own error stays in the log.
test.skipIf(skip)("a failed attestation check logs gh's error (a 403 is not a forgery)", () => {
  const r = run(["check"], { STUB_ATTEST_403: "1" });
  expect(r.out).toContain("HTTP 403: Resource not accessible by personal access token");
  // docs/browser-dev.md § Another Mac: a token without Attestations: read reads
  // as a verification refusal, not a setup error.
  expect(r.code).toBe(0);
  expect(r.out).toContain("refused: herdr-browser-webkit.zip did not verify");
  expect(r.out).not.toContain("setup error");
  expect(existsSync(daily)).toBe(false);
});

test.skipIf(skip)("a tag that points elsewhere is refused", () => {
  const r = run(["check"], { STUB_TAG_POINTS: `commit ${OLD}` });
  expect(r.out).toContain(`does not point at commit ${SHA}`);
  expect(existsSync(daily)).toBe(false);
});

test.skipIf(skip)("an ad hoc build is refused, never re-signed", () => {
  const r = run(["check"], { STUB_SIGN: "adhoc" });
  expect(r.out).toContain("download-only installs only a notarized Developer ID build");
  expect(existsSync(daily)).toBe(false);
  expect(read(join(home, "codesign.log"))).not.toContain("--sign");
});

test.skipIf(skip)("a Developer ID build that fails the notarization check is refused", () => {
  const r = run(["check"], { STUB_SIGN: "unnotarized" });
  expect(r.out).toContain("refused");
  expect(existsSync(daily)).toBe(false);
});

test.skipIf(skip)("red CI installs nothing", () => {
  const r = run(["check"], { STUB_RUNS: ".github/workflows/ci.yml failure\n.github/workflows/ci.yml success\n.github/workflows/fork-browser.yml success" });
  expect(r.out).toContain("not installing");
  expect(existsSync(daily)).toBe(false);
});

test.skipIf(skip)("while Herdr Browser runs, nothing is downloaded", () => {
  const r = run(["check"], { STUB_RUNNING: "1" });
  expect(r.out).toContain("Herdr Browser is running");
  expect(read(join(home, "gh.log"))).not.toContain("release download");
  expect(existsSync(join(state, "installed"))).toBe(false);
});

test.skipIf(skip)("no token: exits 2 and never uses gh's own login", () => {
  const r = run(["check"], { GH_TOKEN: "" });
  expect(r.code).toBe(2);
  expect(r.out).toContain("no Keychain item herdr-fork-gh-token");
  expect(read(join(home, "gh.log"))).toBe("");
});

test.skipIf(skip)("the Keychain token is used when GH_TOKEN is unset", () => {
  const r = run(["check"], { GH_TOKEN: "", STUB_KEYCHAIN: "kc-token" });
  expect(r.code).toBe(0);
  expect(r.out).toContain("gh token: Keychain item herdr-fork-gh-token");
});

test.skipIf(skip)("adopt re-verifies the staged bytes, then puts them in place", () => {
  run(["check"]);
  const staged = join(state, "staged/daily-update");
  expect(existsSync(staged)).toBe(true);
  const r = run(["adopt", SHA]);
  expect(r.code).toBe(0);
  expect(readFileSync(self, "utf8")).toBe(readFileSync(join(fixtures, "assets/daily-update"), "utf8"));
  expect(read(join(state, "script-from"))).toContain(`release browser-${SHA}`);
  expect(existsSync(join(state, "staged"))).toBe(false);
});

test.skipIf(skip)("adopt refuses staged bytes changed after staging", () => {
  run(["check"]);
  const before = readFileSync(self);
  const staged = join(state, "staged/daily-update");
  writeFileSync(staged, readFileSync(staged, "utf8") + "echo injected\n");
  const r = run(["adopt", SHA]);
  expect(r.code).toBe(1);
  expect(r.out).toContain("no longer verifies");
  expect(readFileSync(self).equals(before)).toBe(true);
});

test.skipIf(skip)("adopt refuses a sha other than the staged one", () => {
  run(["check"]);
  const r = run(["adopt", OLD]);
  expect(r.code).toBe(1);
  expect(r.out).toContain("nothing staged for");
});

// N2: one set of pins. The checkout mode's download and download-only both go
// through verify_attested; a second `gh attestation verify` could drift.
test("every attestation check goes through verify_attested", () => {
  const src = readFileSync(SCRIPT, "utf8").split("\n").filter((l) => !l.trimStart().startsWith("#"));
  expect(src.filter((l) => /\bgh attestation verify "\$/.test(l)).length).toBe(1);
  const fetch = readFileSync(SCRIPT, "utf8").split(/\nfetch_release\(\) \{\n/)[1].split(/\n\}\n/)[0];
  expect(fetch).toContain('verify_attested "$dir/$asset" "$sha"');
});

// N4: the bootstrap, as a file run with bash. It shares the stand-ins above;
// the agent path starts empty.
const BOOTSTRAP = join(import.meta.dir, "bootstrap-download-only");
function bootstrap(env: Record<string, string> = {}) {
  rmSync(self, { force: true });
  rmSync(join(state, "mode"), { force: true });
  const r = spawnSync(BASH, [BOOTSTRAP], {
    encoding: "utf8",
    env: { HOME: home, USER: "test", PATH: `${join(home, ".cargo/bin")}:${process.env.PATH}`, STUB_KEYCHAIN: "kc-token", ...env },
  });
  return { code: r.status, out: (r.stdout ?? "") + (r.stderr ?? "") };
}

test.skipIf(skip)("bootstrap installs the verified release script and sets up the agent", () => {
  const r = bootstrap();
  expect(r.code).toBe(0);
  expect(r.out).toContain(`release ${TAG}`);
  expect(readFileSync(self, "utf8")).toBe(readFileSync(join(fixtures, "assets/daily-update"), "utf8"));
  expect(read(join(state, "mode")).trim()).toBe("download-only");
  expect(read(join(state, "script-from"))).toContain(`release browser-${SHA}, bootstrap`);
  expect(read(join(home, "launchctl.log"))).toContain("bootstrap");
  expect(read(join(state, "installed")).trim()).toBe(SHA);
});

test.skipIf(skip)("bootstrap refuses a script asset with a flipped byte and installs nothing", () => {
  const f = join(fixtures, "assets/daily-update");
  writeFileSync(f, readFileSync(f, "utf8") + "x");
  const r = bootstrap();
  expect(r.code).toBe(1);
  expect(r.out).toContain("did not verify");
  expect(existsSync(self)).toBe(false);
});

test.skipIf(skip)("bootstrap refuses attested bytes that are not the script (subject name)", () => {
  copyFileSync(join(fixtures, "assets/herdr-browser-webkit.zip"), join(fixtures, "assets/daily-update"));
  const r = bootstrap();
  expect(r.code).toBe(1);
  expect(existsSync(self)).toBe(false);
});

test.skipIf(skip)("bootstrap picks the release closest to master, not a re-run listed first", () => {
  compare(OLD, "master", "ahead", 3, 0);
  const r = bootstrap({ STUB_RELEASES: RERUN });
  expect(r.out).toContain(`release ${TAG} (0 commits behind master)`);
});

test.skipIf(skip)("bootstrap refuses a tag that points elsewhere", () => {
  const r = bootstrap({ STUB_TAG_POINTS: `commit ${OLD}` });
  expect(r.code).toBe(1);
  expect(existsSync(self)).toBe(false);
});

test("bootstrap never exports the token", () => {
  const src = readFileSync(BOOTSTRAP, "utf8");
  expect(src).not.toMatch(/^\s*export\s+GH_TOKEN/m);
  expect(src).not.toMatch(/^\s*GH_TOKEN=[^ ]*$/m);
  // The sourcing check comes before set -euo pipefail, so a sourcing shell keeps its options.
  expect(src.indexOf("return 2 2>/dev/null || exit 2")).toBeGreaterThan(0);
  expect(src.indexOf("return 2 2>/dev/null || exit 2")).toBeLessThan(src.search(/^set -euo pipefail$/m));
});

// Important 2: a refused token is a setup error, not a silent retry.
const notifyCount = (needle: string) => notified().split("\n").filter((l) => l.includes(needle)).length;

test.skipIf(skip)("a 401 from GitHub is a setup error: gh's text in the log, one notification", () => {
  const r1 = run(["check"], { STUB_LIST_HTTP: "401" });
  expect(r1.code).toBe(2);
  expect(r1.out).toContain("setup error: GitHub refused the token (HTTP 401: Bad credentials");
  const r2 = run(["check"], { STUB_LIST_HTTP: "401" });
  expect(r2.code).toBe(2);
  expect(notifyCount("GitHub refused the token")).toBe(1);
  expect(notified()).not.toContain("check failed");
});

test.skipIf(skip)("a 403 that is not a rate limit is a setup error too", () => {
  const r = run(["check"], { STUB_LIST_HTTP: "403" });
  expect(r.code).toBe(2);
  expect(r.out).toContain("Resource not accessible by personal access token");
});

test.skipIf(skip)("a rate-limited 403 is retried quietly", () => {
  const r = run(["check"], { STUB_LIST_HTTP: "403rl" });
  expect(r.code).toBe(0);
  expect(r.out).toContain("the next check tries again");
  expect(notified()).toBe("");
});

test.skipIf(skip)("a 401 on a later call (compare) is a setup error as well", () => {
  const r = run(["check"], { STUB_COMPARE_HTTP: "401" });
  expect(r.code).toBe(2);
  expect(r.out).toContain("setup error: GitHub refused the token");
});

test.skipIf(skip)("once GitHub takes the token again, the next refusal notifies again", () => {
  run(["check"], { STUB_LIST_HTTP: "401" });
  run(["check"]);
  run(["check"], { STUB_LIST_HTTP: "401" });
  expect(notifyCount("GitHub refused the token")).toBe(2);
});

// Nit: a failed swap leaves the old app in place and records nothing.
test.skipIf(skip)("a failed move of the new app keeps the installed app and its record", () => {
  mkdirSync(join(daily, "Contents"), { recursive: true });
  writeFileSync(join(daily, "Contents/old-marker"), "old\n");
  writeFileSync(join(state, "installed"), OLD + "\n");
  const r = run(["check"], { STUB_MV_FAIL: "1" });
  expect(r.out).toContain("could not move the new app into place");
  expect(read(join(state, "installed")).trim()).toBe(OLD);
  expect(existsSync(join(daily, "Contents/old-marker"))).toBe(true);
  expect(existsSync(join(state, "built"))).toBe(false);
  expect(existsSync(`${daily}.next`)).toBe(false);
});

test.skipIf(skip)("a successful swap replaces the old app and leaves no .prev", () => {
  mkdirSync(join(daily, "Contents"), { recursive: true });
  writeFileSync(join(daily, "Contents/old-marker"), "old\n");
  writeFileSync(join(state, "installed"), OLD + "\n");
  run(["check"]);
  expect(read(join(state, "installed")).trim()).toBe(SHA);
  expect(existsSync(join(daily, "Contents/old-marker"))).toBe(false);
  expect(existsSync(`${daily}.prev`)).toBe(false);
});

// Nits: the bootstrap's sourcing refusal and its first-install bound.
for (const sh of ["bash", "zsh"]) {
  const have = spawnSync("sh", ["-c", `command -v ${sh}`]).status === 0;
  test.skipIf(skip || !have)(`sourcing the bootstrap from ${sh} returns 2 and leaves the shell's options alone`, () => {
    const opts = sh === "zsh" ? "[[ -o errexit ]] && echo errexit-on || echo errexit-off" : "case $- in *e*) echo errexit-on ;; *) echo errexit-off ;; esac";
    const r = spawnSync(sh, ["-c", `source '${BOOTSTRAP}'; echo "rc=$?"; ${opts}`], { encoding: "utf8", env: { HOME: home, PATH: process.env.PATH ?? "" } });
    const out = (r.stdout ?? "") + (r.stderr ?? "");
    expect(out).toContain("run it with bash, not source");
    expect(out).toContain("rc=2");
    expect(out).toContain("errexit-off");
    expect(read(join(home, "security.log"))).toBe("");
  });
}

test.skipIf(skip)("bootstrap refuses a release more than 5 commits behind master, like a first install", () => {
  compare(SHA, "master", "ahead", 6, 0);
  const r = bootstrap();
  expect(r.code).toBe(1);
  expect(r.out).toContain("is 6 commits behind master (more than 5)");
  expect(existsSync(self)).toBe(false);
});

test("the bootstrap's bound is the agent's", () => {
  const n = (src: string, re: RegExp) => src.match(re)?.[1];
  expect(n(readFileSync(BOOTSTRAP, "utf8"), /^max_behind=(\d+)/m)).toBe(n(readFileSync(SCRIPT, "utf8"), /^first_install_max_behind=(\d+)/m));
});

// gh puts "(HTTP 403)" at the end of its line; a cut before classifying lost it.
test.skipIf(skip)("a 403 whose line is longer than 200 characters is still a setup error (release list)", () => {
  const r = run(["check"], { STUB_LIST_HTTP: "403long" });
  expect(r.code).toBe(2);
  expect(r.out).toContain("setup error: GitHub refused the token (gh: Resource not accessible");
});

test.skipIf(skip)("a 403 whose line is longer than 200 characters is still a setup error (compare)", () => {
  const r = run(["check"], { STUB_COMPARE_HTTP: "403long" });
  expect(r.code).toBe(2);
  expect(r.out).toContain("setup error: GitHub refused the token");
});
