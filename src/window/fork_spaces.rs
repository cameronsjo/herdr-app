//! FORK(spaces): runs `tab.move_to_space` and `space.merge`
//! (`crate::fork::spaces`, cameronsjo/herdr-app#15). `src/window.rs` carries
//! only one-line `FORK(spaces)` call sites.

use super::*;
use crate::fork::spaces;

impl AppWindow {
    pub(super) fn fork_spaces(
        &mut self,
        name: &str,
        a: &Value,
        active_ctx: Option<String>,
        cx: &mut Context<Self>,
    ) {
        // The space the tabs leave, the tab that moves (None: all of them) and where to.
        let (source, tab, into) = match name {
            "tab.move_to_space" => {
                let Some(id) = Self::owned(a, "id").or(active_ctx) else {
                    return self.fail("no tab to move");
                };
                let Some(space) = self.model.space_of(&id).filter(|s| s.tab(&id).is_some()) else {
                    return self.fail(format!("no tab {id:?}"));
                };
                let Some(into) = Self::owned(a, "space") else {
                    return self.fail("tab.move_to_space needs space");
                };
                (space.clone(), Some(id), into)
            }
            _ => {
                let Some(id) = Self::owned(a, "id")
                    .or_else(|| self.model.active_space().map(|s| s.id.clone()))
                else {
                    return self.fail("no space to merge");
                };
                let Some(space) = self.model.space(&id) else {
                    return self.fail(format!("no space {id:?}"));
                };
                let Some(into) = Self::owned(a, "into") else {
                    return self.fail("space.merge needs into");
                };
                (space.clone(), None, into)
            }
        };
        if let Err(error) = self.require_document(source.machine.as_deref()) {
            return self.fail(error);
        }
        // The moving tabs' terminals, and the ones labeled for those tabs that
        // have no pane yet (started by a script, or waiting for their layout):
        // all of them get the new space's label.
        let moving: Vec<_> = source
            .tabs
            .iter()
            .filter(|c| tab.as_ref().is_none_or(|t| c.id == *t))
            .collect();
        let ids: Vec<String> = moving.iter().map(|c| c.id.clone()).collect();
        // A `terminal.keep` still in flight lands in its tab, labeled for the
        // space it named: a moving tab would take it to a space its label
        // doesn't name, and a space this empties would make it park the
        // terminal instead. Wait for it.
        let empties = tab.is_none() || source.tabs.len() == 1;
        if self.pending_terminals.values().any(|p| {
            matches!(&p.change, TerminalChange::Keep { space, tab, .. } if ids.contains(tab) || (empties && *space == source.id))
        }) {
            return self.fail(
                "a terminal is still being given a pane here; try again in a moment",
            );
        }
        let mut terminals: Vec<String> = moving
            .iter()
            .flat_map(|c| model::addresses(&c.root))
            .collect();
        terminals.extend(self.labeled_for(&ids));
        // A terminal getting or losing its pane, or whose last move is still being
        // labeled, is labeled for its old place when that ends: wait for it.
        if terminals
            .iter()
            .any(|t| self.changing_terminals.contains(t))
        {
            return self.fail(
                "a terminal here is still getting or losing its pane; try again in a moment",
            );
        }
        let moved = match &tab {
            Some(tab) => spaces::move_tab(
                &mut self.model,
                tab,
                &into,
                a.get("index").and_then(Value::as_u64).map(|i| i as usize),
            ),
            None => spaces::merge(&mut self.model, &source.id, &into),
        };
        match moved {
            Ok(m) => {
                self.relabel_space(terminals, &into, cx);
                self.answer(Ok(
                    json!({ "ok": true, "space": into, "closed": m.closed.then_some(m.from) }),
                ));
            }
            Err(e) => self.fail(e),
        }
    }

    /// Live terminals with no pane whose `herdr.tab` label names one of `tabs`.
    fn labeled_for(&self, tabs: &[String]) -> Vec<String> {
        self.live
            .iter()
            .filter(|(a, _)| self.model.pane_of(a).is_none())
            .filter(|(a, r)| {
                let machine = a.split_once('/').map(|(m, _)| m);
                let label = r.pointer("/labels/herdr.tab").and_then(Value::as_str);
                tabs.iter()
                    .any(|t| label == Some(crate::mirror::local(t, machine)))
            })
            .map(|(a, _)| a.clone())
            .collect()
    }

