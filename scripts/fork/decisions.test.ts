// FORK(decisions): tests for the decisions pane (defaults/mods/decisions). The slate logic runs
// as is; main.js runs against stand-ins for gpui and herdr, as scripts/worktree.test.ts does.
import { test, expect } from "bun:test";
import { readFile } from "node:fs/promises";
import { Slate, parseSlate, parseDir, clean, cleanBlock, OTHER_ID } from "../../defaults/mods/decisions/slate.js";

const DIR = "/home/me/repo/.claude/decisions/2026-11-16-db";
const SPEC = {
  title: "Storage choices",
  intro: "Two calls before I build.",
  mode: "wait",
  decisions: [
    { id: "db", title: "Which database?", context: "We need one store.", options: [
      { id: "pg", label: "Postgres", recommended: true, pros: ["Known"], cons: ["Ops"] },
      { id: "lite", label: "SQLite" },
    ] },
    { id: "extras", title: "Which extras?", context: "Pick any.", select: "multi", options: [
      { id: "cache", label: "Cache" }, { id: "queue", label: "Queue", recommended: true },
    ] },
    { id: "name", title: "What should it be called?", context: "A short name.", type: "text" },
  ],
};

test("parseDir takes only a slate folder", () => {
  expect(parseDir(`${DIR}/`)).toBe(DIR);
  for (const bad of ["relative/.claude/decisions/x", "/home/me/.claude", "/a/../.claude/decisions/x", "/a/.claude/decisions/x/y", '/a"b/.claude/decisions/x', 7, null])
    expect(parseDir(bad)).toBeNull();
});

test("parseSlate refuses what the dashboard's server refuses", () => {
  expect(parseSlate(SPEC).ok).toBe(true);
  expect(parseSlate({ decisions: [] })).toEqual({ ok: false, error: "slate needs a non-empty 'decisions' array" });
  const other = structuredClone(SPEC);
  other.decisions[0].options[1].id = OTHER_ID;
  expect(parseSlate(other).ok).toBe(false);
  const dup = structuredClone(SPEC);
  dup.decisions[1].id = "db";
  expect(parseSlate(dup).ok).toBe(false);
});

test("hidden characters never reach the pane or the response", () => {
  expect(clean("a​b‮c\u{E0041}d\u001be f", 100)).toBe("a b c d e f");
  expect(cleanBlock("  x\ty z", 100)).toBe("  x    y\nz");
});

test("the response has the dashboard's shape", () => {
  const parsed = parseSlate(SPEC);
  if (!parsed.ok) throw new Error(parsed.error);
  const s = new Slate(parsed.spec, DIR);
  const [db, extras, name] = parsed.spec.decisions;
  s.choose(db, "lite");
  s.setNotes(db, "small​ app");
  s.choose(extras, "cache");
  s.choose(extras, "queue");
  s.setResponse(name, "atlas");
  s.setSlateNotes("thanks");
  const r: any = s.response(new Date("2026-11-16T00:00:00Z"));
  expect(r.slate).toBe("2026-11-16-db");
  expect(r.notes).toBe("thanks");
  expect(r.decisions[0]).toMatchObject({ id: "db", choice: "lite", choiceLabel: "SQLite", isRecommended: false, notes: "small  app", select: "single", deferred: false });
  expect(r.decisions[1]).toMatchObject({ choices: ["cache", "queue"], choiceLabels: ["Cache", "Queue"], select: "multi" });
  expect(r.decisions[2]).toMatchObject({ type: "text", response: "atlas" });
});

test("a draft comes back, minus what no longer fits the slate", () => {
  const parsed = parseSlate(SPEC);
  if (!parsed.ok) throw new Error(parsed.error);
  const s = new Slate(parsed.spec, DIR);
  s.restore({ at: 2, notes: "n", picks: { db: { choice: "gone", notes: "x" }, extras: { choices: ["cache", "gone"] }, name: { response: "atlas" } } });
  expect(s.at).toBe(2);
  expect(s.pick("db").choice).toBeNull();
  expect(s.pick("db").notes).toBe("x");
  expect(s.pick("extras").choices).toEqual(["cache"]);
  expect(s.pick("name").response).toBe("atlas");
});

// --- main.js against stand-ins ---

const source = (await readFile(new URL("../../defaults/mods/decisions/main.js", import.meta.url), "utf8"))
  .replace(/^import [^;]*;\n/gm, "").replace("export default class Decisions", "class Decisions");

type Node = { id: string; children: any[]; click?: (e: unknown, cx: unknown) => unknown; key?: (e: any, cx: any) => void };
function element() {
  const node: Node = { id: "", children: [] };
  const el: any = new Proxy(node, { get: (_, method) => {
    if (method === "node") return node;
    return (...args: any[]) => {
      if (method === "id") node.id = args[0];
      if (method === "child") node.children.push(args[0]?.node ?? args[0]);
      if (method === "children") node.children.push(...args[0].map((c: any) => c?.node ?? c));
      if (method === "when" && args[0]) args[1](el);
      if (method === "on_click") node.click = args[0];
      if (method === "on_key_down") node.key = args[0];
      return el;
    };
  } });
  return el;
}
const textOf = (n: any): string => (typeof n === "string" ? n : (n?.children ?? []).map(textOf).join(" "));
const find = (n: any, id: string): Node | undefined => (n?.id === id ? n : (n?.children ?? []).map((c: any) => find(c, id)).find(Boolean));

