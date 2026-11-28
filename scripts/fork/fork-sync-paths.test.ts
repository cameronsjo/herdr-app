// FORK(sync-guard): tests for .github/workflows/fork-sync-paths.yml. The step's
// script is taken from the workflow file as is and run against a gh stand-in
// that lists the PR's files, with the event fields the workflow passes in env.
import { test, expect, beforeAll, afterAll } from "bun:test";
import { mkdtempSync, writeFileSync, readFileSync, rmSync, chmodSync, mkdirSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

const WORKFLOW = join(import.meta.dir, "../../.github/workflows/fork-sync-paths.yml");
const yml = readFileSync(WORKFLOW, "utf8");
const skip = process.platform === "win32";
let dir = "";

// The n-th `run: |` block of the workflow, its 10-space indent removed; it ends
// at the first line indented less.
function runBlock(n: number) {
  const lines = yml.split("        run: |\n")[n].split("\n");
  const end = lines.findIndex((l) => l.trim() !== "" && !l.startsWith("          "));
  return lines.slice(0, end < 0 ? undefined : end).map((l) => l.slice(10)).join("\n") + "\n";
}

beforeAll(() => {
  if (skip) return;
  dir = mkdtempSync(join(tmpdir(), "fork-sync-paths-test-"));
  writeFileSync(join(dir, "step.sh"), runBlock(1));
  writeFileSync(join(dir, "report.sh"), runBlock(2));
  mkdirSync(join(dir, "bin"));
  writeFileSync(join(dir, "bin/gh"), `#!/bin/bash\necho "$*" >> "${dir}/gh.log"\nprintf '%b\\n' "$STUB_FILES"\n`);
  chmodSync(join(dir, "bin/gh"), 0o755);
});
afterAll(() => { if (dir) rmSync(dir, { recursive: true, force: true }); });

type Pr = { ref?: string; repo?: string; title?: string; auto?: boolean; files: string[]; changed?: number };
function check(pr: Pr) {
  const r = spawnSync("bash", [join(dir, "step.sh")], {
    encoding: "utf8",
    env: {
      PATH: `${join(dir, "bin")}:${process.env.PATH}`,
      RUNNER_TEMP: dir,
      REPO: "cameronsjo/herdr-app", PR: "7",
      HEAD_REF: pr.ref ?? "fix/thing", HEAD_REPO: pr.repo ?? "cameronsjo/herdr-app", TITLE: pr.title ?? "fix: a thing",
      AUTO_MERGE: pr.auto ? "true" : "false",
      CHANGED_FILES: String(pr.changed ?? pr.files.length),
      STUB_FILES: pr.files.join("\\n"),
    },
  });
  return { code: r.status, out: (r.stdout ?? "") + (r.stderr ?? "") };
}

test("it runs again when auto-merge is turned on or off, and on every push", () => {
  const types = yml.match(/types: \[([^\]]*)\]/)![1].split(",").map((s) => s.trim());
  for (const t of ["opened", "synchronize", "reopened", "edited", "auto_merge_enabled", "auto_merge_disabled"]) expect(types).toContain(t);
  expect(yml).toContain("pull_request_target:");
  expect(yml).toContain("AUTO_MERGE: ${{ github.event.pull_request.auto_merge != null }}");
});

test.skipIf(skip)("auto-merge on a person's branch touching a protected path fails", () => {
  const r = check({ auto: true, files: ["scripts/fork/daily-update"] });
  expect(r.code).toBe(1);
  expect(r.out).toContain("auto-merge is on");
});

test.skipIf(skip)("auto-merge with no protected path passes", () => {
  expect(check({ auto: true, files: ["src/main.rs", "docs/fork/CHANGES.md"] }).code).toBe(0);
});

test.skipIf(skip)("a person's branch without auto-merge passes, protected paths or not", () => {
  expect(check({ files: ["scripts/fork/daily-update", ".github/workflows/ci.yml"] }).code).toBe(0);
});

for (const path of ["scripts/fork/x", ".github/workflows/ci.yml", ".claude/settings.json", ".claude/agents/a.md", ".mcp.json", "docs/fork/weekly-sync.md", "CLAUDE.md"]) {
  test.skipIf(skip)(`a claude/* branch touching ${path} fails`, () => {
    expect(check({ ref: "claude/project-thread-x", files: ["src/a.rs", path] }).code).toBe(1);
  });
}

test.skipIf(skip)("names still count: sync/*, a sync upstream title, a foreign head", () => {
  expect(check({ ref: "sync/upstream-20261123", files: ["CLAUDE.md"] }).code).toBe(1);
  expect(check({ title: "chore(fork): Sync Upstream herdr-app", files: [".github/x"] }).code).toBe(1);
  expect(check({ repo: "someone/herdr-app", files: [".mcp.json"] }).code).toBe(1);
});

test.skipIf(skip)("a rename out of a protected path counts", () => {
  expect(check({ ref: "claude/x", files: ["misc/moved", "scripts/fork/daily-update"] }).code).toBe(1);
});

for (const path of ["crates/x/CLAUDE.md", "CLAUDE.local.md", "crates/x/CLAUDE.local.md", "crates/x/.claude/skills/s/SKILL.md"]) {
  test.skipIf(skip)(`agent configuration at any depth is protected: ${path}`, () => {
    expect(check({ auto: true, files: ["src/a.rs", path] }).code).toBe(1);
  });
}

