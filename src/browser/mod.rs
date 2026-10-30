//! Browser panes: a Chromium page in a block (`{ "browser": "b1", "url": … }`),
//! as a pane in a tab or a block in the shown layout.
//!
//! macOS builds with the `browser` feature run CEF in windowed mode: each page
//! is a native child view over GPUI's Metal view, kept on its block's bounds
//! every frame (native.rs, view.rs, runtime.rs). Every other build has the
//! same API with no browser behind it: blocks show a placeholder and the
//! commands say what's missing (off.rs). The window only talks to `Browsers`.

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
pub mod agent;
#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
pub mod keys;
#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
pub mod layer;
#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
pub mod registry;

#[cfg(all(target_os = "macos", feature = "browser"))]
mod native;
#[cfg(all(target_os = "macos", feature = "browser"))]
mod proxy;
#[cfg(all(target_os = "macos", feature = "browser"))]
mod runtime;
#[cfg(all(target_os = "macos", feature = "browser"))]
mod view;
#[cfg(all(target_os = "macos", feature = "browser"))]
pub use native::{Browsers, Event};

#[cfg(not(all(target_os = "macos", feature = "browser")))]
mod off;
#[cfg(not(all(target_os = "macos", feature = "browser")))]
pub use off::{Browsers, Event};

/// What the window does after a browser reported something.
#[derive(Debug, Default)]
pub struct Update {
    pub browser: String,
    /// The page moved here: the block's saved url follows.
    pub url: Option<String>,
    /// The page took the keyboard (the person clicked it): its pane is the focused one now.
    pub focused: bool,
    /// The page asked to close (`window.close()`): its pane goes.
    pub close: bool,
    /// The person pressed the mouse on Herdr, outside every page.
    pub clicked_herdr: bool,
    /// A page hid under an overlay or came back: who has the keyboard may change.
    pub covers: bool,
}

/// The theme colors a browser block's placeholder uses.
#[derive(Clone, Copy)]
pub struct Look {
    pub surface: gpui::Hsla,
    pub text: gpui::Hsla,
    pub dim: gpui::Hsla,
}

/// Which page has the keyboard: none while a dialog or popup is open (it
/// takes typing) or while a Herdr control the person pressed has GPUI focus
/// (a layout mod's input, a floating terminal); else the layout block's page
/// the person clicked into, while it shows; else the focused pane's page,
/// while its tab shows. A page hidden under an overlay (`covered`) can't
/// have it: Herdr holds it until the overlay goes, and the page gets it back
/// only if it still wins then. `None`: Herdr (GPUI) has it.
pub fn keyboard_page<'a>(dialog: bool, control: bool, layout_page: Option<&'a str>, pane_page: Option<&'a str>, covered: &[String]) -> Option<&'a str> {
    if dialog || control {
        return None;
    }
    layout_page.or(pane_page).filter(|p| !covered.iter().any(|c| c == p))
}

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
/// A rectangle in window points, top-left origin (as GPUI paints).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
impl Rect {
    /// Shares some area with `other`: edges that only touch, and an empty
    /// rectangle (an overlay not measured yet), don't count.
    pub fn overlaps(&self, other: &Rect) -> bool {
        self.w > 0.0
            && self.h > 0.0
            && other.w > 0.0
            && other.h > 0.0
            && self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }
}

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
/// Which overlay each page hides under: GPUI can't draw over a page's native
/// view, so a page an overlay overlaps hides while it's there and its block
/// shows a placeholder. `covers` are the overlays painted this frame (label,
/// bounds), in paint order; a page names the first that overlaps it. None:
/// the page shows.
pub fn covered<'a>(pages: &[(&'a str, Rect)], covers: &'a [(String, Rect)]) -> Vec<(&'a str, Option<&'a str>)> {
    pages
        .iter()
        .map(|(id, page)| (*id, covers.iter().find(|(_, c)| c.overlaps(page)).map(|(label, _)| label.as_str())))
        .collect()
}

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
/// What has AppKit's first responder in Herdr's window.
#[derive(Clone, Debug, PartialEq)]
pub enum Responder {
    /// GPUI's view: Herdr's own keys, terminals and inputs get typing.
    Herdr,
    Page(String),
    /// Anything else, usually the window itself: AppKit makes the window
    /// first responder when the view that had it leaves (a closed page),
    /// and then plain typing reaches nobody.
    Other,
}

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
impl std::fmt::Display for Responder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Responder::Herdr => f.write_str("Herdr"),
            Responder::Page(id) => f.write_str(id),
            Responder::Other => f.write_str("the window (nobody)"),
        }
    }
}

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
/// What to do so the page `wanted` (None: Herdr) has the keyboard, given
/// who really has it.
#[derive(Debug, PartialEq)]
pub enum Hand<'a> {
    Keep,
    Give(&'a str),
    TakeBack,
}

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
pub fn hand<'a>(wanted: Option<&'a str>, actual: &Responder) -> Hand<'a> {
    match (wanted, actual) {
        (Some(w), Responder::Page(a)) if a == w => Hand::Keep,
        (Some(w), _) => Hand::Give(w),
        (None, Responder::Herdr) => Hand::Keep,
        (None, _) => Hand::TakeBack,
    }
}

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
/// Whether a page may take focus when CEF asks (on creation, on a load, on
/// a click): only the page the window gave the keyboard (`wanted`; None
/// before the window first decided), or one the person just pressed.
pub fn may_focus(page: &str, wanted: Option<Option<&str>>, pressed: bool) -> bool {
    pressed || wanted.is_none_or(|w| w == Some(page))
}


