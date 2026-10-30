//! App-owned pane grips and drag gestures, independent of header mods.
use super::*;
use crate::layout::Edge;

pub(super) struct PaneDrag {
    pane: String,
    tab: String,
    down: Point<Pixels>,
    moving: bool,
    target: Option<(String, Option<Edge>)>,
}

/// The outer quarter of a pane chooses its nearest edge; the middle swaps.
fn drop_zone(bounds: Bounds<Pixels>, at: Point<Pixels>) -> Option<Option<Edge>> {
    if !bounds.contains(&at) || bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
        return None;
    }
    let x = f32::from(at.x - bounds.origin.x) / f32::from(bounds.size.width);
    let y = f32::from(at.y - bounds.origin.y) / f32::from(bounds.size.height);
    let (distance, edge) = [(x, Edge::Left), (1. - x, Edge::Right), (y, Edge::Top), (1. - y, Edge::Bottom)]
        .into_iter().min_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
    Some((distance < 0.25).then_some(edge))
}

/// The top row of a pane without a top header.
const STRIP: f32 = 18.;

fn top_strip(mut bounds: Bounds<Pixels>, row: f32, at: Point<Pixels>) -> bool {
    bounds.size.height = bounds.size.height.min(settings::z(row));
    bounds.contains(&at)
}

/// Whether a pane's grip, centred in its top `row`, would overlap an agent
/// header that starts `top` points down. The grip is one of Herdr's covers,
/// and a cover over any part of that header hides it (`Browsers::agent_header`):
/// a grip shown on hover there would hide "Allow agent" as the pointer
/// reached it.
fn grip_meets_agent_header(row: f32, top: f32) -> bool {
    let y = (row - GRIP) / 2.;
    y < top + AGENT_HEADER && top < y + GRIP
}

/// The pane's grip row (its hover strip and where the grip is centred) for
/// a top header of `header` points and an agent header starting at `agent`:
/// 0 (no grip) where the grip would overlap the agent header.
fn grip_row(header: Option<f32>, agent: Option<f32>) -> f32 {
    let row = header.unwrap_or(STRIP).max(STRIP);
    if agent.is_some_and(|top| grip_meets_agent_header(row, top)) { 0. } else { row }
}

/// The grip's height.
const GRIP: f32 = 18.;

fn inherited_movable(root: &Node, id: &str) -> Option<bool> {
    root.path_to(id)?.iter().rev().find_map(|id| root.find(id)?.movable)
}

impl AppWindow {
    pub(super) fn pane_movable(&self, pane: &str) -> bool {
        let Some(tab) = self.model.active_tab() else { return false };
        if tab.zoomed.is_some() || tab.root.panes().len() < 2 || !tab.root.find(pane).is_some_and(Node::is_pane) {
            return false;
        }
        let owner = self.chrome.visible_blocks().into_iter().find(|n| matches!(n.content(), Some(Content::Context { .. })));
        inherited_movable(&tab.root, pane)
            .or_else(|| owner.and_then(|n| inherited_movable(&self.chrome, &n.id)))
            .unwrap_or(true)
    }

