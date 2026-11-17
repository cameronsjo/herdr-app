//! FORK(browser): a browser block is a mod block named [`BROWSER_MOD`], in
//! memory as on disk: `{ "mod": "browser-prototype", "props": { "browser":
//! "b1", "url": "…", "profile"?: "agent" } }`. Upstream's `Content` enum and
//! its serde shape stay exactly upstream's (cameronsjo/herdr-app#18), and a
//! herdr-app without the browser shows that one pane as a mod that didn't
//! load and keeps it.
//!
//! The fork reads a browser block through [`block`] (or [`BrowserNode`]).
//! Every place upstream treats a mod block as a mod must skip browser blocks:
//! the window's mod slots (`AppWindow::visible`, `owned_mods`), its render
//! arm, the mods' `panes` list, and `missing_mods`. Grep `Content::Mod` in
//! upstream files after a sync for new ones.
//!
//! `browser` is the app's id for the page (`b1`); `url` is where it was last,
//! so a block whose browser isn't open (a restart, another screen) opens
//! there again. `profile: "agent"`: a separate profile for test accounts;
//! only such a browser can be driven by an agent. A browser in a floating
//! block never opens: its native view can't float over GPUI's drawing, and
//! browsers open only from blocks in the tree.

use crate::layout::{Content, Node};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The mod name a browser block is saved under. A mod block by this name
/// whose props name a browser is a browser, not a mod. Not `browser`:
/// Herdr's own browser mod will likely take that name, and neither should
/// read the other's blocks.
pub const BROWSER_MOD: &str = "browser-prototype";

/// A browser block's fields: its mod block's props.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BrowserBlock {
    pub browser: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

/// The browser a block's content shows, if it is a browser block: a
/// [`BROWSER_MOD`] mod block whose props name a browser.
pub fn block(content: &Content) -> Option<BrowserBlock> {
    let Content::Mod { name, props } = content else { return None };
    of_mod(name, props)
}

/// [`block`] for a mod block's name and props.
pub fn of_mod(name: &str, props: &Value) -> Option<BrowserBlock> {
    if name != BROWSER_MOD {
        return None;
    }
    serde_json::from_value::<BrowserBlock>(props.clone()).ok().filter(|b| !b.browser.is_empty())
}

/// Whether a block's content is a browser block (not a mod to load).
pub fn is_browser(content: &Content) -> bool {
    block(content).is_some()
}

/// A browser block's content.
pub fn content(browser: &str, url: &str, profile: Option<&str>) -> Content {
    let b = BrowserBlock { browser: browser.to_owned(), url: url.to_owned(), profile: profile.map(str::to_owned) };
    // A struct of strings always converts.
    Content::Mod { name: BROWSER_MOD.to_owned(), props: serde_json::to_value(b).unwrap_or_default() }
}

/// Points a browser block at a new url (what it reopens at). False if
/// `content` is no browser block.
pub fn set_url(content: &mut Content, url: &str) -> bool {
    if !is_browser(content) {
        return false;
    }
    let Content::Mod { props, .. } = content else { return false };
    props["url"] = Value::String(url.to_owned());
    true
}

/// Reading a node's browser.
pub trait BrowserNode {
    /// The browser a block shows (its id), if it is a browser block.
    fn browser(&self) -> Option<&str>;
}

impl BrowserNode for Node {
    fn browser(&self) -> Option<&str> {
        let content = self.content()?;
        if !is_browser(content) {
            return None;
        }
        let Content::Mod { props, .. } = content else { return None };
        props.get("browser").and_then(Value::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_blocks_round_trip_as_mod_panes() {
        let json = r#"{ "id": "p1", "type": "block", "content": { "mod": "browser-prototype", "props": { "browser": "b1", "url": "https://example.com/" } } }"#;
        let n: Node = serde_json::from_str(json).unwrap();
        let b = block(n.content().unwrap()).unwrap();
        assert_eq!(b, BrowserBlock { browser: "b1".into(), url: "https://example.com/".into(), profile: None });
        assert!(n.is_pane());
        assert_eq!(n.browser(), Some("b1"));
        let back: Node = serde_json::from_str(&serde_json::to_string(&n).unwrap()).unwrap();
        assert_eq!(back, n);
        // A block saved without a url still reads (it opens on a blank page).
        let bare: Node = serde_json::from_str(r#"{ "id": "p2", "type": "block", "content": { "mod": "browser-prototype", "props": { "browser": "b2" } } }"#).unwrap();
        assert_eq!(block(bare.content().unwrap()).map(|b| b.url), Some(String::new()));
        // The other kinds are no browsers.
        let t: Node = serde_json::from_str(r#"{ "id": "t", "type": "block", "content": { "terminal": "x" } }"#).unwrap();
        assert_eq!(t.terminal(), Some("x"));
        assert_eq!(t.browser(), None);
    }

    #[test]
    fn browser_blocks_are_saved_under_a_name_herdr_s_own_browser_mod_wont_use() {
        let n = Node::block("p1", content("b1", "https://example.com/", Some("agent")));
        let saved = serde_json::to_value(&n).unwrap();
        // The shape a herdr-app without the browser reads as one mod pane.
        assert_eq!(
            saved["content"],
            serde_json::json!({ "mod": "browser-prototype", "props": { "browser": "b1", "url": "https://example.com/", "profile": "agent" } })
        );
        assert_eq!(serde_json::from_value::<Node>(saved).unwrap(), n);
        // A `browser` mod block, however its props look, is a mod: a
        // future official browser mod's blocks are never taken over.
        for props in [serde_json::json!(null), serde_json::json!({ "browser": "b1", "url": "https://example.com/" })] {
            let c: Content = serde_json::from_value(serde_json::json!({ "mod": "browser", "props": props })).unwrap();
            assert!(!is_browser(&c), "{props}");
        }
        // A prototype block that names no browser is a mod by that name.
        let c: Content = serde_json::from_value(serde_json::json!({ "mod": "browser-prototype", "props": { "browser": "" } })).unwrap();
        assert!(!is_browser(&c));
    }

    #[test]
    fn set_url_moves_only_a_browser_block() {
        let mut c = content("b1", "https://a.test/", None);
        assert!(set_url(&mut c, "https://b.test/"));
        assert_eq!(block(&c).map(|b| b.url), Some("https://b.test/".into()));
        let mut m = Content::Mod { name: "notes".into(), props: serde_json::json!({ "url": "x" }) };
        assert!(!set_url(&mut m, "https://b.test/"));
        assert_eq!(m, Content::Mod { name: "notes".into(), props: serde_json::json!({ "url": "x" }) });
    }

    #[test]
    fn a_browser_in_a_floating_block_never_fails_the_layout() {
        let json = r#"{ "id": "root", "type": "block", "content": {"agent": "focused"},
          "floating": [ { "id": "f", "content": { "mod": "browser-prototype", "props": { "browser": "b1" } } },
                        { "id": "g", "content": { "mod": "team" } } ] }"#;
        let n: Node = serde_json::from_str(json).unwrap();
        assert_eq!(n.floating.len(), 2);
        // Browsers open only from blocks in the tree, which never includes floats.
        assert!(n.walk().iter().all(|x| x.browser().is_none()));
    }
}
