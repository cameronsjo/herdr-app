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
// Any process can set a label or a name, so a value is drawn on one line with
// control and bidi characters dropped, and cut to `max` characters.
const text = (v, max = 80) => {
  if (typeof v !== "string") return null;
  const s = v.replace(/[\u0000-\u001f\u007f-\u009f\u061c\u200b-\u200f\u2028\u2029\u202a-\u202e\u2066-\u2069\ufeff]+/g, " ").trim();
  if (!s) return null;
  // Cut by code point, so an emoji at the cut stays whole.
  const chars = Array.from(s);
  return chars.length > max ? `${chars.slice(0, max - 1).join("").trimEnd()}\u2026` : s;
};

// A label's value on the terminal, cleaned, or null.
const label = (terminal, key, max) => text(terminal?.labels?.[key], max);

// The terminal's own name, or null.
export function nameOf(all, address) {
  return text(byAddress(all).get(address)?.name);
}

export function taskOf(all, address) {
  return label(byAddress(all).get(address), "task", 200);
}

// What a project group is called and keyed by: the `project` label, else the space.
function project(a, terminal) {
  const p = label(terminal, "project");
  return p ? { key: `p:${p}`, label: p } : { key: `s:${a.space}`, label: a.space_name || "" };
}

// A row names its space when its header is a project, which can span spaces.
export function namesSpace(id, all, address) {
  return id === "project" && label(byAddress(all).get(address), "project") !== null;
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

// FORK(outside-agents): the agents outside Herdr's terminals this sort shows
// (`list`: shared "fork.outside-agents", from defaults/mods/outside-agents).
function outsiders(id, list) {
  const all = Array.isArray(list) ? list : [];
  return id === "needs" ? all.filter((a) => a.status === "blocked") : all;
}

// The empty list's message for this sort, or null for upstream's. `any`: an agent
// in a Herdr terminal reports; nothing to say when agents outside Herdr show.
export function empty(id, any, outside) {
  if (outsiders(id, outside).length) return "";
  return id === "needs" && (any || (Array.isArray(outside) && outside.length)) ? "No agents need you." : null;
}

// FORK(outside-agents): an "Elsewhere" header and a row for each agent outside
// Herdr's terminals, after the rest; nothing on the narrow rail. `status(s)`
// draws upstream's status mark. There is no pane to go to, so a row does nothing.
export function outside(t, id, collapsed, list, status) {
  const shown = collapsed ? [] : outsiders(id, list);
  if (!shown.length) return [];
  const header = div()
    .id("outside-agents")
    .flex()
    .px(8)
    .pt(8)
    .h(30)
    .flex_none()
    .gap(6)
    .items_center()
    .text_size(11)
    .font_medium()
    .child(div().text_color(t.dim).child("Elsewhere"))
    .child(div().text_color(t.dim).child(String(shown.length)));
  const row = (a) => div()
    .id(`outside-row-${a.id}`)
    .flex_none()
    .child(div()
      .flex()
      .px(8)
      .h(30)
      .rounded(8)
      .items_center()
      .gap(9)
      .child(div().flex().flex_1().min_w(0).gap(5).items_center()
        .child(div().min_w(0).max_w(a.title ? 120 : 200).truncate().text_color(a.status === "idle" ? t.muted : t.foreground).child(a.name))
        .when(Boolean(a.title), (el) => el.child(div().flex_1().min_w(0).truncate().text_color(t.dim).child(`\u00b7 ${a.title}`))))
      .child(status(a.status)))
    .when(Boolean(a.task), (el) => el.child(div().pl(8).pr(8).pb(4).mt(-4).text_size(11.5).truncate().text_color(t.dim).child(a.task)));
  return [header, ...shown.map(row)];
}
