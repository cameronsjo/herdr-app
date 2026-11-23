// FORK(markdown): tests for the markdown pane (defaults/mods/markdown). Its path rule runs
// against stand-ins for the herdr imports, as scripts/fork-mods.test.ts does.
import { test, expect } from "bun:test";
import { readFile } from "node:fs/promises";

async function target(p: unknown) {
  const source = (await readFile(new URL("../../defaults/mods/markdown/main.js", import.meta.url), "utf8"))
    .replace(/^import [^;]*;\n/gm, "").replace(/export default class (\w+)/, "class $1");
  return new Function("View", "props", "home", `${source}; return target();`)(class {}, () => p, () => "/home/me");
}

test("markdown takes absolute and home paths, and refuses relative ones", async () => {
  expect((await target({ file: "/repo/README.md" })).file).toBe("/repo/README.md");
  expect((await target({ file: "~/notes/a.md" })).file).toBe("/home/me/notes/a.md");
  expect((await target({ file: "C:\\repo\\a.md" })).file).toBe("C:\\repo\\a.md");
  expect((await target({ file: "C:/repo/a.md" })).file).toBe("C:/repo/a.md");
  expect((await target({ file: "\\\\server\\share\\a.md" })).file).toBe("\\\\server\\share\\a.md");
  for (const file of ["README.md", "./a.md", "C:README.md", "\\\\server", "", 7]) expect((await target({ file })).file).toBeNull();
  expect((await target(null)).file).toBeNull();
});

test("markdown keeps a remote file's machine, and refuses ~ there", async () => {
  expect(await target({ file: "/srv/a.md", on: "vps" })).toEqual({ file: "/srv/a.md", on: "vps", raw: "/srv/a.md" });
  expect((await target({ file: "~/a.md", on: "vps" })).file).toBeNull();
  expect((await target({ file: "/a.md", on: "" })).on).toBeNull();
});
