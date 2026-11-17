// FORK(agents): the fork's additions to upstream's agents list (the TUI fork's
// sidebar tokens, cameronsjo/herdr-app#8). main.js reaches each one from a line
// marked FORK(agents); everything else stays upstream's.
//
// - Task line: `hc label <terminal> task="fix the login test"` puts a dim line
//   under the agent's row; `hc label <terminal> task=` clears it.
// - Name: a terminal started with a name (`hc run --name fix-login`, or
//   `hc name`) shows that name as the row's detail.
// - "Needs you" (sort menu): only agents that need you, most urgent first.
// - "Grouped by project" (sort menu): agents under one header per `project`
//   label (`hc label <terminal> project=herdr`), else per space.
//
// Location needs nothing here: upstream already names a row's space wherever no
// space header is above it.

import { div } from "gpui-kit";

export const FORK_SORTS = [
  { id: "needs", label: "Needs you", icon: "search" },
  { id: "project", label: "Grouped by project", icon: "folder" },
];

// The sort menu's height for upstream's sorts plus these (31 per entry).
export const menuHeight = (upstream) => upstream + 31 * FORK_SORTS.length;

// The needs-you list also sorts by priority, under upstream's section headers.
export const prioritySort = (id) => id === "needs";

// terminals() by address, built once per snapshot: every row asks.
const maps = new WeakMap();
function byAddress(all) {
  let m = maps.get(all);
  if (!m) maps.set(all, (m = new Map(all.map((t) => [t.address, t]))));
  return m;
}
const text = (v) => (typeof v === "string" && v.trim() ? v.trim() : null);

// A label's value on the terminal, trimmed, or null.
const label = (terminal, key) => text(terminal?.labels?.[key]);

// The terminal's own name, or null.
export function nameOf(all, address) {
  return text(byAddress(all).get(address)?.name);
}

export function taskOf(all, address) {
  return label(byAddress(all).get(address), "task");
}

// What a project group is called and keyed by: the `project` label, else the space.
function project(a, terminal) {
  const p = label(terminal, "project");
  return p ? { key: `p\u0000${p}`, label: p } : { key: `s\u0000${a.space}`, label: a.space_name || "" };
}

// The list in this sort's order: needs-you keeps only the agents that need you
// (`urgency(a)` is 4 when it or one of its sub-agents is blocked); grouped by
// project gathers each project behind its first agent, keeping their order.
export function order(id, list, all, urgency) {
  if (id === "needs") return list.filter((a) => urgency(a) === 4);
  if (id !== "project") return list;
  const terminals = byAddress(all);
  const keys = [];
  const members = new Map();
  for (const a of list) {
    const { key } = project(a, terminals.get(a.address));
    if (!members.has(key)) { keys.push(key); members.set(key, []); }
    members.get(key).push(a);
  }
  return keys.flatMap((k) => members.get(k));
}

// Grouped by project: one header per project over `rest` (already in order()).
// Other sorts keep `groups` as upstream built them.
export function groups(id, built, pinnedGroups, rest, all) {
  if (id !== "project") return built;
  const terminals = byAddress(all);
  const out = [...pinnedGroups];
  for (const a of rest) {
    const { key, label: name } = project(a, terminals.get(a.address));
    const gid = `proj-${key}`;
    if (out.at(-1)?.id === gid) out.at(-1).members.push(a);
    else out.push({ id: gid, label: name, status: null, members: [a] });
  }
  return out;
}

// The row with its task line under it, or the row alone.
export function withTask(row, a, all, t, collapsed) {
  const task = collapsed ? null : taskOf(all, a.address);
  if (!task) return row;
  return div()
    .flex_none()
    .child(row)
    .child(div().id(`task-${a.address}`).pl(33).pr(8).pb(4).mt(-4).text_size(11.5).truncate().text_color(t.dim).child(task));
}

// The empty list's message for this sort, or null for upstream's.
export const empty = (id, any) => (id === "needs" && any ? "No agents need you." : null);
