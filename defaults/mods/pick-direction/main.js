// FORK(palette): the palette's direction chooser (cameronsjo/herdr-app#12).
// Opened as a dialog with props { command, title, dirs }: an arrow key, or a
// click on its tile, runs `command` with that direction. Esc closes.

import { View, div } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { run, props, closeOverlay, theme, keys } from "herdr";

const ARROWS = { left: "←", up: "↑", down: "↓", right: "→" };
const ORDER = ["left", "up", "down", "right"];
const COMMANDS = new Set(["pane.split", "pane.go", "pane.swap"]);

export default class PickDirection extends View {
  init(_props, cx) {
    this.handle = cx.focus_handle();
    this.handle.focus();
    cx.timer.after(600, () => this.handle.focus());
  }

  dirs() {
    const d = props()?.dirs;
    return ORDER.filter((x) => Array.isArray(d) && d.includes(x));
  }

  pick(direction) {
    const p = props();
    closeOverlay(p.overlay);
    // Only the palette's direction commands, whoever opened this.
    if (COMMANDS.has(p.command) && ORDER.includes(direction)) run(p.command, { direction });
  }

  render() {
    const t = theme();
    const p = props() ?? {};
    const dirs = this.dirs();
    // Each direction's own key, as the palette showed it before.
    const bound = keys();
    const keyOf = (d) => bound.find((k) => k.section === "global" && k.action === `${p.command}:${d}`)?.keys ?? null;
    const tile = (d) =>
      v_flex()
        .id(`dir-${d}`)
        .w(60)
        .h(64)
        .gap(2)
        .items_center()
        .justify_center()
        .rounded(8)
        .border_1()
        .border_color(t.border)
        .bg(t.raised)
        .cursor_pointer()
        .hover((s) => s.bg(t.hover))
        .on_click(() => this.pick(d))
        .child(div().text_size(18).text_color(t.foreground).child(ARROWS[d]))
        .child(div().text_size(10.5).text_color(t.dim).child(d))
        .when(Boolean(keyOf(d)), (el) => el.child(div().text_size(9.5).text_color(t.dim).child(keyOf(d))));
    return v_flex()
      .id("pick-direction")
      .track_focus(this.handle)
      .on_key_down((e, cx) => {
        if (e.key === "escape") closeOverlay(props().overlay);
        else if (dirs.includes(e.key)) this.pick(e.key);
        else return;
        cx.stop_propagation();
      })
      .size_full()
      .min_h(0)
      .p(18)
      .gap(14)
      .rounded(8)
      .bg(t.popover)
      .text_size(13)
      .child(
        h_flex()
          .items_center()
          .child(div().flex_1().text_size(15).font_semibold().text_color(t.foreground).child(p.title ?? "Pick a direction"))
          .child(div().text_size(11).text_color(t.dim).child("Esc")),
      )
      .child(h_flex().gap(8).justify_center().children(dirs.map(tile)))
      .child(div().text_size(11.5).text_color(t.dim).child("Press an arrow key or click a direction."));
  }
}