// ---- agent access: what the window needs in every build (rules: agent.rs)

/// Where a change to agent access came from (and the audit log's `by=`).
#[cfg_attr(not(all(target_os = "macos", feature = "browser")), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Herdr's own button in the browser pane's header (a person's click).
    PaneControl,
    /// `herdr-app do` over the control socket: agents can reach it.
    Socket,
    /// A mod's `run()` (and keys.json bindings, the native key monitor): agents write mods and keys.
    Mod,
    /// The dev-only switch (debug builds with the `dev-agent-access` feature,
    /// `HERDR_DEV_AGENT_ACCESS=1` at launch).
    DevSwitch,
    Close,
    Quit,
}

#[cfg_attr(not(all(target_os = "macos", feature = "browser")), allow(dead_code))]
impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::PaneControl => "pane control",
            Source::Socket => "socket",
            Source::Mod => "mod or key",
            #[cfg(all(debug_assertions, feature = "dev-agent-access"))]
            Source::DevSwitch => "socket (dev switch, dev-agent-access build)",
            // Never set in builds without the dev feature.
            #[cfg(not(all(debug_assertions, feature = "dev-agent-access")))]
            Source::DevSwitch => "socket",
            Source::Close => "close",
            Source::Quit => "quit",
        }
    }
}

/// The access button in an agent browser's header: its label and what a
/// click sets, both from the state it was painted with. The click acts on
/// what the person saw: a "Stop agent" still on screen after a change it
/// never showed (the socket turned access off) means off. Toggling the live
/// state instead would turn access back on.
pub fn access_button(shown_on: bool) -> (&'static str, bool) {
    if shown_on { ("Stop agent", false) } else { ("Allow agent", true) }
}

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
/// How long "Allow agent" must have been on screen, with that label, no
/// overlay over it, the window active, and the same place and browser,
/// before a click on it counts.
/// Chromium waits the same 500 ms before its permission prompts take a click
/// (`InputEventActivationProtector`).
pub const ALLOW_AFTER: std::time::Duration = std::time::Duration::from_millis(500);

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
/// The first of Herdr's covers (overlays painted this frame) that overlaps
/// any part of `target`: an agent browser's header, or its button. A header
/// with one over it isn't drawn (`layer::top`), and its "Allow agent" doesn't
/// count as shown (`AllowGuard`).
pub fn over<'a>(target: &Rect, covers: &'a [(String, Rect)]) -> Option<&'a str> {
    covers.iter().find(|(_, c)| c.overlaps(target)).map(|(label, _)| label.as_str())
}

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
/// Input protection for the access buttons, as Chromium's
/// `InputEventActivationProtector` does for its prompts. "Allow agent" takes
/// a click only after `ALLOW_AFTER` on screen unchanged (same label, same
/// place, same browser in the block, nothing over any part of its header,
/// the window active and no drag going on), and never a second click of a
/// double click. So neither a double click on "Stop agent" nor a click aimed
/// at something that changed under the pointer turns access on: an overlay
/// that closed on the first click, a socket `off` just before the person
/// clicked "Stop agent", a window activated by the click, or a button an
/// agent slid under the pointer (`layout.resize`, `pane.swap`, `pane.zoom`,
/// `layout.set`) or swapped for another browser's. "Stop agent" takes every
/// click at once. Keyed by the block (node) the button is in.
///
/// What can be over the header: the header is drawn in Herdr's top layer
/// (`layer::top`), after every element and every deferred draw a mod can
/// make, and occludes, so mod and layout content is always under it and
/// gets none of its clicks. Only Herdr's own overlays (`Browsers::cover`:
/// palette, dialogs, popups, menus, toasts, floats, the drag grip and drop
/// preview, the update pill) are drawn above it; where one overlaps any part
/// of the header, the header isn't drawn and its button starts its wait
/// again (`over`).
#[derive(Default)]
pub struct AllowGuard {
    shown: std::collections::HashMap<String, Shown>,
    /// The buttons painted this frame: node, browser, what a click sets, bounds.
    painted: Vec<(String, String, bool, Rect)>,
    /// The headers laid out this frame, by node (drawn or not).
    headers: std::collections::HashMap<String, Rect>,
}

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
#[derive(Clone, Debug)]
struct Shown {
    /// The browser whose header it is.
    browser: String,
    sets: bool,
    /// Where it painted (window points).
    at: Rect,
    /// Something over its header, the window inactive, or a drag going on, this frame.
    covered: bool,
    /// When it started to show as it does now.
    since: std::time::Instant,
}

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
impl AllowGuard {
    /// A button painted this frame in block `node`, for `browser`'s header (window points).
    pub fn painted(&mut self, node: &str, browser: &str, sets: bool, at: Rect) {
        self.painted.push((node.to_owned(), browser.to_owned(), sets, at));
    }

