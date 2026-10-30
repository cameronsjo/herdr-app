//! CEF's lifetime in the app: loaded and started on the first browser, pumped
//! from GPUI's foreground executor, shut down at quit after every browser
//! has closed.
//!
//! macOS runs CEF's UI thread on the main thread, so every CEF call and
//! callback here happens there, except `on_schedule_message_pump_work`, which
//! may come from any thread and only sends on a channel.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{CString, c_int};
use std::ptr::NonNull;
use std::os::unix::ffi::OsStrExt as _;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use cef::*;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, AnyProtocol, Bool};
use objc2::{MainThreadMarker, msg_send};
use objc2_app_kit::NSView;

use super::native::Event;
use super::registry::Page;
use super::view;

const FRAMEWORK: &str = "Chromium Embedded Framework.framework/Chromium Embedded Framework";
/// CEF asks to be pumped at least this often (cef_app.h, CefDoMessageLoopWork).
const MAX_PUMP_DELAY: Duration = Duration::from_millis(33);
/// How long quit waits for browsers to close before giving up on a clean shutdown.
const CLOSE_DEADLINE: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Phase {
    #[default]
    Off,
    /// `cef::initialize` returned; waiting for `on_context_initialized`.
    Starting,
    Ready,
    Closing,
    Down,
    Failed,
}

/// One CEF browser: Herdr's id, the open it belongs to, and CEF's own.
struct Open {
    id: String,
    generation: u64,
    browser: Browser,
}

#[derive(Default)]
struct State {
    phase: Phase,
    /// Browsers asked for before CEF's context was ready: (id, generation, parent, url).
    pending: Vec<(String, u64, Retained<NSView>, String)>,
    /// Asked of CEF, `on_after_created` not seen yet.
    creating: usize,
    /// Every CEF browser that exists, the ones closing included.
    browsers: Vec<Open>,
    /// Each wanted Herdr id's current open. A layout file can show the same id
    /// again while its last browser is still closing; the old browser's late
    /// callbacks then name an old generation and change nothing.
    current: HashMap<String, u64>,
    next_generation: u64,
    /// What each wanted page last said about itself.
    pages: HashMap<String, Page>,
    /// Popups turned into navigations of their own browser, after the callback.
    navigations: Vec<(String, String)>,
    /// DevTools windows open (CEF's own; closed before shutdown).
    devtools: usize,
    /// Browsers Herdr is closing, by CEF identifier: their `do_close` is
    /// ours, not the page's.
    closing: Vec<c_int>,
    app: Option<App>,
}

