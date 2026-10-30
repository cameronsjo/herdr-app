//! Browser panes on macOS with the `browser` feature: the registry decides
//! what is open, runtime.rs drives CEF, view.rs places the native views.

use gpui::{AnyElement, App, Hsla, IntoElement, ParentElement, Styled, Window, canvas, div};
use serde_json::{Value, json};

use super::registry::{self, Page, Registry};
use super::{Update, runtime, view};
use crate::layout::Node;
use crate::model::Model;

/// What CEF reports about one browser (main thread, from the pump).
pub enum Event {
    Page { browser: String, page: Page },
    /// CEF says the page got focus: a click into it, or CEF on its own.
    Focused(String),
    CloseAsked(String),
    /// A mouse press on Herdr (outside every page) while a page had the keyboard.
    ClickedHerdr,
}

pub struct Browsers {
    registry: Registry,
    events: Option<async_channel::Receiver<Event>>,
    /// For the log: who the window last decided has the keyboard, and who
    /// last had it (None: Herdr).
    wanted: Option<Option<String>>,
    holder: Option<Option<String>>,
    /// Why CEF can't run, shown in every browser block.
    error: Option<String>,
}

impl Browsers {
    pub fn new() -> Self {
        let (tx, rx) = async_channel::unbounded();
        runtime::set_events(tx);
        Browsers { registry: Registry::default(), events: Some(rx), wanted: None, holder: None, error: None }
    }

    pub fn events(&mut self) -> Option<async_channel::Receiver<Event>> {
        self.events.take()
    }

    pub fn apply(&mut self, event: Event) -> Update {
        match event {
            Event::Page { browser, page } => {
                let url = page.url.clone();
                let moved = self.registry.page(&browser, page);
                Update { url: moved.then_some(url), browser, ..Update::default() }
            }
            // Only a press in the page makes it the person's choice; CEF also
            // focuses a page it just created, which must not move Herdr's focus
            // (sync_browser_keys then puts the keyboard back where it belongs).
            Event::Focused(browser) if view::clicked(&browser) => {
                tracing::debug!("browser: the person clicked into {browser}");
                self.registry.focused(&browser, crate::model::now_ms());
                Update { browser, focused: true, ..Update::default() }
            }
            Event::Focused(browser) => {
                tracing::debug!("browser: {browser} took focus on its own (not a click)");
                Update { browser, ..Update::default() }
            }
            Event::CloseAsked(browser) => Update { browser, close: true, ..Update::default() },
            Event::ClickedHerdr => Update { clicked_herdr: true, ..Update::default() },
        }
    }

    /// A new browser's id and its normalized url.
    pub fn prepare_open(&self, url: &str, model: &Model, chrome: &Node) -> Result<(String, String), String> {
        let url = registry::normalize_url(url)?;
        let blocks = registry::blocks(model, chrome);
        let taken = blocks.iter().map(|b| b.browser.as_str()).chain(self.registry.list().iter().map(|e| e.browser.as_str()));
        Ok((registry::next_id(taken), url))
    }

    /// Opens browsers for new blocks (and blocks restored from a saved
    /// layout), closes those whose block is gone.
    pub fn reconcile(&mut self, model: &Model, chrome: &Node, window: &mut Window, cx: &mut App) {
        let changes = self.registry.reconcile(&registry::blocks(model, chrome));
        for id in &changes.close {
            runtime::close(id);
            view::remove(id);
        }
        if changes.open.is_empty() {
            return;
        }
        if let Err(e) = runtime::start(cx) {
            tracing::error!("browser: {e:#}");
            self.error = Some(format!("{e:#}"));
            return;
        }
        for (id, url) in changes.open {
            match view::container(&id, window) {
                Ok(parent) => runtime::open(&id, parent, url),
                Err(e) => tracing::error!("browser: no view for {id}: {e:#}"),
            }
        }
        view::install_monitor();
    }

