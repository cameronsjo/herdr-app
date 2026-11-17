// Fork-only (cameronsjo/herdr-app): the settings font picker's rows.
//
// Upstream lists the first 200 fonts and drops the rest, so on a Mac with
// more families than that, fonts late in the alphabet never show until you
// search for them. Each row is drawn in its own font, which is why upstream
// capped it: a plain scroll area shapes every row at once. A uniform list
// draws only the rows on screen, so every installed font can be listed.
import { div, uniform_list } from "gpui-kit";
import { Scrollbar } from "gpui-base";

// The "Default" row's key: no font family is named with a NUL.
const DEFAULT_KEY = "\u0000default";

// `rows` holds family names, with `null` standing for the "Default" row.
export function fontRows(t, which, def, rows, choose) {
  const id = `st-fl-${which}`;
  const key = (i) => (rows[i] === null ? DEFAULT_KEY : rows[i]);
  const row = (name) =>
    div()
      .px(10)
      .py(5)
      .rounded(6)
      .cursor_pointer()
      .hover((s) => s.bg(t.hover))
      .child(div().font_family(name ?? def).text_size(13.5).text_color(t.foreground).child(name ?? `Default (${def})`));
  return div()
    .h(200)
    .p(4)
    .relative()
    .child(
      uniform_list(id, rows.length, key, (range) => {
        const out = [];
        for (let i = range.start; i < range.end; i++) out.push(row(rows[i]));
        return out;
      })
        .h_full()
        .on_item_click((k, cx) => choose(k === DEFAULT_KEY ? null : k, cx)),
    )
    .child(Scrollbar.vertical(id).absolute().inset_0());
}