impl State {
    fn is_current(&self, id: &str, generation: u64) -> bool {
        self.current.get(id) == Some(&generation)
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    static PUMPING: Cell<bool> = const { Cell::new(false) };
    static EVENTS: RefCell<Option<async_channel::Sender<Event>>> = const { RefCell::new(None) };
}

/// `on_schedule_message_pump_work` → the foreground pump task.
static PUMP: OnceLock<async_channel::Sender<i64>> = OnceLock::new();

/// Where page changes, focus and close requests go (the window's `Browsers`).
pub fn set_events(tx: async_channel::Sender<Event>) {
    EVENTS.with_borrow_mut(|e| *e = Some(tx));
}

/// The person pressed the mouse on Herdr while a page had the keyboard (view.rs).
pub fn clicked_herdr() {
    send(Event::ClickedHerdr);
}

/// A page hid under an overlay or came back from under one (view.rs).
pub fn covers_changed() {
    send(Event::Covers);
}

fn send(event: Event) {
    EVENTS.with_borrow(|e| {
        if let Some(tx) = e {
            let _ = tx.try_send(event);
        }
    });
}

/// Herdr's browser `id`: the current open's CEF browser, never one closing.
fn browser(id: &str) -> Option<Browser> {
    STATE.with_borrow(|s| s.browsers.iter().find(|o| s.is_current(&o.id, o.generation) && o.id == id).map(|o| o.browser.clone()))
}

fn need(id: &str) -> Result<Browser, String> {
    browser(id).ok_or_else(|| format!("browser {id} is still opening"))
}

/// The page's own view (CEF's), once the browser exists.
pub fn page_view(id: &str) -> Option<NonNull<NSView>> {
    NonNull::new(browser(id)?.host()?.window_handle().cast::<NSView>())
}

pub fn navigate(id: &str, url: &str) -> Result<(), String> {
    let frame = need(id)?.main_frame().ok_or("the page has no main frame")?;
    frame.load_url(Some(&CefString::from(url)));
    Ok(())
}

/// -1 back, 1 forward.
pub fn history(id: &str, step: i32) -> Result<(), String> {
    let b = need(id)?;
    match step {
        -1 if b.can_go_back() == 1 => b.go_back(),
        1 if b.can_go_forward() == 1 => b.go_forward(),
        -1 => return Err(format!("browser {id} has nothing to go back to")),
        _ => return Err(format!("browser {id} has nothing to go forward to")),
    }
    Ok(())
}

pub fn reload(id: &str) -> Result<(), String> {
    need(id)?.reload();
    Ok(())
}

/// Opens or closes CEF's own DevTools window for a browser (None toggles).
/// Returns whether it's open now. No debugging port: DevTools talks to the
/// browser inside CEF.
pub fn devtools(id: &str, open: Option<bool>) -> Result<bool, String> {
    let host = need(id)?.host().ok_or("the browser has no host")?;
    let is_open = host.has_dev_tools() == 1;
    let want = open.unwrap_or(!is_open);
    if want && !is_open {
        let mut client = DevToolsClient::new(DevToolsLifeSpan::new());
        host.show_dev_tools(Some(&WindowInfo::default()), Some(&mut client), Some(&BrowserSettings::default()), None);
    } else if !want && is_open {
        host.close_dev_tools();
    }
    Ok(want)
}

/// Tells CEF its page has the keyboard (the view is first responder already).
pub fn set_focus(id: &str) {
    if let Some(host) = browser(id).and_then(|b| b.host()) {
        host.set_focus(1);
    }
}

fn phase() -> Phase {
    STATE.with_borrow(|s| s.phase)
}

/// Loads the framework and starts CEF, once. Later calls are no-ops; a start
/// that failed stays failed.
pub fn start(cx: &mut gpui::App) -> anyhow::Result<()> {
    match phase() {
        Phase::Off => {}
        Phase::Failed => anyhow::bail!("CEF failed to start earlier"),
        Phase::Closing | Phase::Down => anyhow::bail!("CEF is shutting down"),
        Phase::Starting | Phase::Ready => return Ok(()),
    }
    // Before initialize: CEF can call `on_context_initialized` from inside it.
    STATE.with_borrow_mut(|s| s.phase = Phase::Starting);
    if let Err(e) = start_cef() {
        STATE.with_borrow_mut(|s| s.phase = Phase::Failed);
        return Err(e);
    }
    start_pump(cx);
    // Quit closes the browsers and shuts CEF down while the window still exists.
    cx.on_app_quit(|_| {
        shutdown();
        async {}
    })
    .detach();
    Ok(())
}

fn start_cef() -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    let framework = exe.parent().unwrap_or(&exe).join("../Frameworks").join(FRAMEWORK);
    if !framework.is_file() {
        anyhow::bail!(
            "the CEF framework is missing at {}: start herdr-app from its dev bundle (just browser-dev)",
            framework.display()
        );
    }
    let path = CString::new(framework.as_os_str().as_bytes())?;
    if load_library(Some(unsafe { &*path.as_ptr() })) != 1 {
        anyhow::bail!("cannot load the CEF framework at {}", framework.display());
    }
    // Before any other CEF call. Null: the framework is another CEF build.
    if api_hash(sys::CEF_API_VERSION_LAST, 0).is_null() {
        anyhow::bail!("the CEF framework at {} is not the build herdr-app expects", framework.display());
    }
    declare_app_protocol()?;

