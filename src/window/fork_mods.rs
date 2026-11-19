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
pub(super) const BACKGROUND: &[&str] = &["open-folder", "space-groups", "name-sync", "outside-agents"];

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

/// A background copy's slot: [`AppWindow::drop_layout_copies`] keeps these, so a
/// layout switch doesn't restart them (an open folder picker or a move in
/// progress would be lost).
pub(super) fn is_background(slot: &str) -> bool {
    slot.starts_with(SLOT)
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

    /// Runs `window::fork_mods::tests::<name>` alone in a test process of its own,
    /// on a fresh app home. The fork's copy of upstream's `window::tests::in_own_process`
    /// (which names its own module's path, and whose signature the fork doesn't widen);
    /// `fork_browser` uses this one too. At a sync, carry any hardening of upstream's
    /// helper over here.
    pub(in crate::window) fn in_own_process(path: &str, name: &str) {
        let home = crate::test_tmp().join(format!("herdr-fork-window-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &format!("{path}::{name}"), "--nocapture", "--test-threads=1"])
            .env("HERDR_APP_HOME", &home)
            .env("HERDR_TEST_WINDOW", name)
            .output()
            .unwrap();
        let _ = std::fs::remove_dir_all(&home);
        let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success() && log.contains("1 passed"), "{log}");
    }

    /// In the process [`in_own_process`] started for `name`: whether to go on.
    pub(in crate::window) fn running_alone(name: &str) -> bool {
        std::env::var("HERDR_TEST_WINDOW").as_deref() == Ok(name)
    }

    #[test]
    fn a_background_copy_loads_and_gets_its_commands() {
        // The window's mod tests share one app home: each runs alone in a process of its own.
        in_own_process("window::fork_mods::tests", "background_copy");
    }

    /// Every background mod's folder with a probe in it: the window loads each with
    /// no layout block, counts it as shown, never renders it, `<mod>.<command>`
    /// reaches it, and a layout switch keeps it running instead of restarting it.
    #[gpui::test]
    fn background_copy(cx: &mut gpui::TestAppContext) {
        if !running_alone("background_copy") {
            return;
        }
        ON.set(true);
        let layout = serde_json::json!({ "id": "root", "type": "block", "content": { "mod": "probe" } });
        let (view, cx) = crate::window::tests::open_window(cx, "probe", "import { View, div } from \"gpui-kit\"; export default class P extends View { render() { return div(); } }", layout.clone());
        cx.run_until_parked();
        for name in BACKGROUND {
            // Start-up installed the shipped mod; swap in the probe the way an edit lands.
            let probe = format!(
                "import {{ View, div }} from \"gpui-kit\";\n\
                 import {{ nextCommand, setShared, shared }} from \"herdr\";\n\
                 export default class Probe extends View {{\n\
                   init(_props, cx) {{ setShared(\"fork.init.{name}\", (shared(\"fork.init.{name}\") ?? 0) + 1); cx.spawn(async () => {{ for (;;) setShared(\"fork.probe.{name}\", await nextCommand()); }}); }}\n\
                   render() {{ setShared(\"fork.render.{name}\", true); return div(); }}\n\
                 }}\n"
            );
            let main = crate::mods::app_dir().join("mods").join(name).join("main.js");
            assert!(main.is_file(), "start-up installs the shipped background mod {name}");
            std::fs::write(&main, probe).unwrap();
            cx.update(|window, cx| view.update(cx, |this, cx| this.files_changed(vec![format!("mods/{name}/main.js").into()], window, cx)));
        }
        // Under outside-agents' two-second start: its file reads run on real threads,
        // which the test scheduler refuses.
        cx.executor().advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
        for name in BACKGROUND {
            let slot = format!("fork-background:{name}");
            let loaded = view.update(cx, |this, _| this.copies.get(&slot).map(|c| (c.root.is_some(), c.error.clone())));
            assert_eq!(loaded, Some((true, None)), "{name}: the background copy loads with no block");
            assert_eq!(crate::mods::shared(&format!("fork.init.{name}")), Some(serde_json::json!(1)), "{name}: its init() runs without a render");
            cx.update(|window, cx| view.update(cx, |this, cx| this.run(&format!("{name}.open:/tmp/x"), serde_json::json!({}), window, cx)));
            cx.run_until_parked();
            assert_eq!(crate::mods::shared(&format!("fork.probe.{name}")), Some(serde_json::json!("open:/tmp/x")), "{name}: its command arrives, payload and all");
        }
        // Another layout: the background copies keep running, not restarted.
        std::fs::write(crate::window::layout_path("other"), layout.to_string()).unwrap();
        cx.update(|window, cx| view.update(cx, |this, cx| this.run("layout.switch", serde_json::json!({ "name": "other" }), window, cx)));
        cx.executor().advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
        for name in BACKGROUND {
            assert_eq!(crate::mods::shared(&format!("fork.init.{name}")), Some(serde_json::json!(1)), "{name}: a layout switch doesn't restart it");
            assert_eq!(crate::mods::shared(&format!("fork.render.{name}")), None, "{name}: render() is never called");
        }
    }

    #[test]
    fn background_slots_name_only_installed_mods() {
        ON.set(true);
        let slots = background(&BTreeMap::new());
        assert!(slots.is_empty(), "a mod with no folder gets no background copy");
    }
}
