// A browser pane's header (the pane mod's, for a pane with PaneInfo.browser):
// back, forward, reload, the address, DevTools. Only commands any mod may run.
// The address is what browsers() gives (no query or fragment unless agent
// access is on). Enter goes there and hands the keyboard back to the page;
// Esc puts the page's address back.

import { div, svg } from "gpui-kit";
import { h_flex, InputState, Input } from "gpui-base";
import { browsers, run } from "herdr";

/** What the person typed, as a url: with a scheme as is; a host gets https://
 *  (http:// for localhost and 127.0.0.1); anything else is a search. */
export function address(text) {
  const s = text.trim();
  if (!s) return null;
  const search = () => "https://duckduckgo.com/?q=" + encodeURIComponent(s);
  if (/\s/.test(s)) return search();
  if (/^[a-z][a-z0-9+.-]*:(?!\d)/i.test(s)) return s;
  const host = s.split(/[/?#]/)[0].replace(/:\d+$/, "").toLowerCase();
  if (host === "localhost" || host === "127.0.0.1") return "http://" + s;
  if (host.includes(".")) return "https://" + s;
  return search();
}

export default class BrowserBar {
  // Created from the pane mod's init: state can't be made during render.
  constructor() {
    this.input = InputState.new({ placeholder: "Search or enter an address" });
    this.editing = false;
    this.shown = null; // the url the field holds while nobody edits it
    this.input.on("focus", (_e, cx) => {
      this.editing = true;
      this.input.set_selected_range({ start: 0, end: this.input.value().length });
      cx.notify();
    });
    this.input.on("blur", (_e, cx) => { this.editing = false; this.shown = null; cx.notify(); });
    this.input.on("submit", () => this.go());
  }

  go() {
    const url = address(this.input.value());
    if (url) run("browser.navigate", { browser: this.browser, url });
    this.leave();
  }

  // The keyboard goes back to the page; the field shows the page's address again.
  leave() {
    this.shown = null;
    run("pane.focus", { pane: this.pane });
  }

  key(e, cx) {
    if (e.key !== "escape" || !this.editing) return;
    cx.stop_propagation();
    this.input.set_value(this.url);
    this.leave();
  }

  // The field follows the page while nobody edits it (set after render: state can't change during it).
  follow(url, cx) {
    if (this.editing || this.shown === url) return;
    this.shown = url;
    cx.timer.after(0, () => { if (!this.editing && this.shown === url) this.input.set_value(url); });
  }

  button(t, id, icon, command, on) {
    const b = div().id(id).p(3).rounded(4).flex_none();
    if (!on) return b.text_color(t.dim).opacity(0.4).child(svg(`mods/pane/${icon}.svg`).size(14));
    return b
      .text_color(t.dim)
      .hover((s) => s.bg(t.hover).text_color(t.foreground))
      .on_click((_e, cx) => {
        cx.stop_propagation();
        run(command, { browser: this.browser });
      })
      .child(svg(`mods/pane/${icon}.svg`).size(14));
  }

  render(t, p, cx) {
    const b = browsers().find((x) => x.browser === p.browser);
    this.browser = p.browser;
    this.pane = p.pane;
    this.url = b?.url ?? p.url ?? "";
    this.follow(this.url, cx);
    const pane = p.pane;
    return h_flex()
      .flex_1()
      .min_w(0)
      .gap(2)
      .items_center()
      .child(this.button(t, `pb-back-${pane}`, "back", "browser.back", b?.can_go_back))
      .child(this.button(t, `pb-fwd-${pane}`, "forward", "browser.forward", b?.can_go_forward))
      .child(this.button(t, `pb-reload-${pane}`, "reload", "browser.reload", Boolean(b)))
      .child(div()
        .id(`pb-url-${pane}`)
        .flex_1()
        .min_w(0)
        .h(22)
        .mx(4)
        .px(8)
        .flex()
        .items_center()
        .rounded(6)
        .bg(t.raised)
        .border_1()
        .border_color(this.editing ? t.accent : t.border)
        .cursor_text()
        // The header's click focuses the pane, which would take the keyboard from the field.
        .on_mouse_down("left", (_e, cx) => cx.stop_propagation())
        .on_key_down((e, cx) => this.key(e, cx))
        .child(Input.new(this.input).w_full().h_full().text_size(12).text_color(t.foreground)))
      .when(Boolean(b?.loading), (el) => el.child(svg("mods/workspace-chrome/spinner.svg").size(12).flex_none().text_color(t.dim).spin(900)))
      .child(this.button(t, `pb-dev-${pane}`, "devtools", "browser.devtools", Boolean(b)));
  }
}