    /// navigate / back / forward / reload / devtools.
    pub fn command(&mut self, name: &str, browser: &str, args: &Value) -> Result<Value, String> {
        if self.registry.get(browser).is_none() {
            return Err(format!("no open browser {browser:?}: see `herdr-app ls`"));
        }
        match name {
            "browser.navigate" => {
                let url = args.get("url").and_then(Value::as_str).unwrap_or_default();
                let url = registry::normalize_url(url)?;
                runtime::navigate(browser, &url)?;
                Ok(json!({ "browser": browser, "url": url }))
            }
            "browser.back" => runtime::history(browser, -1).map(|()| json!({ "browser": browser })),
            "browser.forward" => runtime::history(browser, 1).map(|()| json!({ "browser": browser })),
            "browser.reload" => runtime::reload(browser).map(|()| json!({ "browser": browser })),
            "browser.devtools" => {
                let open = args.get("open").and_then(Value::as_bool);
                runtime::devtools(browser, open).map(|open| json!({ "browser": browser, "devtools": open }))
            }
            other => Err(format!("{other} is not a browser command")),
        }
    }

    pub fn info(&self, browser: &str, focused: bool) -> Option<Value> {
        self.registry.get(browser).map(|e| e.json(focused))
    }

    pub fn list(&self, focused_pane: Option<&str>) -> Vec<Value> {
        self.registry.list().iter().map(|e| e.json(e.pane.is_some() && e.pane.as_deref() == focused_pane)).collect()
    }

    /// Gives the keyboard to `page` (`super::keyboard_page`), or back to GPUI
    /// when it's None. Compared with who really has it (AppKit's first
    /// responder), so a page that took focus on its own loses it again.
    pub fn keys(&mut self, page: Option<&str>, window: &mut Window) {
        if self.wanted.as_ref().map(Option::as_deref) != Some(page) {
            tracing::debug!("browser: the keyboard belongs to {}", page.unwrap_or("Herdr"));
            self.wanted = Some(page.map(str::to_owned));
        }
        let actual = view::keyed_page(window);
        let holder = if actual.as_deref() == page {
            actual
        } else {
            match page {
                // Not yet created: the next sync tries again.
                Some(id) if !view::give_keys(id, window) => actual,
                Some(id) => {
                    runtime::set_focus(id);
                    Some(id.to_owned())
                }
                None => {
                    if let Some(from) = &actual {
                        tracing::debug!("browser: taking the keyboard back from {from}");
                    }
                    view::take_keys(window);
                    None
                }
            }
        };
        if self.holder.as_ref() != Some(&holder) {
            tracing::debug!("browser: {} has the keyboard", holder.as_deref().unwrap_or("Herdr"));
            self.holder = Some(holder);
        }
    }

    pub fn set_bindings(&mut self, bindings: Vec<(String, String)>) {
        view::set_bindings(bindings);
    }

    /// The block: what shows while the page isn't there (opening, an
    /// error, another block owns it), and a canvas that puts the page's
    /// view on the block each frame.
    pub fn element(&self, node: &str, browser: &str, url: &str, dim: Hsla) -> AnyElement {
        let owner = self.registry.owned_by(node).filter(|e| e.browser == browser);
        let note = match (&self.error, owner) {
            (Some(e), _) => format!("The browser can't start: {e}"),
            (None, Some(e)) => format!("{} {}", if e.page.loading { "Opening" } else { "Showing" }, e.page.url),
            (None, None) if self.registry.get(browser).is_some() => format!("Browser {browser} shows in another block."),
            (None, None) => format!("Opening {url}"),
        };
        let mut el = div().size_full().relative().flex().items_center().justify_center().p_4().text_color(dim).child(note);
        if let (None, Some(_)) = (&self.error, owner) {
            let id = browser.to_owned();
            el = el.child(canvas(|_, _, _| {}, move |bounds, _, window, _| view::place(&id, bounds, window)).absolute().size_full());
        }
        el.into_any_element()
    }

    /// Painted after everything else: hides the pages whose block wasn't painted this frame.
    pub fn frame_end(&self) -> Option<AnyElement> {
        Some(canvas(|_, _, _| {}, |_, _, _, _| view::end_frame()).absolute().size_0().into_any_element())
    }
}
