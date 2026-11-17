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
    fn a_background_copy_loads_and_gets_its_commands() {
        // The window's mod tests share one app home: each runs alone in a process of its own.
        let home = crate::test_tmp().join(format!("herdr-fork-mods-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "window::fork_mods::tests::background_copy", "--nocapture", "--test-threads=1"])
            .env("HERDR_APP_HOME", &home)
            .env("HERDR_TEST_WINDOW", "fork_background_copy")
            .output()
            .unwrap();
        let _ = std::fs::remove_dir_all(&home);
        let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success() && log.contains("1 passed"), "{log}");
    }

    /// A background mod's folder with a probe in it: the window loads it with no
    /// layout block, counts it as shown, and `<mod>.<command>` reaches it.
    #[gpui::test]
    fn background_copy(cx: &mut gpui::TestAppContext) {
        if std::env::var("HERDR_TEST_WINDOW").as_deref() != Ok("fork_background_copy") {
            return;
        }
        ON.set(true);
        let probe = "import { View, div } from \"gpui-kit\";\n\
            import { nextCommand, setShared } from \"herdr\";\n\
            export default class Probe extends View {\n\
              init(_props, cx) { setShared(\"fork.init\", true); cx.spawn(async () => { for (;;) setShared(\"fork.probe\", await nextCommand()); }); }\n\
              render() { return div(); }\n\
            }\n";
        let name = BACKGROUND[0];
        let layout = serde_json::json!({ "id": "root", "type": "block", "content": { "mod": "probe" } });
        let (view, cx) = crate::window::tests::open_window(cx, "probe", "import { View, div } from \"gpui-kit\"; export default class P extends View { render() { return div(); } }", layout);
        cx.run_until_parked();
        // Start-up installed the shipped mod; swap in the probe the way an edit lands.
        let main = crate::mods::app_dir().join("mods").join(name).join("main.js");
        assert!(main.is_file(), "start-up installs the shipped background mod");
        std::fs::write(&main, probe).unwrap();
        cx.update(|window, cx| view.update(cx, |this, cx| this.files_changed(vec![format!("mods/{name}/main.js").into()], window, cx)));
        cx.executor().advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
        let slot = format!("fork-background:{name}");
        let loaded = view.update(cx, |this, _| this.copies.get(&slot).map(|c| (c.root.is_some(), c.error.clone())));
        assert_eq!(loaded, Some((true, None)), "the background copy loads with no block");
        assert_eq!(crate::mods::shared("fork.init"), Some(serde_json::json!(true)), "its init() runs without a render");
        cx.update(|window, cx| view.update(cx, |this, cx| this.run(&format!("{name}.open:/tmp/x"), serde_json::json!({}), window, cx)));
        cx.run_until_parked();
        assert_eq!(crate::mods::shared("fork.probe"), Some(serde_json::json!("open:/tmp/x")), "its command arrives, payload and all");
    }

    #[test]
    fn background_slots_name_only_installed_mods() {
        ON.set(true);
        let slots = background(&BTreeMap::new());
        assert!(slots.is_empty(), "a mod with no folder gets no background copy");
    }
}
