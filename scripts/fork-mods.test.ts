// FORK(background-mods): the fork's own mods: the background ones (defaults/mods/open-folder,
// space-groups, name-sync, outside-agents), agents/fork.js, and browser-prototype, a browser block's page.
import { test, expect } from "bun:test";
import { readFile } from "node:fs/promises";

// `env`: stand-ins for the herdr imports a function under test calls (exec, spaces, ...).
async function load(name: string, names: string[], env: Record<string, unknown> = {}) {
  const source = (await readFile(new URL(`../defaults/mods/${name}/main.js`, import.meta.url), "utf8"))
    .replace(/^import [^;]*;\n/gm, "").replace(/export default class (\w+)/, "class $1");
  return new Function("View", "div", ...Object.keys(env), `${source}; return { ${names.join(", ")} };`)(class {}, () => ({}), ...Object.values(env));
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

test("open-folder keeps a folder's spaces, takes a home of /, and counts a home space and a resolved path", async () => {
  const { normal, existing } = await load("open-folder", ["normal", "existing"]);
  expect(normal("~/x", "/")).toBe("/x");
  expect(normal("/home/me/name ", "/home/me")).toBe("/home/me/name ");
  expect(normal("   ", "/home/me")).toBe("");
  // A space with no folder of its own was made at home.
  const all = [space("w1", "", ["t1"]), space("w2", "/tmp/x", ["t2"])];
  expect(existing("~", all, [], "/home/me")?.id).toBe("w1");
  expect(existing("/home/me/", all, [], "/home/me")?.id).toBe("w1");
  // /private/tmp/x is /tmp/x on macOS: the resolved path finds the space too.
  const ps = [{ tab: "t2", cwd: "/private/tmp/x" }];
  expect(existing("/tmp/y", [space("w3", "/elsewhere", ["t2"])], ps, "/home/me", false, "/private/tmp/x")?.id).toBe("w3");
  expect(existing("/tmp/y", [space("w3", "/elsewhere", ["t2"])], ps, "/home/me")).toBeNull();
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

test("space-groups tells a worktree from a submodule and stops when git can't answer", async () => {
  const answers: Record<string, unknown> = {
    "/r/a": { code: 0, stdout: "/r/a/.git\n/r/a/.git\n", stderr: "" },
    "/r/a1": { code: 0, stdout: "/r/a/.git/worktrees/a1\n/r/a/.git\n", stderr: "" },
    "/r/a/sub": { code: 0, stdout: "/r/a/.git/modules/sub\n/r/a/.git/modules/sub\n", stderr: "" },
    "/tmp": { code: 128, stdout: "", stderr: "fatal: not a git repository (or any of the parent directories): .git\n" },
    "/slow": new Error("git took longer than 5 s and was stopped"),
    "/odd": { code: 1, stdout: "", stderr: "fatal: unsafe repository\n" },
    // A deleted folder, or git speaking another language: no repo.
    "/gone": { code: 128, stdout: "", stderr: "fatal: cannot change to '/gone': No such file or directory\n" },
    "/de": { code: 128, stdout: "", stderr: "fatal: kein Git-Repository\n" },
  };
  const calls: string[] = [];
  const exec = async (_cmd: string, args: string[], opts: { cwd?: string }) => {
    expect(opts.cwd).toBeUndefined();
    const cwd = args[1];
    calls.push(cwd);
    const a = answers[cwd];
    if (a instanceof Error) throw a;
    return a;
  };
  const { repo } = await load("space-groups", ["repo"], { exec });
  expect(await repo("/r/a")).toEqual({ repo: "/r/a/.git", linked: false });
  expect(await repo("/r/a1")).toEqual({ repo: "/r/a/.git", linked: true });
  expect(await repo("/r/a/sub")).toEqual({ repo: "/r/a/.git/modules/sub", linked: false });
  expect(await repo("/tmp")).toBeNull();
  expect(await repo("/gone")).toBeNull();
  expect(await repo("/de")).toBeNull();
  await expect(repo("/slow")).rejects.toThrow("git couldn't look at /slow");
  await expect(repo("/odd")).rejects.toThrow("unsafe repository");
  // One git call per folder.
  expect(calls.filter((c) => c === "/r/a").length).toBe(1);
});

test("space-groups looks again when a space appears while git answers, and once per folder", async () => {
  const lists = [
    [{ id: "a", machine: null, active: true, folder: "/r/a", tabs: [] }, { id: "b", machine: null, active: false, folder: "/r/b", tabs: [] }],
  ];
  let list = lists[0];
  const calls: string[] = [];
  const moved: unknown[] = [];
  const exec = async (_c: string, a: string[]) => {
    const cwd = a[1];
    calls.push(cwd);
    // A worktree space of a appears while git answers the first look.
    if (calls.length === 1) list = [...list, { id: "a1", machine: null, active: false, folder: "/r/a1", tabs: [] }, { id: "a2", machine: null, active: false, folder: "/r/a1", tabs: [] }];
    const git = cwd === "/r/a1" ? "/r/a/.git/worktrees/a1\n/r/a/.git\n" : `${cwd}/.git\n${cwd}/.git\n`;
    return { code: 0, stdout: git, stderr: "" };
  };
  const { SpaceGroups } = await load("space-groups", ["SpaceGroups"], {
    exec, spaces: () => list, panes: () => [], run: (name: string, args: unknown) => moved.push([name, args]),
  });
  await SpaceGroups.prototype.move.call({}, 1);
  // a and its worktrees go down past b as one block; a1 and a2 share a folder: one look.
  expect(moved).toEqual([["space.move", { id: "b", index: 1 }]]);
  expect(calls.sort()).toEqual(["/r/a", "/r/a1", "/r/b"]);
});

const term = (address: string, name: string | null, tab: string, state = "live") => ({ address, name, state, labels: { "herdr.tab": tab } });
const tabsOf = (...tabs: { id: string; name: string; named: boolean }[]) => new Map(tabs.map((t) => [t.id, t]));
// The tab each terminal's pane is in (panes()), here taken from the test's own label plus
// the address's machine prefix; the mod itself never reads the label.
const at = (all: { address: string; labels: Record<string, string> }[]) =>
  new Map(all.map((t) => [t.address, t.address.includes("/") ? `${t.address.split("/")[0]}/${t.labels["herdr.tab"]}` : t.labels["herdr.tab"]]));
async function loadPlan() {
  const { plan, tabsByTerminal } = await load("name-sync", ["plan", "tabsByTerminal"]);
  return { raw: plan, tabsByTerminal, plan: (all: any[], tabs: Map<string, any>, last: object) => plan(all, tabs, last, at(all)) };
}

test("name-sync names an unnamed tab after its one named terminal", async () => {
  const { plan } = await loadPlan();
  const tabs = tabsOf({ id: "c1", name: "1", named: false }, { id: "vps/c2", name: "2", named: false }, { id: "c3", name: "3", named: false });
  const all = [term("t1", "fix-login", "c1"), term("vps/t7", "deploy", "c2"), term("t3", "a", "c3"), term("t4", "b", "c3"), term("t5", null, "c1")];
  const { renames, seen } = plan(all, tabs, {});
  // Two named terminals in c3: neither name wins, and neither is decided yet.
  // A remote tab carries its machine prefix.
  expect(renames).toEqual([{ id: "c1", name: "fix-login" }, { id: "vps/c2", name: "deploy" }]);
  // Nothing is recorded until the tabs show the names (see the retry test).
  expect(seen).toEqual({});
  // Once t4 ends, t3 is c3's only named terminal: c3 takes its name.
  expect(plan([all[2], term("t4", "b", "c3", "finished")], tabs, seen).renames).toEqual([{ id: "c3", name: "a" }]);
});

test("name-sync waits for a tab that isn't shown yet", async () => {
  const { plan } = await loadPlan();
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
  const { plan } = await loadPlan();
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

test("name-sync finds a terminal's tab from its pane, not from its label", async () => {
  const { raw, tabsByTerminal } = await loadPlan();
  const where = tabsByTerminal([{ pane: "p1", terminal: "t1", tab: "c1" }, { pane: "p2", terminal: null, tab: "c2" }, { pane: "p3", terminal: "t1", tab: "c3" }]);
  expect([...where]).toEqual([["t1", "c1"]]);
  // t1's label claims c2, an unnamed tab it isn't in: c2 stays as it is.
  const tabs = tabsOf({ id: "c1", name: "work", named: true }, { id: "c2", name: "2", named: false });
  expect(raw([term("t1", "x", "c2")], tabs, {}, where).renames).toEqual([]);
  // A terminal shown in no pane has no tab yet.
  expect(raw([term("t9", "x", "c2")], tabs, {}, where)).toEqual({ renames: [], seen: {} });
});

test("name-sync retries a rename that hasn't shown on the tab", async () => {
  const { plan } = await loadPlan();
  const all = [term("t1", "fix-login-2", "c1")];
  const was = { t1: { name: "fix-login", tab: "c1" } };
  // Asked to rename: what it knew stays, so a rejected rename is asked for again.
  const first = plan(all, tabsOf({ id: "c1", name: "fix-login", named: true }), was);
  expect(first).toEqual({ renames: [{ id: "c1", name: "fix-login-2" }], seen: was });
  expect(plan(all, tabsOf({ id: "c1", name: "fix-login", named: true }), first.seen).renames).toEqual([{ id: "c1", name: "fix-login-2" }]);
  // Once the tab shows the new name, it's recorded and nothing more is asked.
  expect(plan(all, tabsOf({ id: "c1", name: "fix-login-2", named: true }), first.seen)).toEqual({ renames: [], seen: { t1: { name: "fix-login-2", tab: "c1" } } });
  // A new terminal on an unnamed tab: nothing recorded until the name lands.
  const fresh = plan(all, tabsOf({ id: "c1", name: "1", named: false }), {});
  expect(fresh).toEqual({ renames: [{ id: "c1", name: "fix-login-2" }], seen: {} });
});

test("name-sync still follows a rename recorded by the first build (a bare name)", async () => {
  const { plan } = await loadPlan();
  const all = [term("t1", "fix-login-2", "c1")];
  // tab null: the first build didn't record where; any tab still called the old name follows.
  expect(plan(all, tabsOf({ id: "c1", name: "fix-login", named: true }), { t1: { name: "fix-login", tab: null } }).renames).toEqual([{ id: "c1", name: "fix-login-2" }]);
  expect(plan(all, tabsOf({ id: "c1", name: "auth", named: true }), { t1: { name: "fix-login", tab: null } }).renames).toEqual([]);
});

test("name-sync remembers an offline machine's terminals and forgets ended ones", async () => {
  const { plan } = await loadPlan();
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

// `intl`: undefined stands in for QuickJS, which has no Intl.Segmenter.
async function loadFork(intl: unknown = Intl) {
  const source = (await readFile(new URL("../defaults/mods/agents/fork.js", import.meta.url), "utf8"))
    .replace(/^import [^;]*;\n/gm, "").replace(/^export /gm, "");
  return new Function("div", "Intl", `${source}; return { order, groups, nameOf, taskOf, detailOf, empty, FORK_SORTS, menuHeight };`)(() => ({}), intl);
}

const agent = (address: string, space: string, extra = {}) => ({ address, space, space_name: space.toUpperCase(), ...extra });

test("agents: needs you keeps only the agents that need you", async () => {
  const { order, empty } = await loadFork();
  const list = [agent("t1", "s1"), agent("t2", "s1"), agent("t3", "s2")];
  const urgency = (a: { address: string }) => ({ t1: 2, t2: 4, t3: 4 })[a.address];
  expect(order("needs", list, [], urgency).map((a: any) => a.address)).toEqual(["t2", "t3"]);
  expect(order("priority", list, [], urgency)).toBe(list);
  expect(empty("needs", true)).toBe("No agents need you.");
  expect(empty("needs", false)).toBeNull();
  expect(empty("grouped", true)).toBeNull();
});

test("agents: grouped by project gathers each project label behind its first agent", async () => {
  const { order, groups } = await loadFork();
  const all = [
    { address: "t1", labels: { project: "herdr" } },
    { address: "t2", labels: {} },
    { address: "t3", labels: { project: " herdr " } },
    { address: "t4", labels: { project: "" } },
  ];
  const list = [agent("t1", "s1"), agent("t2", "s1"), agent("t3", "s2"), agent("t4", "s1")];
  const ordered = order("project", list, all, () => 1);
  expect(ordered.map((a: any) => a.address)).toEqual(["t1", "t3", "t2", "t4"]);
  const pinned = [{ id: "pinned", label: "Pinned", status: null, members: [] }];
  const out = groups("project", [], pinned, ordered, all);
  expect(out.map((g: any) => [g.label, g.members.map((a: any) => a.address)])).toEqual([
    ["Pinned", []],
    ["herdr", ["t1", "t3"]],
    ["S1", ["t2", "t4"]],
  ]);
  const built = [{ id: "sp-s1" }];
  expect(groups("grouped", built, pinned, ordered, all)).toBe(built);
});

test("agents: a terminal's name and task label, trimmed, or null", async () => {
  const { nameOf, taskOf, FORK_SORTS, menuHeight } = await loadFork();
  const all = [{ address: "t1", name: " fix-login ", labels: { task: "fix the login test" } }, { address: "t2", name: "", labels: { task: "  " } }];
  expect(nameOf(all, "t1")).toBe("fix-login");
  expect(taskOf(all, "t1")).toBe("fix the login test");
  expect(nameOf(all, "t2")).toBeNull();
  expect(taskOf(all, "t2")).toBeNull();
  expect(nameOf(all, "t9")).toBeNull();
  expect(menuHeight(140)).toBe(140 + 31 * FORK_SORTS.length);
});

test("agents: a label drawn on one line with control and bidi characters dropped, and capped", async () => {
  const { nameOf, taskOf, groups } = await loadFork();
  const all = [
    { address: "t1", name: "n".repeat(500), labels: { task: "fake\n\u202eclaude \u00b7 blocked\u0000x", project: "p\u0000q" } },
    { address: "t2", labels: { task: "x".repeat(10000) } },
  ];
  expect(taskOf(all, "t1")).toBe("fake claude \u00b7 blocked x");
  expect(nameOf(all, "t1")).toHaveLength(80);
  expect(nameOf(all, "t1").endsWith("\u2026")).toBe(true);
  expect(taskOf(all, "t2")).toHaveLength(200);
  const out = groups("project", [], [], [{ address: "t1", space: "s1" }], all);
  expect(out[0].label).toBe("p q");
  expect(out[0].id).not.toContain("\u0000");
});

test("agents: ZWJ and ZWNJ stay, other invisible characters go", async () => {
  const { nameOf } = await loadFork();
  const family = "\u{1F468}\u200d\u{1F469}\u200d\u{1F467}";
  const persian = "\u0645\u06cc\u200c\u062e\u0648\u0627\u0647\u0645";
  const all = [{ address: "t1", name: `${family} ${persian}` }, { address: "t2", name: "a\u200bb\u200ec\u200fd" }];
  expect(nameOf(all, "t1")).toBe(`${family} ${persian}`);
  expect(nameOf(all, "t2")).toBe("a b c d");
});

test("agents: the task line is cut at 200 characters, a huge label quickly", async () => {
  const { taskOf } = await loadFork();
  const at = (task: string) => taskOf([{ address: "t1", labels: { task } }], "t1");
  expect(at("x".repeat(200))).toBe("x".repeat(200));
  expect(at("x".repeat(201))).toBe(`${"x".repeat(199)}\u2026`);
  // Ten million characters: only the start is looked at, and the cut still shows.
  const start = Date.now();
  expect(at(`${"\u0000".repeat(5000)}${"y".repeat(10_000_000)}`)).toBeNull();
  expect(at(`${"y".repeat(10_000_000)}`)).toBe(`${"y".repeat(199)}\u2026`);
  expect(Date.now() - start).toBeLessThan(500);
});

test("agents: a cut keeps a flag, an accent and a skin tone whole, with or without Intl", async () => {
  for (const intl of [Intl, undefined]) {
    const { nameOf } = await loadFork(intl);
    const cut = (tail: string) => nameOf([{ address: "t1", name: `${"a".repeat(78)}${tail}` }], "t1");
    // 78 + three clusters is 81: the cut keeps 78 + one cluster + the ellipsis.
    for (const one of ["\u{1F1F3}\u{1F1F1}", "e\u0301", "\u{1F44B}\u{1F3FD}", "\u{1F468}\u200d\u{1F4BB}", "\u2764\ufe0f"]) {
      expect(cut(one + one + one)).toBe(`${"a".repeat(78)}${one}\u2026`);
    }
  }
});

test("agents: a tab the person named stays the row's detail; else the terminal's name", async () => {
  const { detailOf } = await loadFork();
  const all = [{ address: "t1", name: "fix-login" }, { address: "t2", name: "" }];
  expect(detailOf(all, "t1", true, "auth work")).toBe("auth work");
  expect(detailOf(all, "t1", false, "claude title")).toBe("fix-login");
  expect(detailOf(all, "t2", false, "claude title")).toBe("claude title");
  // A named tab whose detail is upstream's live title (named after the program): the name wins.
  expect(detailOf(all, "t1", true, null)).toBe("fix-login");
});

test("agents: needs you hides a pinned agent that doesn't need you", async () => {
  const { order } = await loadFork();
  // A filter, not a sort: pins don't bring back agents that don't need you.
  const list = [agent("t1", "s1", { pinned: true }), agent("t2", "s1"), agent("t3", "s1", { pinned: true })];
  const urgency = (a: { address: string }) => (a.address === "t3" ? 4 : 2);
  expect(order("needs", list, [], urgency).map((a: { address: string }) => a.address)).toEqual(["t3"]);
});

test("agents: two long project labels that share their start stay two groups", async () => {
  const { groups } = await loadFork();
  const all = [{ address: "t1", labels: { project: `${"p".repeat(100)}1` } }, { address: "t2", labels: { project: `${"p".repeat(100)}2` } }];
  const out = groups("project", [], [], [{ address: "t1", space: "s1" }, { address: "t2", space: "s1" }], all);
  expect(out.map((g: { id: string }) => g.id)).toHaveLength(2);
  expect(out[0].label).toBe(out[1].label);
});

test("agents: a cut never splits an emoji", async () => {
  const { nameOf, groups } = await loadFork();
  const all = [{ address: "t1", name: `${"a".repeat(78)}\u{1F600}\u{1F600}\u{1F600}`, labels: { project: `${"a".repeat(78)}\u{1F600}\u{1F600}\u{1F600}` } }];
  const name = nameOf(all, "t1");
  expect(name).toBe(`${"a".repeat(78)}\u{1F600}\u2026`);
  expect(/[\uD800-\uDBFF](?![\uDC00-\uDFFF])/.test(name)).toBe(false);
  expect(() => groups("project", [], [], [{ address: "t1", space: "s1" }], all)).not.toThrow();
});

test("outside-agents reads a heartbeat file and drops it when its ttl passes", async () => {
  const { parse, live } = await load("outside-agents", ["parse", "live"]);
  const now = 1_700_000_000_000;
  const a = parse("cmux", JSON.stringify({ name: " cmux claude ", status: "working", title: "fixing login", ttl: 30 }), now);
  expect(a).toMatchObject({ id: "cmux", name: "cmux claude", status: "working", title: "fixing login", task: null, until: now + 30_000 });
  // `at` in seconds or milliseconds; never later than when it was seen.
  expect(parse("x", JSON.stringify({ status: "idle", at: (now - 10_000) / 1000 }), now).until).toBe(now - 10_000 + 60_000);
  expect(parse("x", JSON.stringify({ status: "idle", at: now + 999_999 }), now).until).toBe(now + 60_000);
  expect(parse("x", JSON.stringify({ status: "error", ttl: 0 }), now)).toMatchObject({ status: "blocked", until: now + 1000 });
  expect(parse("x", JSON.stringify({ status: "busy" }), now)).toBeNull();
  expect(parse("x", "not json", now)).toBeNull();
  expect(parse("x", null, now)).toBeNull();
  expect(parse("../x", JSON.stringify({ status: "idle" }), now)).toBeNull();
  for (const status of ["constructor", "__proto__", "toString"]) expect(parse("x", JSON.stringify({ status }), now)).toBeNull();
  // A status that is not a string is not a status, and does not throw.
  for (const status of [{ toString: null, valueOf: null }, 1, ["idle"]]) expect(parse("x", JSON.stringify({ status }), now)).toBeNull();
  expect(parse("x", JSON.stringify({ status: "idle", pad: "p".repeat(5000) }), now)).toBeNull();
  expect(parse("x", JSON.stringify({ status: "idle", name: "n".repeat(500) }), now).name.length).toBe(80);
  expect(parse("x", JSON.stringify({ status: "idle", name: "a\n\u202eb", title: `${"t".repeat(199)}\u{1F600}x` }), now)).toMatchObject({ name: "a b", title: `${"t".repeat(199)}\u{1F600}` });
  const agents = new Map([["b", { ...a, id: "b", name: "b", until: now + 1, text: "{}" }], ["a", { ...a, id: "a", name: "a", until: now + 1 }], ["old", { ...a, id: "old", name: "c", until: now }]]);
  const shown = live(agents, now);
  expect(shown.map((x: any) => x.id)).toEqual(["a", "b"]);
  // `until` stays, so a reloaded copy keeps the live rows; the file's text doesn't.
  expect(shown[0].until).toBe(now + 1);
  expect(shown[1]).not.toHaveProperty("text");
  // An error keeps its own word on the pill.
  expect(parse("x", JSON.stringify({ status: "error" }), now).reason).toBe("error");
  expect(parse("x", JSON.stringify({ status: "blocked" }), now).reason).toBeNull();
});

test("outside-agents counts bytes, and drops every invisible character", async () => {
  const { parse, clip } = await load("outside-agents", ["parse", "clip"]);
  const now = 1;
  // 4096 characters of a two-byte letter are 8192 bytes: too big for a heartbeat.
  const pad = (c: string, n: number) => JSON.stringify({ status: "idle", pad: c.repeat(n) });
  expect(parse("x", pad("a", 4000), now)).not.toBeNull();
  expect(parse("x", pad("\u00e9", 2100), now)).toBeNull();
  for (const c of ["\u00ad", "\u034f", "\u115f", "\u1160", "\u3164", "\uffa0", "\u180e", "\u2060", "\u2064", "\u206a", "\u206f", "\ufff9", "\ufffb", "\u{e0041}"]) {
    expect(clip(`a${c}b`)).toBe("a b");
  }
  // Joiners and variation selectors stay, as in the agents panel.
  for (const c of ["\u200c", "\u200d", "\ufe0f", "\u{e0100}"]) expect(clip(`a${c}b`)).toBe(`a${c}b`);
});

// The mod's file handling with the folder stood in for: `files` maps a name in
// the folder to its text; `find` lists the plain .json files among them.
async function outsideMod(files: Record<string, string>, extra: Record<string, unknown> = {}) {
  const reads: string[] = [];
  const env = {
    exec: async (cmd: string, args: string[]) => {
      if (cmd === "test") return { code: 1, stdout: "", stderr: "" };
      if (cmd !== "find") throw new Error(cmd);
      const listed = Object.keys(files).filter((n) => n.endsWith(".json") && !n.startsWith("link:")).map((n) => `${args[0]}/${n}`);
      const sep = args.includes("-print0") ? "\0" : "\n";
      return { code: 0, stdout: listed.map((l) => `${l}${sep}`).join(""), stderr: "" };
    },
    readTextFile: async (path: string) => { reads.push(path.split("/").pop()!); return files[path.split("/").pop()!] ?? null; },
    writeTextFile: async () => null,
    window: () => ({ platform: "linux" }),
    ...extra,
  };
  const { OutsideAgents } = await load("outside-agents", ["OutsideAgents"], env);
  const mod = Object.assign(Object.create(OutsideAgents.prototype), { dir: "/h/.config/herdr-app/outside-agents", agents: new Map(), dropped: new Map() });
  return { mod, reads, files };
}

test("outside-agents reads only the plain files right in its folder", async () => {
  const { mod } = await outsideMod({ "a.json": '{"status":"working"}', "bad id!.json": '{"status":"idle"}', "notes.txt": "x" });
  await mod.relist();
  expect([...mod.agents.keys()]).toEqual(["a"]);
  // An event inside a folder in it is not one of its agents.
  const dir = mod.dir;
  const { changedNames } = await load("outside-agents", ["changedNames"]);
  expect([...changedNames([`${dir}/x.json`, `${dir}/sub/y.json`, `${dir}/x.json.tmp`])]).toEqual(["x.json", "x.json.tmp"]);

  // The folder itself in a batch: the watch overflowed, so everything is news.
  expect(changedNames([`${dir}/x.json`, dir])).toBeNull();
  expect(changedNames([`${dir}/`])).toBeNull();
});

test("outside-agents never takes part of a strange name for an agent file", async () => {
  // A plain file named "a<newline>b.json" beside a pipe named b.json: only the
  // plain file is listed, and its name is no valid id, so nothing is read.
  const { mod, reads } = await outsideMod({ "a\nb.json": '{"status":"idle"}', "q\\b.json": '{"status":"idle"}' });
  await mod.relist();
  expect(reads).toEqual([]);
  expect(mod.agents.size).toBe(0);
});

test("outside-agents keeps the last run's live rows until its first read", async () => {
  const now = Date.now();
  let sent: unknown = null;
  const { OutsideAgents } = await load("outside-agents", ["OutsideAgents"], { setShared: (_k: string, v: unknown) => { sent = v; } });
  const m = Object.assign(Object.create(OutsideAgents.prototype), { agents: new Map(), ready: false, sent: "", kept: [{ id: "a", until: now + 60000 }, { id: "b", until: now - 1 }] });
  m.publish();
  expect(sent).toEqual([{ id: "a", until: now + 60000 }]);
  m.ready = true;
  m.publish();
  expect(sent).toEqual([]);
});

test("outside-agents takes a temp-file rename batch as a heartbeat, unchanged text too", async () => {
  const { mod, files, reads } = await outsideMod({ "x.json": '{"status":"working","ttl":5}' });
  await mod.relist();
  const first = mod.agents.get("x").until;
  await new Promise((r) => setTimeout(r, 5));
  reads.length = 0;
  // A writer renames x.json.tmp over x.json with the same text: still a heartbeat.
  await mod.changed([`${mod.dir}/x.json.tmp`, `${mod.dir}/x.json`]);
  expect(reads).toEqual(["x.json"]);
  expect(mod.agents.get("x").until).toBeGreaterThan(first);
  // A relist that reads the same text again is not one.
  const now = mod.agents.get("x").until;
  await new Promise((r) => setTimeout(r, 5));
  await mod.relist();
  expect(mod.agents.get("x").until).toBe(now);
  // A file gone from the folder goes from the list.
  delete files["x.json"];
  await mod.changed([`${mod.dir}/x.json`]);
  expect(mod.agents.size).toBe(0);
});

test("outside-agents makes room for a new agent, and a dropped record doesn't come back on a relist", async () => {
  const files: Record<string, string> = {};
  for (let i = 0; i < 128; i++) files[`a${String(i).padStart(3, "0")}.json`] = JSON.stringify({ status: "working", ttl: 86400 });
  const { mod } = await outsideMod(files);
  await mod.relist();
  expect(mod.agents.size).toBe(128);
  // Full of long-lived records: a new one still gets in, in place of the one that expires soonest.
  files["a000.json"] = JSON.stringify({ status: "working", ttl: 1 });
  await mod.changed([`${mod.dir}/a000.json`]);
  files["zz.json"] = JSON.stringify({ status: "blocked", ttl: 60 });
  await mod.changed([`${mod.dir}/zz.json`]);
  expect(mod.agents.has("zz")).toBe(true);
  expect(mod.agents.has("a000")).toBe(false);
  // a000's file is unchanged: a relist doesn't read it as a fresh heartbeat.
  await mod.relist();
  expect(mod.agents.has("a000")).toBe(false);
});

test("outside-agents leaves a linked folder without a README", async () => {
  const written: string[] = [];
  const { mod } = await outsideMod({}, {
    exec: async (cmd: string) => ({ code: cmd === "test" ? 0 : 1, stdout: "", stderr: "" }),
    writeTextFile: async (p: string) => { written.push(p); return null; },
  });
  await mod.start();
  expect(written).toEqual([]);
});

test("agents: outside agents and the empty message", async () => {
  const { empty } = await loadFork();
  const outside = [{ id: "c", status: "working" }];
  expect(empty("priority", false, outside)).toBe("");
  expect(empty("needs", false, outside)).toBe("No agents need you.");
  expect(empty("needs", false, [{ id: "c", status: "blocked" }])).toBe("");
  expect(empty("priority", false, undefined)).toBeNull();
});

// `env`: stand-ins for the herdr imports and localStorage that load() and withPicked() use.
async function loadPalette(listed: any[] = [], env: Record<string, unknown> = {}) {
  const source = (await readFile(new URL("../defaults/frame/palette/fork.js", import.meta.url), "utf8"))
    .replace(/^import [^;]*;\n/gm, "").replace(/^export /gm, "").replace("let listed = [];", "let listed = __listed;");
  const all = { appHome: () => "/h/.config/herdr-app", commands: () => [{ name: "pane.split" }, { name: "app.quit" }], readTextFile: async () => null, exec: async () => ({ code: 1, stdout: "", stderr: "" }), win: () => ({ platform: "linux" }), fs: { readdir: async () => [] }, shared: () => null, localStorage: { getItem: () => null, setItem: () => {} }, ...env };
  return new Function("__listed", "openOverlay", ...Object.keys(all), `${source}; return { commandsOf, entries, go, listable, text, load, withPicked, current: () => listed };`)(listed, (o: any) => opened.push(o), ...Object.values(all));
}
const opened: any[] = [];
const entry = (name: string, args: any = {}, group = "Pane") => ({ name, args, group, title: name, words: "", id: `${name}:${JSON.stringify(args)}` });

test("palette: a mod lists only its own commands", async () => {
  const { commandsOf } = await loadPalette();
  const got = commandsOf("open-folder", { commands: [
    { command: "open-folder.pick", title: "Open folder…" },
    { command: "space-groups.up", title: "not mine" },
    { command: "open-folder.a.b" },
    { command: "open-folder." },
    { title: "no command" },
    { command: "open-folder.pick", title: "twice" },
  ] });
  expect(got.map((c: any) => c.name)).toEqual(["open-folder.pick"]);
  expect(got[0]).toMatchObject({ group: "Mods", title: "Open folder…", id: "open-folder.pick:{}" });
  expect(commandsOf("x", { commands: "nope" })).toEqual([]);
  expect(commandsOf("x", null)).toEqual([]);
  expect(commandsOf("x", { commands: [{ command: "x.y", title: "Quit\n\u202eHerdr" }] })[0].title).toBe("Quit Herdr");
});

test("palette: a command name over 80 characters is refused, not cut to another", async () => {
  const { commandsOf } = await loadPalette();
  const long = `m.${"a".repeat(79)}`;
  expect(commandsOf("m", { commands: [{ command: long }, { command: long.slice(0, 80) }] }).map((c: any) => c.name)).toEqual([long.slice(0, 80)]);
  // A title is cut, not refused.
  expect(commandsOf("m", { commands: [{ command: "m.x", title: "t".repeat(100) }] })[0].title).toHaveLength(60);
});

test("palette: load() reads each mod's mod.json, falls back to ls, and skips a big one", async () => {
  const files: Record<string, string> = {
    "/h/.config/herdr-app/mods/open-folder/mod.json": JSON.stringify({ commands: [{ command: "open-folder.pick", title: "Open folder…" }] }),
    "/h/.config/herdr-app/mods/big/mod.json": JSON.stringify({ commands: [{ command: "big.x" }], pad: "p".repeat(70000) }),
    "/h/.config/herdr-app/mods/pane/mod.json": JSON.stringify({ commands: [{ command: "pane.x" }] }),
  };
  const lsCalls: string[][] = [];
  const env = {
    fs: { readdir: async () => { throw new Error("outside the home folder"); } },
    exec: async (cmd: string, args: string[]) => { lsCalls.push([cmd, ...args]); return { code: 0, stdout: "open-folder\nbig\npane\n.hidden\n", stderr: "" }; },
    readTextFile: async (p: string) => files[p] ?? null,
  };
  const { load, current } = await loadPalette([], env);
  let done = 0;
  await load(() => done++);
  expect(done).toBe(1);
  expect(lsCalls).toEqual([["ls", "-1A", "--", "/h/.config/herdr-app/mods"]]);
  // pane is an app namespace; big's mod.json is over 64 KB.
  expect(current().map((c: any) => c.name)).toEqual(["open-folder.pick"]);
});

test("palette: a load an open after it overtook changes nothing", async () => {
  let release: () => void = () => {};
  const slow = new Promise<void>((r) => (release = r));
  let first = true;
  const env = {
    fs: { readdir: async () => { if (first) { first = false; await slow; return ["old"]; } return ["new"]; } },
    readTextFile: async (p: string) => JSON.stringify({ commands: [{ command: `${p.split("/").at(-2)}.go` }] }),
  };
  const { load, current } = await loadPalette([], env);
  const older = load(() => {});
  await load(() => {});
  release();
  await older;
  expect(current().map((c: any) => c.name)).toEqual(["new.go"]);
});

test("palette: a direction run from the chooser joins Recent once, in place of its family row", async () => {
  const store: Record<string, string> = {};
  const localStorage = { getItem: (k: string) => store[k] ?? null, setItem: (k: string, v: string) => { store[k] = v; } };
  let picked: unknown = { id: 'pane.split:{"direction":"right"}', family: "pane.split:family", at: 5 };
  const { withPicked } = await loadPalette([], { localStorage, shared: () => picked });
  const recent = withPicked(["pane.split:family", "app.quit:{}"]);
  expect(recent).toEqual(['pane.split:{"direction":"right"}', "app.quit:{}"]);
  expect(JSON.parse(store.recent)).toEqual(recent);
  // The same pick again: Recent as the palette left it.
  expect(withPicked(["app.quit:{}", 'pane.split:{"direction":"right"}'])).toEqual(["app.quit:{}", 'pane.split:{"direction":"right"}']);
  picked = { id: 7 };
  expect(withPicked(["x"])).toEqual(["x"]);
});

test("palette: a folder named like the app's commands lists none", async () => {
  const { listable } = await loadPalette();
  const taken = new Set(["project", "context", "app", "pane"]);
  expect(["open-folder", "space_groups"].every((n) => listable(n, taken))).toBe(true);
  expect(["app", "pane", "project", "context", "a.b", ".hidden", ""].some((n) => listable(n, taken))).toBe(false);
});

test("palette: direction families are one row until you type", async () => {
  const mod = { name: "open-folder.pick", args: {}, group: "Mods", title: "Open folder…", words: "", id: "open-folder.pick:{}" };
  const { entries, go } = await loadPalette([mod]);
  const list = [entry("tab.new", {}, "Tab"), entry("pane.split", { direction: "right" }), entry("pane.split", { direction: "down" }), entry("pane.go", { direction: "left" }), entry("pane.close")];
  const empty = entries(list, "");
  expect(empty.map((e: any) => e.id)).toEqual(["tab.new:{}", "pane.split:family", "pane.go:family", "pane.close:{}", "open-folder.pick:{}"]);
  expect(empty[1]).toMatchObject({ title: "Split pane…", dirs: ["right", "down"] });
  expect(entries(list, "split r").map((e: any) => e.id)).toContain('pane.split:{"direction":"right"}');
  expect(entries([...list, mod], "").filter((e: any) => e.id === mod.id)).toHaveLength(1);
  // A picked direction in Recent stays listed, so Recent can show it.
  const picked = 'pane.split:{"direction":"down"}';
  const withRecent = entries(list, "", [picked]).map((e: any) => e.id);
  expect(withRecent).toContain(picked);
  expect(withRecent).toContain("pane.split:family");
  expect(entries(list, "", ["a", "b", "c", "d", "e", picked]).map((e: any) => e.id)).not.toContain(picked);
  expect(go(list[0])).toBe(false);
  expect(go(empty[1])).toBe(true);
  expect(opened.at(-1)).toMatchObject({ mod: "pick-direction", props: { command: "pane.split", title: "Split pane", dirs: ["right", "down"] } });
});