    let profile = crate::mods::app_dir().join("browser/profile");
    std::fs::create_dir_all(&profile)?;
    let profile_str = profile.to_string_lossy();
    let log = crate::mods::app_dir().join("browser/cef.log");
    let settings = Settings {
        // Renderers run in Chromium's macOS sandbox (the helper starts it).
        no_sandbox: 0,
        external_message_pump: 1,
        root_cache_path: CefString::from(profile_str.as_ref()),
        cache_path: CefString::from(profile_str.as_ref()),
        log_file: CefString::from(log.to_string_lossy().as_ref()),
        log_severity: LogSeverity::WARNING,
        // Never a DevTools port: agents get CDP through the app (plan Task 5).
        remote_debugging_port: 0,
        ..Default::default()
    };
    let args = args::Args::new();
    let mut app = HerdrCefApp::new(HerdrProcessHandler::new());
    if initialize(Some(args.as_main_args()), Some(&settings), Some(&mut app), std::ptr::null_mut()) != 1 {
        anyhow::bail!("CEF did not start (exit code {})", get_exit_code());
    }
    STATE.with_borrow_mut(|s| s.app = Some(app));
    tracing::info!("browser: CEF started, profile {}", profile.display());
    Ok(())
}

/// Chromium checks `[NSApp conformsToProtocol:@protocol(CrAppProtocol)]`. GPUI's
/// `GPUIApplication` has the methods (vendor/gpui-pre-macos, HERDR:); the
/// protocols exist only once CEF's framework is loaded, so conformance is
/// declared here.
fn declare_app_protocol() -> anyhow::Result<()> {
    let mtm = MainThreadMarker::new().ok_or_else(|| anyhow::anyhow!("CEF must start on the main thread"))?;
    let class = AnyClass::get(c"GPUIApplication").ok_or_else(|| anyhow::anyhow!("no GPUIApplication class"))?;
    let mut declared = Vec::new();
    for name in [c"CrAppProtocol", c"CrAppControlProtocol", c"CefAppProtocol"] {
        if let Some(protocol) = AnyProtocol::get(name) {
            // SAFETY: both are live runtime objects; adding a protocol is idempotent.
            unsafe { objc2::ffi::class_addProtocol((class as *const AnyClass).cast_mut(), protocol) };
            declared.push(name.to_string_lossy());
        }
    }
    let app = objc2_app_kit::NSApp(mtm);
    let app: &AnyObject = &app;
    let cr = AnyProtocol::get(c"CrAppProtocol").ok_or_else(|| anyhow::anyhow!("CEF's framework has no CrAppProtocol"))?;
    // SAFETY: plain NSObject queries on NSApp.
    let conforms: Bool = unsafe { msg_send![app, conformsToProtocol: cr] };
    let handling: Bool = unsafe { msg_send![app, isHandlingSendEvent] };
    if !conforms.as_bool() {
        anyhow::bail!("NSApp does not conform to CrAppProtocol after adding it");
    }
    tracing::info!(
        "browser: NSApp conforms to CrAppProtocol ({}), isHandlingSendEvent {}",
        declared.join(", "),
        handling.as_bool()
    );
    Ok(())
}

/// The external pump: CEF says when it wants work (`delay` ms, any thread);
/// a foreground task runs it, plus a tick at least every 33 ms as CEF asks.
fn start_pump(cx: &mut gpui::App) {
    let (tx, rx) = async_channel::unbounded::<i64>();
    if PUMP.set(tx).is_err() {
        return;
    }
    let fg = cx.foreground_executor().clone();
    let bg = cx.background_executor().clone();
    let (fg2, bg2) = (fg.clone(), bg.clone());
    fg.spawn(async move {
        while let Ok(mut delay) = rx.recv().await {
            // Coalesce a burst into its soonest request.
            while let Ok(next) = rx.try_recv() {
                delay = delay.min(next);
            }
            if delay <= 0 {
                pump();
            } else {
                let wait = Duration::from_millis(delay as u64).min(MAX_PUMP_DELAY);
                let bg = bg2.clone();
                fg2.spawn(async move {
                    bg.timer(wait).await;
                    pump();
                })
                .detach();
            }
            if matches!(phase(), Phase::Down | Phase::Failed) {
                break;
            }
        }
    })
    .detach();
    let fg3 = cx.foreground_executor().clone();
    fg3.spawn(async move {
        loop {
            bg.timer(MAX_PUMP_DELAY).await;
            if matches!(phase(), Phase::Down | Phase::Failed) {
                break;
            }
            pump();
        }
    })
    .detach();
}