    /// Block `node`'s header this frame (window points), the button's whole bar.
    pub fn header(&mut self, node: &str, at: Rect) {
        self.headers.insert(node.to_owned(), at);
    }

    /// The frame's end, with the overlays painted in it. `ready`: the window
    /// is active and no drag is going on. A button that changed what it does,
    /// moved, now belongs to another browser, has anything over any part of
    /// its header, has just come out from under one, or first shows now,
    /// starts its wait again. One not painted in this frame is forgotten (it
    /// starts again when it comes back).
    pub fn end_frame(&mut self, covers: &[(String, Rect)], ready: bool, now: std::time::Instant) {
        let mut shown = std::collections::HashMap::new();
        let headers = std::mem::take(&mut self.headers);
        for (node, browser, sets, at) in self.painted.drain(..) {
            let header = headers.get(&node).copied().unwrap_or(at);
            let covered = !ready || over(&header, covers).is_some() || over(&at, covers).is_some();
            let since = match self.shown.get(&node) {
                Some(s) if s.sets == sets && s.browser == browser && same_place(&s.at, &at) && !s.covered && !covered => s.since,
                _ => now,
            };
            shown.insert(node, Shown { browser, sets, at, covered, since });
        }
        self.shown = shown;
    }

    /// Whether a click on `node`'s button, for `browser`, which sets `sets`, counts.
    pub fn may_click(&self, node: &str, browser: &str, sets: bool, click_count: usize, now: std::time::Instant) -> bool {
        if !sets {
            return true;
        }
        click_count <= 1
            && self.shown.get(node).is_some_and(|s| s.sets && s.browser == browser && !s.covered && now.saturating_duration_since(s.since) >= ALLOW_AFTER)
    }
}

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
/// The same place on screen: every edge within half a point (float noise
/// from layout, not a move a person could miss).
fn same_place(a: &Rect, b: &Rect) -> bool {
    const SLACK: f32 = 0.5;
    (a.x - b.x).abs() <= SLACK && (a.y - b.y).abs() <= SLACK && (a.w - b.w).abs() <= SLACK && (a.h - b.h).abs() <= SLACK
}

#[cfg(all(debug_assertions, feature = "dev-agent-access"))]
static DEV_SWITCH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What every agent header and the window's agent strip say while the dev
/// switch is on: the person's click is no longer the only way in. Builds
/// without the dev feature carry no such text.
pub fn dev_switch_note() -> Option<&'static str> {
    #[cfg(all(debug_assertions, feature = "dev-agent-access"))]
    if dev_switch() {
        return Some("DEV SWITCH: the control socket can turn agent access on");
    }
    None
}

/// Debug builds with the `dev-agent-access` feature only:
/// `HERDR_DEV_AGENT_ACCESS=1` in the app's own environment at launch lets
/// the control socket turn agent access on, so an agent can test the path
/// end to end without a person's click. That drops the consent property, so
/// the dev bundle people run (`just browser-dev`) is built without it. Read
/// once at startup and removed from the environment, so terminals never
/// inherit it. Other builds compile none of this: the variable means nothing.
#[cfg_attr(not(all(target_os = "macos", feature = "browser")), allow(dead_code))]
pub fn read_dev_switch() {
    #[cfg(all(debug_assertions, feature = "dev-agent-access"))]
    {
        const NAME: &str = "HERDR_DEV_AGENT_ACCESS";
        let on = std::env::var(NAME).is_ok_and(|v| v == "1");
        // SAFETY: called first thing in main, before any other thread exists.
        unsafe { std::env::remove_var(NAME) };
        DEV_SWITCH.store(on, std::sync::atomic::Ordering::SeqCst);
    }
}

