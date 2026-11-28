//! FORK(palette): the palette's direction families and the `pick-direction`
//! chooser (`defaults/frame/palette/fork.js`, cameronsjo/herdr-app#12), drawn
//! by the shipped mods against upstream's populated fixture.

use super::*;

#[gpui::test]
fn an_empty_palette_shows_one_row_per_direction_family(cx: &mut TestAppContext) {
    cx.update(herdr_shell::init);
    let staged = Staged::new();
    let tree = describe(
        cx,
        &staged,
        "frame/palette",
        true,
        props(),
        updates("current", "current"),
        None,
    )
    .unwrap();
    assert!(tree.contains("Split pane…"), "{tree}");
    assert!(!tree.contains("Split right"), "{tree}");
    let typed = describe(
        cx,
        &staged,
        "frame/palette",
        true,
        props(),
        updates("current", "current"),
        Some("this.query = \"split right\";"),
    )
    .unwrap();
    // Typed, each direction is its own row again (its title split by the match highlight).
    assert!(
        typed.contains("\"Split\"") && !typed.contains("Split pane"),
        "{typed}"
    );
}

#[gpui::test]
fn the_chooser_draws_a_tile_per_direction_with_its_key(cx: &mut TestAppContext) {
    cx.update(herdr_shell::init);
    let staged = Staged::new();
    let mut p = props();
    p.as_object_mut().unwrap().extend(
        json!({"command":"pane.split","title":"Split pane","dirs":["right","down"]})
            .as_object()
            .unwrap()
            .clone(),
    );
    let tree = describe(
        cx,
        &staged,
        "mods/pick-direction",
        true,
        p,
        updates("current", "current"),
        None,
    )
    .unwrap();
    for want in ["Split pane", "dir-right", "dir-down", "ctrl-shift-o"] {
        assert!(tree.contains(want), "missing {want:?}: {tree}");
    }
    assert!(!tree.contains("dir-left"), "{tree}");
}
