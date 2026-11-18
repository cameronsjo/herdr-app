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

import { appHome, readTextFile, openOverlay, commands, exec, window as win } from "herdr";
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

// The commands one mod.json lists: only `<that mod>.<command>`, plain names.
export function commandsOf(mod, manifest) {
  const list = Array.isArray(manifest?.commands) ? manifest.commands : [];
  const out = [];
  for (const c of list.slice(0, 20)) {
    const command = text(c?.command, 80);
    if (!command?.startsWith(`${mod}.`) || !/^[\w-]+$/.test(command.slice(mod.length + 1))) continue;
    if (out.some((o) => o.name === command)) continue;
    out.push({ name: command, args: {}, group: "Mods", title: text(c.title, 60) ?? command, words: `${mod} ${text(c.words, 120) ?? ""}`, id: `${command}:{}` });
  }
  return out;
}

// The folder's entries. Mods may read only under the home folder, so an app home
// set elsewhere (HERDR_APP_HOME) is listed by `ls`, or `dir` on Windows.
async function folders(dir) {
  try {
    return await fs.readdir(dir);
  } catch (e) {
    if (win()?.platform !== "windows") {
      const out = await exec("ls", ["-1A", "--", dir]);
      if (out.code === 0) return out.stdout.split("\n");
    } else if (!/[&|<>^%!"()]/.test(dir)) {
      // cmd parses its line again: a path with its special characters isn't listed.
      const out = await exec("cmd", ["/d", "/c", "dir", "/b", "/ad", dir.replace(/\//g, "\\")]);
      if (out.code === 0) return out.stdout.split(/\r?\n/);
    }
    throw e;
  }
}

// Reads every mod's mod.json under the app home; `done()` once there.
export async function load(done) {
  try {
    const root = `${appHome().replace(/[\\/]+$/, "")}/mods`;
    const taken = appSpaces();
    const names = (await folders(root)).filter((n) => listable(n, taken)).sort().slice(0, 200);
    const found = [];
    for (const mod of names) {
      const raw = await readTextFile(`${root}/${mod}/mod.json`).catch(() => null);
      if (!raw || raw.length > 65536) continue;
      try { found.push(...commandsOf(mod, JSON.parse(raw))); } catch {}
    }
    listed = found;
  } catch {
    listed = [];
  }
  done();
}

// The palette's entries with the mods' commands added and, with nothing typed,
// each direction family as one row in place of its first member.
export function entries(list, query) {
  const all = [...list, ...listed.filter((m) => !list.some((e) => e.name === m.name))];
  if (query.trim()) return all;
  const out = [];
  for (const e of all) {
    const f = FAMILIES.find((f) => f.name === e.name && e.args?.direction);
    if (!f) { out.push(e); continue; }
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