/// One `CefDoMessageLoopWork`, never nested (a page's context menu runs a
/// native loop inside it, and GPUI tasks still run there).
fn pump() {
    if !matches!(phase(), Phase::Starting | Phase::Ready | Phase::Closing) || PUMPING.get() {
        return;
    }
    PUMPING.set(true);
    do_message_loop_work();
    PUMPING.set(false);
    finish_navigations();
}

/// Closes a browser the way CEF wants for a child view: its DevTools first,
/// then remove its view (the superview holds the only reference, so it is
/// freed and CEF sees its window go), then a forced `CloseBrowser` as a
/// backstop. `on_before_close` follows asynchronously.
fn close_browser(browser: &Browser) {
    let Some(host) = browser.host() else { return };
    if host.has_dev_tools() == 1 {
        host.close_dev_tools();
    }
    let view = host.window_handle().cast::<NSView>();
    // SAFETY: CEF's browser view, on the main thread; CEF keeps no reference
    // of its own, so this only borrows it until it's removed.
    if let Some(view) = unsafe { view.as_ref() } {
        view.removeFromSuperview();
    }
    host.close_browser(1);
}

/// Closes Herdr's browser `id` (its block is gone). One still being created
/// closes as soon as it exists; one never started is dropped.
pub fn close(id: &str) {
    let b = browser(id);
    STATE.with_borrow_mut(|s| {
        s.pending.retain(|(p, _, _, _)| p != id);
        // Not current any more: one still being created closes once
        // `on_after_created` sees that; later callbacks of this open change nothing.
        s.current.remove(id);
        s.pages.remove(id);
    });
    if let Some(b) = b {
        tracing::info!("browser: closing {id} (cef {})", b.identifier());
        STATE.with_borrow_mut(|s| s.closing.push(b.identifier()));
        objc2::rc::autoreleasepool(|_| close_browser(&b));
    }
}

fn finish_navigations() {
    let navs = STATE.with_borrow_mut(|s| std::mem::take(&mut s.navigations));
    for (id, url) in navs {
        let _ = navigate(&id, &url);
    }
}

/// Opens browser `id` filling `parent`, now or once CEF's context is ready.
pub fn open(id: &str, parent: Retained<NSView>, url: String) {
    let generation = STATE.with_borrow_mut(|s| {
        s.next_generation += 1;
        s.current.insert(id.to_owned(), s.next_generation);
        s.pages.insert(id.to_owned(), Page { url: url.clone(), loading: true, ..Page::default() });
        s.next_generation
    });
    match phase() {
        Phase::Starting => STATE.with_borrow_mut(|s| s.pending.push((id.to_owned(), generation, parent, url))),
        Phase::Ready => create(id, generation, &parent, &url),
        other => tracing::warn!("browser: {id} not opened, CEF is {other:?}"),
    }
}

fn create(id: &str, generation: u64, parent: &NSView, url: &str) {
    let size = parent.bounds().size;
    let bounds = Rect { x: 0, y: 0, width: size.width as c_int, height: size.height as c_int };
    let handle: sys::cef_window_handle_t = (parent as *const NSView).cast_mut().cast();
    let window_info = WindowInfo::default().set_as_child(handle, &bounds);
    let id = id.to_owned();
    let mut client = HerdrClient::new(
        HerdrLifeSpan::new(id.clone(), generation),
        HerdrLoad::new(id.clone(), generation),
        HerdrDisplay::new(id.clone(), generation),
        HerdrFocus::new(id.clone(), generation),
    );
    STATE.with_borrow_mut(|s| s.creating += 1);
    let asked = browser_host_create_browser(
        Some(&window_info),
        Some(&mut client),
        Some(&CefString::from(url)),
        Some(&BrowserSettings::default()),
        None,
        None,
    );
    if asked != 1 {
        STATE.with_borrow_mut(|s| s.creating -= 1);
        tracing::error!("browser: CEF refused to create {id}");
    } else {
        tracing::info!("browser: creating {id} at {}", short(url));
    }
}

