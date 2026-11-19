//! FORK(browser): the browser pane's part of the window. Everything the fork
//! adds to `AppWindow` lives here, so `src/window.rs` carries only one-line
//! `FORK(browser)` call sites and upstream merges touch as little as possible
//! (`CLAUDE.md` § Merge-friendly by construction). A child module sees the
//! window's private fields and methods.

use super::*;
use crate::browser::block::{self, BrowserNode};

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
    /// Local terminals this app has labeled as its own this run
    /// (`fork_claim_terminals`): each once, so two apps on one core don't
    /// take a terminal back and forth.
    #[cfg(all(target_os = "macos", feature = "browser-pane"))]
    claimed: std::collections::HashSet<String>,
    /// When each terminal's last `terminal.label` write failed, so a retry
    /// waits `agent::LABEL_RETRY` rather than every listing tick.
    #[cfg(all(target_os = "macos", feature = "browser-pane"))]
    label_failed: std::collections::HashMap<String, std::time::Instant>,
    /// When this app last asked whether the apps that took its terminals
    /// still serve their sockets (`agent::taken_by_others`).
    #[cfg(all(target_os = "macos", feature = "browser-pane"))]
    taken_checked: Option<std::time::Instant>,
    /// Whether the shipped `browser-prototype` mod was edited (its files
    /// differ from `installed.json`'s record): then the window draws browser
    /// blocks itself and places the page whole, so an edited mod can't draw
    /// over a page's area until the next start moves it aside
    /// (cameronsjo/herdr-app#43). Read at start and when its files change.
    prototype_edited: bool,
}

impl ForkBrowser {
    pub(super) fn new() -> Self {
        Self {
            browsers: crate::browser::Browsers::new(),
            layout_page: None,
            control_focus: None,
            #[cfg(all(target_os = "macos", feature = "browser-pane"))]
            page_keys: Vec::new(),
            #[cfg(all(target_os = "macos", feature = "browser-pane"))]
            claimed: Default::default(),
            #[cfg(all(target_os = "macos", feature = "browser-pane"))]
            label_failed: Default::default(),
            #[cfg(all(target_os = "macos", feature = "browser-pane"))]
            taken_checked: None,
            prototype_edited: prototype_edited(),
        }
    }
}

/// Whether the shipped `browser-prototype` mod differs from what Herdr
/// installed (`mods::kinds` counts it as edited). Never where browsers
/// don't run: there the window draws every browser block anyway.
fn prototype_edited() -> bool {
    crate::browser::AVAILABLE && crate::mods::kinds(&std::collections::BTreeSet::from([block::BROWSER_MOD.to_owned()])).1 > 0
}

