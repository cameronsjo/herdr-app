// FORK(sync-guard): tests for .github/workflows/fork-sync-paths.yml. The step's
// script is taken from the workflow file as is and run against a gh stand-in
// that lists the PR's files, with the event fields the workflow passes in env.
import { test, expect, beforeAll, afterAll } from "bun:test";
import { mkdtempSync, writeFileSync, readFileSync, rmSync, chmodSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

const WORKFLOW = join(import.meta.dir, "../../.github/workflows/fork-sync-paths.yml");
const yml = readFileSync(WORKFLOW, "utf8");
const skip = process.platform === "win32";
let dir = "";

beforeAll(() => {
  if (skip) return;
  dir = mkdtempSync(join(tmpdir(), "fork-sync-paths-test-"));
  const run = yml.split("        run: |\n")[1];
  writeFileSync(join(dir, "step.sh"), run.split("\n").map((l) => l.slice(10)).join("\n"));
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

test.skipIf(skip)("a nested CLAUDE.md is not the protected one", () => {
  expect(check({ ref: "claude/x", files: ["src/CLAUDE.md"] }).code).toBe(0);
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