pub fn dev_switch() -> bool {
    #[cfg(all(debug_assertions, feature = "dev-agent-access"))]
    return DEV_SWITCH.load(std::sync::atomic::Ordering::SeqCst);
    #[cfg(not(all(debug_assertions, feature = "dev-agent-access")))]
    false
}

/// The url without its userinfo, query and fragment, wherever a url leaves
/// the app for a browser agents can't drive (`herdr-app ls`, mods'
/// `browsers()`, the status of a block whose browser isn't open), and in
/// logs: OAuth codes, magic links and passwords stay out of reach. The tab
/// document keeps the whole url (restore needs it).
pub fn public_url(url: &str) -> String {
    let url = url.split(['?', '#']).next().unwrap_or_default();
    match url.split_once("://") {
        Some((scheme, rest)) => {
            let (authority, path) = rest.split_at(rest.find(['/', '\\']).unwrap_or(rest.len()));
            let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
            format!("{scheme}://{host}{path}")
        }
        None => url.to_owned(),
    }
}

/// A page's title wherever its url goes out as `public_url`. Chromium titles
/// a page that has none with its url ("127.0.0.1:8765/cb?code=…", no scheme
/// or userinfo, maybe unescaped), so a title that starts with the page's
/// host, or carries its query, is cut the same way. Other titles pass.
#[cfg_attr(not(all(target_os = "macos", feature = "browser")), allow(dead_code))]
pub fn public_title(title: &str, url: &str) -> String {
    let (before, query) = url.split_once('?').map_or((url, ""), |(b, q)| (b, q.split('#').next().unwrap_or_default()));
    let host = public_url(before)
        .split_once("://")
        .map(|(_, rest)| rest.split(['/', '\\']).next().unwrap_or_default().to_ascii_lowercase())
        .unwrap_or_default();
    let shown = title.trim_start();
    let shown = shown.split_once("://").filter(|(scheme, _)| !scheme.contains([' ', '/'])).map_or(shown, |(_, rest)| rest);
    let shown = shown.split_once('@').filter(|(user, _)| !user.contains(['/', ' '])).map_or(shown, |(_, rest)| rest).to_ascii_lowercase();
    let hosts = [host.as_str(), host.strip_prefix("www.").unwrap_or_default()];
    let carries_query = !query.is_empty() && title.contains(query);
    if !carries_query && !hosts.iter().any(|h| !h.is_empty() && shown.starts_with(h)) {
        return title.to_owned();
    }
    let title = if carries_query { title.replace(query, "") } else { title.to_owned() };
    let cut = title.split(['?', '#']).next().unwrap_or_default();
    match cut.split_once('@') {
        Some((user, rest)) if !user.contains(['/', ' ']) => rest.to_owned(),
        _ => cut.to_owned(),
    }
}

/// Text from outside Herdr (an agent's CDP method name, a url it asked for,
/// a key binding's action) as a log line may carry it: control characters,
/// line and paragraph separators and bidi controls escaped (`\n`,
/// `\u{202e}`), so it can't start a line of its own or reorder one; at most
/// 200 characters.
#[cfg_attr(not(all(target_os = "macos", feature = "browser")), allow(dead_code))]
pub fn log_safe(text: &str) -> String {
    let mut out = String::new();
    for (i, ch) in text.chars().enumerate() {
        if i == 200 {
            out.push('…');
            break;
        }
        if ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
            out.extend(ch.escape_default());
        } else {
            out.push(ch);
        }
    }
    out
}

// Errors an agent sees (pinned in agent.rs's tests).

pub fn not_in_herdr() -> String {
    "this terminal is not in Herdr (HC_TERMINAL_ID is unset): agent-browser can only drive a Herdr browser pane from a Herdr terminal".into()
}
pub fn parked(terminal: &str) -> String {
    format!("terminal {terminal} has no pane in Herdr (parked): give it one with herdr-app do terminal.keep --terminal {terminal}")
}
pub fn remote(terminal: &str) -> String {
    format!("terminal {terminal} runs on another machine: browser panes are only for this machine's terminals")
}

/// Whether browser panes work in this build.
pub const AVAILABLE: bool = cfg!(all(target_os = "macos", feature = "browser"));

