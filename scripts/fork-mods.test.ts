// FORK(background-mods): the fork's own mods: the background ones (defaults/mods/open-folder,
// space-groups, name-sync) and browser-prototype, a browser block's page.
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
  // Two named terminals in c3: neither name wins, and neither is decided yet.
  // A remote tab carries its machine prefix.
  expect(renames).toEqual([{ id: "c1", name: "fix-login" }, { id: "vps/c2", name: "deploy" }]);
  expect(seen).toEqual({ t1: { name: "fix-login", tab: "c1" }, "vps/t7": { name: "deploy", tab: "vps/c2" } });
  // Once t4 ends, t3 is c3's only named terminal: c3 takes its name.
  expect(plan([all[2], term("t4", "b", "c3", "finished")], tabs, seen).renames).toEqual([{ id: "c3", name: "a" }]);
});

test("name-sync waits for a tab that isn't shown yet", async () => {
  const { plan } = await load("name-sync", ["plan"]);
  const all = [term("t1", "fix-login", "c9")];
  const first = plan(all, tabsOf(), {});
  expect(first).toEqual({ renames: [], seen: {} });
  expect(plan(all, tabsOf({ id: "c9", name: "1", named: false }), first.seen).renames).toEqual([{ id: "c9", name: "fix-login" }]);
  // Renamed while its tab was still loading: the old tab name still follows.
  const known = { t1: { name: "old", tab: "c9" } };
  const loading = plan(all, tabsOf(), known);
  expect(loading.seen).toEqual(known);
  expect(plan(all, tabsOf({ id: "c9", name: "old", named: true }), loading.seen).renames).toEqual([{ id: "c9", name: "fix-login" }]);
});

test("name-sync leaves a tab someone named, unless it still carries the old name", async () => {
  const { plan } = await load("name-sync", ["plan"]);
  const all = [term("t1", "fix-login-2", "c1")];
  const was = (name: string, tab = "c1") => ({ t1: { name, tab } });
  // Renamed since we looked, and the tab still says the old name: it follows.
  expect(plan(all, tabsOf({ id: "c1", name: "fix-login", named: true }), was("fix-login")).renames).toEqual([{ id: "c1", name: "fix-login-2" }]);
  // The person named the tab something else: left alone.
  expect(plan(all, tabsOf({ id: "c1", name: "auth work", named: true }), was("fix-login")).renames).toEqual([]);
  // A named tab we've never seen a name for: left alone.
  expect(plan(all, tabsOf({ id: "c1", name: "auth work", named: true }), {}).renames).toEqual([]);
  // Nothing changed: nothing to do, even after the person cleared the tab's name.
  expect(plan(all, tabsOf({ id: "c1", name: "fix-login-2", named: true }), was("fix-login-2")).renames).toEqual([]);
  expect(plan(all, tabsOf({ id: "c1", name: "1", named: false }), was("fix-login-2")).renames).toEqual([]);
  // Moved to another unnamed tab: that tab takes the name.
  expect(plan(all, tabsOf({ id: "c1", name: "1", named: false }), was("fix-login-2", "c0")).renames).toEqual([{ id: "c1", name: "fix-login-2" }]);
});

test("name-sync remembers an offline machine's terminals and forgets ended ones", async () => {
  const { plan } = await load("name-sync", ["plan"]);
  const last = { t1: { name: "x", tab: "c1" }, t2: { name: "gone", tab: "c1" }, "vps/t9": { name: "away", tab: "vps/c2" } };
  const { seen } = plan([term("t1", "x", "c1", "finished")], tabsOf({ id: "c1", name: "1", named: false }), last);
  // t1 ended; t2 is no longer listed on a machine that lists terminals; vps lists nothing.
  expect(seen).toEqual({ "vps/t9": { name: "away", tab: "vps/c2" } });
});

test("browser-prototype's placeholder never shows a saved address's query or sign-in", async () => {
  const { shownUrl, placeholder } = await load("browser-prototype", ["shownUrl", "placeholder"]);
  expect(shownUrl("https://me:pw@example.test/cb?code=42#top")).toBe("https://example.test/cb");
  expect(shownUrl("https://example.test/a@b")).toBe("https://example.test/a@b");
  expect(shownUrl("https://me:p@ss@example.test/")).toBe("https://example.test/");
  expect(placeholder(undefined, "x", true).note).toContain("floating");
  expect(placeholder(undefined, undefined)).toEqual({ title: null, note: "Opening…" });
  expect(shownUrl(undefined)).toBe("");
  // Not open yet: the saved address, bare.
  expect(placeholder(undefined, "https://example.test/cb?code=42")).toEqual({ title: null, note: "Opening https://example.test/cb" });
  // Open: what browsers() says (already without a query unless agent access is on).
  expect(placeholder({ loading: true, url: "https://example.test/", title: "" }, "x")).toEqual({ title: null, note: "Opening https://example.test/" });
  expect(placeholder({ loading: false, url: "https://example.test/", title: "Example" }, "x")).toEqual({ title: "Example", note: "https://example.test/" });
  expect(placeholder({ loading: false, url: "https://example.test/", title: "" }, "x")).toEqual({ title: null, note: "https://example.test/" });
});
