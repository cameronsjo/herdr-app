// FORK(palette): the fork's additions to the frame's command palette (the TUI
// fork's palette, cameronsjo/herdr-app#12). main.js reaches each one from a
// line marked FORK(palette); everything else stays upstream's.
//
// - Mod commands: a mod lists its own `<mod>.<command>`s in its mod.json,
//   `"commands": [{ "command": "open-folder.pick", "title": "Open folder…" }]`,
//   and they show in a "Mods" group. A mod lists only commands it owns.
// - Direction chooser: with nothing typed, "Split pane…", "Focus pane…" and
//   "Swap pane…" are one row each; choosing one opens `pick-direction`, where
//   an arrow key (or a click) picks the way. Typing still finds "Split right".

import { appHome, readTextFile, openOverlay, commands, exec, shared, window as win } from "herdr";
import * as fs from "fs/promises";

// Commands the palette names once per direction, gathered into one row.
const FAMILIES = [
  { name: "pane.split", group: "Pane", title: "Split pane…", dirs: ["right", "down"] },
  { name: "pane.go", group: "Pane", title: "Focus pane…", dirs: ["left", "right", "up", "down"] },
  { name: "pane.swap", group: "Pane", title: "Swap pane…", dirs: ["left", "right", "up", "down"] },
];

// Every mod's listed commands, read on each palette open (`load`). Module state on
// purpose: shared by every open, so a later open starts from the last read.
let listed = [];
// Bumped by each load: only the newest one may set `listed`.
let loads = 0;

// Where pick-direction leaves the direction row it ran ({id, family, at}), for Recent.
export const PICKED = "fork.palette.picked";

// One line, control and format characters dropped, cut to `max` code points.
const text = (v, max) => {
  if (typeof v !== "string") return null;
  const s = v.replace(/[\p{Cc}\p{Cf}\u2028\u2029]+/gu, " ").trim();
  return s ? Array.from(s).slice(0, max).join("") : null;
};

// Names a mod folder can't take for its commands: the app's own command
// namespaces (`app.`, `pane.`, ...) and the aliases keys.json accepts.
function appSpaces() {
  return new Set(["project", "context", ...commands().map((c) => c.name.split(".")[0])]);
}

// A mod folder whose commands the palette may list.
export const listable = (name, taken) => /^[\w-]+$/.test(name) && !taken.has(name);

// The commands one mod.json lists: only `<that mod>.<command>`, plain names of
// at most 80 characters (a longer one is refused, never cut to another name).
export function commandsOf(mod, manifest) {
  const list = Array.isArray(manifest?.commands) ? manifest.commands : [];
  const out = [];
  for (const c of list.slice(0, 20)) {
    const command = c?.command;
    if (typeof command !== "string" || command.length > 80 || !command.startsWith(`${mod}.`) || !/^[\w-]+$/.test(command.slice(mod.length + 1))) continue;
    if (out.some((o) => o.name === command)) continue;
    out.push({ name: command, args: {}, group: "Mods", title: text(c.title, 60) ?? command, words: `${mod} ${text(c.words, 120) ?? ""}`, id: `${command}:{}` });
  }
  return out;
}

// The folder's entries, and whether `fs` could read the directory directly.
// Mods may read only under the home/app folder, so an app home set elsewhere
// (HERDR_APP_HOME) is listed by `ls`, or `dir` on Windows, and read via `readTextFile`.
async function folders(dir) {
  try {
    return { entries: await fs.readdir(dir), direct: typeof fs.readFile === "function" };
  } catch (e) {
    if (win()?.platform !== "windows") {
      const out = await exec("ls", ["-1A", "--", dir]);
      if (out.code === 0) return { entries: out.stdout.split("\n"), direct: false };
    } else if (!/[&|<>^%!"()]/.test(dir)) {
      // cmd parses its line again: a path with its special characters isn't listed.
      const out = await exec("cmd", ["/d", "/c", "dir", "/b", "/ad", dir.replace(/\//g, "\\")]);
      if (out.code === 0) return { entries: out.stdout.split(/\r?\n/), direct: false };
    }
    throw e;
  }
}

// Reads every mod's mod.json under the app home; `done()` once there. A load
// that an open after it overtook changes nothing.
export async function load(done) {
  const mine = ++loads;
  try {
    const root = `${appHome().replace(/[\\/]+$/, "")}/mods`;
    const taken = appSpaces();
    const { entries, direct } = await folders(root);
    const names = entries.filter((n) => listable(n, taken)).sort().slice(0, 200);
    const found = [];
    for (const mod of names) {
      const path = `${root}/${mod}/mod.json`;
      const raw = direct
        ? await fs.readFile(path, "utf8").catch(() => null)
        : await readTextFile(path).catch(() => null);
      if (!raw || raw.length > 65536) continue;
      try { found.push(...commandsOf(mod, JSON.parse(raw))); } catch {}
    }
    if (mine !== loads) return;
    listed = found;
  } catch {
    if (mine !== loads) return;
    listed = [];
  }
  done();
}

// `recent` (the palette's Recent ids) with the direction row pick-direction
// last ran in front, in place of its family row, once per pick. Saved, since
// the palette saves Recent only when it runs something itself.
export function withPicked(recent) {
  const p = shared(PICKED);
  if (!p || typeof p.id !== "string" || typeof p.family !== "string" || !Number.isFinite(p.at)) return recent;
  let seen = 0;
  try { seen = Number(localStorage.getItem("fork.picked")) || 0; } catch {}
  if (p.at <= seen) return recent;
  const out = [p.id, ...recent.filter((r) => r !== p.id && r !== p.family)].slice(0, 20);
  try {
    localStorage.setItem("fork.picked", String(p.at));
    localStorage.setItem("recent", JSON.stringify(out));
  } catch {}
  return out;
}

// The palette's entries with the mods' commands added and, with nothing typed,
// each direction family as one row in place of its first member. A direction
// row among the first five `recent` ids stays too, so Recent can show it; Recent
// shows those five, so it never also lands in its group.
export function entries(list, query, recent = []) {
  const all = [...list, ...listed.filter((m) => !list.some((e) => e.name === m.name))];
  if (query.trim()) return all;
  const shown = recent.slice(0, 5);
  const out = [];
  for (const e of all) {
    const f = FAMILIES.find((f) => f.name === e.name && e.args?.direction);
    if (!f) { out.push(e); continue; }
    if (shown.includes(e.id)) out.push(e);
    if (out.some((o) => o.family === f.name)) continue;
    out.push({ name: f.name, args: {}, group: f.group, title: f.title, words: "", family: f.name, dirs: f.dirs, id: `${f.name}:family` });
  }
  return out;
}

// Runs what the palette chose when it's the fork's: a family opens the chooser.
export function go(e) {
  if (!e?.family) return false;
  openOverlay({ mod: "pick-direction", kind: "dialog", width: 320, height: 200, props: { command: e.name, title: e.title.replace(/…$/, ""), dirs: e.dirs } });
  return true;
}