#[cfg(test)]
mod allow_tests {
    //! The access buttons' input protection (`AllowGuard`), frame by frame.
    use super::{ALLOW_AFTER, AllowGuard, Rect, access_button};
    use std::time::{Duration, Instant};

    const BUTTON: Rect = Rect { x: 1180.0, y: 40.0, w: 80.0, h: 20.0 };
    const ON: bool = true;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// One frame at `at` with p1's header button painted (`shown_on`: access
    /// on, so "Stop agent") and these overlays.
    fn frame(g: &mut AllowGuard, shown_on: bool, covers: &[(String, Rect)], at: Instant) {
        g.painted("p1", "b1", access_button(shown_on).1, BUTTON);
        g.end_frame(covers, true, at);
    }

    fn allow(g: &AllowGuard, clicks: usize, at: Instant) -> bool {
        g.may_click("p1", "b1", true, clicks, at)
    }

    #[test]
    fn allow_takes_a_click_only_after_half_a_second_unchanged() {
        let t = Instant::now();
        let mut g = AllowGuard::default();
        frame(&mut g, !ON, &[], t);
        assert!(!allow(&g, 1, t + ms(499)));
        assert!(allow(&g, 1, t + ms(500)));
        assert_eq!(ALLOW_AFTER, ms(500));
        // Frames that change nothing keep the wait it already did.
        frame(&mut g, !ON, &[], t + ms(300));
        assert!(allow(&g, 1, t + ms(500)));
    }

    #[test]
    fn a_double_click_on_stop_agent_does_not_turn_access_back_on() {
        let t = Instant::now();
        let mut g = AllowGuard::default();
        frame(&mut g, ON, &[], t);
        // First click: "Stop agent", at once.
        assert!(g.may_click("p1", "b1", false, 1, t + ms(10)));
        // The next frame paints "Allow agent" under the pointer.
        frame(&mut g, !ON, &[], t + ms(20));
        assert!(!allow(&g, 2, t + ms(120)), "the double click's second click turned access on");
        assert!(!allow(&g, 1, t + ms(120)));
        // A second click of a double click never counts, however late.
        assert!(!allow(&g, 2, t + ms(900)));
        assert!(allow(&g, 1, t + ms(520)));
    }

    #[test]
    fn an_overlay_that_closes_on_the_first_click_does_not_pass_the_second_to_allow() {
        let t = Instant::now();
        let mut g = AllowGuard::default();
        frame(&mut g, !ON, &[], t);
        // A mod overlay over the button; nothing repaints for 10 s.
        let over = vec![("popup bait".to_owned(), Rect { x: 1100.0, y: 0.0, w: 180.0, h: 120.0 })];
        frame(&mut g, !ON, &over, t + ms(1_000));
        assert!(!allow(&g, 1, t + ms(5_000)), "a covered button took a click");
        // It closes on the person's first click; the frame after shows the button.
        frame(&mut g, !ON, &[], t + ms(11_000));
        assert!(!allow(&g, 1, t + ms(11_100)));
        assert!(!allow(&g, 2, t + ms(11_100)));
        assert!(allow(&g, 1, t + ms(11_500)));
    }

    #[test]
    fn a_socket_off_just_before_the_click_does_not_land_on_allow() {
        let t = Instant::now();
        let mut g = AllowGuard::default();
        frame(&mut g, ON, &[], t);
        // The agent turns access off over the socket as the person reaches for "Stop agent".
        frame(&mut g, !ON, &[], t + ms(60_000));
        assert!(!allow(&g, 1, t + ms(60_200)));
    }

    #[test]
    fn an_inactive_window_or_a_button_off_screen_starts_the_wait_again() {
        let t = Instant::now();
        let mut g = AllowGuard::default();
        frame(&mut g, !ON, &[], t);
        g.painted("p1", "b1", true, BUTTON);
        g.end_frame(&[], false, t + ms(1_000));
        assert!(!allow(&g, 1, t + ms(5_000)), "a click into an inactive window counted");
        frame(&mut g, !ON, &[], t + ms(6_000));
        assert!(!allow(&g, 1, t + ms(6_100)));
        assert!(allow(&g, 1, t + ms(6_500)));
        // Another tab shows (no button painted), then this one again.
        g.end_frame(&[], true, t + ms(7_000));
        assert!(!allow(&g, 1, t + ms(7_100)));
        frame(&mut g, !ON, &[], t + ms(8_000));
        assert!(!allow(&g, 1, t + ms(8_100)));
    }

