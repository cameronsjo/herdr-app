// FORK(outside-agents): list agents that run outside Herdr's terminals (in
// cmux, a plain terminal, CI) in the agents list (the TUI fork's
// `agent register`, cameronsjo/herdr-app#14).
//
// Each agent keeps one JSON file in ~/.config/herdr-app/outside-agents/ (the
// README written there says how), and rewrites it as its heartbeat:
//
//   {"name": "cmux claude", "status": "working", "title": "fixing login", "task": "…", "ttl": 60}
//
// It drops out of the list `ttl` seconds after its last write (60 by default),
// or at once when its file is deleted. The list goes to the agents mod through
// shared "fork.outside-agents". Runs in the background, with no block of its own
// (src/window/fork_mods.rs).

import { View, div } from "gpui-kit";
import { appHome, readTextFile, writeTextFile, watchFolder, exec, shared, setShared, window, checking } from "herdr";

const DIR = "outside-agents";
const KEY = "fork.outside-agents";
const MAX = 64;
// A heartbeat is a few lines; a bigger file is not one.
const MAX_TEXT = 4096;
// The first look waits this long after start, which also keeps its file reads
// (on real threads) out of the window tests' scheduler.
const START = 2000;
// The most file names one listing keeps.
const LISTED = 1024;
// How often expired records drop out, in ms.
const TICK = 1000;
// Old texts of records dropped for room: a relist that reads one again is not a heartbeat.
const MAX_DROPPED = 1024;
const TTL = { min: 1, def: 60, max: 86400 };
const STATUS = { working: "working", blocked: "blocked", error: "blocked", done: "done", idle: "idle" };
const README = `# Agents outside Herdr

Herdr's agents list shows an agent that runs somewhere else (cmux, a plain
terminal, CI) while it keeps a file here. One JSON file per agent, named
<id>.json; rewrite it as the heartbeat:

    {"name": "cmux claude", "status": "working", "title": "fixing login", "task": "the login test", "ttl": 60}

status: working, blocked (needs you), done or idle. Only status is needed.
ttl: seconds after the last write before it drops out (1 to 86400, 60 if unset).
at: when it was written, in seconds or milliseconds since 1970, if the writer
knows; otherwise Herdr counts from when it saw the file change.
Delete the file to take the agent off the list at once.

A sandboxed agent needs write access to this folder only. Never grant it the
folder above (the app home): that holds mods, which run code in Herdr.
`;

// Any process can write a heartbeat, so text is drawn on one line with control,
// bidi and invisible characters dropped, cut to `n` code points (as agents/fork.js does).
const INVISIBLE = /[\u0000-\u001f\u007f-\u009f\u00ad\u034f\u061c\u115f\u1160\u180e\u200b-\u200f\u2028-\u202e\u2060-\u2064\u2066-\u206f\u3164\ufe00-\ufe0f\ufeff\uffa0\ufff9-\ufffb\u{e0000}-\u{e007f}\u{e0100}-\u{e01ef}]+/gu;
const clip = (v, n = 200) => {
  if (typeof v !== "string") return null;
  const s = v.replace(INVISIBLE, " ").trim();
  return s ? Array.from(s).slice(0, n).join("").trimEnd() : null;
};
const fileId = (path) => path.split(/[\\/]/).pop().replace(/\.json$/, "");
const validId = (id) => /^[\w.-]{1,64}$/.test(id);

// UTF-8 bytes in `text`, counting no further than `max + 1`.
function bytes(text, max) {
  if (text.length > max) return max + 1;
  let n = 0;
  for (const c of text) {
    const cp = c.codePointAt(0);
    n += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
    if (n > max) return n;
  }
  return n;
}

// One file's agent, or null: not JSON, no known status, or a bad id.
function parse(id, text, seenAt) {
  if (!validId(id) || typeof text !== "string" || bytes(text, MAX_TEXT) > MAX_TEXT) return null;
  let v;
  try { v = JSON.parse(text); } catch { return null; }
  const status = typeof v?.status === "string" && Object.prototype.hasOwnProperty.call(STATUS, v.status) ? STATUS[v.status] : null;
  if (!status) return null;
  const ttl = Math.min(TTL.max, Math.max(TTL.min, Number.isFinite(v.ttl) ? v.ttl : TTL.def));
  // Seconds or milliseconds; a time ahead of now counts as now.
  const at = Number.isFinite(v.at) && v.at > 0 ? (v.at < 1e12 ? v.at * 1000 : v.at) : seenAt;
  // `reason`: an error keeps its own word on the pill ("Error", not "Needs you").
  return { id, name: clip(v.name, 80) ?? id, status, reason: v.status === "error" ? "error" : null, title: clip(v.title), task: clip(v.task), until: Math.min(at, seenAt) + ttl * 1000 };
}

// The agents still alive at `now`, by name, at most MAX. `until` stays on each,
// so a reloaded copy of this mod can keep showing the live ones.
function live(agents, now) {
  return [...agents.values()]
    .filter((a) => a.until > now)
    .sort((x, y) => x.name.localeCompare(y.name) || x.id.localeCompare(y.id))
    .slice(0, MAX)
    .map(({ text, ...a }) => a);
}

// The names a folder event is about: `paths` (absolute) whose parent is the
// watched folder itself, not a folder inside it.
const changedNames = (paths) => new Set(paths.filter((p) => p.split(/[\\/]/).at(-2) === DIR).map((p) => p.split(/[\\/]/).pop()));

