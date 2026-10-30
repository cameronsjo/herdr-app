//! Herdr's top layer for an agent browser's header: drawn after everything
//! else in the window, so nothing a mod or a layout draws can sit over the
//! "Allow agent" button.
//!
//! GPUI paints the element tree in order, then every `deferred` element in
//! order of priority (gpui-kit's popups, dialogs, toasts and tooltips are
//! `deferred` at 200 or less). The header goes in a deferred draw at
//! [`TOP`], above all of them, and occludes: no element under it gets the
//! pointer. Hit testing follows the order draws are prepared in, and GPUI
//! prepares a draw nested in another (a popover in a mod's dialog) a round
//! later; a `HERDR:` patch in `vendor/gpui-pre` (`prepaint_deferred_draws`)
//! prepares draws at `usize::MAX` after all others, nested ones included,
//! so the header is on top for clicks as it is for paint. Herdr's own
//! overlays (palette, dialogs, menus, toasts, floats) are drawn before it,
//! so where one overlaps the header the header isn't drawn at all that
//! frame (`may_draw` says no): the overlay shows as it should, and the
//! button has no place on screen, so "Allow agent" starts its wait again
//! (`super::AllowGuard`).

use std::cell::Cell;
use std::rc::Rc;

use gpui::{AnyElement, App, Bounds, ContentMask, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, Window};

/// The deferred-draw priority of the agent header: above every priority a
/// mod's content can get (gpui-kit: dialogs `10 + depth`, popups 100,
/// toasts 101, tooltips 200). GPUI (with Herdr's patch) prepares draws at
/// this priority last, so they are hit-tested above every other draw.
pub const TOP: usize = usize::MAX;

/// `child`, drawn in Herdr's top layer, inside the clip of where it is laid
/// out. `may_draw` gets its bounds while the window prepares the frame,
/// after every other element has; false: it isn't drawn this frame (no
/// paint, no hitbox).
pub fn top(child: AnyElement, may_draw: impl FnOnce(Bounds<Pixels>) -> bool + 'static) -> AnyElement {
    let mask = Rc::new(Cell::new(None));
    Top { gate: Some(Gate { child, may_draw: Some(Box::new(may_draw)), mask: mask.clone() }.into_any_element()), mask }.into_any_element()
}

/// In the tree: takes the child's place in the layout, then hands it to the
/// window's deferred draws at [`TOP`] (as GPUI's `deferred` does, but keeping
/// the clip it's in).
struct Top {
    gate: Option<AnyElement>,
    /// The clip where it's laid out, for the gate.
    mask: Rc<Cell<Option<ContentMask<Pixels>>>>,
}

/// In the deferred draw: asks `may_draw`, then draws the child or nothing.
struct Gate {
    child: AnyElement,
    may_draw: Option<Box<dyn FnOnce(Bounds<Pixels>) -> bool>>,
    /// Set by `Top` as it hands the gate over.
    mask: Rc<Cell<Option<ContentMask<Pixels>>>>,
}

impl IntoElement for Top {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl IntoElement for Gate {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Top {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        (self.gate.as_mut().expect("laid out once").request_layout(window, cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), window: &mut Window, _: &mut App) {
        let gate = self.gate.take().expect("prepainted once");
        // The clip it is in now: a header never draws outside its pane.
        let mask = window.content_mask();
        self.mask.set(Some(mask));
        window.defer_draw(gate, window.element_offset(), TOP, Some(mask));
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut (), _: &mut Window, _: &mut App) {}
}

impl Element for Gate {
    type RequestLayoutState = ();
    /// Whether it draws this frame.
    type PrepaintState = bool;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) -> bool {
        let draw = self.may_draw.take().is_some_and(|f| f(bounds));
        if draw {
            // Deferred draws prepare with no clip: its hitbox keeps the clip it's painted with.
            window.with_content_mask(self.mask.get(), |window| self.child.prepaint(window, cx));
        }
        draw
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), draw: &mut bool, window: &mut Window, cx: &mut App) {
        if *draw {
            self.child.paint(window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    //! The top layer against a stand-in for mod content drawn over the header.
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::prelude::*;
    use gpui::{Context, Modifiers, TestAppContext, Window, canvas, deferred, div, point, px};

    /// A header with a button at the top left, and "mod" content drawn after
    /// it in the tree and as a gpui-kit-style popup (deferred, priority 200),
    /// both over the button and neither occluding: what a bait mod draws.
    struct Scene {
        top: bool,
        clear: bool,
        /// The header's container clips it to this width.
        clip: Option<f32>,
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Render for Scene {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let log = self.log.clone();
            let mark = move |name: &'static str| {
                let log = log.clone();
                canvas(|_, _, _| {}, move |_, _, _, _| log.borrow_mut().push(name)).absolute().size_full()
            };
            let header = div()
                .id("header")
                .absolute()
                .top_0()
                .left_0()
                .w(px(300.0))
                .h(px(26.0))
                .occlude()
                .child(mark("header"))
                .child(div().id("allow").w(px(80.0)).h(px(20.0)).on_click(cx.listener(|this: &mut Self, _, _, _| this.log.borrow_mut().push("allow clicked"))));
            let clear = self.clear;
            let header = if self.top { super::top(header.into_any_element(), move |_| clear) } else { header.into_any_element() };
            let bait = |name: &'static str, id: &'static str| {
                div()
                    .id(id)
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(px(300.0))
                    .h(px(200.0))
                    .child(mark(name))
                    .on_click(cx.listener(move |this: &mut Self, _, _, _| this.log.borrow_mut().push(id)))
            };
            div()
                .size_full()
                .relative()
                .child(div().size_full().relative().when_some(self.clip, |d, w| d.w(px(w)).overflow_hidden()).child(header))
                .child(bait("mod", "mod clicked"))
                .child(deferred(bait("popup", "popup clicked")).with_priority(200))
        }
    }

    fn run(cx: &mut TestAppContext, top: bool, clear: bool) -> Vec<&'static str> {
        run_clipped(cx, top, clear, None)
    }

    fn run_clipped(cx: &mut TestAppContext, top: bool, clear: bool, clip: Option<f32>) -> Vec<&'static str> {
        let log = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view(|_, _| Scene { top, clear, clip, log: log.clone() });
        cx.run_until_parked();
        let painted = log.borrow().clone();
        log.borrow_mut().clear();
        cx.simulate_click(point(px(10.0), px(10.0)), Modifiers::none());
        cx.run_until_parked();
        let clicked: Vec<_> = log.borrow().iter().copied().filter(|e| e.ends_with("clicked")).collect();
        painted.iter().copied().filter(|e| !e.ends_with("clicked")).take(3).chain(clicked).collect()
    }

