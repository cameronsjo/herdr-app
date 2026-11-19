// FORK(browser): a browser block's page (cameronsjo/herdr-app#4). Herdr saves a
// browser pane as this mod's block, { "mod": "browser-prototype", "props":
// { "browser": "b1", "url": "…", "profile"?: "agent" } }, and the window runs
// it like any pane mod, with props { browser, profile? } (never the url). It draws what shows while the page isn't there (its
// title and address while it opens, or while a menu or dialog covers it) and
// says where the page goes with nativeView. The page is a native view on top:
// Herdr shows it only while this block shows and nothing covers it, and keeps
// the keyboard, agent access and the agent header (above this mod) to itself.
// The bar above the page is the pane mod's (pane/browser.js).

import { View, div } from "gpui-kit";
import { nativeView } from "herdr-native";
import { browsers, panes, props, theme } from "herdr";

/** A saved address without its query, fragment or sign-in (codes and magic
 *  links stay in the layout): what a page that isn't open shows. */
function shownUrl(url) {
  if (typeof url !== "string") return "";
  const bare = url.split(/[?#]/)[0];
  const m = /^([a-z][a-z0-9+.-]*:\/\/)([^/]*)(.*)$/i.exec(bare);
  if (!m) return bare;
  // The sign-in ends at the authority's last @ (a password may hold one).
  const at = m[2].lastIndexOf("@");
  return m[1] + (at < 0 ? m[2] : m[2].slice(at + 1)) + m[3];
}

/** What the placeholder says for this block's browser (`info`, from
 *  browsers(), or undefined while it isn't open yet): { title, note }. */
function placeholder(info, savedUrl, floating) {
  if (floating) return { title: null, note: "A browser doesn't open in a floating block: put it in a pane or a layout block." };
  if (!info) {
    // panes() lists tab panes only: a layout block's page has no saved address here.
    const shown = shownUrl(savedUrl);
    return { title: null, note: shown ? `Opening ${shown}` : "Opening…" };
  }
  if (info.loading) return { title: null, note: `Opening ${info.url}` };
  return { title: info.title || null, note: info.url };
}

export default class BrowserPage extends View {
  render() {
    const t = theme();
    // The props name the browser only: the page's url isn't in them (it
    // follows every navigation). A page not open yet shows its pane's saved
    // address, which panes() gives without its query.
    const { browser, float } = props() ?? {};
    const saved = panes().find((p) => p.browser === browser)?.url;
    const { title, note } = placeholder(browsers().find((b) => b.browser === browser), saved, Boolean(float));
    // A float never holds the page (its report would move it). It is sized
    // to its content, where size_full() collapses to nothing
    // (cameronsjo/herdr-app#43), so its note gets a width of its own.
    if (float) return div().w(320).p(16).bg(t.surface).text_color(t.dim).child(note);
    const card = div()
      .size_full()
      .flex()
      .flex_col()
      .items_center()
      .justify_center()
      .gap(4)
      .p(16)
      .overflow_hidden()
      .bg(t.surface)
      .when(Boolean(title), (d) => d.child(div().text_color(t.foreground).child(title)))
      .child(div().text_color(t.dim).child(note));
    return div()
      .size_full()
      .relative()
      .child(card)
      .when(Boolean(browser), (d) => d.child(nativeView("browser", browser).absolute().inset_0()));
  }
}