function field() {
  let value = "";
  const handlers: Record<string, Function> = {};
  return { on: (ev: string, fn: Function) => { handlers[ev] = fn; return true; }, value: () => value,
    set_value: (v: string) => { value = v; }, set_auto_grow() {}, type(v: string, cx: any) { value = v; handlers.change?.({}, cx); },
    fire(ev: string, cx: any) { handlers[ev]?.({}, cx); }, node: element() };
}

async function harness(dir: unknown, files: Record<string, string | null>, opts: { holdRead?: boolean; failRead?: boolean } = {}) {
  let release = () => {};
  const gate = opts.holdRead ? new Promise<void>((r) => { release = r; }) : Promise.resolve();
  const reads: string[] = [];
  const writes: [string, string, string | null][] = [];
  const store = new Map<string, string>();
  const fields: any[] = [];
  const pending: Promise<unknown>[] = [];
  const herdr = {
    props: () => (dir === undefined ? { pane: "p1" } : { dir, run: "run-1" }),
    readTextFile: async (p: string) => { reads.push(p); await gate; if (opts.failRead) throw new Error("read failed"); return files[p] ?? null; },
    writeTextFile: async (p: string, text: string, expected: string | null) => {
      if (expected === null && files[p] != null) throw new Error("exists");
      files[p] = text; writes.push([p, text, expected]); return null;
    },
    nextCommand: () => new Promise(() => {}),
    theme: () => new Proxy({}, { get: (_, k) => String(k) }),
    settings: () => ({}),
  };
  const gpui = {
    View: class {}, div: element, v_flex: element, h_flex: element,
    InputState: { new: () => { const f = field(); fields.push(f); return f; } },
    TextareaState: { new: () => { const f = field(); fields.push(f); return f; } },
    Input: { new: (f: any) => f.node }, Textarea: { new: (f: any) => f.node },
  };
  const slate = await import("../../defaults/mods/decisions/slate.js");
  const localStorage = { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => store.set(k, v), removeItem: (k: string) => store.delete(k) };
  const scope = { ...herdr, ...gpui, ...slate, localStorage };
  const Decisions = new Function(...Object.keys(scope), `${source}\nreturn Decisions;`)(...Object.values(scope));
  const cx: any = { timer: { after: (ms: number, fn: Function) => setTimeout(fn, 0) }, focus_handle: () => ({ focus() {} }), spawn: (fn: Function) => { pending.push(fn(cx)); }, notify() {}, stop_propagation() {} };
  const view = new Decisions();
  view.init({}, cx);
  await new Promise((r) => setTimeout(r, 0)); // the slate read; the focus loop never ends
  const draw = () => view.render().node;
  const click = async (id: string) => {
    const n = find(draw(), id);
    if (!n?.click) throw new Error(`no button ${id}: ${textOf(draw())}`);
    await n.click({}, cx);
  };
  const press = (key: string) => draw().key?.({ key, modifiers: {} }, cx);
  return { view, cx, draw, click, press, reads, writes, store, fields, files, release: () => release() };
}

const files = () => ({ [`${DIR}/slate.json`]: JSON.stringify(SPEC), [`${DIR}/response.json`]: null as string | null });

test("with no folder, the pane says how it opens and reads nothing", async () => {
  const h = await harness(undefined, {});
  expect(textOf(h.draw())).toContain("No slate here");
  expect(h.reads).toEqual([]);
});

test("a folder that isn't a slate folder is refused before any read", async () => {
  const h = await harness("/etc", {});
  expect(textOf(h.draw())).toContain("isn't a slate folder");
  expect(h.reads).toEqual([]);
});

test("pick, type, review and send writes response.json once", async () => {
  const h = await harness(DIR, files());
  expect(textOf(h.draw())).toContain("Which database?");
  h.press("b");
  expect(h.view.slate.pick("db").choice).toBe("lite");
  await h.click("next");
  h.press("a");
  h.press("b");
  expect(h.view.slate.pick("extras").choices).toEqual(["cache", "queue"]);
  await h.click("next");
  const [, answer] = h.fields;
  answer.type("atlas", h.cx);
  expect(h.view.slate.pick("name").response).toBe("atlas");
  await h.click("next");
  expect(textOf(h.draw())).toContain("3 answered");
  await h.click("send");
  expect(h.writes.length).toBe(1);
  const [path, text, expected] = h.writes[0];
  expect(path).toBe(`${DIR}/response.json`);
  expect(expected).toBeNull();
  expect(JSON.parse(text).decisions[2].response).toBe("atlas");
  expect(JSON.parse(text).run).toBe("run-1");
  expect(textOf(h.draw())).toContain("Sent");
  expect(h.store.size).toBe(0);
});

