//! A Chromium browser inside the window: CEF in windowed mode, so the page is
//! a native child view on top of GPUI's Metal view (macOS, `browser` feature).
//!
//! Spike (plan Task 1): with `HERDR_BROWSER_SPIKE=1` the main window gets one
//! browser at a fixed spot, showing `HERDR_BROWSER_SPIKE_URL` or a built-in
//! test page. It needs the dev bundle (`just browser-dev`): CEF's framework
//! and helper apps live inside `Herdr Dev.app`.

mod runtime;

use base64::Engine as _;
use objc2::{MainThreadMarker, MainThreadOnly as _};
use objc2::rc::Retained;
use objc2_app_kit::{NSAutoresizingMaskOptions, NSView, NSWindowOrderingMode};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Where the spike browser sits, in points from the content view's top left.
const SPIKE_RECT: (f64, f64, f64, f64) = (40.0, 80.0, 720.0, 480.0);

/// A page with what the go/no-go gate tries by hand: text fields (dead keys,
/// IME), a `<select>` popup, a long list (momentum scroll), text to
/// right-click.
const SPIKE_PAGE: &str = r#"<!doctype html><meta charset="utf-8"><title>Herdr browser spike</title>
<style>body{font:15px -apple-system,sans-serif;margin:16px;background:#fdfdf8}li{padding:4px 0}</style>
<h1>Herdr browser spike</h1>
<p>Right-click this text for the context menu.</p>
<p><input id="text" placeholder="type here: option-e e, then Japanese IME" size="40"></p>
<p><textarea id="area" rows="3" cols="40" placeholder="more typing"></textarea></p>
<p><select id="pick"><option>first</option><option>second</option><option>third</option></select></p>
<ol id="list"></ol>
<script>for(let i=1;i<=200;i++){const li=document.createElement('li');li.textContent='row '+i;list.append(li)}</script>"#;

/// Opens the spike browser in the main window when `HERDR_BROWSER_SPIKE=1`.
pub fn spike<V: 'static>(window: gpui::WindowHandle<V>, cx: &mut gpui::App) {
    if std::env::var("HERDR_BROWSER_SPIKE").ok().as_deref() != Some("1") {
        return;
    }
    let url = std::env::var("HERDR_BROWSER_SPIKE_URL")
        .ok()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| {
            format!("data:text/html;base64,{}", base64::engine::general_purpose::STANDARD.encode(SPIKE_PAGE))
        });
    let parent = match window.update(cx, |_, window, _| spike_container(window)) {
        Ok(Ok(view)) => view,
        Ok(Err(e)) | Err(e) => return tracing::error!("browser: no view for the spike browser: {e:#}"),
    };
    if let Err(e) = runtime::start(cx) {
        return tracing::error!("browser: {e:#}");
    }
    runtime::open(parent, url);
}

/// A plain view of our own over GPUI's view, at `SPIKE_RECT`; the browser
/// fills it (CEF sizes its view to its parent).
fn spike_container(window: &gpui::Window) -> anyhow::Result<Retained<NSView>> {
    let mtm = MainThreadMarker::new().ok_or_else(|| anyhow::anyhow!("not on the main thread"))?;
    let handle = HasWindowHandle::window_handle(window).map_err(|e| anyhow::anyhow!("{e}"))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        anyhow::bail!("not an AppKit window");
    };
    // SAFETY: GPUI's window handle is its live GPUIView, used on the main thread.
    let gpui_view: &NSView = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    // SAFETY: a main-thread read of the view tree.
    let content = unsafe { gpui_view.superview() }.ok_or_else(|| anyhow::anyhow!("GPUI's view has no superview"))?;
    let (left, top, width, height) = SPIKE_RECT;
    // AppKit's origin is bottom left (the content view isn't flipped).
    let y = content.bounds().size.height - top - height;
    let frame = NSRect::new(NSPoint::new(left, y), NSSize::new(width, height));
    let container = NSView::initWithFrame(NSView::alloc(mtm), frame);
    // Keeps its distance from the top when the window resizes.
    container.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    container.setWantsLayer(true);
    content.addSubview_positioned_relativeTo(&container, NSWindowOrderingMode::Above, Some(gpui_view));
    Ok(container)
}
