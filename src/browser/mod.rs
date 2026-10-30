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
            Source::DevSwitch => "socket (dev switch, dev-agent-access build)",
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
/// overlay over it and the window active, before a click on it counts.
/// Chromium waits the same 500 ms before its permission prompts take a click
/// (`InputEventActivationProtector`).
pub const ALLOW_AFTER: std::time::Duration = std::time::Duration::from_millis(500);

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
/// Input protection for the access buttons, as Chromium's
/// `InputEventActivationProtector` does for its prompts. "Allow agent" takes
/// a click only after `ALLOW_AFTER` on screen unchanged (same label, no
/// overlay over it, window active), and never a second click of a double
/// click. So neither a double click on "Stop agent" nor a click aimed at
/// something that changed under the pointer turns access on: an overlay that
/// closed on the first click, a socket `off` just before the person clicked
/// "Stop agent", or a window activated by the click. "Stop agent" takes
/// every click at once. Keyed by the block (node) the button is in.
#[derive(Default)]
pub struct AllowGuard {
    shown: std::collections::HashMap<String, Shown>,
    /// The buttons painted this frame: node, what a click sets, bounds.
    painted: Vec<(String, bool, Rect)>,
}

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
#[derive(Clone, Copy, Debug)]
struct Shown {
    sets: bool,
    /// Under an overlay, or the window inactive, this frame.
    covered: bool,
    /// When it started to show as it does now.
    since: std::time::Instant,
}

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
impl AllowGuard {
    /// A button painted this frame (window points).
    pub fn painted(&mut self, node: &str, sets: bool, at: Rect) {
        self.painted.push((node.to_owned(), sets, at));
    }

    /// The frame's end, with the overlays painted in it. A button that
    /// changed what it does, is covered, has just come out from under a
    /// cover, or first shows now, starts its wait again. One not painted in
    /// this frame is forgotten (it starts again when it comes back).
    pub fn end_frame(&mut self, covers: &[(String, Rect)], window_active: bool, now: std::time::Instant) {
        let mut shown = std::collections::HashMap::new();
        for (node, sets, at) in self.painted.drain(..) {
            let covered = !window_active || covers.iter().any(|(_, c)| c.overlaps(&at));
            let since = match self.shown.get(&node) {
                Some(s) if s.sets == sets && !s.covered && !covered => s.since,
                _ => now,
            };
            shown.insert(node, Shown { sets, covered, since });
        }
        self.shown = shown;
    }

    /// Whether a click on `node`'s button, which sets `sets`, counts.
    pub fn may_click(&self, node: &str, sets: bool, click_count: usize, now: std::time::Instant) -> bool {
        if !sets {
            return true;
        }
        click_count <= 1 && self.shown.get(node).is_some_and(|s| s.sets && !s.covered && now.saturating_duration_since(s.since) >= ALLOW_AFTER)
    }
}

#[cfg(all(debug_assertions, feature = "dev-agent-access"))]
static DEV_SWITCH:std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What every agent header and the window's agent strip say while the dev
/// switch is on: the person's click is no longer the only way in.
pub const DEV_SWITCH_NOTE: &str = "DEV SWITCH: the control socket can turn agent access on";

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

/// The url without its query and fragment, wherever a url leaves the app
/// for a browser agents can't drive (`herdr-app ls`, mods' `browsers()`, the
/// status of a block whose browser isn't open): OAuth codes and magic links
/// stay out of reach. The tab document keeps the whole url (restore needs it).
pub fn without_query(url: &str) -> String {
    url.split(['?', '#']).next().unwrap_or_default().to_owned()
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
        g.painted("p1", access_button(shown_on).1, BUTTON);
        g.end_frame(covers, true, at);
    }

    fn allow(g: &AllowGuard, clicks: usize, at: Instant) -> bool {
        g.may_click("p1", true, clicks, at)
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
        assert!(g.may_click("p1", false, 1, t + ms(10)));
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
        g.painted("p1", true, BUTTON);
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
        assert!(!g.may_click("p9", true, 1, t + ms(60_000)));
        // "Stop agent": any click, any time, painted or not.
        assert!(g.may_click("p9", false, 3, t));
    }
}

#[cfg(test)]
mod tests {
    use super::{Hand, Rect, Responder, covered, hand, keyboard_page, may_focus};

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
