// FORK(name-sync): one name follows one agent onto its tab (the TUI fork's
// cameronsjo/herdr#97, cameronsjo/herdr-app#10).
//
// A terminal started with a name (`hc run --name fix-login …`) names its tab,
// when it is the only named terminal there and nobody named the tab. When that
// terminal's name changes later, a tab still called by the old name follows it.
// A tab someone renamed to anything else is left alone. Herdr's own pane header
// already shows the agent's title, so only tabs need this. Runs in the
// background, with no block of its own (src/window/fork_mods.rs), and looks
// every two seconds: background copies aren't redrawn when terminals change.

import { View, div } from "gpui-kit";
import { terminals, spaces, run, checking } from "herdr";

const EVERY = 2000;

// The tab a terminal is in, as spaces() names it: its herdr.tab label, with the
// machine prefix (`vps/`) its address carries.
function tabOf(t) {
  const label = t.labels?.["herdr.tab"];
  if (!label) return null;
  const slash = t.address.indexOf("/");
  return slash === -1 ? label : `${t.address.slice(0, slash)}/${label}`;
}

// The tab.rename calls to make, and what each named terminal was called now.
// `all`: terminals(); `tabs`: id -> TabInfo; `last`: address -> name last seen.
function plan(all, tabs, last) {
  const named = all.filter((t) => t.state === "live" && typeof t.name === "string" && t.name.trim() && tabOf(t));
  const perTab = new Map();
  for (const t of named) perTab.set(tabOf(t), (perTab.get(tabOf(t)) ?? 0) + 1);
  const renames = [];
  // Keep what we knew of terminals not listed now (an offline machine, a core
  // still starting): when they come back, a rename made meanwhile still follows.
  const listed = new Set(all.map((t) => t.address));
  const seen = Object.fromEntries(Object.entries(last).filter(([address]) => !listed.has(address)));
  for (const t of named) {
    const id = tabOf(t), tab = tabs.get(id), name = t.name.trim();
    seen[t.address] = name;
    const before = last[t.address];
    if (!tab || before === name || perTab.get(id) !== 1) continue;
    // New to us: only a tab nobody named. Renamed: a tab still called the old name too.
    const follows = !tab.named || (before !== undefined && tab.name === before);
    if (follows && tab.name !== name) renames.push({ id, name });
  }
  return { renames, seen };
}

export default class NameSync extends View {
  init(_props, cx) {
    if (checking()) return;
    try { this.last = JSON.parse(localStorage.getItem("names") ?? "{}"); } catch { this.last = {}; }
    if (!this.last || typeof this.last !== "object" || Array.isArray(this.last)) this.last = {};
    cx.timer.every(EVERY, () => this.tick());
    this.tick();
  }

  tick() {
    const tabs = new Map(spaces().flatMap((s) => s.tabs ?? []).map((c) => [c.id, c]));
    const { renames, seen } = plan(terminals(), tabs, this.last);
    for (const r of renames) run("tab.rename", r);
    if (JSON.stringify(seen) === JSON.stringify(this.last)) return;
    this.last = seen;
    localStorage.setItem("names", JSON.stringify(seen));
  }

  render() {
    return div();
  }
}
