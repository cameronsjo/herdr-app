// FORK(background-mods): the fork's background mods (defaults/mods/open-folder, space-groups).
import { test, expect } from "bun:test";
import { readFile } from "node:fs/promises";

async function load(name: string, names: string[]) {
  const source = (await readFile(new URL(`../defaults/mods/${name}/main.js`, import.meta.url), "utf8"))
    .replace(/^import [^;]*;\n/gm, "").replace(/export default class (\w+)/, "class $1");
  return new Function("View", "div", `${source}; return { ${names.join(", ")} };`)(class {}, () => ({}));
}

const space = (id: string, folder: string, tabs: string[] = [], machine: string | null = null) => ({ id, folder, machine, tabs: tabs.map((t) => ({ id: t })) });

test("open-folder finds the space made in a folder, or whose first terminal is there", async () => {
  const { existing, normal } = await load("open-folder", ["existing", "normal"]);
  expect(normal("~/code/app/", "/home/me")).toBe("/home/me/code/app");
  expect(normal("~", "/home/me/")).toBe("/home/me");
  expect(normal("/", "/home/me")).toBe("/");
  const all = [space("w1", "/home/me", ["t1"]), space("w2", "/home/me/api", ["t2"]), space("w3", "/home/me/code/app", ["t3"], "vps")];
  const ps = [{ tab: "t1", cwd: "/home/me/code/app" }, { tab: "t2", cwd: "/home/me/api" }, { tab: "t3", cwd: "/home/me/code/app" }];
  expect(existing("/home/me/api/", all, ps, "/home/me")?.id).toBe("w2");
  // Made at home, but its first terminal is in the repo now; the remote space never counts.
  expect(existing("~/code/app", all, ps, "/home/me")?.id).toBe("w1");
  expect(existing("/home/me/other", all, ps, "/home/me")).toBeNull();
  expect(existing("  ", all, ps, "/home/me")).toBeNull();
  // A space made in the folder wins over one that only wandered into it.
  const made = [...all, space("w4", "/home/me/code/app", ["t4"])];
  expect(existing("/home/me/code/app", made, [...ps, { tab: "t4", cwd: "/tmp" }], "/home/me")?.id).toBe("w4");
});

test("open-folder reads open:<folder> and pick", async () => {
  const { parse, absolute, normal } = await load("open-folder", ["parse", "absolute", "normal"]);
  expect(absolute(normal("~/x", "/home/me"))).toBe(true);
  expect(absolute(normal("code/x", "/home/me"))).toBe(false);
  // Windows: drive and UNC paths count, a bare drive or a relative path doesn't.
  expect(absolute(normal("C:\\Users\\me\\repo\\", "C:\\Users\\me"))).toBe(true);
  expect(normal("C:\\Users\\me\\repo\\", "C:\\Users\\me")).toBe("C:\\Users\\me\\repo");
  expect(normal("C:\\", "C:\\Users\\me")).toBe("C:\\");
  expect(normal("~\\repo", "C:\\Users\\me\\")).toBe("C:\\Users\\me\\repo");
  expect(absolute("C:/Users/me")).toBe(true);
  expect(absolute("\\\\server\\share\\repo")).toBe(true);
  expect(absolute("C:repo")).toBe(false);
  expect(absolute("repo\\x")).toBe(false);
  expect(parse("open:/a/b:c")).toEqual({ open: "/a/b:c" });
  expect(parse("pick")).toEqual({ pick: true });
  expect(parse("open:")).toBeNull();
  expect(parse("focus")).toBeNull();
});

const ids = (n: string) => n.split(" ").map((id) => ({ id, machine: id.startsWith("r") ? "vps" : null }));
const apply = (order: string[], calls: { id: string; index: number }[]) => {
  const out = [...order];
  for (const c of calls) out.splice(c.index - 1, 0, out.splice(out.indexOf(c.id), 1)[0]);
  return out.join(" ");
};

test("space-groups moves a repo with its worktrees past one neighbouring block", async () => {
  const { moves } = await load("space-groups", ["moves", "blocks"]);
  // a is a main checkout with worktrees a1 a2; b has none; c is no repo.
  const git = { a: { repo: "/r/a/.git", linked: false }, a1: { repo: "/r/a/.git", linked: true }, a2: { repo: "/r/a/.git", linked: true }, b: { repo: "/r/b/.git", linked: false }, c: null };
  const all = ids("b a a1 a2 c");
  const order = all.map((s) => s.id);
  expect(apply(order, moves(all, git, "a", -1))).toBe("a a1 a2 b c");
  expect(apply(order, moves(all, git, "a", 1))).toBe("b c a a1 a2");
  // From a worktree space, its whole group moves.
  expect(apply(order, moves(all, git, "a2", -1))).toBe("a a1 a2 b c");
  // b goes down past the whole group, not between a and its worktrees.
  expect(apply(order, moves(all, git, "b", 1))).toBe("a a1 a2 b c");
  expect(moves(all, git, "b", -1)).toEqual([]);
  expect(moves(all, git, "c", 1)).toEqual([]);
});

test("space-groups stays on its machine and leaves a worktree with no main alone", async () => {
  const { moves } = await load("space-groups", ["moves"]);
  const git = { x1: { repo: "/r/x/.git", linked: true }, b: null, r1: null, r2: null };
  const all = ids("x1 b r1 r2");
  const order = all.map((s) => s.id);
  // b's only neighbour up on this computer is x1, a lone worktree; r1 is another machine's.
  expect(apply(order, moves(all, git, "b", -1))).toBe("b x1 r1 r2");
  expect(moves(all, git, "b", 1)).toEqual([]);
  expect(apply(order, moves(all, git, "r2", -1))).toBe("x1 b r2 r1");
});
