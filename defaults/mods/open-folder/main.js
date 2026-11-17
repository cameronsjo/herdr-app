// FORK(open-folder): open a folder, going to the space already there instead
// of making a second one (the TUI fork's `workspace open`, cameronsjo/herdr-app#9).
//
//   herdr-app do open-folder.open:/path/to/repo   (also ~/repo)
//   "alt-o": "open-folder.pick"                    (keys.json: asks for the folder)
//
// A space is "there" when it was made in that folder or its first terminal is in
// it now (the folder the spaces list names it after). Only this computer's spaces
// count, and a new space is made on this computer. Runs in the background, with
// no block of its own (src/window/fork_mods.rs).

import { View, div } from "gpui-kit";
import { spaces, panes, run, pickFolder, home, notify, nextCommand, checking, window } from "herdr";

// The folder as spaces() and panes() report it: ~ expanded, no trailing
// separator (a root keeps its own). `win`: Windows, where \ separates too.
function normal(path, homeDir, win = false) {
  const sep = win ? /[\\/]+$/ : /\/+$/;
  let p = String(path ?? "").trim();
  if (p === "~" || p.startsWith("~/") || (win && p.startsWith("~\\"))) p = (homeDir.replace(sep, "") || homeDir.trim().slice(0, 1)) + p.slice(1);
  const stripped = p.replace(sep, "");
  if (stripped) p = stripped;
  else if (p) p = p[0];
  // A bare drive left by the strip (C:\ -> C:) is its root again.
  if (win && /^[A-Za-z]:$/.test(p) && path.trim().length > 2) p += "\\";
  return p;
}

// A relative folder would resolve against the app's own working directory.
// Absolute: /x, and on Windows C:\x, C:/x or \\server\share too.
const absolute = (p, win = false) => p.startsWith("/") || (win && /^([A-Za-z]:[\\/]|\\\\)/.test(p));

// Two spellings of one folder compare equal: on Windows / and \ are one
// separator and case doesn't matter (C:/Users/x is C:\users\x).
const key = (p, homeDir, win) => {
  const n = normal(p, homeDir, win);
  return win ? n.replace(/\//g, "\\").toLowerCase() : n;
};

// The first of this computer's spaces in `folder` (made there, else its first
// terminal is there now), or null.
function existing(folder, all, ps, homeDir, win = false) {
  const want = key(folder, homeDir, win);
  if (!want) return null;
  const local = all.filter((s) => (s.machine ?? null) === null);
  const first = (s) => {
    const tabs = new Set((s.tabs ?? []).map((c) => c.id));
    return ps.find((p) => tabs.has(p.tab) && p.cwd)?.cwd ?? null;
  };
  return local.find((s) => s.folder && key(s.folder, homeDir, win) === want)
    ?? local.find((s) => { const c = first(s); return c !== null && key(c, homeDir, win) === want; })
    ?? null;
}

// What a command asks for: { open: folder } or { pick: true }, else null.
function parse(command) {
  if (command === "pick") return { pick: true };
  if (command.startsWith("open:")) {
    const folder = command.slice(5);
    return folder.trim() ? { open: folder } : null;
  }
  return null;
}

export default class OpenFolder extends View {
  init(_props, cx) {
    if (checking()) return;
    cx.spawn(async (cx) => {
      for (;;) {
        const command = await nextCommand();
        const asked = parse(command);
        if (!asked) {
          notify({ title: "open-folder", body: `Unknown command ${JSON.stringify(command)}: use open:<folder> or pick.`, level: "error" });
          continue;
        }
        // One failure must not end the loop: later commands would go nowhere.
        try {
          const folder = asked.pick ? await pickFolder({ prompt: "Open" }) : asked.open;
          if (!folder) continue;
          const win = window()?.platform === "windows";
          if (!absolute(normal(folder, home(), win), win)) {
            notify({ title: "open-folder", body: `Give a whole path (${win ? "C:\\..." : "/..."} or ~/...), not ${JSON.stringify(folder)}.`, level: "error" });
            continue;
          }
          this.open(folder, win);
        } catch (e) {
          notify({ title: "Can't open the folder", body: String(e?.message ?? e), level: "error" });
        }
      }
    });
  }

  open(folder, win) {
    const homeDir = home();
    const there = existing(folder, spaces(), panes(), homeDir, win);
    if (there) run("space.select", { id: there.id });
    else run("space.new", { folder: normal(folder, homeDir, win), machine: "" });
  }

  render() {
    return div();
  }
}