    #[test]
    fn an_overlay_elsewhere_changes_nothing_and_stop_is_always_at_once() {
        let t = Instant::now();
        let mut g = AllowGuard::default();
        frame(&mut g, !ON, &[], t);
        let toast = vec![("toast toast".to_owned(), Rect { x: 924.0, y: 712.0, w: 340.0, h: 72.0 })];
        frame(&mut g, !ON, &toast, t + ms(300));
        assert!(allow(&g, 1, t + ms(500)));
        // Never painted: no click counts.
        assert!(!g.may_click("p9", "b1", true, 1, t + ms(60_000)));
        // "Stop agent": any click, any time, painted or not.
        assert!(g.may_click("p9", "b9", false, 3, t));
    }

    #[test]
    fn a_button_that_moved_under_the_pointer_starts_the_wait_again() {
        let t = Instant::now();
        let mut g = AllowGuard::default();
        frame(&mut g, !ON, &[], t);
        assert!(allow(&g, 1, t + ms(10_000)));
        // The agent resizes or swaps panes: the same block's button slides 40 pt left.
        let moved = Rect { x: BUTTON.x - 40.0, ..BUTTON };
        g.painted("p1", "b1", true, moved);
        g.end_frame(&[], true, t + ms(10_000));
        assert!(!allow(&g, 1, t + ms(10_100)), "a button that just moved took a click");
        assert!(allow(&g, 1, t + ms(10_500)));
        // It moves down, then stays put: the wait runs from the last move.
        let lower = Rect { y: BUTTON.y + 30.0, ..moved };
        g.painted("p1", "b1", true, lower);
        g.end_frame(&[], true, t + ms(11_000));
        g.painted("p1", "b1", true, lower);
        g.end_frame(&[], true, t + ms(11_300));
        assert!(!allow(&g, 1, t + ms(11_400)));
        assert!(allow(&g, 1, t + ms(11_500)));
        // Layout noise under half a point is not a move.
        g.painted("p1", "b1", true, Rect { x: lower.x + 0.25, ..lower });
        g.end_frame(&[], true, t + ms(12_000));
        assert!(allow(&g, 1, t + ms(12_000)));
    }

    #[test]
    fn anything_over_any_part_of_the_agent_header_keeps_allow_from_counting() {
        let t = Instant::now();
        let mut g = AllowGuard::default();
        // p1's header bar; the button sits at its right end.
        let header = Rect { x: 640.0, y: 36.0, w: 640.0, h: 28.0 };
        let shown = |g: &mut AllowGuard, covers: &[(String, Rect)], ready: bool, at: Instant| {
            g.header("p1", header);
            g.painted("p1", "b1", true, BUTTON);
            g.end_frame(covers, ready, at);
        };
        shown(&mut g, &[], true, t);
        assert!(allow(&g, 1, t + ms(500)));
        // A popup over the header's left end: clear of the button, but over the header.
        let left = vec![("popup bait".to_owned(), Rect { x: 600.0, y: 30.0, w: 100.0, h: 40.0 })];
        assert_eq!(super::over(&header, &left), Some("popup bait"));
        assert_eq!(super::over(&BUTTON, &left), None);
        shown(&mut g, &left, true, t + ms(1_000));
        assert!(!allow(&g, 1, t + ms(5_000)), "a button whose header had something over it took a click");
        // It goes: the wait starts again.
        shown(&mut g, &[], true, t + ms(6_000));
        assert!(!allow(&g, 1, t + ms(6_100)));
        assert!(allow(&g, 1, t + ms(6_500)));
        // Edges that only touch the header are not over it.
        let below = vec![("toast toast".to_owned(), Rect { x: 640.0, y: 64.0, w: 300.0, h: 50.0 })];
        assert_eq!(super::over(&header, &below), None);
        shown(&mut g, &below, true, t + ms(7_000));
        assert!(allow(&g, 1, t + ms(7_000)));
        // A drag going on (or the window inactive): nothing counts until it ends.
        shown(&mut g, &[], false, t + ms(8_000));
        assert!(!allow(&g, 1, t + ms(9_000)));
    }

    #[test]
    fn a_block_that_now_holds_another_browser_starts_the_wait_again() {
        let t = Instant::now();
        let mut g = AllowGuard::default();
        frame(&mut g, !ON, &[], t);
        assert!(allow(&g, 1, t + ms(10_000)));
        // A layout edit puts b2 into p1, its button where b1's was.
        g.painted("p1", "b2", true, BUTTON);
        g.end_frame(&[], true, t + ms(10_000));
        assert!(!g.may_click("p1", "b2", true, 1, t + ms(10_100)), "another browser's button took a click at once");
        assert!(g.may_click("p1", "b2", true, 1, t + ms(10_500)));
        // A click handler for the browser the block no longer shows never counts.
        assert!(!allow(&g, 1, t + ms(60_000)));
    }
}

