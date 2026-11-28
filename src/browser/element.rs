//! FORK(browser): `nativeView`, a mod element that hosts a native view
//! (cameronsjo/herdr-app#4). A mod imports it from `herdr-native`:
//!
//! ```js
//! import { nativeView } from "herdr-native";
//! div().relative().size_full().child(nativeView("browser", props().browser).absolute().inset_0())
//! ```
//!
//! The element draws nothing itself: it says where in its block the native
//! view goes. The only kind is `browser`, a browser block's page, and the
//! `browser-prototype` mod (`defaults/mods/browser-prototype/`) is what draws
//! it. The app still owns everything else about the page: whether it opens
//! (`registry`), which block owns it, hiding it under overlays
//! (`view::end_frame`), who has the keyboard (`keyboard_page`), and agent
//! access.
//!
//! Why the window, not the element, places the page: gpui-shell caches a mod's
//! view, so a frame in which the mod didn't change reuses its last paint and
//! the element's paint doesn't run. The browser block's frame
//! (`AppWindow::fork_render_browser`) is painted every frame, after the mod:
//! it asks [`spot`] where the page goes, and gets this frame's report when the
//! mod painted, else the last one, kept relative to the block. A report
//! outside the block that owns the page is ignored, so the page never leaves
//! its block.
//!
//! Reports are keyed by the id a mod names, not by which mod painted them:
//! another mod that draws `nativeView("browser", "b1")` with a rect inside
//! b1's block can move or resize the page within that block. A report
//! outside the block is ignored, so it can't take the page out of the block,
//! over the agent header, or over an overlay, nor hide it.
//!
//! So a mod can't hide the page by leaving the element out of a later render:
//! the page shows wherever the mod last put it until the block goes or an
//! overlay covers it. The shipped mod always draws it.

use std::{cell::RefCell, collections::HashMap, sync::Arc};

use gpui::{AnyElement, Bounds, ParentElement as _, Pixels, Point, Size, Styled as _, canvas, div};
use gpui_shell::{
    ArgumentDescriptor, ArgumentSchema, COMPONENT_REGISTRY_API_VERSION, ComponentArgument, ComponentDescriptor, ComponentMaterializer, ComponentPayload,
    ComponentRegistry, ConstructorDescriptor, FrozenComponentRegistry, MaterializeRequest, RegistryError,
};

/// The module a mod imports `nativeView` from.
pub const MODULE: &str = "herdr-native";

/// The native views a mod can place. Only Herdr's own: a mod can't create
/// one, only say where an existing one goes.
const KINDS: &[&str] = &["browser"];

/// How far (px) a report may stick out of its block and still count:
/// layout rounding, not a way out.
const SLACK: f32 = 0.5;

/// What a `nativeView` element names.
struct Hosted {
    id: String,
}

thread_local! {
    /// This frame's reports: id → every rect a `nativeView` element for it
    /// painted at, in paint order. Cleared by [`end_frame`].
    static REPORTS: RefCell<HashMap<String, Vec<Bounds<Pixels>>>> = RefCell::default();
    /// The last report each block's frame took: id → (offset from the
    /// block's origin, size). Used on frames where the mod's paint was
    /// reused.
    static HELD: RefCell<HashMap<String, (Point<Pixels>, Size<Pixels>)>> = RefCell::default();
}

/// The component catalog every mod VM gets: `herdr-native`'s `nativeView`.
/// An empty catalog if it can't be built (logged), so mods still run.
pub fn components() -> FrozenComponentRegistry {
    catalog().unwrap_or_else(|e| {
        tracing::error!("nativeView is unavailable: {e}");
        FrozenComponentRegistry::default()
    })
}

fn catalog() -> Result<FrozenComponentRegistry, RegistryError> {
    let mut registry = ComponentRegistry::new(COMPONENT_REGISTRY_API_VERSION, MODULE)?;
    let args = vec![ArgumentDescriptor::new("kind", ArgumentSchema::Enum(KINDS)), ArgumentDescriptor::new("id", ArgumentSchema::String)];
    registry.register(
        ComponentDescriptor::new("NativeView", Arc::new(Materializer))
            .with_constructors(vec![ConstructorDescriptor::new("nativeView", args, payload)])
            .with_methods(Vec::new())
            .with_documentation(
                "Where a native view Herdr owns goes in this mod's block: `nativeView(\"browser\", id)` for a browser block's page. \
                 Size it like any element (usually `.absolute().inset_0()`). It draws nothing; the page shows only while the block that owns it shows.",
            ),
    )?;
    registry.freeze()
}

