// FORK(browser): a browser block's page (cameronsjo/herdr-app#4). Herdr saves a
// browser pane as this mod's block, { "mod": "browser-prototype", "props":
// { "browser": "b1", "url": "…", "profile"?: "agent" } }, and the window runs
// it like any pane mod. It draws what shows while the page isn't there (its
// title and address while it opens, or while a menu or dialog covers it) and
// says where the page goes with nativeView. The page is a native view on top:
// Herdr shows it only while this block shows and nothing covers it, and keeps
// the keyboard, agent access and the agent header (above this mod) to itself.
// The bar above the page is the pane mod's (pane/browser.js).

import { View, div } from "gpui-kit";
import { nativeView } from "herdr-native";
import { browsers, props, theme } from "herdr";

/** A saved address without its query, fragment or sign-in (codes and magic
 *  links stay in the layout): what a page that isn't open shows. */
function shownUrl(url) {
  if (typeof url !== "string") return "";
  const bare = url.split(/[?#]/)[0];
  return bare.replace(/^([a-z][a-z0-9+.-]*:\/\/)[^/@]*@/i, "$1");
}

/** What the placeholder says for this block's browser (`info`, from
 *  browsers(), or undefined while it isn't open yet): { title, note }. */
function placeholder(info, savedUrl) {
  if (!info) return { title: null, note: `Opening ${shownUrl(savedUrl)}` };
  if (info.loading) return { title: null, note: `Opening ${info.url}` };
  return { title: info.title || null, note: info.url };
}

export default class BrowserPage extends View {
  render() {
    const t = theme();
    const { browser, url } = props() ?? {};
    const { title, note } = placeholder(browsers().find((b) => b.browser === browser), url);
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
