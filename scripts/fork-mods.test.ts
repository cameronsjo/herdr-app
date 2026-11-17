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
  const { parse, absolute, normal, existing } = await load("open-folder", ["parse", "absolute", "normal", "existing"]);
  expect(absolute(normal("~/x", "/home/me"))).toBe(true);
  expect(absolute(normal("code/x", "/home/me"))).toBe(false);
  expect(normal("//", "/home/me")).toBe("/");
  expect(normal("~", "/")).toBe("/");
  // Elsewhere a backslash is part of a name, and C:/x is relative.
  expect(normal("/home/me/dir\\", "/home/me")).toBe("/home/me/dir\\");
  expect(absolute(normal("C:/x", "/home/me"))).toBe(false);
  expect(absolute(normal("~\\repo", "/home/me"))).toBe(false);
  // Windows: drive and UNC paths count, a bare drive or a relative path doesn't.
  const win = true;
  expect(normal("C:\\Users\\me\\repo\\", "C:\\Users\\me", win)).toBe("C:\\Users\\me\\repo");
  expect(normal("C:\\", "C:\\Users\\me", win)).toBe("C:\\");
  expect(normal("C:\\\\", "C:\\Users\\me", win)).toBe("C:\\");
  expect(normal("~\\repo", "C:\\Users\\me\\", win)).toBe("C:\\Users\\me\\repo");
  expect(absolute("C:/Users/me", win)).toBe(true);
  expect(absolute("\\\\server\\share\\repo", win)).toBe(true);
  expect(absolute("C:repo", win)).toBe(false);
  expect(absolute("C:", win)).toBe(false);
  expect(absolute("repo\\x", win)).toBe(false);
  expect(parse("open:/a/b:c")).toEqual({ open: "/a/b:c" });
  expect(parse("pick")).toEqual({ pick: true });
  expect(parse("open:")).toBeNull();
  expect(parse("focus")).toBeNull();
  // On Windows C:/Users/Me/repo and c:\users\me\repo\ are one folder.
  const spacesWin = [{ id: "w1", folder: "C:\\Users\\Me\\repo", machine: null, tabs: [] }];
  expect(existing("c:/users/me/repo/", spacesWin, [], "C:\\Users\\Me", true)?.id).toBe("w1");
  expect(existing("c:/users/me/repo/", spacesWin, [], "C:\\Users\\Me", false)).toBeNull();
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

const term = (address: string, name: string | null, tab: string, state = "live") => ({ address, name, state, labels: { "herdr.tab": tab } });
const tabsOf = (...tabs: { id: string; name: string; named: boolean }[]) => new Map(tabs.map((t) => [t.id, t]));

test("name-sync names an unnamed tab after its one named terminal", async () => {
  const { plan } = await load("name-sync", ["plan"]);
  const tabs = tabsOf({ id: "c1", name: "1", named: false }, { id: "vps/c2", name: "2", named: false }, { id: "c3", name: "3", named: false });
  const all = [term("t1", "fix-login", "c1"), term("vps/t7", "deploy", "c2"), term("t3", "a", "c3"), term("t4", "b", "c3"), term("t5", null, "c1")];
  const { renames, seen } = plan(all, tabs, {});
  // Two named terminals in c3: neither name wins. A remote tab carries its machine prefix.
  expect(renames).toEqual([{ id: "c1", name: "fix-login" }, { id: "vps/c2", name: "deploy" }]);
  expect(seen).toEqual({ t1: "fix-login", "vps/t7": "deploy", t3: "a", t4: "b" });
});

test("name-sync leaves a tab someone named, unless it still carries the old name", async () => {
  const { plan } = await load("name-sync", ["plan"]);
  const all = [term("t1", "fix-login-2", "c1")];
  // Renamed since we looked, and the tab still says the old name: it follows.
  expect(plan(all, tabsOf({ id: "c1", name: "fix-login", named: true }), { t1: "fix-login" }).renames).toEqual([{ id: "c1", name: "fix-login-2" }]);
  // The person named the tab something else: left alone.
  expect(plan(all, tabsOf({ id: "c1", name: "auth work", named: true }), { t1: "fix-login" }).renames).toEqual([]);
  // A named tab we've never seen a name for: left alone.
  expect(plan(all, tabsOf({ id: "c1", name: "auth work", named: true }), {}).renames).toEqual([]);
  // Nothing changed: nothing to do.
  expect(plan(all, tabsOf({ id: "c1", name: "fix-login-2", named: true }), { t1: "fix-login-2" }).renames).toEqual([]);
});

test("name-sync remembers terminals that aren't listed and forgets ended ones", async () => {
  const { plan } = await load("name-sync", ["plan"]);
  const { seen } = plan([term("t1", "x", "c1", "exited")], tabsOf({ id: "c1", name: "1", named: false }), { t1: "x", "vps/t9": "away" });
  expect(seen).toEqual({ "vps/t9": "away" });
});