fn payload(args: &[ComponentArgument]) -> Result<ComponentPayload, String> {
    match args {
        [ComponentArgument::Enum(kind), ComponentArgument::String(id)] if KINDS.contains(&kind.as_str()) => Ok(ComponentPayload::new(Hosted { id: id.clone() })),
        _ => Err("nativeView(kind, id): kind is \"browser\", id a browser id".into()),
    }
}

struct Materializer;

impl ComponentMaterializer for Materializer {
    fn materialize(&self, request: MaterializeRequest<'_>) -> anyhow::Result<AnyElement> {
        let id = request.payload().downcast_ref::<Hosted>().map(|h| h.id.clone()).ok_or_else(|| anyhow::anyhow!("nativeView: no view named"))?;
        request.finish(div().child(canvas(|_, _, _| {}, move |bounds, _, _, _| report(&id, bounds)).size_full()))
    }
}

/// A `nativeView` element painted at `bounds` this frame.
fn report(id: &str, bounds: Bounds<Pixels>) {
    // No pages in this build: nothing would take the report.
    if !super::AVAILABLE {
        return;
    }
    REPORTS.with_borrow_mut(|r| r.entry(id.to_owned()).or_default().push(bounds));
}

fn inside(b: &Bounds<Pixels>, block: &Bounds<Pixels>) -> bool {
    let (x, y, w, h) = (f32::from(b.origin.x), f32::from(b.origin.y), f32::from(b.size.width), f32::from(b.size.height));
    let (bx, by, bw, bh) = (f32::from(block.origin.x), f32::from(block.origin.y), f32::from(block.size.width), f32::from(block.size.height));
    w > 0.0 && h > 0.0 && x >= bx - SLACK && y >= by - SLACK && x + w <= bx + bw + SLACK && y + h <= by + bh + SLACK
}

/// Where `id`'s view goes this frame, in the block (`block`, its bounds)
/// that owns it: the mod's last report this frame inside the block, else
/// the last one taken, moved with the block. None: the mod hasn't placed it
/// inside the block yet (still loading).
#[cfg_attr(not(all(target_os = "macos", feature = "browser-pane")), allow(dead_code))]
pub fn spot(id: &str, block: Bounds<Pixels>) -> Option<Bounds<Pixels>> {
    let reports = REPORTS.with_borrow_mut(|r| r.remove(id)).unwrap_or_default();
    // A report outside the block (another mod drawing the same id) is
    // ignored, not taken as "hide": the block's own mod, its paint reused
    // this frame, still holds the spot.
    let mut chosen = None;
    for b in reports.into_iter().rev() {
        if inside(&b, &block) {
            if chosen.is_none() {
                chosen = Some(b);
            }
        } else {
            tracing::debug!(
                "browser: nativeView {} rejected report {},{} {}x{} outside block {},{} {}x{}",
                super::log_safe(id),
                f32::from(b.origin.x),
                f32::from(b.origin.y),
                f32::from(b.size.width),
                f32::from(b.size.height),
                f32::from(block.origin.x),
                f32::from(block.origin.y),
                f32::from(block.size.width),
                f32::from(block.size.height),
            );
        }
    }
    if let Some(b) = chosen {
        HELD.with_borrow_mut(|h| h.insert(id.to_owned(), (b.origin - block.origin, b.size)));
        return Some(b);
    }
    HELD.with_borrow(|h| h.get(id).map(|(offset, size)| Bounds { origin: block.origin + *offset, size: *size })).filter(|b| inside(b, &block))
}

/// The page closed: a page opened later under the same id starts from its
/// mod's first report, not this one's last spot.
#[cfg_attr(not(all(target_os = "macos", feature = "browser-pane")), allow(dead_code))]
pub fn forget(id: &str) {
    HELD.with_borrow_mut(|h| h.remove(id));
}

/// After a frame: reports no block took (an id nobody owns) go.
#[cfg_attr(not(all(target_os = "macos", feature = "browser-pane")), allow(dead_code))]
pub fn end_frame() {
    REPORTS.with_borrow_mut(HashMap::clear);
}

