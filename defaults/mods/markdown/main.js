// A pane that renders a markdown file and follows its saves.
// Open one with `pane.split {direction: "right", mod: "markdown", props: {file, on?}}`:
// `file` is an absolute path (or `~/…`); `on` is the machine it lives on (null: this computer).
import { View, div } from "gpui-kit";
import { v_flex, h_flex, Editor, EditorState, TextView } from "gpui-base";
import { props, home, readTextFile, watchFolder, theme, settings, checking } from "herdr";

const LIMIT = 400000;
// POSIX, drive-rooted (C:\ or C:/) and UNC (\\server\share) paths; not drive-relative C:file.
const ABSOLUTE = /^(\/|[A-Za-z]:[\\/]|\\\\[^\\]+\\[^\\]+)/;

function target() {
  const p = props() ?? {};
  const raw = typeof p.file === "string" ? p.file : ""; // untrimmed: spaces can be part of a file name
  const on = typeof p.on === "string" && p.on ? p.on : null;
  // `~` is the home of this computer; a remote file must already be absolute.
  const file = raw === "~" || raw.startsWith("~/") ? (on ? null : home() + raw.slice(1)) : ABSOLUTE.test(raw) ? raw : null;
  return { file, on, raw };
}

export default class Markdown extends View {
  init(_props, cx) {
    this.source = false; this.text = null; this.error = null; this.code = null; this.reads = 0;
    const { file, on, raw } = target();
    this.file = file; this.on = on;
    if (!file) { this.error = raw ? `Give an absolute path, not "${raw}"` : "No file to show"; return; }
    this.load(cx);
    if (checking()) return;
    // The poll also catches a save made before the folder watch started.
    cx.timer.every(on ? 5000 : 10000, (cx) => this.load(cx));
    if (!on) this.follow(cx);
  }

  // A save often replaces the file, so watch its folder. Watch batches carry canonical
  // paths that may not match this.file's spelling, so reload on any batch; load skips
  // a re-render when the text is unchanged.
  follow(cx) {
    const folder = this.file.replace(/[\\/][^\\/]*$/, "") || "/";
    cx.spawn(async (cx) => {
      try {
        for (;;) {
          const changed = await watchFolder(folder);
          // [] means the watch was released (reclaimed for room): the poll carries on.
          if (!changed.length) return;
          this.load(cx);
        }
      } catch { /* the poll carries on */ }
    });
  }

  load(cx) {
    const file = this.file, on = this.on, read = ++this.reads;
    cx.spawn(async (cx) => {
      let text, error = null;
      try { text = await readTextFile(file, { on }); }
      catch (e) { error = `Cannot read this file: ${e}`; }
      // A later read has already started: its result wins.
      if (read !== this.reads) return;
      if (!error && text === null) error = "This file is no longer available";
      else if (!error && text.includes("\0")) error = "Binary file · not markdown";
      if (error) {
        if (this.error === error && this.text === null) return;
        this.error = error; this.text = null; this.code?.release(); this.code = null;
      } else {
        const next = text.length > LIMIT ? text.slice(0, LIMIT) + "\n\n… (file truncated)" : text;
        if (next === this.text && !this.error) return;
        this.text = next; this.error = null; this.code?.release(); this.code = null;
        if (this.source) this.code = this.editor();
      }
      cx.notify();
    });
  }

  // Retained state: call from init, an event handler or a task, never from render.
  editor() {
    const t = theme();
    return EditorState.new({ language: "md", readonly: true, value: this.text, font_family: settings().font_family, font_size: 12,
      syntax: { keyword: t.ansi5, string: t.ansi2, comment: t.dim, function: t.ansi4, type: t.ansi3, number: t.ansi1, constant: t.ansi6 } });
  }

  toggle(cx) {
    this.source = !this.source;
    if (this.source && !this.code && this.text !== null) this.code = this.editor();
    cx.notify();
  }

  render(_cx) {
    const t = theme(), name = this.file?.split(/[\\/]/).pop() ?? "Markdown";
    const body = this.text === null
      ? div().p(14).text_color(t.dim).child(this.error || "Loading…")
      : this.source && this.code
        ? Editor.new(this.code).flex_1().min_h(0)
        : div().id("markdown-body").flex_1().min_h(0).px(18).py(14)
          .child(TextView.markdown("markdown-text", this.text).scrollable(true).selectable(true).size_full());
    return v_flex().id("markdown").size_full().min_h(0).overflow_hidden().bg(t.background).text_color(t.foreground).text_size(13)
      .child(h_flex().h(36).flex_none().px(12).gap(8).items_center().border_b(1).border_color(t.border)
        .child(div().font_semibold().truncate().child(name))
        .child(div().flex_1().min_w(0).truncate().text_size(11).text_color(t.dim).child(this.on ? `${this.on}:${this.file}` : this.file ?? ""))
        .when(this.text !== null, (el) => el.child(div().id("markdown-mode").px(8).py(2).rounded(5).border_1().border_color(t.border).text_size(11)
          .text_color(t.muted).cursor_pointer().hover((s) => s.bg(t.hover)).on_click((_e, cx) => this.toggle(cx)).child(this.source ? "Rendered" : "Source"))))
      .child(body);
  }
}
