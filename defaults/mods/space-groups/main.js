// FORK(space-groups): move the shown space up or down the list as one block with
// its git worktrees (the TUI fork's grouped reorder, cameronsjo/herdr-app#13).
//
//   "alt-shift-up": "space-groups.up", "alt-shift-down": "space-groups.down"   (keys.json)
//   herdr-app do space-groups.up
//
// A group is a repo's main checkout and the worktree spaces after it, as the
// spaces list nests them. The move goes past one whole neighbouring group (or a
// lone space) on the same machine, and the members keep their order. On a
// worktree space, its whole group moves. Runs in the background, with no block of
// its own (src/window/fork_mods.rs).

import { View, div } from "gpui-kit";
import { spaces, panes, run, exec, nextCommand, notify, checking } from "herdr";

const AT_ONCE = 6;

// Where a space is now: its first terminal's live folder, else where it was made
// (the spaces list's rule).
function place(s, ps) {
  const tabs = new Set((s.tabs ?? []).map((c) => c.id));
  return ps.find((p) => tabs.has(p.tab) && p.cwd)?.cwd || s.folder;
}

// Blocks of space ids in list order. `git[id]`: { repo, linked } or null. A
// worktree joins the block of the first main checkout of its repo on its machine
// before it; with none before it, it stands alone (the spaces list then moves it
// under its main on its own).
function blocks(all, git) {
  const out = [];
  const mains = new Map(); // JSON [machine, repo] -> its block
  for (const s of all) {
    const g = git[s.id];
    const key = g ? JSON.stringify([s.machine ?? null, g.repo]) : null;
    if (g?.linked && mains.has(key)) {
      mains.get(key).ids.push(s.id);
      continue;
    }
    const block = { machine: s.machine ?? null, ids: [s.id] };
    if (g && !g.linked && !mains.has(key)) mains.set(key, block);
    out.push(block);
  }
  return out;
}

// The space.move calls ({ id, index } from 1, applied in order) that move the
// block holding `id` past its neighbour in `by` (-1 up, 1 down) on its machine.
// [] when it is already first or last there.
function moves(all, git, id, by) {
  const list = blocks(all, git);
  const at = list.findIndex((b) => b.ids.includes(id));
  if (at === -1) return [];
  let to = at + by;
  while (to >= 0 && to < list.length && list[to].machine !== list[at].machine) to += by;
  if (to < 0 || to >= list.length || list[to].machine !== list[at].machine) return [];
  const target = list.map((b) => b.ids);
  [target[at], target[to]] = [target[to], target[at]];
  const want = target.flat();
  const order = all.map((s) => s.id);
  const calls = [];
  for (let i = 0; i < want.length; i++) {
    const from = order.indexOf(want[i]);
    if (from === i) continue;
    calls.push({ id: want[i], index: i + 1 });
    order.splice(i, 0, order.splice(from, 1)[0]);
  }
  return calls;
}

export default class SpaceGroups extends View {
  init(_props, cx) {
    if (checking()) return;
    cx.spawn(async (cx) => {
      for (;;) {
        const command = await nextCommand();
        const by = command === "up" ? -1 : command === "down" ? 1 : 0;
        if (!by) {
          notify({ title: "space-groups", body: `Unknown command ${JSON.stringify(command)}: use up or down.`, level: "error" });
          continue;
        }
        try {
          await this.move(by);
        } catch (e) {
          notify({ title: "Can't move the space", body: String(e?.message ?? e), level: "error" });
        }
      }
    });
  }

  async move(by) {
    const git = {};
    const looked = new Map(); // JSON [machine, folder] -> its lookup, shared by spaces in one folder
    // The list may change while git answers (a worktree space made meanwhile has
    // no lookup and would land out of its group): plan only once every space on
    // the machine has one, looking again for the new ones a few times.
    for (let round = 0; round < 3; round++) {
      const all = spaces();
      const shown = all.find((s) => s.active);
      if (!shown) return;
      const ps = panes();
      const mine = all.filter((s) => (s.machine ?? null) === (shown.machine ?? null));
      const missing = mine.filter((s) => !(s.id in git));
      if (!missing.length) {
        for (const call of moves(all, git, shown.id, by)) run("space.move", call);
        return;
      }
      // A few at a time, as the spaces list does: hundreds at once run out of open
      // files or a machine core's running execs.
      for (let i = 0; i < missing.length; i += AT_ONCE) {
        await Promise.all(missing.slice(i, i + AT_ONCE).map(async (s) => {
          const where = place(s, ps);
          const k = JSON.stringify([s.machine ?? null, where]);
          if (!looked.has(k)) looked.set(k, repo(where, s.machine));
          git[s.id] = await looked.get(k);
        }));
      }
    }
    throw new Error("The spaces kept changing; try again.");
  }

  render() {
    return div();
  }
}

// The repo a folder is in ({ repo: its common .git, linked: a worktree }), or
// null when it is in none. Throws when git couldn't answer (timed out, failed,
// unreachable machine): reading that as "no repo" would split a group.
async function repo(cwd, on) {
  if (!cwd) return null;
  let r;
  try {
    // One call: the git dir and the common dir differ only in a linked worktree
    // (a submodule's are the same, so it isn't taken for one).
    r = await exec("git", ["rev-parse", "--path-format=absolute", "--git-dir", "--git-common-dir"], { cwd, on: on ?? null, timeout: 5000 });
  } catch (e) {
    throw new Error(`git couldn't look at ${cwd}: ${e?.message ?? e}`);
  }
  if (r.code !== 0) {
    if (/not a git repository/i.test(r.stderr)) return null;
    throw new Error(`git couldn't look at ${cwd}: ${r.stderr.trim() || `exit ${r.code}`}`);
  }
  const [dir, common] = r.stdout.split("\n");
  if (!dir || !common) throw new Error(`git couldn't look at ${cwd}: unexpected answer`);
  return { repo: common, linked: dir !== common };
}