#[cfg(test)]
pub fn debug_held(id: &str) -> Option<(Point<Pixels>, Size<Pixels>)> {
    HELD.with_borrow(|h| h.get(id).copied())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size};

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds { origin: point(px(x), px(y)), size: size(px(w), px(h)) }
    }

    fn reset() {
        REPORTS.with_borrow_mut(HashMap::clear);
        HELD.with_borrow_mut(HashMap::clear);
    }

    /// `report` without the AVAILABLE gate, so Linux tests run the logic.
    fn painted(id: &str, b: Bounds<Pixels>) {
        REPORTS.with_borrow_mut(|r| r.entry(id.to_owned()).or_default().push(b));
    }

    #[test]
    fn the_catalog_builds_with_native_view_in_herdr_native() {
        let c = catalog().expect("catalog");
        assert_eq!(c.module_specifier(), Some(MODULE));
        assert_eq!(c.descriptors().map(|d| d.name()).collect::<Vec<_>>(), ["NativeView"]);
    }

    #[test]
    fn only_a_known_kind_and_a_string_id_make_a_view() {
        assert!(payload(&[ComponentArgument::Enum("browser".into()), ComponentArgument::String("b1".into())]).is_ok());
        assert!(payload(&[ComponentArgument::Enum("webcam".into()), ComponentArgument::String("b1".into())]).is_err());
        assert!(payload(&[ComponentArgument::Enum("browser".into())]).is_err());
    }

    #[test]
    fn a_report_inside_the_block_places_the_view_and_is_kept_for_reused_frames() {
        reset();
        let block = rect(100.0, 50.0, 400.0, 300.0);
        painted("b1", rect(100.0, 80.0, 400.0, 270.0));
        assert_eq!(spot("b1", block), Some(rect(100.0, 80.0, 400.0, 270.0)));
        end_frame();
        // The mod's paint was reused: no report, same place.
        assert_eq!(spot("b1", block), Some(rect(100.0, 80.0, 400.0, 270.0)));
        // The block moved without resizing (its paint reused too): the view follows.
        assert_eq!(spot("b1", rect(140.0, 50.0, 400.0, 300.0)), Some(rect(140.0, 80.0, 400.0, 270.0)));
        // The block shrank and the mod hasn't painted yet: the old size doesn't fit, no place.
        assert_eq!(spot("b1", rect(100.0, 50.0, 200.0, 300.0)), None);
    }

    #[test]
    fn a_report_outside_its_block_never_places_the_page() {
        reset();
        let block = rect(0.0, 0.0, 300.0, 300.0);
        painted("b1", rect(0.0, 0.0, 300.0, 300.0));
        assert!(spot("b1", block).is_some());
        // Another mod draws the same page elsewhere on a frame where the block's own
        // mod was reused: ignored, and the page stays where its own mod put it.
        painted("b1", rect(500.0, 0.0, 300.0, 300.0));
        assert_eq!(spot("b1", block), Some(block));
        assert_eq!(spot("b1", block), Some(block));
        // Never placed inside its block, a page has no spot however many reports it gets.
        painted("b2", rect(500.0, 0.0, 300.0, 300.0));
        assert_eq!(spot("b2", block), None);
    }

    #[test]
    fn of_several_reports_the_last_inside_the_block_wins() {
        reset();
        let block = rect(0.0, 0.0, 300.0, 300.0);
        painted("b1", rect(0.0, 0.0, 100.0, 100.0));
        painted("b1", rect(0.0, 0.0, 300.0, 300.0));
        painted("b1", rect(900.0, 0.0, 300.0, 300.0));
        assert_eq!(spot("b1", block), Some(rect(0.0, 0.0, 300.0, 300.0)));
    }

    #[test]
    fn rounding_is_slack_but_an_empty_view_is_no_place() {
        reset();
        let block = rect(10.0, 10.0, 100.0, 100.0);
        painted("b1", rect(9.75, 10.0, 100.5, 100.0));
        assert!(spot("b1", block).is_some());
        painted("b2", rect(10.0, 10.0, 0.0, 100.0));
        assert_eq!(spot("b2", block), None);
    }

    #[test]
    fn a_closed_page_forgets_its_spot() {
        reset();
        let block = rect(0.0, 0.0, 300.0, 300.0);
        painted("b1", block);
        assert!(spot("b1", block).is_some());
        forget("b1");
        assert_eq!(spot("b1", block), None);
    }

    #[test]
    fn a_report_no_block_took_is_dropped_at_frame_end() {
        reset();
        let block = rect(0.0, 0.0, 300.0, 300.0);
        painted("b9", block);
        end_frame();
        assert_eq!(spot("b9", block), None, "a report no block took this frame doesn't place a later one");
    }
}
