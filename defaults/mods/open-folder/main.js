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
import { spaces, panes, run, exec, pickFolder, home, notify, nextCommand, checking, window } from "herdr";

// The folder as spaces() and panes() report it: ~ expanded, no trailing
// separator (a root keeps its own). `win`: Windows, where \ separates too.
// Spaces are kept: a folder's name may end in one. Blank is "".
function normal(path, homeDir, win = false) {
  const sep = win ? /[\\/]+$/ : /\/+$/;
  let p = String(path ?? "");
  if (!p.trim()) return "";
  if (p === "~" || p.startsWith("~/") || (win && p.startsWith("~\\"))) {
    // A home of / strips to "": ~/x is then /x, not //x.
    const base = homeDir.replace(sep, "");
    p = p === "~" ? base || homeDir.trim().slice(0, 1) : base + p.slice(1);
  }
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
// terminal is there now), or null. `resolved`: the folder with symlinks
// resolved (/tmp is /private/tmp on macOS), which counts as the same folder.
function existing(folder, all, ps, homeDir, win = false, resolved = null) {
  const want = new Set([folder, resolved].filter((f) => f).map((f) => key(f, homeDir, win)).filter((k) => k));
  if (!want.size) return null;
  const local = all.filter((s) => (s.machine ?? null) === null);
  const first = (s) => {
    const tabs = new Set((s.tabs ?? []).map((c) => c.id));
    return ps.find((p) => tabs.has(p.tab) && p.cwd)?.cwd ?? null;
  };
  // A space with no folder of its own was made at home.
  return local.find((s) => want.has(key(s.folder || homeDir, homeDir, win)))
    ?? local.find((s) => { const c = first(s); return c !== null && want.has(key(c, homeDir, win)); })
    ?? null;
}

// `folder` with symlinks resolved, or null (Windows, or it can't be read).
async function physical(folder, win) {
  if (win) return null;
  try {
    const r = await exec("/bin/pwd", ["-P"], { cwd: folder, timeout: 5000 });
    return r.code === 0 ? r.stdout.replace(/\n$/, "") || null : null;
  } catch {
    return null;
  }
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
          await this.open(folder, win);
        } catch (e) {
          notify({ title: "Can't open the folder", body: String(e?.message ?? e), level: "error" });
        }
      }
    });
  }

  async open(folder, win) {
    const homeDir = home();
    const resolved = await physical(normal(folder, homeDir, win), win);
    const there = existing(folder, spaces(), panes(), homeDir, win, resolved);
    if (there) run("space.select", { id: there.id });
    else run("space.new", { folder: normal(folder, homeDir, win), machine: "" });
  }

  render() {
    return div();
  }
}
