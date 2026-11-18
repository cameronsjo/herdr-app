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

const machineOf = (address) => (address.includes("/") ? address.slice(0, address.indexOf("/")) : "");

// The tab.rename calls to make, and what each named terminal was called and
// where. `all`: terminals(); `tabs`: id -> TabInfo; `last`: address ->
// {name, tab} last decided on.
function plan(all, tabs, last) {
  const named = all.filter((t) => t.state === "live" && typeof t.name === "string" && t.name.trim() && tabOf(t));
  const perTab = new Map();
  for (const t of named) perTab.set(tabOf(t), (perTab.get(tabOf(t)) ?? 0) + 1);
  const renames = [];
  // Keep what we knew of terminals on a machine with nothing listed now (offline,
  // or its core still starting): when they come back, a rename made meanwhile
  // still follows. A machine that lists terminals has forgotten the rest.
  const listed = new Set(all.map((t) => t.address));
  const machines = new Set(all.map((t) => machineOf(t.address)));
  const seen = Object.fromEntries(Object.entries(last).filter(([address]) => !listed.has(address) && !machines.has(machineOf(address))));
  for (const t of named) {
    const id = tabOf(t), tab = tabs.get(id), name = t.name.trim(), prev = last[t.address];
    // Nothing to decide yet (its tab isn't shown yet, or another named terminal
    // shares it): keep what we knew, so the decision is made once it can be.
    if (!tab || perTab.get(id) !== 1) {
      if (prev) seen[t.address] = prev;
      continue;
    }
    seen[t.address] = { name, tab: id };
    // A terminal new to this tab counts as unseen.
    const before = prev?.tab === id ? prev.name : undefined;
    if (before === name) continue;
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
    // Only {name, tab} entries: an older build kept bare names.
    this.last = Object.fromEntries(Object.entries(this.last).filter(([, v]) => typeof v?.name === "string" && typeof v?.tab === "string"));
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
