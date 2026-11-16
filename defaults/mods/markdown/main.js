// A pane that renders a markdown file and follows its saves.
// Open one with `pane.split {direction: "right", mod: "markdown", props: {file, on?}}`:
// `file` is an absolute path (or `~/…`); `on` is the machine it lives on (null: this computer).
import { View, div } from "gpui-kit";
import { v_flex, h_flex, Editor, EditorState, TextView } from "gpui-base";
import { props, home, readTextFile, watchFolder, theme, settings, checking } from "herdr";

const LIMIT = 400000;

function target() {
  const p = props() ?? {};
  const raw = typeof p.file === "string" ? p.file.trim() : "";
  const on = typeof p.on === "string" && p.on ? p.on : null;
  // `~` is the home of this computer; a remote file must already be absolute.
  const file = raw === "~" || raw.startsWith("~/") ? (on ? null : home() + raw.slice(1)) : raw.startsWith("/") || /^[A-Za-z]:[\\/]/.test(raw) ? raw : null;
  return { file, on, raw };
}

export default class Markdown extends View {
  init(_props, cx) {
    this.source = false; this.text = null; this.error = null; this.code = null;
    const { file, on, raw } = target();
    this.file = file; this.on = on;
    if (!file) { this.error = raw ? `Give an absolute path, not "${raw}"` : "No file to show"; return; }
    this.load(cx);
    if (checking()) return;
    if (on) cx.timer.every(5000, (cx) => this.load(cx));
    else this.follow(cx);
  }

  // A save often replaces the file, so watch its folder and reload on any batch naming it.
  follow(cx) {
    const folder = this.file.replace(/[\\/][^\\/]*$/, "") || "/";
    cx.spawn(async (cx) => {
      try {
        for (;;) {
          const changed = await watchFolder(folder);
          // [] means the watch was released (reclaimed for room): poll instead.
          if (!changed.length) { this.load(cx); cx.timer.every(5000, (cx) => this.load(cx)); return; }
          if (changed.some((p) => p === this.file || p === folder)) this.load(cx);
        }
      } catch { cx.timer.every(5000, (cx) => this.load(cx)); }
    });
  }

  load(cx) {
    const file = this.file, on = this.on;
    cx.spawn(async (cx) => {
      try {
        const text = await readTextFile(file, { on });
        if (text === null) { this.error = "This file is no longer available"; this.text = null; }
        else if (text.includes("\0")) { this.error = "Binary file · not markdown"; this.text = null; }
        else {
          const next = text.length > LIMIT ? text.slice(0, LIMIT) + "\n\n… (file truncated)" : text;
          if (next === this.text) return;
          this.text = next; this.error = null; this.code?.release(); this.code = null;
        }
      } catch (e) { this.error = `Cannot read this file: ${e}`; this.text = null; }
      cx.notify();
    });
  }

  editor() {
    if (this.code) return this.code;
    const t = theme();
    this.code = EditorState.new({ language: "md", readonly: true, value: this.text, font_family: settings().font_family, font_size: 12,
      syntax: { keyword: t.ansi5, string: t.ansi2, comment: t.dim, function: t.ansi4, type: t.ansi3, number: t.ansi1, constant: t.ansi6 } });
    return this.code;
  }

  toggle(cx) { this.source = !this.source; cx.notify(); }

  render(_cx) {
    const t = theme(), name = this.file?.split(/[\\/]/).pop() ?? "Markdown";
    const body = this.text === null
      ? div().p(14).text_color(t.dim).child(this.error || "Loading…")
      : this.source
        ? Editor.new(this.editor()).flex_1().min_h(0)
        : div().id("markdown-body").flex_1().min_h(0).px(18).py(14)
          .child(TextView.markdown("markdown-text", this.text).scrollable(true).selectable(true).size_full());
    return v_flex().id("markdown").size_full().min_h(0).overflow_hidden().bg(t.background).text_color(t.foreground).text_size(13)
      .child(h_flex().h(36).flex_none().px(12).gap(8).items_center().border_b(1).border_color(t.border)
        .child(div().font_semibold().truncate().child(name))
        .child(div().flex_1().min_w(0).truncate().text_size(11).text_color(t.dim).child(this.on ? `${this.on}:${this.file}` : this.file ?? ""))
        .when(this.text !== null, (el) => el.child(div().id("markdown-mode").px(8).py(2).rounded(5).border_1().border_color(t.border).text_size(11)
          .text_color(t.muted).cursor_pointer().hover((s) => s.bg(t.hover)).on_click((_e, cx) => this.toggle(cx)).child(this.source ? "Rendered" : "Source"))))
      .when(this.text !== null && Boolean(this.error), (el) => el.child(div().px(12).py(4).text_size(11).text_color(t.blocked).child(this.error)))
      .child(body);
  }
}
