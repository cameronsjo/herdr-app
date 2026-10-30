//! Herdr's own keys while a page has the keyboard. A native page view sits
//! before GPUI in AppKit's responder chain, so keys.json never sees a key
//! pressed in a page; an `NSEvent` local monitor (view.rs) asks `find` first
//! and runs the bound action instead. Pure: no AppKit here.

use gpui::Keystroke;

/// A key press as AppKit reports it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Press {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub cmd: bool,
    /// The key without modifiers, lowercase (`]`, `t`), or GPUI's name for
    /// a named key (`escape`, `left`, `tab`, `f2`).
    pub key: String,
    /// What the key types with shift (`}` for shift-]), for bindings written
    /// with the shifted character (`cmd-+`).
    pub shifted: Option<String>,
}

/// The action bound to `press`, if any. `bindings`: (keys, action) from the
/// sections that apply everywhere (`global`, the shown `layout:<name>`, the
/// escape hatch). Sequences (`cmd-k cmd-s`) are skipped: a page keeps keys
/// one at a time.
pub fn find<'a>(bindings: &'a [(String, String)], press: &Press) -> Option<&'a str> {
    bindings.iter().find(|(keys, _)| matches(keys, press)).map(|(_, action)| action.as_str())
}

fn matches(keys: &str, press: &Press) -> bool {
    if keys.contains(' ') {
        return false;
    }
    let Ok(k) = Keystroke::parse(keys) else { return false };
    let m = k.modifiers;
    if m.function || m.control != press.ctrl || m.alt != press.alt || m.platform != press.cmd {
        return false;
    }
    let key = k.key.to_lowercase();
    if m.shift == press.shift && key == press.key {
        return true;
    }
    // `cmd-+` is pressed as cmd-shift-=: shift made another character (not
    // just a capital letter, which would make cmd-shift-t match cmd-t).
    let shifted = press.shifted.as_deref().map(str::to_lowercase).filter(|s| *s != press.key);
    !m.shift && press.shift && shifted.is_some_and(|s| s == key)
}

/// GPUI's name for a key AppKit reports by its virtual key code, for keys
/// that type no character worth matching.
pub fn named(key_code: u16) -> Option<&'static str> {
    Some(match key_code {
        36 | 76 => "enter",
        48 => "tab",
        49 => "space",
        51 => "backspace",
        53 => "escape",
        115 => "home",
        116 => "pageup",
        117 => "delete",
        119 => "end",
        121 => "pagedown",
        123 => "left",
        124 => "right",
        125 => "down",
        126 => "up",
        122 => "f1",
        120 => "f2",
        99 => "f3",
        118 => "f4",
        96 => "f5",
        97 => "f6",
        98 => "f7",
        100 => "f8",
        101 => "f9",
        109 => "f10",
        103 => "f11",
        111 => "f12",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bindings() -> Vec<(String, String)> {
        [
            ("cmd-t", "tab.new"),
            ("cmd-shift-]", "tab.next"),
            ("cmd-alt-left", "pane.go:left"),
            ("ctrl-1", "space.select:1"),
            ("cmd-+", "app.zoom_in"),
            ("f2", "tab.rename"),
            ("cmd-k cmd-s", "app.help"),
            ("ctrl-alt-shift-escape", "app.reset"),
        ]
        .into_iter()
        .map(|(k, a)| (k.to_owned(), a.to_owned()))
        .collect()
    }

    fn press(mods: &str, key: &str, shifted: Option<&str>) -> Press {
        Press {
            ctrl: mods.contains('c'),
            alt: mods.contains('a'),
            shift: mods.contains('s'),
            cmd: mods.contains('m'),
            key: key.into(),
            shifted: shifted.map(Into::into),
        }
    }

    #[test]
    fn bound_global_keys_are_found_with_their_exact_modifiers() {
        let b = bindings();
        assert_eq!(find(&b, &press("m", "t", Some("t"))), Some("tab.new"));
        assert_eq!(find(&b, &press("ms", "]", Some("}"))), Some("tab.next"));
        assert_eq!(find(&b, &press("ma", "left", None)), Some("pane.go:left"));
        assert_eq!(find(&b, &press("c", "1", Some("1"))), Some("space.select:1"));
        assert_eq!(find(&b, &press("", "f2", None)), Some("tab.rename"));
        // The escape hatch works with a page focused.
        assert_eq!(find(&b, &press("cas", "escape", None)), Some("app.reset"));
    }

    #[test]
    fn a_shifted_character_binding_matches_its_shifted_press() {
        assert_eq!(find(&bindings(), &press("ms", "=", Some("+"))), Some("app.zoom_in"));
    }

    #[test]
    fn page_keys_stay_with_the_page() {
        let b = bindings();
        // Typing, copy and paste, and near misses go to the page.
        assert_eq!(find(&b, &press("", "t", Some("t"))), None);
        assert_eq!(find(&b, &press("m", "c", Some("c"))), None);
        assert_eq!(find(&b, &press("ms", "t", Some("T"))), None);
        assert_eq!(find(&b, &press("m", "]", Some("]"))), None);
        assert_eq!(find(&b, &press("cs", "escape", None)), None);
        // Sequences are never matched from a page.
        assert_eq!(find(&b, &press("m", "k", Some("k"))), None);
    }

    #[test]
    fn named_keys_use_gpuis_names() {
        assert_eq!(named(53), Some("escape"));
        assert_eq!(named(123), Some("left"));
        assert_eq!(named(120), Some("f2"));
        assert_eq!(named(0), None);
    }
}
