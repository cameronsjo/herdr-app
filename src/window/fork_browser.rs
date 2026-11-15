//! FORK(browser): the browser pane's part of the window. Everything the fork
//! adds to `AppWindow` lives here, so `src/window.rs` carries only one-line
//! `FORK(browser)` call sites and upstream merges touch as little as possible
//! (`CLAUDE.md` § Merge-friendly by construction). A child module sees the
//! window's private fields and methods.

use super::*;

/// The height of an agent browser's header (`AppWindow::agent_bar`).
const AGENT_HEADER: f32 = 26.0;

/// The window's browser state: one field on `AppWindow` (`fork`).
pub(super) struct ForkBrowser {
    /// Browser blocks' pages (`browser/`): placeholders where browsers aren't built in.
    pub(super) browsers: crate::browser::Browsers,
    /// The layout block's page the person clicked into (a browser id): it
    /// keeps the keyboard while it shows, until focus moves on purpose.
    layout_page: Option<String>,
    /// A Herdr control the person pressed and that took GPUI focus (a layout
    /// mod's input, a floating terminal): while it keeps that focus, it keeps
    /// the keyboard from the focused pane's page.
    control_focus: Option<FocusHandle>,
    /// keys.json's bindings as `(section, keys, action)`, from the last file
    /// that loaded: the key monitor's source while a page has the keyboard.
    #[cfg(all(target_os = "macos", feature = "browser-pane"))]
    page_keys: Vec<(String, String, String)>,
}

impl ForkBrowser {
    pub(super) fn new() -> Self {
        Self {
            browsers: crate::browser::Browsers::new(),
            layout_page: None,
            control_focus: None,
            #[cfg(all(target_os = "macos", feature = "browser-pane"))]
            page_keys: Vec::new(),
        }
    }
}

/// Whether a pane frame's header is drawn over the pane (`reveal: "hover"`)
/// rather than beside it. Never over a browser pane: GPUI can't draw over
/// the page's native view, so it would show only where Herdr draws (an
/// agent browser's header, which it would hide on hover).
pub(super) fn header_over(hover: bool, content: Option<&Content>) -> bool {
    hover && !matches!(content, Some(Content::Browser { .. }))
}

/// Whether a command changes the tab document (`dispatch`'s structural list).
pub(super) fn structural(name: &str) -> bool {
    matches!(name, "browser.open" | "browser.close")
}

/// `needs_model_pane` for browser commands.
pub(super) fn needs_model_pane(name: &str, args: &Value) -> bool {
    name == "browser.open" && AppWindow::arg(args, "pane").is_none() && AppWindow::arg(args, "place") != Some("tab")
}

/// A press that moves GPUI focus onto a control (a layout mod's input, a
/// floating terminal) gives that control the keyboard, AppKit's included,
/// even while the focused pane is a browser page. Capture runs before the
/// control focuses itself; the check waits until after.
pub(super) fn capture_press(_: &mut AppWindow, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<AppWindow>) {
    let before = window.focused(cx);
    cx.defer_in(window, move |this, window, cx| {
        let after = window.focused(cx);
        if let Some(f) = after.filter(|f| Some(f) != before.as_ref() && *f != this.focus) {
            this.fork.control_focus = Some(f);
            this.sync_browser_keys(window);
        }
    });
}

/// A browser block in `herdr-app status`: the open browser's url and title
/// (`info`, which keeps the query only while agent access is on), or, when
/// it isn't open, the tab document's url without query or fragment (codes
/// and magic links stay in the tab document; restore needs them).
fn browser_status(info: Option<Value>, browser: &str, pane: &str, saved_url: &str) -> Value {
    match info {
        Some(i) => json!({ "browser": browser, "pane": pane, "url": i["url"], "title": i["title"] }),
        None => json!({ "browser": browser, "pane": pane, "url": crate::browser::public_url(saved_url) }),
    }
}