    /// Points the moved terminals' `herdr.space` label at their new space, so a
    /// terminal that loses its pane (a lost layout) is put back there, not in the
    /// old space. The tab keeps its id, so `herdr.tab` stays. Each terminal counts
    /// as changing until its label is written, as for `terminal.park`: no park,
    /// move or adoption overtakes it. A failed write is logged.
    fn relabel_space(&mut self, terminals: Vec<String>, space: &str, cx: &mut Context<Self>) {
        for addr in terminals {
            let machine = addr.split_once('/').map(|(m, _)| m);
            let params = json!({ "terminal": addr, "labels": { "herdr.space": crate::mirror::local(space, machine) } });
            let core = self.core.clone();
            self.changing_terminals.insert(addr.clone());
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { core.call("terminal.label", params) })
                    .await;
                if let Err(e) = result {
                    tracing::warn!("herdr.space label for {addr}: {e:#}");
                }
                let _ = this.update(cx, |this, _| this.changing_terminals.remove(&addr));
            })
            .detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    /// Runs the window test `name` alone in a process of its own: the
    /// window's tests share one app home.
    fn alone(name: &str) {
        let home =
            crate::test_tmp().join(format!("herdr-fork-spaces-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("window::fork_spaces::tests::{name}"),
                "--nocapture",
                "--test-threads=1",
            ])
            .env("HERDR_APP_HOME", &home)
            .env("HERDR_TEST_WINDOW", format!("fork_{name}"))
            .output()
            .unwrap();
        let _ = std::fs::remove_dir_all(&home);
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.status.success() && log.contains("1 passed"), "{log}");
    }

    #[test]
    fn tabs_move_between_spaces_through_the_commands() {
        alone("move_and_merge");
    }

    #[test]
    fn a_failed_label_write_still_puts_the_terminal_back_with_its_tab() {
        alone("failed_label");
    }

    #[test]
    fn a_move_waits_for_a_keep_in_flight() {
        alone("keep_in_flight");
    }

    /// A window with spaces s1 (tabs t1, t2) and s2 (t3), each tab's
    /// terminal live, t1 shown.
    fn two_spaces(
        cx: &mut gpui::TestAppContext,
    ) -> (gpui::Entity<super::AppWindow>, &mut gpui::VisualTestContext) {
        let (view, cx) = crate::window::tests::open_window(
            cx,
            "unused",
            "",
            json!({
                "id": "context", "type": "block", "content": { "tab": "active" }
            }),
        );
        view.update_in(cx, |this, window, cx| {
            this.model
                .add_space("s1", "one", "/", super::Tab::new("t1", "", "a", "term-a"));
            this.model
                .add_tab_unselected("s1", super::Tab::new("t2", "", "b", "term-b"));
            this.model
                .add_space("s2", "two", "/", super::Tab::new("t3", "", "c", "term-c"));
            for t in ["term-a", "term-b", "term-c"] {
                this.live.insert(t.into(), json!({ "state": "live" }));
            }
            this.model.select_tab("t1");
            this.documents.entry(String::new()).or_default().available = true;
            this.sync(window, cx);
        });
        (view, cx)
    }

    #[gpui::test]
    fn failed_label(cx: &mut gpui::TestAppContext) {
        if std::env::var("HERDR_TEST_WINDOW").as_deref() != Ok("fork_failed_label") {
            return;
        }
        let (view, cx) = two_spaces(cx);
        // term-e has no pane, labeled for t2 in s1. The core refuses every
        // label write, so after the moves its labels still name s1.
        let record =
            json!({ "state": "live", "labels": { "herdr.space": "s1", "herdr.tab": "t2" } });
        let reply = record.clone();
        let core = crate::hc::scripted(move |method, _| match method {
            "terminal.label" => Err(json!({ "code": -32000, "message": "refused" })),
            "terminal.get" => Ok(reply.clone()),
            _ => Ok(json!({})),
        });
        view.update_in(cx, |this, window, cx| {
            this.core = core;
            for (tab, into) in [("t2", "s2"), ("t1", "s2")] {
                let out = this.run_as(
                    "tab.move_to_space",
                    json!({ "id": tab, "space": into }),
                    false,
                    window,
                    cx,
                );
                assert!(out.get("error").is_none(), "{out}");
            }
            assert!(this.model.space("s1").is_none());
        });
        cx.run_until_parked();
        view.update_in(cx, |this, window, cx| {
            assert!(this.changing_terminals.is_empty());
            // It shows up now (a core event, a reconnect): the window adopts it.
            this.live.insert("term-e".into(), record);
            this.reconcile(window, cx);
            assert!(this.changing_terminals.contains("term-e"));
        });
        cx.run_until_parked();
        view.update_in(cx, |this, _, _| {
            // With t2 in s2, not in a new copy of s1.
            assert!(this.model.space("s1").is_none());
            let pane = this.model.pane_of("term-e").expect("term-e has a pane");
            assert_eq!(
                this.model.space_of(&pane).map(|s| s.id.as_str()),
                Some("s2")
            );
            assert_eq!(this.model.tab_of(&pane).map(|c| c.id.as_str()), Some("t2"));
            assert_eq!(this.model.tab("t2").map(|c| c.root.panes().len()), Some(2));
        });
    }

    #[gpui::test]
    fn keep_in_flight(cx: &mut gpui::TestAppContext) {
        if std::env::var("HERDR_TEST_WINDOW").as_deref() != Ok("fork_keep_in_flight") {
            return;
        }
        let (view, cx) = two_spaces(cx);
        view.update_in(cx, |this, window, cx| {
            let pending = |space: &str, tab: &str| super::PendingTerminalChange {
                change: super::TerminalChange::Keep {
                    space: space.into(),
                    tab: tab.into(),
                    pane: "p9".into(),
                    name: "x".into(),
                    target: None,
                    axis: super::Axis::Row,
                },
                reply: std::sync::mpsc::channel().1,
            };
            let try_move = |this: &mut super::AppWindow,
                            name: &str,
                            args: serde_json::Value,
                            window: &mut gpui::Window,
                            cx: &mut gpui::Context<super::AppWindow>| {
                this.run_as(name, args, false, window, cx)
            };
            // A keep landing in t2: t2 can't move, s1 can't merge away.
            this.pending_terminals
                .insert("term-k".into(), pending("s1", "t2"));
            for (name, args) in [
                ("tab.move_to_space", json!({ "id": "t2", "space": "s2" })),
                ("space.merge", json!({ "id": "s1", "into": "s2" })),
            ] {
                let out = try_move(this, name, args.clone(), window, cx);
                assert!(out.get("error").is_some(), "{name} {args}: {out}");
            }
            // Another tab of s1 moves, leaving s1 there for the keep.
            let out = try_move(
                this,
                "tab.move_to_space",
                json!({ "id": "t1", "space": "s2" }),
                window,
                cx,
            );
            assert!(out.get("error").is_none(), "{out}");
            // A keep making a new tab in s1: s1's last tab can't take s1 away.
            this.pending_terminals.clear();
            this.pending_terminals
                .insert("term-k".into(), pending("s1", "tb-new"));
            let out = try_move(
                this,
                "tab.move_to_space",
                json!({ "id": "t2", "space": "s2" }),
                window,
                cx,
            );
            assert!(out.get("error").is_some(), "{out}");
            assert_eq!(this.model.space("s1").map(|s| s.tabs.len()), Some(1));
            // Once it lands, the move goes ahead.
            this.pending_terminals.clear();
            let out = try_move(
                this,
                "tab.move_to_space",
                json!({ "id": "t2", "space": "s2" }),
                window,
                cx,
            );
            assert_eq!(out["closed"], json!("s1"), "{out}");
        });
    }

    #[gpui::test]
    fn move_and_merge(cx: &mut gpui::TestAppContext) {
        if std::env::var("HERDR_TEST_WINDOW").as_deref() != Ok("fork_move_and_merge") {
            return;
        }
        let (view, cx) = crate::window::tests::open_window(
            cx,
            "unused",
            "",
            json!({
                "id": "context", "type": "block", "content": { "tab": "active" }
            }),
        );
        let tabs = |this: &super::AppWindow, space: &str| {
            this.model
                .space(space)
                .map(|s| s.tabs.iter().map(|c| c.id.clone()).collect::<Vec<_>>())
        };
        view.update_in(cx, |this, window, cx| {
            this.model
                .add_space("s1", "one", "/", super::Tab::new("t1", "", "a", "term-a"));
            this.model
                .add_tab_unselected("s1", super::Tab::new("t2", "", "b", "term-b"));
            this.model
                .add_space("s2", "two", "/", super::Tab::new("t3", "", "c", "term-c"));
            this.model
                .add_space("s3", "three", "/", super::Tab::new("t4", "", "d", "term-d"));
            // term-e has no pane yet but is labeled for t2: it follows t2's moves.
            for t in ["term-a", "term-b", "term-c", "term-d", "term-e"] {
                this.live.insert(t.into(), json!({ "state": "live" }));
            }
            this.live.insert(
                "term-e".into(),
                json!({ "state": "live", "labels": { "herdr.space": "s1", "herdr.tab": "t2" } }),
            );
            this.model.select_tab("t1");
            this.documents.entry(String::new()).or_default().available = true;
            this.sync(window, cx);
            // The shown tab, by default, to the front of s2: it stays shown, and the
            // window shows it there (the `where` label follows).
            let out = this.run_as(
                "tab.move_to_space",
                json!({ "space": "s2", "index": 1 }),
                false,
                window,
                cx,
            );
            assert_eq!(out, json!({ "ok": true, "space": "s2", "closed": null }));
            assert_eq!(tabs(this, "s1"), Some(vec!["t2".into()]));
            assert_eq!(tabs(this, "s2"), Some(vec!["t1".into(), "t3".into()]));
            assert_eq!(
                this.model.active_tab().map(|c| c.id.clone()),
                Some("t1".into())
            );
            assert_eq!(
                this.written_where.get("term-a").map(String::as_str),
                Some("two / 1")
            );
            // Its label is still being written: a second move of it waits.
            assert!(this.changing_terminals.contains("term-a"));
            let out = this.run_as(
                "tab.move_to_space",
                json!({ "id": "t1", "space": "s3" }),
                false,
                window,
                cx,
            );
            assert!(out.get("error").is_some(), "{out}");
            // The last tab of s1 takes s1 with it; term-e, labeled for t2, is relabeled too.
            let out = this.run_as(
                "tab.move_to_space",
                json!({ "id": "t2", "space": "s3" }),
                false,
                window,
                cx,
            );
            assert_eq!(out["closed"], json!("s1"));
            assert_eq!(tabs(this, "s1"), None);
            assert_eq!(tabs(this, "s3"), Some(vec!["t4".into(), "t2".into()]));
            assert!(this.changing_terminals.contains("term-e"));
        });
        cx.run_until_parked();
        view.update_in(cx, |this, window, cx| {
            // The labels are written (here the offline core refuses; nothing waits).
            assert!(this.changing_terminals.is_empty());
            // Even when a label write failed (as here), a terminal still labeled for
            // t2 in the closed s1 joins t2 where it is, not a new copy of s1.
            this.adopt("term-e", "s1", Some("t2"));
            assert_eq!(tabs(this, "s1"), None);
            assert_eq!(this.model.tab("t2").map(|c| c.root.panes().len()), Some(2));
            let out = this.run_as(
                "space.merge",
                json!({ "id": "s2", "into": "s3" }),
                false,
                window,
                cx,
            );
            assert_eq!(out["closed"], json!("s2"), "{out}");
            assert_eq!(
                tabs(this, "s3"),
                Some(vec!["t4".into(), "t2".into(), "t1".into(), "t3".into()])
            );
            assert_eq!(
                this.model.active_tab().map(|c| c.id.clone()),
                Some("t1".into())
            );
            for (name, args) in [
                ("tab.move_to_space", json!({ "space": "nope" })),
                ("space.merge", json!({ "into": "s3" })),
                ("tab.move_to_space", json!({})),
            ] {
                let out = this.run_as(name, args.clone(), false, window, cx);
                assert!(out.get("error").is_some(), "{name} {args}: {out}");
            }
            assert_eq!(this.model.spaces.len(), 1);
        });
    }
}
