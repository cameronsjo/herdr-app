//! FORK(spaces): moving a tab to another space, and merging one space into
//! another (cameronsjo/herdr-app#15, ported from the TUI fork's
//! cameronsjo/herdr#107 and #53). Upstream's `tab.move` stays inside a space.
//!
//! Only the model changes here: the tab keeps its id, splits and terminals,
//! and the window writes the layout document as for any other move.
//! `src/window/fork_spaces.rs` runs the commands.

use crate::model::{self, Model};

/// What a move did, for the reply and the terminals' `herdr.space` labels.
#[derive(Debug, PartialEq)]
pub struct Moved {
    /// The space the tabs left; gone when its last tab left.
    pub from: String,
    pub closed: bool,
    /// The terminals of the tabs that moved.
    pub terminals: Vec<String>,
}

/// Moves tab `tab` into space `into`, at `index` (1 = first; None: last).
/// The last tab of a space takes the space away with it, its terminals still
/// running. A shown tab stays shown, in its new space.
pub fn move_tab(
    m: &mut Model,
    tab: &str,
    into: &str,
    index: Option<usize>,
) -> Result<Moved, String> {
    let Some(from) = m.spaces.iter().position(|s| s.tab(tab).is_some()) else {
        return Err(format!("no tab {tab:?}"));
    };
    let Some(to) = m.spaces.iter().position(|s| s.id == into) else {
        return Err(format!("no space {into:?}"));
    };
    if from == to {
        return Err("the tab is already in that space; tab.move orders it there".into());
    }
    if m.spaces[from].machine != m.spaces[to].machine {
        return Err(
            "a tab's terminals run on its space's machine: move it to a space on the same machine"
                .into(),
        );
    }
    let shown =
        m.active_index() == Some(from) && m.spaces[from].active_tab().is_some_and(|c| c.id == tab);
    let src = &mut m.spaces[from];
    let i = src
        .tabs
        .iter()
        .position(|c| c.id == tab)
        .unwrap_or_default();
    let moved = src.tabs.remove(i);
    if src.active.as_deref() == Some(tab) {
        // The tab that slides into the slot, or the last one, as closing a tab does.
        src.active = src.tabs.get(i).or(src.tabs.last()).map(|c| c.id.clone());
    }
    let terminals = model::addresses(&moved.root);
    let dst = &mut m.spaces[to];
    let at = index.map_or(dst.tabs.len(), |i| i.saturating_sub(1).min(dst.tabs.len()));
    dst.tabs.insert(at, moved);
    let from_id = m.spaces[from].id.clone();
    let closed = m.spaces[from].tabs.is_empty();
    if closed {
        drop_space(m, from);
    }
    if shown {
        m.select_tab(tab);
    }
    Ok(Moved {
        from: from_id,
        closed,
        terminals,
    })
}

/// Moves every tab of space `from` to the end of space `into`, in order, and
/// takes `from` away. Its selected tab is selected in `into` when `from` was shown.
pub fn merge(m: &mut Model, from: &str, into: &str) -> Result<Moved, String> {
    let Some(src) = m.space(from) else {
        return Err(format!("no space {from:?}"));
    };
    let Some(dst) = m.space(into) else {
        return Err(format!("no space {into:?}"));
    };
    if from == into {
        return Err("a space can't merge into itself".into());
    }
    if src.machine != dst.machine {
        return Err(
            "a space's terminals run on its machine: merge it into a space on the same machine"
                .into(),
        );
    }
    let shown = m.active.as_deref() == Some(from) || m.active_space().is_some_and(|s| s.id == from);
    let selected = src.active_tab().map(|c| c.id.clone());
    let pos = m
        .spaces
        .iter()
        .position(|s| s.id == from)
        .unwrap_or_default();
    let tabs = std::mem::take(&mut m.spaces[pos].tabs);
    let terminals = tabs
        .iter()
        .flat_map(|c| model::addresses(&c.root))
        .collect();
    if let Some(dst) = m.space_mut(into) {
        dst.tabs.extend(tabs);
    }
    drop_space(m, pos);
    if shown && let Some(t) = selected {
        m.select_tab(&t);
    }
    Ok(Moved {
        from: from.to_owned(),
        closed: true,
        terminals,
    })
}

/// Where a terminal labeled for `space` and tab `tab` is put back: the space
/// that holds the tab now (a move keeps its id), else the labeled one. Labels
/// are rewritten after a move, but a write that failed, or one a quit cut off,
/// would otherwise make a second tab with that id in a new copy of the old space.
pub fn space_for_adoption(m: &Model, space: &str, tab: Option<&str>) -> String {
    tab.and_then(|t| m.spaces.iter().find(|s| s.tab(t).is_some()))
        .map_or_else(|| space.to_owned(), |s| s.id.clone())
}