impl AppWindow {
    /// Pages' events reach `browser_event` (from `AppWindow::new`).
    pub(super) fn fork_spawn_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(events) = self.fork.browsers.events() {
            self._tasks.push(cx.spawn_in(window, async move |this, cx| {
                while let Ok(event) = events.recv().await {
                    if this.update_in(cx, |this, window, cx| this.browser_event(event, window, cx)).is_err() {
                        break;
                    }
                }
            }));
        }
    }

    /// keys.json loaded: the key monitor's bindings follow.
    pub(super) fn fork_keys_loaded(&mut self) {
        #[cfg(all(target_os = "macos", feature = "browser-pane"))]
        {
            self.fork.page_keys = crate::keys::listing();
        }
        self.browser_keys();
    }

    /// The keys Herdr keeps while a page has the keyboard (`browser::keys::page_bindings`),
    /// from the keys.json GPUI last loaded: a file with a problem keeps the old ones, as GPUI does.
    pub(super) fn browser_keys(&mut self) {
        #[cfg(all(target_os = "macos", feature = "browser-pane"))]
        self.fork.browsers.set_bindings(crate::browser::keys::page_bindings(&self.fork.page_keys, &self.layout_name));
    }

    /// Focus moved on purpose: a layout block's page, or a control the
    /// person pressed, stops holding the keyboard.
    pub(super) fn fork_focus_moved(&mut self) {
        self.fork.layout_page = None;
        self.fork.control_focus = None;
    }

    /// After `sync`: pages follow the model, and the keyboard follows them.
    pub(super) fn fork_after_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fork.browsers.reconcile(&self.model, &self.chrome, window, cx);
        self.sync_browser_keys(window);
    }

    /// Who has the keyboard (`browser::keyboard_page`): a dialog or popup, or
    /// a control the person pressed while it keeps GPUI focus, else a layout
    /// block's page the person clicked, else the focused pane's page while its
    /// tab shows and no layout terminal block has focus, else GPUI. Runs after
    /// every request, press, focus loss, overlay change and page event, so a
    /// change that skips `sync` (a restore, a popup) still moves the keyboard.
    pub(super) fn sync_browser_keys(&mut self, window: &mut Window) {
        let dialog = self.overlays.iter().any(|o| o.kind.modal_keys());
        let control = self.fork.control_focus.as_ref().is_some_and(|f| f.is_focused(window));
        if !control {
            self.fork.control_focus = None;
        }
        if let Some(b) = &self.fork.layout_page
            && !self.chrome.visible_blocks().iter().any(|n| n.browser() == Some(b.as_str()))
        {
            self.fork.layout_page = None;
        }
        let pane_page = self.pane_page();
        let covered = self.fork.browsers.covered();
        let page = crate::browser::keyboard_page(dialog, control, self.fork.layout_page.as_deref(), pane_page.as_deref(), &covered).map(str::to_owned);
        self.fork.browsers.keys(page.as_deref(), window);
    }

    /// The focused pane's page, while its tab shows and no layout terminal
    /// block has the focus.
    fn pane_page(&self) -> Option<String> {
        let block = terminal_block(&self.chrome, self.focused_pane.as_deref()).is_some();
        (!block && self.shows_tab())
            .then(|| self.model.focused_pane())
            .flatten()
            .and_then(|p| self.model.active_tab().and_then(|t| t.shown().find(&p)).and_then(|n| n.browser()).map(str::to_owned))
    }

    /// The browser the person is on: a layout block's page they clicked
    /// into, else the focused pane's page (`pane_page`), as for the keyboard.
    fn focused_browser(&self) -> Option<String> {
        self.fork.layout_page.clone().or_else(|| self.pane_page())
    }

    /// The pane showing a browser.
    fn browser_pane(&self, browser: &str) -> Option<String> {
        self.model.tabs().flat_map(|t| t.root.walk()).find(|n| n.browser() == Some(browser)).map(|n| n.id.clone())
    }

    /// A page changed, took the keyboard or asked to close.
    fn browser_event(&mut self, event: crate::browser::Event, window: &mut Window, cx: &mut Context<Self>) {
        let u = self.fork.browsers.apply(event);
        if u.clicked_herdr {
            // A click on Herdr: a layout block's page lets go (a pane's page
            // keeps it until the click moves the focused pane).
            self.fork.layout_page = None;
            return self.sync_browser_keys(window);
        }
        if u.covers {
            // A page hid under an overlay (Herdr types meanwhile) or came back
            // (it gets the keyboard again only if it still should have it).
            return self.sync_browser_keys(window);
        }
        let pane = self.browser_pane(&u.browser);
        if let Some(url) = &u.url {
            self.save_browser_url(&u.browser, url);
        }
        if u.close && let Some(pane) = &pane {
            let r = self.run_as("pane.close", json!({ "pane": pane }), false, window, cx);
            if let Some(e) = r.get("error") {
                tracing::warn!("browser {}: the page asked to close: {e}", crate::browser::log_safe(&u.browser));
            }
            return;
        }
        if u.focused {
            // The person clicked into the page: a control that had the keyboard
            // lets go (GPUI focus too, so its caret goes); a layout block's page
            // holds the keyboard; a pane's page makes its pane the focused one.
            if let Some(f) = self.fork.control_focus.take()
                && f.is_focused(window)
            {
                self.restore_focus(window, cx);
            }
            self.fork.layout_page = pane.is_none().then(|| u.browser.clone());
            if let Some(pane) = pane.filter(|p| self.model.focused_pane().as_ref() != Some(p) || self.focused_pane.as_ref() != Some(p)) {
                self.model.focus_pane(&pane);
                self.focused_pane = None;
                return self.sync(window, cx);
            }
        }
        self.sync_browser_keys(window);
        self.publish(cx);
        cx.notify();
    }

    /// The header of an agent-profile browser block: a warning that this is
    /// a test profile, whether agent access is on (and which terminal last
    /// attached, as it claimed), and its button. It is a status marker, not
    /// the control: "Allow agent…" only opens the native macOS dialog
    /// (`ask_agent_access`), and access turns on only from that dialog's
    /// Allow. "Stop agent" turns access off at once.
    fn agent_bar(&self, node: &str, browser: &str, cx: &mut Context<Self>) -> AnyElement {
        let (on, last) = self.fork.browsers.agent_state(browser).unwrap_or((false, None));
        // The id, so the person can match the native dialog ("Browser b3, …") to this pane.
        let id = crate::browser::log_safe(browser);
        let state = match (on, last) {
            (false, _) => format!("Agent access off · {id}"),
            (true, None) => format!("Agent access ON · {id}"),
            (true, Some(t)) => format!("Agent access ON · {id} · last: terminal {t} (claimed)"),
        };
        let b = browser.to_owned();
        let (label, sets) = crate::browser::access_button(on);
        div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_2()
            .h(settings::z(AGENT_HEADER))
            .bg(if on { c("blocked") } else { c("raised") })
            .text_size(settings::z(12.0))
            .text_color(c("foreground"))
            .child(div().flex_1().overflow_hidden().whitespace_nowrap().child(crate::browser::dev_switch_note().unwrap_or("Test profile: don't sign into real accounts.")))
            .child(div().whitespace_nowrap().child(state))
            .child(
                div()
                    .id(SharedString::from(format!("agent-access-{node}")))
                    .px_2()
                    .rounded_sm()
                    .bg(c("accent"))
                    .text_color(c("accent_foreground"))
                    .cursor_pointer()
                    .child(label)
                    // It does what the label said, not a toggle of the live state.
                    .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                        if sets {
                            // A double click's second click opens nothing.
                            if ev.click_count() <= 1 {
                                this.ask_agent_access(&b, window, cx);
                            }
                            return;
                        }
                        let _ = this.set_agent_access(&b, false, crate::browser::Source::PaneControl);
                        this.publish(cx);
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    /// "Allow agent…": the native macOS dialog asking whether agents may
    /// drive `browser` (`browser::confirm`). Only its Allow turns access on
    /// (`Browsers::answer`). It runs from a foreground task, outside every
    /// borrow of the window: AppKit's modal loop runs inside the call.
    fn ask_agent_access(&mut self, browser: &str, window: &mut Window, cx: &mut Context<Self>) {
        let ask = match self.fork.browsers.ask(browser) {
            Ok(ask) => ask,
            Err(e) => {
                tracing::info!("browser: no agent access dialog for {}: {e}", crate::browser::log_safe(browser));
                let spec = json!({"kind":"toast", "notification":{"title":"agent access","body":e,"level":"error"}});
                self.open_overlay(mods::new_overlay_id(), spec, window, cx);
                return;
            }
        };
        cx.spawn_in(window, async move |this, cx| {
            let reply = crate::browser::confirm(&ask);
            let _ = this.update_in(cx, |this, window, cx| {
                if let Err(e) = this.fork.browsers.answer(&ask, reply) {
                    let spec = json!({"kind":"toast", "notification":{"title":"agent access","body":e,"level":"error"}});
                    this.open_overlay(mods::new_overlay_id(), spec, window, cx);
                }
                this.publish(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// While any browser has agent access on, a strip along the bottom of the
    /// window names each one, with a Stop button. It sits outside the layout,
    /// so hiding, moving or editing a block, or switching tab or space, can't
    /// hide it. It is informational: a mod, a theme or a page can still
    /// cover or recolor it.
    fn agent_watch(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let on = self.fork.browsers.agents_on();
        if on.is_empty() {
            return None;
        }
        let mut strip = div()
            .id("agent-watch")
            .flex_none()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_2()
            .h(settings::z(24.0))
            .overflow_hidden()
            .bg(c("blocked"))
            .text_size(settings::z(12.0))
            .text_color(c("foreground"));
        if let Some(note) = crate::browser::dev_switch_note() {
            strip = strip.child(div().whitespace_nowrap().child(note));
        }
        for (browser, last) in on {
            let label = match last {
                Some(t) => format!("Agent access ON: {browser} · last: terminal {t} (claimed)"),
                None => format!("Agent access ON: {browser}"),
            };
            let b = browser.clone();
            strip = strip.child(div().whitespace_nowrap().child(label)).child(
                div()
                    .id(SharedString::from(format!("agent-watch-stop-{browser}")))
                    .px_2()
                    .rounded_sm()
                    .bg(c("accent"))
                    .text_color(c("accent_foreground"))
                    .cursor_pointer()
                    .child("Stop agent")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let _ = this.set_agent_access(&b, false, crate::browser::Source::PaneControl);
                        this.publish(cx);
                        cx.notify();
                    })),
            );
        }
        Some(strip.into_any_element())
    }

    /// The window's tree with the agent-access strip, when one shows, taking
    /// the bottom of the window from the layout.
    pub(super) fn fork_with_agent_watch(&self, tree: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        match self.agent_watch(cx) {
            Some(strip) => div().size_full().flex().flex_col().child(div().flex_1().min_h_0().flex().child(tree)).child(strip).into_any_element(),
            None => tree,
        }
    }

    /// Turns agent access for `browser` on or off. On only
    /// from the native dialog's Allow (or the dev switch of a dev-agent-access
    /// test build); off
    /// from anywhere, and off revokes the agent's token and ends its
    /// connection, which reopens the page (nothing the agent set survives).
    fn set_agent_access(&mut self, browser: &str, on: bool, from: crate::browser::Source) -> Result<Value, String> {
        self.fork.browsers.set_agent_access(browser, on, from)?;
        Ok(json!({ "browser": browser, "agent_access": on }))
    }

    /// The socket's `attach` request (agent-browser's provider,
    /// `herdr-app __agent-browser-provider`): a token url for the caller's
    /// browser, if the person allowed it. Only the socket answers this; it is
    /// no command, so no mod or key can.
    pub(super) fn fork_attach_reply(&mut self, attach: &Value, cx: &mut Context<Self>) -> Value {
        match self.attach_agent(attach) {
            // The marker's "last: terminal tN" follows at once.
            Ok(v) => {
                self.publish(cx);
                cx.notify();
                v
            }
            Err(e) => json!({ "error": e }),
        }
    }

    /// `{ attach: { terminal, session, browser? } }`: picks the caller's
    /// browser (`browser::agent::select`) and hands back its token url.
    /// `terminal` is the caller's claim (`HC_TERMINAL_ID`): a routing hint,
    /// not an identity; the gate is the person's switch on the browser.
    fn attach_agent(&mut self, req: &Value) -> Result<Value, String> {
        let terminal = req.get("terminal").and_then(Value::as_str).filter(|t| !t.is_empty()).ok_or_else(crate::browser::not_in_herdr)?;
        if terminal.contains('/') {
            return Err(crate::browser::remote(terminal));
        }
        // Terminal ids are per core: `t3` on another core is another terminal.
        crate::browser::same_core(terminal, req.get("core").and_then(Value::as_str), crate::browser::core_id(&self.core.hello).as_deref())?;
        let pane = self.model.tabs().flat_map(|t| t.root.walk()).find(|n| n.terminal() == Some(terminal)).map(|n| n.id.clone());
        let Some(pane) = pane else {
            return Err(if self.live.contains_key(terminal) { crate::browser::parked(terminal) } else { format!("no terminal {terminal} in Herdr") });
        };
        let space = self.model.space_of(&pane).ok_or_else(|| crate::browser::parked(terminal))?;
        if space.machine.is_some() {
            return Err(crate::browser::remote(terminal));
        }
        let (space, tab) = (space.id.clone(), self.model.tab_of(&pane).map(|t| t.id.clone()).unwrap_or_default());
        let wanted = req.get("browser").and_then(Value::as_str);
        let tab_of = |p: &str| self.model.tab_of(p).map(|t| t.id.clone());
        let check = req.get("check").and_then(Value::as_bool).unwrap_or(false);
        let r = self.fork.browsers.attach(terminal, &space, &tab, wanted, check, &tab_of);
        if let Ok(v) = &r
            && !check
        {
            tracing::info!("browser: agent attach browser={} terminal={} (claimed, unverified) selection={}", crate::browser::log_safe(v["browser"].as_str().unwrap_or("?")), crate::browser::log_safe(terminal), v["selection"].as_str().unwrap_or("?"));
        }
        r
    }

    /// A browser pane's saved url follows its page: it reopens where it was.
    /// (A browser block of the layout keeps the url its file gives it.)
    fn save_browser_url(&mut self, browser: &str, url: &str) {
        let Some(pane) = self.browser_pane(browser) else { return };
        if let Some(n) = self.model.tab_of(&pane).map(|t| t.id.clone()).and_then(|t| self.model.tab_mut(&t)).and_then(|t| t.root.find_mut(&pane))
            && let Kind::Block { content: Content::Browser { url: saved, .. } } = &mut n.kind
        {
            *saved = url.to_owned();
        }
    }

    /// `browser.open`: a browser pane beside a pane, or as a new tab in its space.
    /// `profile`: `"agent"` (a separate Chromium profile, the only kind an
    /// agent may drive) or `"person"` (default).
    fn open_browser(&mut self, url: &str, place: &str, pane: Option<String>, profile: Option<&str>) -> Result<Value, String> {
        let profile = match profile {
            None | Some("person") => None,
            Some("agent") => Some("agent".to_owned()),
            Some(other) => return Err(format!("browser.open profile is \"agent\" or \"person\", not {other:?}")),
        };
        let (browser, url) = self.fork.browsers.prepare_open(url, &self.model, &self.chrome)?;
        let target = pane.or_else(|| self.model.focused_pane());
        let block = |id: &str| Node::block(id, Content::Browser { browser: browser.clone(), url: url.clone(), profile: profile.clone() });
        match place {
            "right" | "down" => {
                let target = target.ok_or("no pane to open the browser beside: make a space first (space.new), or use place \"tab\"")?;
                if self.model.tab_of(&target).is_none() {
                    return Err(format!("no pane {target:?}"));
                }
                let machine = self.model.space_of(&target).and_then(|s| s.machine.clone());
                self.require_document(machine.as_deref())?;
                let new = self.model.fresh_id_on("p", machine.as_deref());
                let axis = if place == "right" { Axis::Row } else { Axis::Column };
                self.model.split_with(&target, axis, block(&new));
                Ok(json!({ "browser": browser, "pane": new, "url": url }))
            }
            "tab" => {
                let space = target.as_deref().and_then(|p| self.model.space_of(p)).or_else(|| self.model.active_space()).map(|s| s.id.clone());
                let space = space.ok_or("no space yet: make one first (space.new)")?;
                let machine = self.machine_of(&space);
                self.require_document(machine.as_deref())?;
                let (tab, new) = (self.model.fresh_id_on("tb", machine.as_deref()), self.model.fresh_id_on("p", machine.as_deref()));
                self.model.add_tab(&space, Tab::with_pane(&tab, "", block(&new)));
                self.model.active = Some(space.clone());
                Ok(json!({ "browser": browser, "pane": new, "tab": tab, "space": space, "url": url }))
            }
            other => Err(format!("browser.open place is \"right\", \"down\" or \"tab\", not {other:?}")),
        }
    }

    /// `browser.*` commands. Returns whether `dispatch` returns at once
    /// (every command but `browser.open`, which goes on through dispatch's
    /// common tail, as `terminal.open` does).
    pub(super) fn fork_dispatch(&mut self, name: &str, a: &Value, person: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match name {
            "browser.open" => {
                let r = self.open_browser(Self::arg(a, "url").unwrap_or_default(), Self::arg(a, "place").unwrap_or("right"), Self::owned(a, "pane"), Self::arg(a, "profile"));
                self.answer(r);
                return false;
            }
            // Off from anywhere; on only from the native dialog's Allow
            // (`ask_agent_access`; the pane button only opens it), never from
            // here: an agent can reach the socket and write mods. `person` here
            // means a mod's run(), a key or a menu, which agents can also write.
            "browser.agent_access" => {
                let b = Self::arg(a, "browser").unwrap_or_default().to_owned();
                let on = a.get("on").and_then(Value::as_bool).unwrap_or(false);
                let from = if person {
                    crate::browser::Source::Mod
                } else if on && crate::browser::dev_switch() {
                    #[cfg(all(debug_assertions, feature = "dev-agent-access"))]
                    tracing::warn!("browser: DEV SWITCH (dev-agent-access build): the control socket turns agent access on for {}", crate::browser::log_safe(&b));
                    crate::browser::Source::DevSwitch
                } else {
                    crate::browser::Source::Socket
                };
                let r = self.set_agent_access(&b, on, from);
                // Not through sync (nothing else changed), but the header, the
                // strip and the mods' data must show the new state at once.
                self.publish(cx);
                cx.notify();
                self.answer(r);
            }
            "browser.close" => {
                let b = Self::arg(a, "browser").unwrap_or_default();
                if let Some(pane) = self.browser_pane(b) {
                    self.dispatch("pane.close", &json!({ "pane": pane }), person, window, cx);
                    return true;
                }
                let r = match self.chrome.walk().into_iter().find(|n| n.browser() == Some(b)) {
                    Some(n) => Err(format!("browser {b} is a block of the shown layout (node {:?}): hide it with layout.hide, or edit the layout", n.id)),
                    None => Err(format!("no browser {b:?}: see `herdr-app ls`")),
                };
                self.answer(r);
            }
            "browser.agent_browser_setup" => self.answer(crate::browser::agent_browser_setup()),
            "browser.navigate" | "browser.back" | "browser.forward" | "browser.reload" | "browser.devtools" | "browser.zoom" | "browser.viewport" | "browser.open_external" => {
                // Zoom and viewport default to the focused browser (keys.json's `browser` keys name none).
                let b = Self::owned(a, "browser").or_else(|| self.focused_browser()).unwrap_or_default();
                let r = self.fork.browsers.command(name, &b, a);
                if let Some(url) = r.as_ref().ok().and_then(|v| v.get("url")).and_then(Value::as_str) {
                    let url = url.to_owned();
                    self.save_browser_url(&b, &url);
                }
                // The engine's word for the person (WebKit: its inspector is Safari's).
                if let Some(note) = r.as_ref().ok().and_then(|v| v.get("note")).and_then(Value::as_str) {
                    let note = note.to_owned();
                    self.layout_notice("Web Inspector", &note, window, cx);
                }
                self.answer(r);
            }
            other => self.answer(Err(format!("unknown command {other:?}"))),
        }
        true
    }

    /// A browser pane in the mods' `panes` list: its page, no terminal.
    pub(super) fn fork_pane_json(&self, p: &Node, tab: &str, focused: bool, zoomed: bool) -> Option<Value> {
        let Some(Content::Browser { browser, url, .. }) = p.content() else { return None };
        // A browser that isn't open: its saved url, without query or fragment
        // (codes and magic links stay in the tab document only; an open one's
        // comes from `info`).
        let url = &crate::browser::public_url(url);
        let info = self.fork.browsers.info(browser, focused);
        let field = |k: &str| info.as_ref().and_then(|i| i.get(k)).and_then(Value::as_str).filter(|v| !v.is_empty()).map(str::to_owned);
        let title = field("title").or_else(|| field("url")).unwrap_or_else(|| url.clone());
        Some(json!({ "pane": p.id, "terminal": null, "browser": browser, "url": field("url").unwrap_or_else(|| url.clone()),
            "tab": tab, "title": title, "focused": focused, "zoomed": zoomed, "movable": self.pane_movable(&p.id) }))
    }

    /// A tab in `herdr-app status`, with its browser blocks.
    pub(super) fn fork_status_tab(&self, mut tab: Value, c: &Tab) -> Value {
        let browsers: Vec<Value> = c
            .root
            .walk()
            .into_iter()
            .filter_map(|n| match n.content() {
                Some(Content::Browser { browser, url, .. }) => Some(browser_status(self.fork.browsers.info(browser, false), browser, &n.id, url)),
                _ => None,
            })
            .collect();
        if !browsers.is_empty() {
            tab["browsers"] = json!(browsers);
        }
        tab
    }

    /// `herdr-app status`, with every open browser.
    pub(super) fn fork_status_world(&self, mut world: Value) -> Value {
        let focused = self.model.focused_pane();
        let browsers = self.fork.browsers.list(focused.as_deref());
        if !browsers.is_empty() {
            world["browsers"] = json!(browsers);
        }
        world
    }

    /// A browser block's view. An agent-profile browser gets Herdr's own
    /// header: the test-profile warning, the agent-access state, and the
    /// button that opens the native dialog (the only way agent access turns
    /// on). Whether to draw it comes from the open browser (fixed when it
    /// opened), never the block's `profile`: agents can edit layouts, and
    /// removing the field must not hide the marker while access is on.
    pub(super) fn fork_render_browser(&self, n: &Node, browser: &str, url: &str, cx: &mut Context<Self>) -> AnyElement {
        let id = n.id.clone();
        let bar = self.fork.browsers.agent_state(browser).map(|_| self.agent_bar(&n.id, browser, cx));
        div()
            .size_full()
            .flex()
            .flex_col()
            .children(bar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    // The page takes clicks itself once it shows; until then a click focuses the pane.
                    .on_any_mouse_down(cx.listener(move |this, _, window, cx| {
                        if this.model.tab_of(&id).is_some() && this.model.focused_pane().as_deref() != Some(id.as_str()) {
                            this.model.focus_pane(&id);
                            this.focused_pane = None;
                            this.sync(window, cx);
                        }
                    }))
                    .child(self.fork.browsers.element(&n.id, browser, url, crate::browser::Look { surface: c("surface"), text: c("foreground"), dim: c("dim") })),
            )
            .into_any_element()
    }

    /// What a page hides under, for the log: "popup frame-palette".
    pub(super) fn fork_cover_label(&self, o: &Overlay) -> String {
        let kind = match o.kind {
            OverlayKind::Dialog => "dialog",
            OverlayKind::Popup => "popup",
            OverlayKind::Toast => "toast",
            OverlayKind::Window => "window",
        };
        format!("{kind} {}", self.copies.get(&o.slot).map_or(o.id.as_str(), |c| c.inst.name.as_str()))
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: it brings `gpui::test`, which shadows `#[test]`.
    use super::{browser_status, header_over, needs_model_pane, structural};
    use crate::layout::Content;
    use serde_json::json;

    #[test]
    fn a_browser_block_that_isnt_open_shows_its_url_without_query() {
        let saved = "https://example.test/cb?step=2&id=42#top";
        assert_eq!(browser_status(None, "b1", "p1", saved), serde_json::json!({ "browser": "b1", "pane": "p1", "url": "https://example.test/cb" }));
        let with_user = "https://me:pw@example.test/cb?step=2";
        assert_eq!(browser_status(None, "b1", "p1", with_user)["url"], "https://example.test/cb");
        let open = serde_json::json!({ "url": "https://example.test/cb?step=2", "title": "T" });
        assert_eq!(browser_status(Some(open), "b1", "p1", saved)["url"], "https://example.test/cb?step=2");
    }

    #[test]
    fn a_hover_header_is_never_drawn_over_a_browser_pane() {
        let browser = Content::Browser { browser: "b1".into(), url: "https://example.test/".into(), profile: Some("agent".into()) };
        assert!(!header_over(true, Some(&browser)), "a hover header would sit on the agent header");
        assert!(!header_over(false, Some(&browser)));
        let terminal = Content::Terminal { terminal: "t1".into() };
        assert!(header_over(true, Some(&terminal)));
        assert!(!header_over(false, Some(&terminal)));
    }

    #[test]
    fn browser_commands_are_structural_and_open_beside_a_pane_needs_one() {
        assert!(structural("browser.open") && structural("browser.close"));
        assert!(!structural("browser.navigate"));
        assert!(needs_model_pane("browser.open", &json!({})));
        assert!(!needs_model_pane("browser.open", &json!({ "place": "tab" })));
        assert!(!needs_model_pane("browser.open", &json!({ "pane": "p1" })));
        assert!(!needs_model_pane("browser.close", &json!({})));
    }
}