    /// `agent`: where the pane's agent header starts, if it has one. A pane
    /// whose grip would overlap it has no grip (no hover strip either); it
    /// still moves with `pane.swap` or `layout.set`, and takes drops.
    pub(super) fn pane_grip(&self, pane: &str, header: Option<f32>, agent: Option<f32>, cx: &mut Context<Self>) -> Option<AnyElement> {
        let row = grip_row(header, agent);
        self.pane_grip_rows.borrow_mut().insert(pane.to_owned(), row);
        if row == 0. { return None; }
        if !self.pane_movable(pane) { return None; }
        let dragging = self.pane_drag.as_ref().is_some_and(|d| d.pane == pane);
        if self.pane_grip_hover.as_deref() != Some(pane) && !dragging { return None; }
        let pane = pane.to_owned();
        Some(div().id(SharedString::from(format!("grip-{pane}")))
            .absolute().top(settings::z((row - GRIP) / 2.)).left(relative(0.5)).ml(-settings::z(14.))
            .w(settings::z(28.)).h(settings::z(GRIP)).rounded(settings::z(4.))
            .bg(c("background").opacity(0.85))
            .hover(|s| s.bg(c("hover")))
            .flex().flex_col().items_center().justify_center().gap(settings::z(3.)).occlude()
            .children(self.browsers.cover(format!("pane grip ({pane})")))
            .group("pane-grip")
            .cursor(if dragging { CursorStyle::ClosedHand } else { CursorStyle::OpenHand })
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                this.pane_press(&pane, ev.position, cx);
                cx.stop_propagation();
            }))
            .children((0..2).map(|_| div().flex().gap(settings::z(3.)).children((0..3).map(|_| div().size(settings::z(2.)).rounded_full()
                .bg(if dragging { c("foreground") } else { c("dim") })
                .group_hover("pane-grip", |s| s.bg(c("foreground")))))))
            .into_any_element())
    }

    fn pane_hover(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let hovered = self.model.active_tab().and_then(|tab| {
            let bounds = self.bounds.borrow();
            let rows = self.pane_grip_rows.borrow();
            tab.shown().visible_blocks().into_iter().find_map(|n| {
                let b = bounds.get(&n.id)?;
                let row = rows.get(&n.id).copied().unwrap_or(STRIP);
                (self.pane_movable(&n.id) && top_strip(*b, row, at)).then(|| n.id.clone())
            })
        });
        if hovered != self.pane_grip_hover {
            self.pane_grip_hover = hovered;
            cx.notify();
        }
    }

    pub(super) fn pane_press(&mut self, pane: &str, at: Point<Pixels>, cx: &mut Context<Self>) {
        if self.pane_movable(pane) {
            self.pane_drag = Some(PaneDrag {
                pane: pane.to_owned(), tab: self.model.active_tab().unwrap().id.clone(),
                down: at, moving: false, target: None,
            });
            cx.notify();
        }
    }

    fn pane_target(&self, at: Point<Pixels>) -> Option<(String, Option<Edge>)> {
        let d = self.pane_drag.as_ref()?;
        let tab = self.model.active_tab().filter(|t| t.id == d.tab && t.zoomed.is_none())?;
        let bounds = self.bounds.borrow();
        tab.shown().visible_blocks().into_iter().filter(|n| n.is_pane() && n.id != d.pane)
            .find_map(|n| Some((n.id.clone(), drop_zone(*bounds.get(&n.id)?, at)?)))
    }

    fn pane_move(&mut self, at: Point<Pixels>, pressed: bool, cx: &mut Context<Self>) {
        let Some(d) = self.pane_drag.as_mut() else { return };
        if !pressed {
            self.cancel_pane_drag(cx);
            return;
        }
        if !d.moving && (at.x - d.down.x).abs().max((at.y - d.down.y).abs()) < settings::z(4.) {
            return;
        }
        d.moving = true;
        let target = self.pane_target(at);
        self.pane_drag.as_mut().unwrap().target = target;
        cx.notify();
    }

    pub(super) fn cancel_pane_drag(&mut self, cx: &mut Context<Self>) {
        if self.pane_drag.take().is_some() {
            cx.notify();
        }
    }

    fn pane_release(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let target = self.pane_target(at);
        let Some(d) = self.pane_drag.take() else { return false };
        if !d.moving {
            cx.notify();
            return true;
        }
        if self.pane_movable(&d.pane)
            && let Some((target, edge)) = target
            && self.model.move_pane(&d.pane, &target, edge)
        {
            self.sync(window, cx);
        }
        cx.defer_in(window, |this, window, cx| this.restore_focus(window, cx));
        cx.notify();
        true
    }

    pub(super) fn pane_drag_layer(&self, cx: &mut Context<Self>) -> AnyElement {
        let preview = self.pane_drag.as_ref().and_then(|d| d.target.as_ref()).and_then(|(id, edge)| {
            let mut b = *self.bounds.borrow().get(id)?;
            match edge {
                Some(Edge::Left) => b.size.width /= 2.,
                Some(Edge::Right) => { b.size.width /= 2.; b.origin.x += b.size.width; }
                Some(Edge::Top) => b.size.height /= 2.,
                Some(Edge::Bottom) => { b.size.height /= 2.; b.origin.y += b.size.height; }
                None => {}
            }
            Some(div().absolute().left(b.origin.x).top(b.origin.y).w(b.size.width).h(b.size.height)
                .children(self.browsers.cover(format!("pane drop preview ({id})")))
                .bg(c("accent").opacity(0.18)).border_2().border_color(c("accent")).into_any_element())
        });
        let this = cx.entity().downgrade();
        div().id("pane-drag-layer").absolute().inset_0()
            .when(self.pane_drag.is_some(), |d| d.occlude().cursor(CursorStyle::ClosedHand))
            .children(preview).child(canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let t = this.clone();
                window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = t.update(cx, |this, cx| {
                            this.pane_hover(ev.position, cx);
                            this.pane_move(ev.position, ev.pressed_button == Some(MouseButton::Left), cx);
                        });
                    }
                });
                let t = this.clone();
                window.on_mouse_event(move |ev: &MouseUpEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture && ev.button == MouseButton::Left
                        && t.update(cx, |this, cx| this.pane_release(ev.position, window, cx)).unwrap_or(false)
                    { cx.stop_propagation(); }
                });
            },
        ).absolute().size_full()).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{GRIP, STRIP, drop_zone, grip_row, inherited_movable, top_strip};
    use crate::layout::{Axis, Content, Edge, Node};
    use gpui::{Bounds, point, px, size};
    #[test]
    fn zones_are_relative_to_the_pane_and_outside_cancels() {
        let b = Bounds::new(point(px(100.), px(200.)), size(px(800.), px(400.)));
        for (x, y, expected) in [(500., 400., Some(None)), (110., 400., Some(Some(Edge::Left))),
            (890., 400., Some(Some(Edge::Right))), (500., 210., Some(Some(Edge::Top))),
            (500., 590., Some(Some(Edge::Bottom))), (99., 400., None)] {
            assert_eq!(drop_zone(b, point(px(x), px(y))), expected);
        }
    }
    #[test]
    fn hover_strip_is_only_the_panes_top_edge() {
        let b = Bounds::new(point(px(100.), px(200.)), size(px(800.), px(400.)));
        assert!(top_strip(b, STRIP, point(px(110.), px(201.))));
        assert!(!top_strip(b, STRIP, point(px(110.), px(250.))));
        assert!(!top_strip(b, STRIP, point(px(99.), px(201.))));
        assert!(!top_strip(b, STRIP, point(px(500.), px(199.))));
        // A 40px top header is the whole row.
        assert!(top_strip(b, 40., point(px(110.), px(235.))));
        assert!(!top_strip(b, 40., point(px(110.), px(245.))));
    }
    #[test]
    fn a_grip_never_overlaps_an_agent_header() {
        let b = Bounds::new(point(px(100.), px(200.)), size(px(800.), px(400.)));
        // No pane frame, or a bottom header: the agent header is the pane's
        // top 26 points, and the grip would sit on it: no grip, no hover strip.
        assert_eq!(grip_row(None, Some(0.)), 0.);
        for y in [200.5, 205., 210., 217.] {
            assert!(!top_strip(b, grip_row(None, Some(0.)), point(px(500.), px(y))), "hover at y {y} shows a grip");
        }
        // A top header under 18 points leaves the grip half on the agent header.
        assert_eq!(grip_row(Some(10.), Some(10.)), 0.);
        // A top header of 18 or more holds the whole grip above the agent header.
        assert_eq!(grip_row(Some(18.), Some(18.)), 18.);
        assert_eq!(grip_row(Some(30.), Some(30.)), 30.);
        // Panes with no agent header keep their grip.
        assert_eq!(grip_row(None, None), STRIP);
        // Every row that keeps a grip keeps it clear of the agent header.
        for top in [0., 5., 10., 17., 18., 26., 30., 60.] {
            let row = grip_row((top > 0.).then_some(top), Some(top));
            let y = (row - GRIP) / 2.;
            assert!(row == 0. || y + GRIP <= top, "grip at {y}..{} meets the agent header at {top}", y + GRIP);
        }
    }
    #[test]
    fn movable_uses_the_nearest_explicit_ancestor() {
        let mut root = Node::container("root", Axis::Row, vec![Node::container("inner", Axis::Column,
            vec![Node::block("pane", Content::Terminal { terminal: "t1".into() })])]);
        assert_eq!(inherited_movable(&root, "pane"), None);
        root.movable = Some(false);
        assert_eq!(inherited_movable(&root, "pane"), Some(false));
        root.find_mut("inner").unwrap().movable = Some(true);
        assert_eq!(inherited_movable(&root, "pane"), Some(true));
    }
}