/// Whether a pane frame's header is drawn over the pane (`reveal: "hover"`)
/// rather than beside it. Never over a browser pane: GPUI can't draw over
/// the page's native view, so it would show only where Herdr draws (an
/// agent browser's header, which it would hide on hover).
pub(super) fn header_over(hover: bool, content: Option<&Content>) -> bool {
    hover && !content.is_some_and(block::is_browser)
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

    /// Mod folders whose files changed (`edited`, by name): re-reads whether
    /// `browser-prototype` is still as installed.
    pub(super) fn fork_mods_edited(&mut self, edited: &BTreeSet<String>) {
        if edited.contains(block::BROWSER_MOD) {
            self.fork.prototype_edited = prototype_edited();
        }
    }

    /// Focus moved on purpose: a layout block's page, or a control the
    /// person pressed, stops holding the keyboard.
    pub(super) fn fork_focus_moved(&mut self) {
        self.fork.layout_page = None;
        self.fork.control_focus = None;
    }

    /// Labels each of this app's local terminals with its control socket
    /// (`agent::SOCKET_LABEL`), so agent-browser in it finds the app that
    /// holds it now, not the one that started it (cameronsjo/herdr-app#6).
    /// A terminal is labeled when the core lists it with another socket or
    /// none, and then not again while another app holds it: the app started
    /// last holds the terminals it shows. Only while this app serves the
    /// socket it names; a failed write waits `agent::LABEL_RETRY`; and a
    /// terminal another app took comes back once that app quit
    /// (`agent::app_quit`, cameronsjo/herdr-app#31).
    pub(super) fn fork_claim_terminals(&mut self, cx: &mut Context<Self>) {
        #[cfg(all(target_os = "macos", feature = "browser-pane"))]
        {
            use crate::browser::agent;
            if !crate::browser::socket::serving() {
                return;
            }
            let socket = crate::mods::app_dir().join("herdr-app.sock").to_string_lossy().into_owned();
            let label = agent::SOCKET_LABEL;
            let now = std::time::Instant::now();
            for addr in agent::claim_todo(&self.model, &self.live, &self.fork.claimed, &socket) {
                if !agent::label_retry_due(self.fork.label_failed.get(&addr).copied(), now) {
                    continue;
                }
                self.fork.claimed.insert(addr.clone());
                let core = self.core.clone();
                let params = json!({ "terminal": addr, "labels": { (label): socket } });
                cx.spawn(async move |this, cx| {
                    let result = cx.background_executor().spawn(async move { core.call("terminal.label", params) }).await;
                    let _ = this.update(cx, |this, _| match result {
                        Ok(_) => {
                            this.fork.label_failed.remove(&addr);
                        }
                        Err(e) => {
                            // One warning per run of failures; the rest wait and log quietly.
                            if this.fork.label_failed.insert(addr.clone(), std::time::Instant::now()).is_none() {
                                tracing::warn!("browser: cannot label terminal {}: {e:#}", crate::browser::log_safe(&addr));
                            } else {
                                tracing::debug!("browser: still cannot label terminal {}: {e:#}", crate::browser::log_safe(&addr));
                            }
                            // Try again on a listing after the pause.
                            this.fork.claimed.remove(&addr);
                        }
                    });
                })
                .detach();
            }
            let live = &self.live;
            self.fork.claimed.retain(|a| live.contains_key(a));
            self.fork.label_failed.retain(|a, _| live.contains_key(a));
            // Terminals another app took: ask, off the main thread and at most
            // every LABEL_RETRY, whether that app still serves its socket.
            let taken = agent::taken_by_others(&self.live, &self.fork.claimed, &socket);
            let due = self.fork.taken_checked.is_none_or(|at| now.saturating_duration_since(at) >= agent::LABEL_RETRY);
            if !taken.is_empty() && due {
                self.fork.taken_checked = Some(now);
                cx.spawn(async move |this, cx| {
                    let released = cx
                        .background_executor()
                        .spawn(async move { taken.into_iter().filter(|(s, _)| agent::app_quit(s)).collect::<Vec<_>>() })
                        .await;
                    if released.is_empty() {
                        return;
                    }
                    let _ = this.update(cx, |this, _| {
                        // Only terminals the core still lists with the quit app's
                        // socket: one this app labeled meanwhile stays claimed, or
                        // a newer app's label would be taken from it at once.
                        let back = agent::still_labeled(&this.live, &released);
                        for t in &back {
                            this.fork.claimed.remove(t);
                        }
                        if !back.is_empty() {
                            tracing::info!("browser: taking back {} terminal(s) whose app quit", back.len());
                        }
                    });
                })
                .detach();
            }
        }
        #[cfg(not(all(target_os = "macos", feature = "browser-pane")))]
        let _ = cx;
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
        // A mod block holding the keyboard (`pane.go` onto a focusable mod,
        // upstream's `focus_block`) is a Herdr control too: the focused pane
        // stays the browser's, but typing goes to the mod.
        let control = control || self.focused_block.as_ref().is_some_and(|b| self.mod_block_shown(b));
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
            // A mod block that held the keyboard lets go first (as a terminal
            // click does), or the page could never take it back from a mod an
            // agent focused through the socket (`pane.go`).
            self.focused_block = None;
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
            return Err(if self.live.contains_key(terminal) { crate::browser::parked(terminal) } else { crate::browser::no_terminal(terminal) });
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
            && let Kind::Block { content } = &mut n.kind
        {
            block::set_url(content, url);
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
        let block = |id: &str| Node::block(id, block::content(&browser, &url, profile.as_deref()));
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
        let block::BrowserBlock { browser, url, .. } = p.content().and_then(block::block)?;
        let (browser, url) = (&browser, &url);
        // A browser that isn't open: its saved url, without query or fragment
        // (codes and magic links stay in the tab document only; an open one's
        // comes from `info`).
        let url = &crate::browser::public_url(url);
        let info = self.fork.browsers.info(browser, focused);
        let field = |k: &str| info.as_ref().and_then(|i| i.get(k)).and_then(Value::as_str).filter(|v| !v.is_empty()).map(str::to_owned);
        let title = field("title").or_else(|| field("url")).unwrap_or_else(|| url.clone());
        Some(json!({ "pane": p.id, "terminal": null, "browser": browser, "url": field("url").unwrap_or_else(|| url.clone()),
            "tab": tab, "title": title, "name": p.title, "focused": focused, "zoomed": zoomed, "movable": self.pane_movable(&p.id) }))
    }

    /// A tab in `herdr-app status`, with its browser blocks.
    pub(super) fn fork_status_tab(&self, mut tab: Value, c: &Tab) -> Value {
        let browsers: Vec<Value> = c
            .root
            .walk()
            .into_iter()
            .filter_map(|n| n.content().and_then(block::block).map(|b| browser_status(self.fork.browsers.info(&b.browser, false), &b.browser, &n.id, &b.url)))
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
    ///
    /// The body is the `browser-prototype` mod's copy for this block, which
    /// draws the placeholder and says where the page goes (`nativeView`),
    /// with a canvas after it that places the page every frame
    /// (`browser::element`). The window draws the body itself when the page
    /// can't show here (`Browsers::drawn_by_mod`).
    pub(super) fn fork_render_browser(&self, n: &Node, browser: &str, url: &str, cx: &mut Context<Self>) -> AnyElement {
        let id = n.id.clone();
        let bar = self.fork.browsers.agent_state(browser).map(|_| self.agent_bar(&n.id, browser, cx));
        let look = crate::browser::Look { surface: c("surface"), text: c("foreground"), dim: c("dim") };
        let root = self.copies.get(&n.id).and_then(|c| c.root.clone());
        let owns = self.fork.browsers.owns(&n.id, browser);
        let b = browser.to_owned();
        let body = if let Some(root) = root.filter(|_| !self.fork.prototype_edited && self.fork.browsers.drawn_by_mod(&n.id, browser)) {
            let place = owns.then(|| canvas(|_, _, _| {}, move |bounds, _, window, _| crate::browser::place(&b, bounds, window)).absolute().size_full());
            // The mod's root as `render_slot` draws a pane mod, without its
            // click handler: that one focuses the pane as a mod that keeps the
            // keyboard (publish, save), and the handler below would at once undo
            // it, so one click wrote the session twice (cameronsjo/herdr-app#43).
            let slot = div()
                .size_full()
                .key_context(super::key_context("Mod", "mod", block::BROWSER_MOD))
                // A layout block (not a tab's pane or an overlay's): as
                // `render_slot` does, a bare click leaves the keyboard with the
                // focused pane.
                .when(self.model.tab_of(&n.id).is_none() && !self.overlays.iter().any(|o| o.layout.as_ref().is_some_and(|l| l.find(&n.id).is_some())), |d| {
                    d.capture_any_mouse_down(cx.listener(|_, _, window, cx| cx.defer_in(window, |this, window, cx| this.refocus_pane(window, cx))))
                })
                .child(root);
            div().size_full().relative().child(slot).children(place).into_any_element()
        } else {
            // The window's own placeholder. Also when the mod didn't load (the mods' VM
            // failed, a broken edit) or was edited: the page then fills the body, so it
            // never depends on the mod runtime or code Herdr didn't ship.
            let fill = owns.then(|| canvas(|_, _, _| {}, move |bounds, _, window, _| crate::browser::place_whole(&b, bounds, window)).absolute().size_full());
            div().size_full().relative().child(self.fork.browsers.element(&n.id, browser, url, look)).children(fill).into_any_element()
        };
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
                    // The page takes clicks itself once it shows; until then a click focuses the
                    // pane and gives the page the keyboard: a browser pane's keyboard is its
                    // page's, never the mod's (`focused_pane` stays None).
                    .on_any_mouse_down(cx.listener(move |this, _, window, cx| {
                        if this.model.tab_of(&id).is_none() {
                            return;
                        }
                        if this.model.focused_pane().as_deref() != Some(id.as_str()) {
                            this.model.focus_pane(&id);
                        }
                        this.focused_pane = None;
                        this.sync(window, cx);
                    }))
                    .child(body),
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
    use crate::browser::block;
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
        let browser = block::content("b1", "https://example.test/", Some("agent"));
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

/// A browser block runs as an ordinary mod: the shipped `browser-prototype`
/// (cameronsjo/herdr-app#4).
#[cfg(test)]
mod window_tests {
    use crate::browser::block::BROWSER_MOD;
    use serde_json::json;

    /// `window::tests::in_own_process` for this module: the test alone in a
    /// process of its own, on a fresh app home (a window takes the app home
    /// and the mods' request channel, which are process-wide).
    fn in_own_process(name: &str) {
        let home = crate::test_tmp().join(format!("herdr-fork-window-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &format!("window::fork_browser::window_tests::{name}"), "--nocapture", "--test-threads=1"])
            .env("HERDR_APP_HOME", &home)
            .env("HERDR_TEST_WINDOW", name)
            .output()
            .unwrap();
        let _ = std::fs::remove_dir_all(&home);
        let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success() && log.contains("1 passed"), "{log}");
    }

    fn running_alone(name: &str) -> bool {
        std::env::var("HERDR_TEST_WINDOW").as_deref() == Ok(name)
    }

    #[test]
    fn a_browser_block_runs_the_browser_prototype_mod() {
        in_own_process("window_runs_the_browser_prototype_mod");
    }

    #[gpui::test]
    fn window_runs_the_browser_prototype_mod(cx: &mut gpui::TestAppContext) {
        if !running_alone("window_runs_the_browser_prototype_mod") {
            return;
        }
        let home = crate::mods::app_dir();
        assert!(std::env::var_os("HERDR_APP_HOME").is_some_and(|h| home == std::path::Path::new(&h)), "never the real app home");
        crate::mods::write_workspace().unwrap();
        std::fs::create_dir_all(home.join("layouts")).unwrap();
        // A browser block in the tree, a floating one (float_mod adds a
        // `float` prop), a stack header and a real mod beside them.
        let layout = json!({ "id": "root", "type": "row", "children": [
            { "id": "page", "type": "block", "content": { "mod": BROWSER_MOD, "props": { "browser": "b1", "url": "about:blank" } } },
            { "id": "other", "type": "block", "content": { "mod": "frame-none" } },
            // A stack whose header is a browser-prototype mod with a url: its
            // copy goes through mod_props too.
            { "id": "st", "type": "stack", "active": "s1",
              "header": { "content": { "mod": BROWSER_MOD, "props": { "browser": "b3", "url": "https://example.test/h?code=7" } } },
              "children": [ { "id": "s1", "type": "block", "content": { "mod": "frame-none" } } ] } ],
          "floating": [ { "id": "f", "content": { "mod": BROWSER_MOD, "props": { "browser": "b2", "url": "https://example.test/cb?code=42" } } } ] });
        std::fs::write(home.join("layouts/browsers.json"), layout.to_string()).unwrap();
        cx.update(crate::mods::init);
        let requests = crate::mods::install_policy();
        crate::mods::set_shared("layout".into(), json!("browsers"));
        crate::mods::set_shared("frame.welcomed".into(), json!("test"));
        let (view, cx) = cx.add_window_view(|window, cx| super::AppWindow::with_file_events(crate::hc::offline(), requests, Some(crate::files::FileEvents::for_test(home.clone())), window, cx));
        cx.run_until_parked();
        view.update(cx, |this, _| {
            assert_eq!(this.layout_name, "browsers");
            let (_, shown) = this.visible();
            assert!(shown.iter().any(|(s, i)| s == "page" && i.name == BROWSER_MOD), "visible: {:?}", shown.iter().map(|(s, _)| s).collect::<Vec<_>>());
            assert!(shown.iter().any(|(s, i)| s == "st#header" && i.name == BROWSER_MOD), "the stack header's copy is visible");
            let page = this.copies.get("page").expect("the browser block's mod copy");
            assert_eq!(page.inst.name, BROWSER_MOD);
            // It loaded: `herdr-native` (nativeView) resolves in the mods' VM.
            assert!(page.root.is_some() && page.error.is_none(), "browser-prototype did not load: {:?}", page.error);
            assert!(super::missing_mods("browsers").iter().all(|m| m != BROWSER_MOD), "the mod ships with Herdr");
            // No copy's props hold a page's url, the float's included.
            let owned = this.owned_mods();
            for (slot, i) in shown.iter().map(|(s, i)| (s, i)).chain(owned.iter()) {
                assert!(i.props.get("url").is_none(), "{slot} got a url: {}", i.props);
            }
            assert_eq!(owned.get(&super::floats::slot("f")).map(|i| &i.props["float"]), Some(&json!("f")));
        });
        // A new VM (memory restart) keeps the catalog, or every page shows "did not load".
        cx.update(|window, cx| view.update(cx, |this, cx| assert!(this.restart_mods(window, cx))));
        cx.run_until_parked();
        view.update(cx, |this, _| {
            let page = this.copies.get("page").expect("the browser block's mod copy after a restart");
            assert!(page.root.is_some() && page.error.is_none(), "browser-prototype did not load after restart_mods: {:?}", page.error);
        });
    }

    /// The body of `fn <name>(` in `source`: up to the next item at the same
    /// four-space indent. Each source-order test reads one function, so a
    /// match elsewhere in window.rs can't stand in for the seam.
    fn fn_body<'a>(source: &'a str, name: &str) -> &'a str {
        let start = source.find(&format!("\n    fn {name}(")).unwrap_or_else(|| panic!("fn {name} is in window.rs"));
        let rest = &source[start + 1..];
        let end = rest.find("\n    }\n").unwrap_or_else(|| panic!("fn {name} has an end"));
        &rest[..end]
    }

    /// window.rs with `\n` line ends: a Windows checkout has `\r\n`.
    fn window_rs() -> String {
        include_str!("../window.rs").replace("\r\n", "\n")
    }

    /// Where `needle` is in `body`, asserting it occurs exactly once.
    fn once(body: &str, needle: &str, what: &str) -> usize {
        assert_eq!(body.matches(needle).count(), 1, "{what} must occur exactly once");
        body.find(needle).unwrap_or_default()
    }

    /// The mods' `panes` list asks `fork_pane_json` before upstream's mod-pane
    /// branch, or a browser pane is listed as a mod named `browser-prototype`.
    /// A sync that moves the line back passes every other test, so this reads
    /// the order from the source.
    #[test]
    fn the_panes_list_asks_for_a_browser_before_a_mod() {
        let source = window_rs();
        let publish = fn_body(&source, "publish");
        let browser = once(publish, "if let Some(v) = self.fork_pane_json(", "the panes-list seam in fn publish");
        let mod_pane = once(publish, "if let Some(Content::Mod { name, .. }) = p.content() {", "upstream's mod-pane branch in fn publish");
        assert!(browser < mod_pane, "fork_pane_json must come before the mod-pane branch in the panes list");
    }

    /// An edited `browser-prototype` stops drawing over pages only if
    /// `files_changed` tells the fork which mods changed. A sync that drops
    /// the call passes every other test (it only matters on macOS).
    #[test]
    fn files_changed_tells_the_fork_which_mods_were_edited() {
        let source = window_rs();
        let changed = fn_body(&source, "files_changed");
        let stamps = once(changed, "let edited: BTreeSet<String> =", "upstream's edited-mods set in fn files_changed");
        let fork = once(changed, "self.fork_mods_edited(&edited);", "the fork_mods_edited seam in fn files_changed");
        assert!(stamps < fork, "fork_mods_edited must follow the edited set");
    }

    /// `render_block` draws a browser block before upstream's `match content`,
    /// whose mod arm would draw the mod copy alone: the frame, agent header
    /// and page placement are the window's, so the pane would show only the
    /// placeholder. A sync that drops the early return passes every other test.
    #[test]
    fn render_block_draws_a_browser_before_matching_its_content() {
        let source = window_rs();
        let render = fn_body(&source, "render_block");
        let browser = once(render, "if let Some(b) = crate::browser::block::block(content) { return self.fork_render_browser(", "the browser early return in fn render_block");
        let matched = once(render, "match content {", "upstream's content match in fn render_block");
        assert!(browser < matched, "the browser early return must come before `match content` in render_block");
    }
}
