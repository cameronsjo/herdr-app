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
import { appHome, readTextFile, writeTextFile, watchFolder, exec, setShared, window, checking } from "herdr";
import * as fs from "fs/promises";

const DIR = "outside-agents";
const KEY = "fork.outside-agents";
const MAX = 64;
// A heartbeat is a few lines; a bigger file is not one.
const MAX_TEXT = 4096;
const START = 2000;
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
`;

// Any process can write a heartbeat, so text is drawn on one line with control,
// bidi and invisible characters dropped, cut to `n` code points (as agents/fork.js does).
const clip = (v, n = 200) => {
  if (typeof v !== "string") return null;
  const s = v.replace(/[\u0000-\u001f\u007f-\u009f\u061c\u200b-\u200f\u2028\u2029\u202a-\u202e\u2066-\u2069\ufeff]+/g, " ").trim();
  return s ? Array.from(s).slice(0, n).join("").trimEnd() : null;
};
const fileId = (path) => path.split(/[\\/]/).pop().replace(/\.json$/, "");

// One file's agent, or null: not JSON, no known status, or a bad id.
function parse(id, text, seenAt) {
  if (!/^[\w.-]{1,64}$/.test(id) || typeof text !== "string" || text.length > MAX_TEXT) return null;
  let v;
  try { v = JSON.parse(text); } catch { return null; }
  const status = Object.hasOwn(STATUS, v?.status) ? STATUS[v.status] : null;
  if (!status) return null;
  const ttl = Math.min(TTL.max, Math.max(TTL.min, Number.isFinite(v.ttl) ? v.ttl : TTL.def));
  // Seconds or milliseconds; a time ahead of now counts as now.
  const at = Number.isFinite(v.at) && v.at > 0 ? (v.at < 1e12 ? v.at * 1000 : v.at) : seenAt;
  return { id, name: clip(v.name, 80) ?? id, status, title: clip(v.title), task: clip(v.task), until: Math.min(at, seenAt) + ttl * 1000 };
}

// The agents still alive at `now`, by name, at most MAX.
function live(agents, now) {
  return [...agents.values()]
    .filter((a) => a.until > now)
    .sort((x, y) => x.name.localeCompare(y.name) || x.id.localeCompare(y.id))
    .slice(0, MAX)
    .map(({ until, text, ...a }) => a);
}

export default class OutsideAgents extends View {
  init(_props, cx) {
    if (checking()) return;
    this.dir = `${appHome().replace(/[\\/]+$/, "")}/${DIR}`;
    this.agents = new Map();
    // Shared state outlives the app: clear the last run's list until this one reads.
    this.sent = "[]";
    setShared(KEY, []);
    cx.spawn(async (cx) => {
      // Start-up first: the first look waits two seconds.
      await new Promise((done) => cx.timer.after(START, done));
      for (;;) {
        // Makes the folder and reads it; again after any failure, five seconds on.
        try {
          await this.start();
          for (;;) {
            const paths = await watchFolder(this.dir);
            // The folder itself (a batch too big to list, or its README) means look again.
            const files = paths.filter((p) => p.endsWith(".json"));
            if (files.length < paths.length || !paths.length) await this.relist();
            // A changed file is a heartbeat even when a relist came with it (a temp-file rename).
            if (files.length) await this.load(files);
            this.publish();
          }
        } catch {
          await new Promise((done) => cx.timer.after(5000, done));
        }
      }
    });
    cx.timer.every(5000, () => this.publish());
  }

  // Makes the folder (with its README) and reads what is there.
  async start() {
    const readme = `${this.dir}/README.md`;
    if ((await readTextFile(readme)) == null) {
      try { await writeTextFile(readme, README, null); } catch {}
    }
    await this.relist();
    this.publish();
  }

  async relist() {
    const names = (await this.names()).filter((n) => n.endsWith(".json")).slice(0, MAX * 2);
    const keep = new Set(names.map(fileId));
    for (const id of this.agents.keys()) if (!keep.has(id)) this.agents.delete(id);
    await this.load(names.map((n) => `${this.dir}/${n}`), true);
  }

  // The folder's file names. Mods may read only under the home folder, so an app
  // home set elsewhere (HERDR_APP_HOME) is listed by `ls`, or `dir` on Windows.
  async names() {
    try {
      return await fs.readdir(this.dir);
    } catch (e) {
      if (window()?.platform !== "windows") {
        const out = await exec("ls", ["-1A", "--", this.dir]);
        if (out.code === 0) return out.stdout.split("\n");
      } else {
        const out = await exec("cmd", ["/d", "/c", "dir", "/b", "/a-d", this.dir.replace(/\//g, "\\")]);
        // An empty folder: "File Not Found", exit 1, no output.
        if (out.code === 0 || !out.stdout.trim()) return out.stdout.split(/\r?\n/);
      }
      throw e;
    }
  }

  // `relisting`: a file read again unchanged is not a heartbeat; a change event is,
  // even when the agent wrote the same text.
  async load(paths, relisting = false) {
    const now = Date.now();
    for (const path of paths) {
      const id = fileId(path);
      const text = await readTextFile(path).catch(() => null);
      if (relisting && text != null && this.agents.get(id)?.text === text) continue;
      const agent = parse(id, text, now);
      if (!agent) { this.agents.delete(id); continue; }
      // An expired record stays (so an unchanged file isn't read as a new heartbeat)
      // until room is needed: at most twice what shows are kept.
      if (!this.agents.has(id) && this.agents.size >= MAX * 2) {
        for (const [old, a] of this.agents) if (a.until <= now) this.agents.delete(old);
        if (this.agents.size >= MAX * 2) continue;
      }
      this.agents.set(id, { ...agent, text });
    }
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
