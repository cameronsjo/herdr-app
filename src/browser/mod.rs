//! Browser panes: a Chromium page in a block (`{ "browser": "b1", "url": … }`),
//! as a pane in a tab or a block in the shown layout.
//!
//! macOS builds with the `browser` feature run CEF in windowed mode: each page
//! is a native child view over GPUI's Metal view, kept on its block's bounds
//! every frame (native.rs, view.rs, runtime.rs). Every other build has the
//! same API with no browser behind it: blocks show a placeholder and the
//! commands say what's missing (off.rs). The window only talks to `Browsers`.

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
pub mod keys;
#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
pub mod registry;

#[cfg(all(target_os = "macos", feature = "browser"))]
mod native;
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

/// Whether browser panes work in this build.
pub const AVAILABLE: bool = cfg!(all(target_os = "macos", feature = "browser"));

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