/// Takes an emptied space out. Not `remove_space`: nothing in it closed, so it
/// isn't a recent space to open again.
fn drop_space(m: &mut Model, at: usize) {
    let was_active = m.active_index() == Some(at);
    m.spaces.remove(at);
    if was_active {
        m.active = m.spaces.get(at).or(m.spaces.last()).map(|s| s.id.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Tab;

    fn three() -> Model {
        let mut m = Model::default();
        m.add_space("p1", "", "/a", Tab::new("c1", "", "x1", "t1"));
        m.add_tab("p1", Tab::new("c2", "", "x2", "t2"));
        m.add_space("p2", "", "/b", Tab::new("c3", "", "x3", "t3"));
        m
    }

    fn tabs(m: &Model, space: &str) -> Vec<String> {
        m.space(space)
            .map(|s| s.tabs.iter().map(|c| c.id.clone()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn a_tab_moves_with_its_terminals_and_the_shown_one_stays_shown() {
        let mut m = three();
        m.select_tab("c2");
        let r = move_tab(&mut m, "c2", "p2", Some(1)).unwrap();
        assert_eq!(
            r,
            Moved {
                from: "p1".into(),
                closed: false,
                terminals: vec!["t2".into()]
            }
        );
        assert_eq!(tabs(&m, "p1"), ["c1"]);
        assert_eq!(tabs(&m, "p2"), ["c2", "c3"]);
        assert_eq!(m.active.as_deref(), Some("p2"));
        assert_eq!(m.active_tab().map(|c| c.id.as_str()), Some("c2"));
        assert_eq!(m.space("p1").and_then(|s| s.active.as_deref()), Some("c1"));
    }

    #[test]
    fn a_tab_not_shown_moves_to_the_end_and_nothing_else_changes() {
        let mut m = three();
        m.select_tab("c1");
        move_tab(&mut m, "c2", "p2", None).unwrap();
        assert_eq!(tabs(&m, "p2"), ["c3", "c2"]);
        assert_eq!(m.active_tab().map(|c| c.id.as_str()), Some("c1"));
        assert_eq!(
            m.space("p2")
                .and_then(|s| s.active_tab())
                .map(|c| c.id.as_str()),
            Some("c3")
        );
    }

    #[test]
    fn the_last_tab_takes_its_space_away_without_making_it_recent() {
        let mut m = three();
        m.select_tab("c3");
        let r = move_tab(&mut m, "c3", "p1", Some(99)).unwrap();
        assert!(r.closed);
        assert!(m.space("p2").is_none());
        assert_eq!(tabs(&m, "p1"), ["c1", "c2", "c3"]);
        assert_eq!(m.active_tab().map(|c| c.id.as_str()), Some("c3"));
        assert!(m.recent.is_empty());
    }

    #[test]
    fn a_move_refuses_its_own_space_another_machine_and_unknown_ids() {
        let mut m = three();
        assert!(move_tab(&mut m, "c1", "p1", None).is_err());
        assert!(move_tab(&mut m, "nope", "p2", None).is_err());
        assert!(move_tab(&mut m, "c1", "nope", None).is_err());
        m.space_mut("p2").unwrap().machine = Some("m5".into());
        assert!(move_tab(&mut m, "c1", "p2", None).is_err());
        assert!(merge(&mut m, "p1", "p2").is_err());
        assert_eq!(tabs(&m, "p1"), ["c1", "c2"]);
    }

    #[test]
    fn a_merge_appends_every_tab_in_order_and_keeps_the_shown_tab() {
        let mut m = three();
        m.select_tab("c2");
        let r = merge(&mut m, "p1", "p2").unwrap();
        assert_eq!(r.terminals, ["t1", "t2"]);
        assert!(m.space("p1").is_none());
        assert_eq!(tabs(&m, "p2"), ["c3", "c1", "c2"]);
        assert_eq!(m.active_tab().map(|c| c.id.as_str()), Some("c2"));
        assert!(merge(&mut m, "p2", "p2").is_err());
        assert!(m.recent.is_empty());
    }

    #[test]
    fn a_terminal_labeled_for_a_moved_tab_goes_where_the_tab_is() {
        let mut m = three();
        move_tab(&mut m, "c2", "p2", None).unwrap();
        move_tab(&mut m, "c1", "p2", None).unwrap();
        assert_eq!(space_for_adoption(&m, "p1", Some("c1")), "p2");
        assert_eq!(space_for_adoption(&m, "p1", Some("gone")), "p1");
        assert_eq!(space_for_adoption(&m, "p9", None), "p9");
    }

    #[test]
    fn a_merge_of_a_space_not_shown_leaves_the_shown_tab() {
        let mut m = three();
        m.select_tab("c3");
        merge(&mut m, "p1", "p2").unwrap();
        assert_eq!(m.active_tab().map(|c| c.id.as_str()), Some("c3"));
    }
}