/// Records a page change and tells the window (only for the id's current open).
fn page_changed(id: &str, generation: u64, change: impl FnOnce(&mut Page)) {
    let page = STATE.with_borrow_mut(|s| {
        if !s.is_current(id, generation) {
            return None;
        }
        let page = s.pages.entry(id.to_owned()).or_default();
        change(page);
        Some(page.clone())
    });
    if let Some(page) = page {
        send(Event::Page { browser: id.to_owned(), page });
    }
}

fn is_current(id: &str, generation: u64) -> bool {
    STATE.with_borrow(|s| s.is_current(id, generation))
}

fn context_ready() {
    let pending = STATE.with_borrow_mut(|s| {
        if s.phase == Phase::Starting {
            s.phase = Phase::Ready;
        }
        std::mem::take(&mut s.pending)
    });
    tracing::info!("browser: CEF context ready");
    for (id, generation, parent, url) in pending {
        create(&id, generation, &parent, &url);
    }
}

/// Quit: close every browser (asynchronous in CEF), pump until each one's
/// `on_before_close`, then `CefShutdown`. Runs inside GPUI's quit, while the
/// window still exists. Shutting CEF down with a browser still open crashes,
/// so after the deadline it is skipped and the helpers end with the app.
pub fn shutdown() {
    if !matches!(phase(), Phase::Starting | Phase::Ready) {
        return;
    }
    if PUMPING.get() {
        tracing::warn!("browser: quit inside a CEF callback; CEF not shut down");
        return;
    }
    let deadline = Instant::now() + CLOSE_DEADLINE;
    let pump_until = |done: &dyn Fn(&State) -> bool| {
        while !STATE.with_borrow(|s| done(s)) && Instant::now() < deadline {
            // Its own pool, as no run loop drains one here: the closed
            // browser's view must really be freed for CEF to finish closing.
            objc2::rc::autoreleasepool(|_| do_message_loop_work());
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    // A browser still being created can't be closed yet.
    pump_until(&|s| s.phase != Phase::Starting && s.creating == 0);
    // Those already closing finish on their own.
    let browsers = STATE.with_borrow_mut(|s| {
        s.phase = Phase::Closing;
        s.pending.clear();
        let open: Vec<Browser> = s.browsers.iter().filter(|o| !s.closing.contains(&o.browser.identifier())).map(|o| o.browser.clone()).collect();
        s.closing.extend(open.iter().map(|b| b.identifier()));
        open
    });
    tracing::info!("browser: closing {} browser(s)", browsers.len());
    // Forced: no beforeunload prompt can hold up quitting.
    objc2::rc::autoreleasepool(|_| browsers.iter().for_each(close_browser));
    pump_until(&|s| s.browsers.is_empty() && s.devtools == 0);
    let left = STATE.with_borrow(|s| s.browsers.len() + s.devtools);
    if left > 0 {
        tracing::warn!("browser: {left} browser(s) still open after {CLOSE_DEADLINE:?}; CEF not shut down");
    } else {
        cef::shutdown();
        tracing::info!("browser: CEF shut down");
    }
    STATE.with_borrow_mut(|s| {
        s.phase = Phase::Down;
        s.app = None;
    });
}

fn short(url: &str) -> String {
    if url.len() <= 80 { url.to_string() } else { format!("{}…", &url[..url.floor_char_boundary(77)]) }
}

wrap_app! {
    struct HerdrCefApp {
        handler: BrowserProcessHandler,
    }

    impl App {
        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            let Some(command_line) = command_line else { return };
            // A mock key for the profile's cookie encryption: the spike never
            // touches the person's login keychain. Don't sign into real
            // accounts in this profile.
            command_line.append_switch(Some(&CefString::from("use-mock-keychain")));
            // Chromium clones the whole app bundle on every launch for its
            // updater (MacAppCodeSignClone); Herdr has its own updater.
            let name = CefString::from("disable-features");
            let existing = CefString::from(&command_line.switch_value(Some(&name))).to_string();
            let features = if existing.is_empty() {
                "MacAppCodeSignClone".to_string()
            } else {
                format!("{existing},MacAppCodeSignClone")
            };
            command_line.append_switch_with_value(Some(&name), Some(&CefString::from(features.as_str())));
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(self.handler.clone())
        }
    }
}

wrap_browser_process_handler! {
    struct HerdrProcessHandler;

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            context_ready();
        }

        fn on_schedule_message_pump_work(&self, delay_ms: i64) {
            if let Some(tx) = PUMP.get() {
                let _ = tx.try_send(delay_ms);
            }
        }
    }
}

