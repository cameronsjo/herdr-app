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
[ "$GH_CONFIG_DIR" = "$HOME/.local/share/herdr-fork/gh-config" ] || { echo "gh stub: GH_CONFIG_DIR is '$GH_CONFIG_DIR'" >&2; exit 9; }
[ -n "$GH_TOKEN" ] || { echo "gh stub: no GH_TOKEN" >&2; exit 9; }
case "$1 $2" in
  "release list") echo "\${STUB_TAG:-${TAG}}"; exit 0 ;;
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
    got=$(shasum -a 256 "$file" | cut -d' ' -f1)
    rec="$F/attested/$got"
    [ -f "$rec" ] || exit 1
    read -r subj_name want_sha < "$rec"
    [ "$src" = "$want_sha" ] && [ "$signer" = "$want_sha" ] || exit 1
    json='[{"verificationResult":{"statement":{"subject":[{"name":"'"$subj_name"'","digest":{"sha256":"'"$got"'"}}]}}}]'
    if [ -n "$jqx" ]; then jq -r "$jqx" <<<"$json"; else echo "$json"; fi
    exit 0 ;;
esac
case "$*" in
  *"/git/ref/tags/browser-"*) echo "\${STUB_TAG_POINTS:-commit ${SHA}}" ;;
  *"/compare/"*"...master"*) echo "\${STUB_ON_MASTER:-identical}" ;;
  *"/compare/"*) echo "\${STUB_AHEAD:-ahead}" ;;
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
  stub("security", `[ -n "\${STUB_KEYCHAIN:-}" ] && echo "$STUB_KEYCHAIN" || exit 44`);
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
  const r = run(["check"], { STUB_AHEAD: "behind" });
  expect(r.out).toContain("is not ahead of the installed");
  expect(read(join(state, "installed")).trim()).toBe(OLD);
  expect(existsSync(daily)).toBe(false);
});

test.skipIf(skip)("a release whose commit is not on master is refused", () => {
  const r = run(["check"], { STUB_ON_MASTER: "diverged" });
  expect(r.out).toContain("is not on master");
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