test.skipIf(skip)("a file that only mentions CLAUDE.md in its name is not protected", () => {
  expect(check({ auto: true, files: ["docs/NOT-CLAUDE.md", "src/claude/x.rs"] }).code).toBe(0);
});

test.skipIf(skip)("3000 or more changed files fails, whoever merges", () => {
  const r = check({ changed: 3000, files: ["src/a.rs"] });
  expect(r.code).toBe(1);
  expect(r.out).toContain("at most 3000");
  expect(check({ changed: 2999, files: ["src/a.rs"] }).code).toBe(0);
});

test.skipIf(skip)("the title is data, never run", () => {
  const r = check({ title: '$(echo PWNED >&2)"; exit 0', ref: "claude/x", files: ["scripts/fork/x"] });
  expect(r.code).toBe(1);
  expect(r.out).not.toContain("PWNED");
});

// The App step: a curl stand-in answers the installation and token calls and
// keeps what is posted; the JWT must verify against the App's public key.
function report(outcome: string, text = "rules said this\n") {
  writeFileSync(join(dir, "sync-paths.txt"), text);
  writeFileSync(join(dir, "bin/curl"), `#!/bin/bash
echo "$*" >> "${dir}/curl.log"
for a in "$@"; do case "$a" in "Authorization: Bearer "*) echo "\${a#Authorization: Bearer }" > "${dir}/last-bearer" ;; esac; done
case "$*" in
  *"/repos/cameronsjo/herdr-app/installation"*) echo '{"id": 42}' ;;
  *"/app/installations/42/access_tokens"*) printf '%s' "\${@: -1}" > "${dir}/token-request"; echo '{"token": "ghs_test"}' ;;
  *"/check-runs"*) cat > "${dir}/posted" ;;
  *) echo "curl stub: unexpected: $*" >&2; exit 3 ;;
esac`);
  chmodSync(join(dir, "bin/curl"), 0o755);
  const r = spawnSync("bash", [join(dir, "report.sh")], {
    encoding: "utf8",
    env: {
      PATH: `${join(dir, "bin")}:${process.env.PATH}`, RUNNER_TEMP: dir,
      APP_ID: "123456", APP_KEY: readFileSync(join(dir, "test-key.pem"), "utf8"),
      REPO: "cameronsjo/herdr-app", HEAD_SHA: "a".repeat(40), OUTCOME: outcome, RUN_URL: "https://example.invalid/run/1",
    },
  });
  return { code: r.status, out: (r.stdout ?? "") + (r.stderr ?? "") };
}

test.skipIf(skip)("the App step posts the rules' outcome as sync-paths on the PR head, with a token for checks alone", () => {
  const g = spawnSync("openssl", ["genrsa", "-out", join(dir, "test-key.pem"), "2048"], { encoding: "utf8" });
  expect(g.status, g.stderr).toBe(0);
  spawnSync("openssl", ["rsa", "-in", join(dir, "test-key.pem"), "-pubout", "-out", join(dir, "test-key.pub")]);
  for (const [outcome, want] of [["success", "success"], ["failure", "failure"], ["cancelled", "failure"]]) {
    const r = report(outcome);
    expect(r.code).toBe(0);
    const posted = JSON.parse(readFileSync(join(dir, "posted"), "utf8"));
    expect(posted.name).toBe("sync-paths");
    expect(posted.head_sha).toBe("a".repeat(40));
    expect(posted.conclusion).toBe(want);
    expect(posted.output.text).toContain("rules said this");
  }
  expect(JSON.parse(readFileSync(join(dir, "token-request"), "utf8"))).toEqual({ repositories: ["herdr-app"], permissions: { checks: "write" } });
  expect(readFileSync(join(dir, "last-bearer"), "utf8").trim()).toBe("ghs_test");
  // The JWT the step signed (the bearer of the installation call) verifies and names the App.
  const jwt = readFileSync(join(dir, "curl.log"), "utf8").match(/Authorization: Bearer (\S+\.\S+\.\S+) https:\/\/api\.github\.com\/repos\/cameronsjo\/herdr-app\/installation/)![1];
  const [h, p, sig] = jwt.split(".");
  const unb64 = (x: string) => Buffer.from(x.replace(/-/g, "+").replace(/_/g, "/"), "base64");
  expect(JSON.parse(unb64(p).toString()).iss).toBe("123456");
  writeFileSync(join(dir, "jwt.in"), `${h}.${p}`);
  writeFileSync(join(dir, "jwt.sig"), unb64(sig));
  const v = spawnSync("openssl", ["dgst", "-sha256", "-verify", join(dir, "test-key.pub"), "-signature", join(dir, "jwt.sig"), join(dir, "jwt.in")], { encoding: "utf8" });
  expect(v.stdout).toContain("Verified OK");
  // The step removes the key file it wrote.
  expect(existsSync(join(dir, "app.pem"))).toBe(false);
});

test("the App's key is read only in the step that posts, and only from the master-only environment", () => {
  expect(yml).toContain("environment: sync-paths-app");
  expect(yml.match(/secrets\.SYNC_PATHS_APP_KEY/g)?.length).toBe(1);
  expect(yml).toContain("if: ${{ always() && vars.SYNC_PATHS_APP_ID != '' }}");
  // The Actions job still fails on its own, so the check stays meaningful before the App is set up.
  expect(yml).toContain("if: ${{ always() && steps.rules.outcome != 'success' }}");
});