// Characters cmd.exe reads as syntax (or expands) in an unquoted argument.
const CMD_UNSAFE = /[&|<>^%!"()]/;

export default class OutsideAgents extends View {
  init(_props, cx) {
    if (checking()) return;
    this.dir = `${appHome().replace(/[\\/]+$/, "")}/${DIR}`;
    this.agents = new Map();
    this.dropped = new Map();
    // Shared state outlives the app: keep only what is still live, so the last
    // run's rows never outlive their ttl, and a reloaded copy doesn't blink.
    const kept = shared(KEY);
    const now = Date.now();
    const still = Array.isArray(kept) ? kept.filter((a) => a && Number.isFinite(a.until) && a.until > now) : [];
    this.sent = JSON.stringify(still);
    setShared(KEY, still);
    cx.spawn(async (cx) => {
      await new Promise((done) => cx.timer.after(START, done));
      for (;;) {
        // Makes the folder and reads it; again after any failure, five seconds on.
        try {
          await this.start();
          // Watching before the first read, so a write in between isn't missed.
          let watch = watchFolder(this.dir);
          await this.relist();
          this.publish();
          for (;;) {
            const paths = await watch;
            watch = watchFolder(this.dir);
            await this.changed(paths);
            this.publish();
          }
        } catch {
          await new Promise((done) => cx.timer.after(5000, done));
        }
      }
    });
    cx.timer.every(TICK, () => this.publish());
  }

  // Makes the folder with its README, unless the folder is a link: the README
  // would then land wherever it points.
  async start() {
    if (window()?.platform !== "windows" && (await exec("test", ["-L", this.dir])).code === 0) return;
    const readme = `${this.dir}/README.md`;
    if ((await readTextFile(readme)) == null) {
      try { await writeTextFile(readme, README, null); } catch {}
    }
  }

  // A folder event: the files it names are heartbeats (a change event is one
  // even when the agent wrote the same text); any other new file is read once.
  async changed(paths) {
    const names = await this.names();
    const keep = new Set(names.map(fileId));
    for (const id of this.agents.keys()) if (!keep.has(id)) this.agents.delete(id);
    const hit = changedNames(paths);
    await this.load(names.filter((n) => hit.has(n)).map((n) => `${this.dir}/${n}`));
    const fresh = names.filter((n) => !hit.has(n) && !this.agents.has(fileId(n)) && !this.dropped.has(fileId(n)));
    await this.load(fresh.slice(0, MAX * 2).map((n) => `${this.dir}/${n}`), true);
  }

  async relist() {
    const names = await this.names();
    const keep = new Set(names.map(fileId));
    for (const id of this.agents.keys()) if (!keep.has(id)) this.agents.delete(id);
    await this.load(names.slice(0, MAX * 2).map((n) => `${this.dir}/${n}`), true);
  }

  // The folder's agent files: plain files only, right in it, with a valid id,
  // at most LISTED (a relist reads the first MAX * 2; an event reads its own). Not links, folders, pipes or devices: reading a pipe would
  // hang, and a link reads a file outside the folder. Listed with `find` (or
  // `dir` on Windows), since mods have no stat.
  async names() {
    let out;
    if (window()?.platform !== "windows") {
      out = await exec("find", [this.dir, "-mindepth", "1", "-maxdepth", "1", "-type", "f", "-name", "*.json"]);
      if (out.code !== 0) throw new Error(out.stderr.trim() || `find exited ${out.code}`);
    } else {
      const dir = this.dir.replace(/\//g, "\\");
      if (CMD_UNSAFE.test(dir)) throw new Error("the app home's path has characters cmd can't take as a plain argument");
      // Files only, no reparse points (links).
      out = await exec("cmd", ["/d", "/c", "dir", "/b", "/a-d-l", dir]);
      // An empty folder: "File Not Found" on stderr, exit 1, no output.
      if (out.code !== 0 && !/File Not Found/i.test(out.stderr)) throw new Error(out.stderr.trim() || `dir exited ${out.code}`);
    }
    return out.stdout.split(/\r?\n/)
      .map((p) => p.split(/[\\/]/).pop())
      .filter((n) => n.endsWith(".json") && validId(fileId(n)))
      .sort()
      .slice(0, LISTED);
  }

  // `relisting`: a file read again unchanged is not a heartbeat; nor is a
  // record dropped for room read again with the same text.
  async load(paths, relisting = false) {
    const now = Date.now();
    for (const path of paths) {
      const id = fileId(path);
      const text = await readTextFile(path).catch(() => null);
      if (relisting && text != null && (this.agents.get(id)?.text === text || this.dropped.get(id) === text)) continue;
      this.dropped.delete(id);
      const agent = parse(id, text, now);
      if (!agent) { this.agents.delete(id); continue; }
      if (!this.agents.has(id) && this.agents.size >= MAX * 2 && !this.makeRoom(agent, now)) continue;
      this.agents.set(id, { ...agent, text });
    }
  }

  // Room for `agent` in a full map: the expired records go, else the one that
  // expires soonest, if it goes before `agent` does. False when none can go.
  makeRoom(agent, now) {
    const drop = (id) => {
      const a = this.agents.get(id);
      this.agents.delete(id);
      this.dropped.delete(id);
      this.dropped.set(id, a.text);
      while (this.dropped.size > MAX_DROPPED) this.dropped.delete(this.dropped.keys().next().value);
    };
    for (const [id, a] of [...this.agents]) if (a.until <= now) drop(id);
    if (this.agents.size < MAX * 2) return true;
    let soonest = null;
    for (const [id, a] of this.agents) if (!soonest || a.until < soonest[1].until) soonest = [id, a];
    if (!soonest || soonest[1].until >= agent.until) return false;
    drop(soonest[0]);
    return true;
  }

  publish() {
    const list = live(this.agents, Date.now());
    const json = JSON.stringify(list);
    if (json === this.sent) return;
    this.sent = json;
    setShared(KEY, list);
  }

  render() {
    return div();
  }
}