wrap_client! {
    struct HerdrClient {
        life_span: LifeSpanHandler,
        load: LoadHandler,
        display: DisplayHandler,
        focus: FocusHandler,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.life_span.clone())
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(self.load.clone())
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(self.display.clone())
        }

        fn focus_handler(&self) -> Option<FocusHandler> {
            Some(self.focus.clone())
        }
    }
}

wrap_life_span_handler! {
    struct HerdrLifeSpan {
        id: String,
        generation: u64,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser else { return };
            let view = browser.host().map(|h| h.window_handle().cast::<NSView>());
            // SAFETY: CEF's browser view, read on the main thread.
            let placed = view.and_then(|v| unsafe { v.as_ref() }).map(|v| {
                let f = v.frame();
                // CEF sized the page to its container when it was asked for;
                // the block may have placed the container since (the first
                // browser waits for CEF to start), and place() only resizes
                // pages that exist. Fill the container now.
                // SAFETY: a main-thread read of the view tree.
                let fitted = unsafe { v.superview() }.map(|parent| {
                    let size = parent.bounds().size;
                    if f.size != size {
                        v.setFrame(objc2_foundation::NSRect::new(objc2_foundation::NSPoint::new(0.0, 0.0), size));
                    }
                    format!(", fits its block at {}x{}", size.width, size.height)
                });
                format!("{}x{}, in a window: {}{}", f.size.width, f.size.height, v.window().is_some(), fitted.unwrap_or_default())
            });
            tracing::info!("browser: created {} (cef {}), view {}", self.id, browser.identifier(), placed.unwrap_or_else(|| "missing".into()));
            let browser = browser.clone();
            // Its block went while CEF was creating it: close it right away.
            let wanted = STATE.with_borrow_mut(|s| {
                s.creating = s.creating.saturating_sub(1);
                s.browsers.push(Open { id: self.id.clone(), generation: self.generation, browser: browser.clone() });
                s.is_current(&self.id, self.generation)
            });
            if !wanted {
                tracing::info!("browser: {} (cef {}) is no longer wanted; closing it", self.id, browser.identifier());
                STATE.with_borrow_mut(|s| s.closing.push(browser.identifier()));
                close_browser(&browser);
            }
        }

        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut c_int>,
        ) -> c_int {
            // No top-level Chromium windows: a popup or a new-tab link opens in this pane.
            if let Some(url) = target_url.map(|u| u.to_string()).filter(|u| !u.is_empty())
                && is_current(&self.id, self.generation)
            {
                tracing::info!("browser: {} popup {} opens in its pane", self.id, short(&url));
                STATE.with_borrow_mut(|s| s.navigations.push((self.id.clone(), url)));
            }
            1
        }

        fn do_close(&self, browser: Option<&mut Browser>) -> c_int {
            // 0 would make CEF send performClose: to the browser's top-level
            // window: Herdr's own. 1: Herdr closes it. Closes Herdr starts
            // land here too (ignored); a page that closes itself
            // (window.close()) asks the window to close its pane.
            let Some(browser) = browser else { return 1 };
            let cef = browser.identifier();
            let ours = STATE.with_borrow(|s| s.phase == Phase::Closing || s.closing.contains(&cef));
            if !ours && is_current(&self.id, self.generation) {
                tracing::info!("browser: {} asked to close", self.id);
                send(Event::CloseAsked(self.id.clone()));
            }
            1
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            let cef = browser.map(|b| b.identifier());
            STATE.with_borrow_mut(|s| {
                // By CEF's identifier: a newer browser with the same Herdr id stays.
                s.browsers.retain(|o| Some(o.browser.identifier()) != cef);
                s.closing.retain(|b| Some(*b) != cef);
                if s.is_current(&self.id, self.generation) {
                    s.current.remove(&self.id);
                    s.pages.remove(&self.id);
                }
            });
            tracing::info!("browser: closed {} (cef {})", self.id, cef.unwrap_or_default());
        }
    }
}