test("letters typed into a field never pick options", async () => {
  const h = await harness(DIR, files());
  h.fields[0].fire("focus", h.cx);
  h.press("a");
  expect(h.view.slate.pick("db").choice).toBeNull();
});

test("an answer already saved shows as sent and is never replaced", async () => {
  const f = files();
  f[`${DIR}/response.json`] = JSON.stringify({ run: "run-1" });
  const h = await harness(DIR, f);
  expect(textOf(h.draw())).toContain("Sent");
  expect(find(h.draw(), "send")).toBeUndefined();
});

test("close without sending writes the closed marker, not a response", async () => {
  const h = await harness(DIR, files());
  h.view.slate.goTo(3);
  await h.click("set-aside");
  expect(h.writes.map((w) => w[0])).toEqual([`${DIR}/closed.json`]);
  expect(JSON.parse(h.writes[0][1]).run).toBe("run-1");
  expect(textOf(h.draw())).toContain("Closed without sending");
});

test("the draft survives a reload of the pane", async () => {
  const h = await harness(DIR, files());
  h.press("b");
  const saved = [...h.store.values()][0];
  expect(JSON.parse(saved).picks.db.choice).toBe("lite");
});

test("a slate the pane can't show tells the script, so it opens the browser", async () => {
  const f = files();
  f[`${DIR}/slate.json`] = JSON.stringify({ decisions: [{ id: "bad\u202e", title: "t", context: "c" }] });
  const h = await harness(DIR, f);
  await new Promise((r) => setTimeout(r, 0));
  expect(textOf(h.draw())).toContain("Can't show this slate");
  expect(textOf(h.draw())).not.toContain("\u202e");
  const marker = JSON.parse(h.files[`${DIR}/closed.json`] ?? "{}");
  expect(marker.run).toBe("run-1");
  expect(marker.error).toContain("kebab-case");
});

test("closing the pane unsent counts as closed; after Send it writes nothing", async () => {
  const h = await harness(DIR, files());
  await h.view.close();
  expect(JSON.parse(h.files[`${DIR}/closed.json`] ?? "{}").reason).toBe("pane closed");
  const sent = await harness(DIR, files());
  sent.view.slate.goTo(3);
  await sent.click("send");
  await sent.view.close();
  expect(sent.writes.map((w) => w[0])).toEqual([`${DIR}/response.json`]);
});

test("a draft belongs to its run", async () => {
  const h = await harness(DIR, files());
  h.press("b");
  expect([...h.store.keys()]).toEqual([`draft:${DIR}:run-1`]);
});

test("a failed close marker keeps the pane open with a way to retry", async () => {
  const f = files();
  f[`${DIR}/closed.json`] = "{}"; // an older run's marker the script has not moved aside yet
  const h = await harness(DIR, f);
  h.view.slate.goTo(3);
  await h.click("set-aside");
  expect(textOf(h.draw())).toContain("Couldn't tell Claude");
  delete f[`${DIR}/closed.json`];
  h.files[`${DIR}/closed.json`] = null;
  await h.click("set-aside");
  expect(textOf(h.draw())).toContain("Closed without sending");
});

test("nothing changes while the answers are being saved", async () => {
  const h = await harness(DIR, files());
  h.view.saving = true;
  h.press("b");
  h.fields[0].type("late note", h.cx);
  expect(h.view.slate.pick("db").choice).toBeNull();
  expect(h.view.slate.pick("db").notes).toBe("");
});

test("closing while the slate loads records it, and the load does not reopen the pane", async () => {
  const h = await harness(DIR, files(), { holdRead: true });
  await h.view.close();
  h.release();
  await new Promise((r) => setTimeout(r, 0));
  expect(JSON.parse(h.files[`${DIR}/closed.json`] ?? "{}").reason).toBe("pane closed");
  expect(h.view.phase).toBe("loading");
});

test("a response from another run does not show as sent", async () => {
  const f = files();
  f[`${DIR}/response.json`] = JSON.stringify({ run: "older" });
  const h = await harness(DIR, f);
  expect(textOf(h.draw())).toContain("Which database?");
});

test("text typed while saving goes back to what is being saved", async () => {
  const h = await harness(DIR, files());
  h.view.slate.goTo(3);
  h.view.loadFields();
  h.view.saving = true;
  const extra = h.fields[2];
  extra.type("typed too late", h.cx);
  expect(extra.value()).toBe("");
  expect(h.view.slate.notes).toBe("");
});

test("a read that fails after the pane closed leaves only the close marker", async () => {
  const h = await harness(DIR, files(), { holdRead: true, failRead: true });
  await h.view.close();
  h.release();
  await new Promise((r) => setTimeout(r, 0));
  expect(JSON.parse(h.files[`${DIR}/closed.json`] ?? "{}").reason).toBe("pane closed");
  expect(h.view.phase).toBe("loading");
});
