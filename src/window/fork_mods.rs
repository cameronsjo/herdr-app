//! FORK(background-mods): fork mods that run without a block in the layout.
//!
//! A mod copy normally lives only while a layout block, pane, float or overlay
//! names it, and `<mod>.<command>` reaches only shown copies. The fork's ports of
//! TUI features (cameronsjo/herdr-app#9, #13, ...) are commands and watchers with
//! no screen of their own, and the shipped layouts are upstream's files. So the
//! window loads one copy of each mod in [`BACKGROUND`] on every layout, counts it
//! as shown, and never draws it. Its `init()` runs at load, its `cx.spawn` and
//! `cx.timer` tasks run, `nextCommand()` gets its commands; `render()` is not
//! called (live data changes don't wake it: poll with `cx.timer` if it watches).
//!
//! `src/window.rs` adds them where `sync()` loads copies, where
//! `publish_shown_copies()` names the shown ones, and in `owned_mods()` (one
//! `FORK(background-mods)` line each). Not in `visible()`: telemetry reads that,
//! and these mods are never on screen.

use super::*;

/// The shipped fork mods that run in the background (`defaults/mods/<name>/`).
pub(super) const BACKGROUND: &[&str] = &["open-folder", "space-groups"];

/// Where a background copy lives in `AppWindow::copies`. No layout node has
/// this id: layout ids are the layout file's, and they never hold a colon here.
const SLOT: &str = "fork-background:";

/// The background copies to keep: one per [`BACKGROUND`] mod whose folder is
/// installed now (Herdr restores a deleted shipped mod at its next start).
pub(super) fn background(folders: &BTreeMap<String, Found>) -> Vec<(String, Instance)> {
    // Upstream's window tests count the copies they load; background copies stay out of them.
    #[cfg(test)]
    if !tests::ON.get() {
        return Vec::new();
    }
    BACKGROUND
        .iter()
        .filter(|name| folders.contains_key(**name))
        .map(|name| (format!("{SLOT}{name}"), Instance { name: (*name).to_owned(), props: json!({ "background": true }) }))
        .collect()
}

/// `shown` with the background copies added.
pub(super) fn with_background(mut shown: Vec<(String, Instance)>, folders: &BTreeMap<String, Found>) -> Vec<(String, Instance)> {
    shown.extend(background(folders));
    shown
}

#[cfg(test)]
pub(super) mod tests {
    use super::{BACKGROUND, background};
    use std::collections::BTreeMap;

    thread_local! {
        /// Off in every test but these, so upstream's copy counts hold.
        pub(in crate::window) static ON: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    #[test]
    fn every_background_mod_ships() {
        for name in BACKGROUND {
            assert!(
                crate::mods::DEFAULT_MODS.iter().any(|(p, _)| p.split('/').next() == Some(*name)),
                "{name} is listed in BACKGROUND but not shipped in defaults/mods"
            );
        }
    }

    #[test]
    fn background_slots_name_only_installed_mods() {
        ON.set(true);
        let slots = background(&BTreeMap::new());
        assert!(slots.is_empty(), "a mod with no folder gets no background copy");
    }
}
