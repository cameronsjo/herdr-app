//! FORK(browser): how a browser block is saved. `Content::Browser` is
//! written as a mod block named [`BROWSER_MOD`], so a herdr-app without the
//! browser shows that one pane as a mod that didn't load and keeps it.
//!
//! `Content` (de)serializes through [`Saved`], which MUST mirror upstream's
//! serde shape for `Content` exactly: after a sync, compare upstream's
//! `Content` attributes in `src/layout.rs` with `Saved` here. A new upstream
//! variant fails to compile in the `From` impls; a changed attribute does not.
//!
//! `Content::Browser`: a web page (macOS builds with a browser engine; a
//! placeholder elsewhere). `browser` is the app's id for it (`b1`); `url` is
//! where it was last, so a block whose browser isn't open (a restart, another
//! screen) opens there again. `profile: "agent"`: a separate profile for test
//! accounts; only such a browser can be driven by an agent. A browser in a
//! floating block never opens: its native view can't float over GPUI's
//! drawing, and browsers open only from blocks in the tree.

use super::*;

/// The mod name a browser block is saved under: `{ "mod": "browser-prototype",
/// "props": { "browser": "b1", "url": "…", "profile"?: "agent" } }`. A mod
/// block by this name whose props name a browser is a browser, not a mod.
/// Not `browser`: Herdr's own browser mod will likely take that name, and
/// neither should read the other's blocks.
pub const BROWSER_MOD: &str = "browser-prototype";

/// How a block is written in a layout file or a tab document. Every herdr-app
/// that shares a core reads the whole document with this, and a block that
/// matches no variant fails all of it, so new kinds of block are saved as one
/// of these, never as a new variant.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum Saved {
    Terminal {
        terminal: String,
    },
    Mod {
        #[serde(rename = "mod")]
        name: String,
        #[serde(default, skip_serializing_if = "Value::is_null")]
        props: Value,
    },
    Context {
        /// `context`: 0.1.0's word, in layouts made then.
        #[serde(alias = "context")]
        tab: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pane: Option<PaneFrame>,
    },
    Agent {
        agent: String,
    },
}

/// A browser block's fields, as a mod block's props.
#[derive(Serialize, Deserialize)]
struct BrowserProps {
    browser: String,
    #[serde(default)]
    url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    profile: Option<String>,
}

impl From<Saved> for Content {
    fn from(saved: Saved) -> Content {
        match saved {
            Saved::Terminal { terminal } => Content::Terminal { terminal },
            Saved::Mod { name, props } => match browser_props(&name, &props) {
                Some(BrowserProps { browser, url, profile }) => Content::Browser { browser, url, profile },
                None => Content::Mod { name, props },
            },
            Saved::Context { tab, pane } => Content::Context { tab, pane },
            Saved::Agent { agent } => Content::Agent { agent },
        }
    }
}

impl From<Content> for Saved {
    fn from(content: Content) -> Saved {
        match content {
            Content::Terminal { terminal } => Saved::Terminal { terminal },
            Content::Mod { name, props } => Saved::Mod { name, props },
            Content::Context { tab, pane } => Saved::Context { tab, pane },
            Content::Agent { agent } => Saved::Agent { agent },
            Content::Browser { browser, url, profile } => Saved::Mod {
                name: BROWSER_MOD.to_string(),
                // A struct of strings always converts.
                props: serde_json::to_value(BrowserProps { browser, url, profile }).unwrap_or_default(),
            },
        }
    }
}

/// A [`BROWSER_MOD`] block's props, when they name a browser.
fn browser_props(name: &str, props: &Value) -> Option<BrowserProps> {
    if name != BROWSER_MOD {
        return None;
    }
    serde_json::from_value::<BrowserProps>(props.clone()).ok().filter(|p| !p.browser.is_empty())
}

impl Node {
    /// The browser a block shows.
    pub fn browser(&self) -> Option<&str> {
        match self.content() {
            Some(Content::Browser { browser, .. }) => Some(browser),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_blocks_round_trip_as_panes() {
        let json = r#"{ "id": "p1", "type": "block", "content": { "mod": "browser-prototype", "props": { "browser": "b1", "url": "https://example.com/" } } }"#;
        let n: Node = serde_json::from_str(json).unwrap();
        assert_eq!(n.content(), Some(&Content::Browser { browser: "b1".into(), url: "https://example.com/".into(), profile: None }));
        assert!(n.is_pane());
        assert_eq!(n.browser(), Some("b1"));
        let back: Node = serde_json::from_str(&serde_json::to_string(&n).unwrap()).unwrap();
        assert_eq!(back, n);
        // A block saved without a url still parses (it opens on a blank page).
        let bare: Node = serde_json::from_str(r#"{ "id": "p2", "type": "block", "content": { "mod": "browser-prototype", "props": { "browser": "b2" } } }"#).unwrap();
        assert_eq!(bare.content(), Some(&Content::Browser { browser: "b2".into(), url: String::new(), profile: None }));
        // The other kinds still parse as themselves.
        let t: Node = serde_json::from_str(r#"{ "id": "t", "type": "block", "content": { "terminal": "x" } }"#).unwrap();
        assert_eq!(t.terminal(), Some("x"));
    }

    #[test]
    fn browser_blocks_are_saved_under_a_name_herdr_s_own_browser_mod_wont_use() {
        let n = Node::block("p1", Content::Browser { browser: "b1".into(), url: "https://example.com/".into(), profile: Some("agent".into()) });
        let saved = serde_json::to_value(&n).unwrap();
        // The shape a herdr-app without this prototype reads as one mod pane.
        assert_eq!(
            saved["content"],
            serde_json::json!({ "mod": "browser-prototype", "props": { "browser": "b1", "url": "https://example.com/", "profile": "agent" } })
        );
        assert_eq!(serde_json::from_value::<Node>(saved).unwrap(), n);
        // A `browser` mod block, however its props look, is a mod: a
        // future official browser mod's blocks are never taken over.
        for props in [serde_json::json!(null), serde_json::json!({ "browser": "b1", "url": "https://example.com/" })] {
            let c: Content = serde_json::from_value(serde_json::json!({ "mod": "browser", "props": props })).unwrap();
            assert_eq!(c, Content::Mod { name: "browser".into(), props: props.clone() }, "{props}");
        }
        // A prototype block that names no browser is a mod by that name.
        let c: Content = serde_json::from_value(serde_json::json!({ "mod": "browser-prototype", "props": { "browser": "" } })).unwrap();
        assert!(matches!(c, Content::Mod { .. }));
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
