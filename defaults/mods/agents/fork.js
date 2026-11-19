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
import { run } from "herdr";

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
// control, bidi and invisible characters dropped, and cut to `max` characters.
// ZWJ and ZWNJ stay: they join emoji (👨‍👩‍👧) and spell words (Persian); so do
// variation selectors (❤️). Tag characters go: they hide text, at the cost of
// subdivision flags. outside-agents drops the same set.
const DROP = /[\u0000-\u001f\u007f-\u009f\u00ad\u034f\u061c\u115f\u1160\u180e\u200b\u200e\u200f\u2028-\u202e\u2060-\u2064\u2066-\u206f\u3164\ufeff\uffa0\ufff9-\ufffb\u{e0000}-\u{e007f}]+/gu;
// Code points that belong to the one before them: a cut never ends just before one.
const JOINED = /^[\p{M}\u200d\ufe0e\ufe0f\u{1f3fb}-\u{1f3ff}\u{e0020}-\u{e007f}]$/u;
const segmenter = typeof Intl !== "undefined" && typeof Intl.Segmenter === "function" ? new Intl.Segmenter() : null;
const text = (v, max = 80) => {
  if (typeof v !== "string") return null;
  // Only look at what can show: a huge label would cost every redraw.
  const room = max * 8 + 16;
  const long = v.length > room;
  const s = (long ? v.slice(0, room) : v).replace(DROP, " ").trim();
  if (!s) return null;
  // Cut by whole characters (grapheme clusters where Intl has them), so a flag,
  // an accent or a skin tone at the cut stays whole.
  const parts = segmenter ? Array.from(segmenter.segment(s), (g) => g.segment) : clusters(s);
  if (parts.length <= max && !long) return s;
  return `${parts.slice(0, max - 1).join("").trimEnd()}\u2026`;
};

// Without Intl.Segmenter (QuickJS): code points, with joiners, marks, variation
// selectors and skin tones kept on the character before them, and regional
// indicators (flags) in pairs.
function clusters(s) {
  const out = [];
  let ri = false;
  for (const c of s) {
    const isRi = c >= "\u{1f1e6}" && c <= "\u{1f1ff}";
    const last = out.at(-1);
    if (last !== undefined && (JOINED.test(c) || last.endsWith("\u200d") || (isRi && ri))) {
      out[out.length - 1] += c;
      ri = false;
    } else {
      out.push(c);
      ri = isRi;
    }
  }
  return out;
}

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
// The key is the whole cleaned label, so two long labels that share their
// first 80 characters stay two groups; only the shown name is cut.
function project(a, terminal) {
  const p = label(terminal, "project");
  return p ? { key: `p:${label(terminal, "project", 1024)}`, label: p } : { key: `s:${a.space}`, label: a.space_name || "" };
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

// The row with its task line under it, or the row alone. The two hover and
// click as one: a click on the task line focuses the agent too. The line starts
// under the name: past the row's icon when it has one (16 px + a 9 px gap).
export function withTask(row, a, all, t, collapsed) {
  const task = collapsed ? null : taskOf(all, a.address);
  if (!task) return row;
  // The row paints and clicks for itself; the task line does the same for its
  // own strip, so nothing is painted (or focused) twice.
  return div()
    .id(`with-task-${a.address}`)
    .flex_none()
    .child(row)
    .child(div().id(`task-${a.address}`).rounded(8).when(a.focused, (el) => el.bg(t.selected)).hover((s) => s.bg(t.hover)).cursor_pointer()
      .on_click(() => run("pane.focus", { pane: a.pane }))
      .pl(a.program?.icon ? 33 : 8).pr(8).pb(4).mt(-4).text_size(11.5).truncate().text_color(t.dim).child(task));
}

// The row's detail: a tab the person named, when the agent is alone in it;
// else the terminal's own name; else upstream's detail.
export function detailOf(all, address, personNamedTab, upstream) {
  return (personNamedTab ? upstream : null) ?? nameOf(all, address) ?? upstream;
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
// Herdr's terminals, after the rest; nothing on the narrow rail. `status(a)`
// draws upstream's status mark for the agent's status and reason. There is no pane to go to, so a row does nothing.
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
      .child(status(a)))
    .when(Boolean(a.task), (el) => el.child(div().pl(8).pr(8).pb(4).mt(-4).text_size(11.5).truncate().text_color(t.dim).child(a.task)));
  return [header, ...shown.map(row)];
}