    #[gpui::test]
    fn the_agent_header_paints_over_everything_a_mod_draws_and_takes_its_clicks(cx: &mut TestAppContext) {
        // In the top layer: painted last, and a click on its button reaches
        // the button and nothing drawn under it.
        assert_eq!(run(cx, true, true), vec!["mod", "popup", "header", "allow clicked"]);
    }

    #[gpui::test]
    fn without_the_top_layer_mod_content_sits_over_the_button_and_passes_the_click(cx: &mut TestAppContext) {
        // The bug class this layer closes: the bait paints over the header and
        // the click lands on the button under it as well.
        let out = run(cx, false, true);
        assert_eq!(&out[..3], &["header", "mod", "popup"]);
        assert!(out.contains(&"allow clicked"), "{out:?}");
    }

    #[gpui::test]
    fn the_header_keeps_the_clip_it_is_laid_out_in(cx: &mut TestAppContext) {
        // Its pane shows only the first 5 points: a click at x 10 is outside
        // it, so it reaches what is drawn there, never the clipped-off button.
        let out = run_clipped(cx, true, true, Some(5.0));
        assert!(!out.contains(&"allow clicked") && out.contains(&"mod clicked"), "{out:?}");
    }

    #[gpui::test]
    fn a_header_that_may_not_draw_has_no_paint_and_no_hitbox(cx: &mut TestAppContext) {
        let out = run(cx, true, false);
        assert!(!out.contains(&"header") && !out.contains(&"allow clicked"), "{out:?}");
    }

    /// A mod's dialog (deferred, priority 10) holding an open popover
    /// (deferred again, priority 100) over the header: a draw nested in a
    /// draw, which GPUI prepares a round after the header's.
    struct Nested {
        occlude: bool,
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Render for Nested {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let log = self.log.clone();
            let mark = move |name: &'static str| {
                let log = log.clone();
                canvas(|_, _, _| {}, move |_, _, _, _| log.borrow_mut().push(name)).absolute().size_full()
            };
            let header = div()
                .id("header")
                .absolute()
                .top_0()
                .left_0()
                .w(px(300.0))
                .h(px(26.0))
                .occlude()
                .child(mark("header"))
                .child(div().id("allow").w(px(80.0)).h(px(20.0)).on_click(cx.listener(|this: &mut Self, _, _, _| this.log.borrow_mut().push("allow clicked"))));
            let popover = div()
                .id("popover")
                .absolute()
                .top_0()
                .left_0()
                .w(px(300.0))
                .h(px(200.0))
                .when(self.occlude, |d| d.occlude())
                .child(mark("popover"))
                .on_click(cx.listener(|this: &mut Self, _, _, _| this.log.borrow_mut().push("popover clicked")));
            let dialog = div().absolute().top_0().left_0().size_full().child(deferred(popover).with_priority(100));
            div()
                .size_full()
                .relative()
                .child(div().size_full().relative().child(super::top(header.into_any_element(), |_| true)))
                .child(deferred(dialog).with_priority(10))
        }
    }

    fn run_nested(cx: &mut TestAppContext, occlude: bool) -> Vec<&'static str> {
        let log = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view(|_, _| Nested { occlude, log: log.clone() });
        cx.run_until_parked();
        let painted = log.borrow().clone();
        log.borrow_mut().clear();
        cx.simulate_click(point(px(10.0), px(10.0)), Modifiers::none());
        cx.run_until_parked();
        let clicked: Vec<_> = log.borrow().iter().copied().filter(|e| e.ends_with("clicked")).collect();
        painted.iter().copied().take(2).chain(clicked).collect()
    }

    #[gpui::test]
    fn a_mods_nested_popover_paints_and_hit_tests_under_the_header(cx: &mut TestAppContext) {
        // Hit order follows paint order: the click on the visible button
        // reaches it, and nothing a mod nests under it gets the click.
        assert_eq!(run_nested(cx, true), vec!["popover", "header", "allow clicked"]);
    }

    #[gpui::test]
    fn a_mods_nested_popover_that_does_not_occlude_gets_no_click_through_the_header(cx: &mut TestAppContext) {
        assert_eq!(run_nested(cx, false), vec!["popover", "header", "allow clicked"]);
    }
}