wrap_load_handler! {
    struct HerdrLoad {
        id: String,
        generation: u64,
    }

    impl LoadHandler {
        fn on_loading_state_change(&self, _browser: Option<&mut Browser>, is_loading: c_int, can_go_back: c_int, can_go_forward: c_int) {
            page_changed(&self.id, self.generation, |p| {
                p.loading = is_loading == 1;
                p.can_go_back = can_go_back == 1;
                p.can_go_forward = can_go_forward == 1;
            });
        }

        fn on_load_end(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, http_status_code: c_int) {
            let Some(frame) = frame else { return };
            if frame.is_main() == 1 {
                let url = CefString::from(&frame.url()).to_string();
                tracing::info!("browser: {} load end {} status {http_status_code}", self.id, short(&url));
            }
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            if frame.is_some_and(|f| f.is_main() == 1) {
                let text = error_text.map(|t| t.to_string()).unwrap_or_default();
                let url = failed_url.map(|u| u.to_string()).unwrap_or_default();
                tracing::warn!("browser: {} load error {} {error_code:?} {text}", self.id, short(&url));
            }
        }
    }
}

wrap_display_handler! {
    struct HerdrDisplay {
        id: String,
        generation: u64,
    }

    impl DisplayHandler {
        fn on_address_change(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, url: Option<&CefString>) {
            if frame.is_some_and(|f| f.is_main() == 1) {
                let url = url.map(|u| u.to_string()).unwrap_or_default();
                page_changed(&self.id, self.generation, |p| p.url = url);
            }
        }

        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            let title = title.map(|t| t.to_string()).unwrap_or_default();
            page_changed(&self.id, self.generation, |p| p.title = title);
        }
    }
}

wrap_focus_handler! {
    struct HerdrFocus {
        id: String,
        generation: u64,
    }

    impl FocusHandler {
        /// CEF asks before a page takes focus (it does for every browser it
        /// creates, and on loads): refused unless the window gave this page
        /// the keyboard or the person just pressed the mouse in it.
        fn on_set_focus(&self, _browser: Option<&mut Browser>, source: FocusSource) -> c_int {
            let source = if source == FocusSource::NAVIGATION { "navigation" } else { "system" };
            let current = is_current(&self.id, self.generation);
            let allowed = current && view::may_take_focus(&self.id);
            if !allowed {
                tracing::debug!("browser: {} asked for the keyboard ({source}); refused, it isn't the page that should have it", self.id);
            }
            c_int::from(!allowed)
        }

        fn on_got_focus(&self, _browser: Option<&mut Browser>) {
            if is_current(&self.id, self.generation) {
                send(Event::Focused(self.id.clone()));
            }
        }
    }
}

wrap_client! {
    struct DevToolsClient {
        life_span: LifeSpanHandler,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.life_span.clone())
        }
    }
}

// CEF's DevTools window: counted, so quit waits for it to close too.
wrap_life_span_handler! {
    struct DevToolsLifeSpan;

    impl LifeSpanHandler {
        fn on_after_created(&self, _browser: Option<&mut Browser>) {
            let open = STATE.with_borrow_mut(|s| {
                s.devtools += 1;
                s.devtools
            });
            tracing::info!("browser: a DevTools window opened ({open} open)");
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            let open = STATE.with_borrow_mut(|s| {
                s.devtools = s.devtools.saturating_sub(1);
                s.devtools
            });
            tracing::info!("browser: a DevTools window closed ({open} open)");
        }
    }
}