#[cfg(test)]
mod tests {
    use super::{Hand, Rect, Responder, covered, hand, keyboard_page, log_safe, may_focus, public_title, public_url};

    #[test]
    fn public_urls_have_no_userinfo_query_or_fragment() {
        assert_eq!(public_url("https://user:pw@example.test/cb?code=SECRET#frag"), "https://example.test/cb");
        assert_eq!(public_url("http://me@127.0.0.1:8765/x"), "http://127.0.0.1:8765/x");
        assert_eq!(public_url("http://127.0.0.1:8765/x.html"), "http://127.0.0.1:8765/x.html");
        assert_eq!(public_url("https://a.test/p@q?x=1"), "https://a.test/p@q");
        assert_eq!(public_url("about:blank"), "about:blank");
    }

    #[test]
    fn a_title_that_is_the_url_goes_out_as_the_url_does() {
        // Chromium's title for a page with no <title>: the url with no scheme.
        let url = "http://127.0.0.1:18766/notitle.html?code=SECRETQ2#frag";
        assert_eq!(public_title("127.0.0.1:18766/notitle.html?code=SECRETQ2", url), "127.0.0.1:18766/notitle.html");
        assert_eq!(public_title("http://127.0.0.1:18766/notitle.html?code=SECRETQ2", url), "http://127.0.0.1:18766/notitle.html");
        assert_eq!(public_title("user:pw@127.0.0.1:18766/notitle.html?code=SECRETQ2", "http://user:pw@127.0.0.1:18766/notitle.html?code=SECRETQ2"), "127.0.0.1:18766/notitle.html");
        assert_eq!(public_title("example.test/cb?code=1", "https://www.example.test/cb?code=1"), "example.test/cb");
        assert_eq!(public_title("EXAMPLE.test/cb#t=1", "https://example.test/cb#t=1"), "EXAMPLE.test/cb");
        // A title that carries the query, however it starts.
        assert_eq!(public_title("Signed in: code=SECRETQ3", "https://a.test/cb?code=SECRETQ3"), "Signed in: ");
        assert!(!public_title("x ?code=SECRETQ3", "https://a.test/cb?code=SECRETQ3").contains("SECRET"));
        // A page's own title passes, question marks and all.
        assert_eq!(public_title("What is a pane? · Docs", "https://docs.test/panes?x=1"), "What is a pane? · Docs");
        assert_eq!(public_title("Herdr fixture", "http://127.0.0.1:8765/browser-fixture.html"), "Herdr fixture");
        assert_eq!(public_title("", "about:blank"), "");
    }

    #[test]
    fn logged_text_cannot_start_a_line_of_its_own() {
        assert_eq!(log_safe("Page.navigate"), "Page.navigate");
        assert_eq!(log_safe("Evil.x\nFORGED\r\u{1b}[2J\u{2028}\u{202e}"), "Evil.x\\nFORGED\\r\\u{1b}[2J\\u{2028}\\u{202e}");
        let long = "a".repeat(500);
        assert_eq!(log_safe(&long).chars().count(), 201);
    }

    fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    /// A 1280x800 window: b1 the right half below the tab bar, b2 a side column.
    fn pages() -> Vec<(&'static str, Rect)> {
        vec![("b1", r(640.0, 80.0, 640.0, 720.0)), ("b2", r(0.0, 80.0, 300.0, 720.0))]
    }

    #[test]
    fn no_overlay_hides_no_page() {
        assert_eq!(covered(&pages(), &[]), vec![("b1", None), ("b2", None)]);
    }

    #[test]
    fn an_overlay_hides_only_the_pages_it_overlaps() {
        // The palette hangs from the top, centered: over b1's left edge, clear of b2.
        let covers = vec![("popup frame-palette".to_owned(), r(330.0, 60.0, 620.0, 500.0))];
        assert_eq!(covered(&pages(), &covers), vec![("b1", Some("popup frame-palette")), ("b2", None)]);
        // A toast in the bottom right corner: b1 only.
        let covers = vec![("toast toast".to_owned(), r(924.0, 712.0, 340.0, 72.0))];
        assert_eq!(covered(&pages(), &covers), vec![("b1", Some("toast toast")), ("b2", None)]);
    }

    #[test]
    fn a_dialogs_backdrop_hides_every_page() {
        let covers = vec![("dialog prompt".to_owned(), r(0.0, 0.0, 1280.0, 800.0))];
        assert_eq!(covered(&pages(), &covers), vec![("b1", Some("dialog prompt")), ("b2", Some("dialog prompt"))]);
    }

    #[test]
    fn the_first_overlay_painted_over_a_page_names_it() {
        let covers = vec![("float notes".to_owned(), r(700.0, 100.0, 100.0, 100.0)), ("toast toast".to_owned(), r(924.0, 712.0, 340.0, 72.0))];
        assert_eq!(covered(&pages(), &covers)[0], ("b1", Some("float notes")));
    }

    #[test]
    fn touching_edges_and_empty_overlays_hide_nothing() {
        // Ends exactly where b1 starts; a float not measured yet (0x0) inside b1.
        let covers = vec![("float side".to_owned(), r(300.0, 80.0, 340.0, 720.0)), ("float new".to_owned(), r(700.0, 100.0, 0.0, 0.0))];
        assert_eq!(covered(&pages(), &covers), vec![("b1", None), ("b2", None)]);
    }

    #[test]
    fn a_page_under_an_overlay_hands_the_keyboard_to_herdr() {
        let hidden = ["b1".to_owned()];
        // The focused pane's page is under a toast: Herdr types meanwhile.
        assert_eq!(keyboard_page(false, false, None, Some("b1"), &hidden), None);
        // A clicked layout page under an overlay doesn't pass it to the pane's page.
        assert_eq!(keyboard_page(false, false, Some("b1"), Some("b2"), &hidden), None);
        // Another page isn't affected.
        assert_eq!(keyboard_page(false, false, None, Some("b2"), &hidden), Some("b2"));
        // The overlay went: the page that still should have it gets it back.
        assert_eq!(keyboard_page(false, false, None, Some("b1"), &[]), Some("b1"));
    }

    #[test]
    fn closing_the_page_with_the_keyboard_hands_it_back_to_herdr() {
        // The page's view left the window, AppKit made the window first
        // responder: Herdr must take the keyboard back, or typing goes nowhere.
        assert_eq!(hand(None, &Responder::Other), Hand::TakeBack);
        assert_eq!(hand(None, &Responder::Page("b1".into())), Hand::TakeBack);
        assert_eq!(hand(None, &Responder::Herdr), Hand::Keep);
    }

    #[test]
    fn a_page_gets_the_keyboard_from_anyone_else() {
        assert_eq!(hand(Some("b2"), &Responder::Page("b2".into())), Hand::Keep);
        assert_eq!(hand(Some("b2"), &Responder::Page("b1".into())), Hand::Give("b2"));
        assert_eq!(hand(Some("b2"), &Responder::Herdr), Hand::Give("b2"));
        assert_eq!(hand(Some("b2"), &Responder::Other), Hand::Give("b2"));
    }

    #[test]
    fn only_the_wanted_or_pressed_page_takes_focus() {
        // CEF's creation focus on a page that shouldn't have the keyboard.
        assert!(!may_focus("b1", Some(Some("b2")), false));
        assert!(!may_focus("b1", Some(None), false));
        // The window gave it the keyboard, or the person clicked it.
        assert!(may_focus("b2", Some(Some("b2")), false));
        assert!(may_focus("b1", Some(None), true));
        // Before the window decided anything.
        assert!(may_focus("b1", None, false));
    }

    #[test]
    fn a_dialog_takes_the_keyboard_from_every_page() {
        assert_eq!(keyboard_page(true, false, Some("b5"), Some("b1"), &[]), None);
        assert_eq!(keyboard_page(true, false, None, Some("b1"), &[]), None);
    }

    #[test]
    fn a_control_the_person_pressed_keeps_the_keyboard_from_every_page() {
        // The Changes panel's commit box, a floating terminal: typing goes there.
        assert_eq!(keyboard_page(false, true, None, Some("b1"), &[]), None);
        assert_eq!(keyboard_page(false, true, Some("b5"), Some("b1"), &[]), None);
    }

    #[test]
    fn a_clicked_layout_page_wins_over_the_focused_pane() {
        assert_eq!(keyboard_page(false, false, Some("b5"), Some("b1"), &[]), Some("b5"));
        assert_eq!(keyboard_page(false, false, Some("b5"), None, &[]), Some("b5"));
    }

    #[test]
    fn otherwise_the_focused_panes_page_or_herdr() {
        assert_eq!(keyboard_page(false, false, None, Some("b1"), &[]), Some("b1"));
        assert_eq!(keyboard_page(false, false, None, None, &[]), None);
    }
}
