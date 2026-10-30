//! CEF's lifetime in the app: loaded and started on the first browser, pumped
//! from GPUI's foreground executor, shut down at quit after every browser
//! has closed.
//!
//! macOS runs CEF's UI thread on the main thread, so every CEF call and
//! callback here happens there, except `on_schedule_message_pump_work`, which
//! may come from any thread and only sends on a channel.

use std::cell::{Cell, RefCell};
use std::ffi::{CString, c_int};
use std::os::unix::ffi::OsStrExt as _;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use cef::*;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, AnyProtocol, Bool};
use objc2::{MainThreadMarker, msg_send};
use objc2_app_kit::NSView;

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

#[derive(Default)]
struct State {
    phase: Phase,
    /// Browsers asked for before CEF's context was ready.
    pending: Vec<(Retained<NSView>, String)>,
    /// Asked of CEF, `on_after_created` not seen yet.
    creating: usize,
    browsers: Vec<Browser>,
    /// Closes the page itself asked for (`window.close()`): finished after
    /// `do_close` returns, never inside it.
    tear_down: Vec<c_int>,
    app: Option<App>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    static PUMPING: Cell<bool> = const { Cell::new(false) };
}

/// `on_schedule_message_pump_work` → the foreground pump task.
static PUMP: OnceLock<async_channel::Sender<i64>> = OnceLock::new();

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
    finish_closes();
}

/// Closes a browser the way CEF wants for a child view: remove its view (the
/// superview holds the only reference, so it is freed and CEF sees its window
/// go), then a forced `CloseBrowser` as a backstop.
fn close(browser: &Browser) {
    let Some(host) = browser.host() else { return };
    let view = host.window_handle().cast::<NSView>();
    // SAFETY: CEF's browser view, on the main thread; CEF keeps no reference
    // of its own, so this only borrows it until it's removed.
    if let Some(view) = unsafe { view.as_ref() } {
        view.removeFromSuperview();
    }
    host.close_browser(1);
}

fn finish_closes() {
    let ids = STATE.with_borrow_mut(|s| std::mem::take(&mut s.tear_down));
    for id in ids {
        let browser = STATE.with_borrow(|s| s.browsers.iter().find(|b| b.identifier() == id).cloned());
        if let Some(browser) = browser {
            objc2::rc::autoreleasepool(|_| close(&browser));
        }
    }
}

/// Opens a browser filling `parent`, now or once CEF's context is ready.
pub fn open(parent: Retained<NSView>, url: String) {
    match phase() {
        Phase::Starting => STATE.with_borrow_mut(|s| s.pending.push((parent, url))),
        Phase::Ready => create(&parent, &url),
        other => tracing::warn!("browser: not opened, CEF is {other:?}"),
    }
}

fn create(parent: &NSView, url: &str) {
    let size = parent.bounds().size;
    let bounds = Rect { x: 0, y: 0, width: size.width as c_int, height: size.height as c_int };
    let handle: sys::cef_window_handle_t = (parent as *const NSView).cast_mut().cast();
    let window_info = WindowInfo::default().set_as_child(handle, &bounds);
    let mut client = HerdrClient::new(HerdrLifeSpan::new(), HerdrLoad::new());
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
        tracing::error!("browser: CEF refused to create a browser");
    } else {
        tracing::info!("browser: creating {}", short(url));
    }
}

fn context_ready() {
    let pending = STATE.with_borrow_mut(|s| {
        if s.phase == Phase::Starting {
            s.phase = Phase::Ready;
        }
        std::mem::take(&mut s.pending)
    });
    tracing::info!("browser: CEF context ready");
    for (parent, url) in pending {
        create(&parent, &url);
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
            finish_closes();
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    // A browser still being created can't be closed yet.
    pump_until(&|s| s.phase != Phase::Starting && s.creating == 0);
    let browsers = STATE.with_borrow_mut(|s| {
        s.phase = Phase::Closing;
        s.pending.clear();
        s.browsers.clone()
    });
    tracing::info!("browser: closing {} browser(s)", browsers.len());
    // Forced: no beforeunload prompt can hold up quitting.
    objc2::rc::autoreleasepool(|_| browsers.iter().for_each(close));
    pump_until(&|s| s.browsers.is_empty());
    let left = STATE.with_borrow(|s| s.browsers.len());
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
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.life_span.clone())
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(self.load.clone())
        }
    }
}

wrap_life_span_handler! {
    struct HerdrLifeSpan;

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser else { return };
            let view = browser.host().map(|h| h.window_handle().cast::<NSView>());
            // SAFETY: CEF's browser view, read on the main thread.
            let placed = view.and_then(|v| unsafe { v.as_ref() }).map(|v| {
                let f = v.frame();
                format!("{}x{} at {},{}, in a window: {}", f.size.width, f.size.height, f.origin.x, f.origin.y, v.window().is_some())
            });
            tracing::info!("browser: created {}, view {}", browser.identifier(), placed.unwrap_or_else(|| "missing".into()));
            let browser = browser.clone();
            STATE.with_borrow_mut(|s| {
                s.creating = s.creating.saturating_sub(1);
                s.browsers.push(browser);
            });
        }

        fn do_close(&self, browser: Option<&mut Browser>) -> c_int {
            // 0 would make CEF send performClose: to the browser's top-level
            // window: Herdr's own. 1: we finish the close ourselves, right
            // after this returns (a view freed inside DoClose is missed).
            // Closes Herdr starts remove the view first, so CEF skips this.
            if let Some(browser) = browser {
                let id = browser.identifier();
                tracing::info!("browser: do_close {id}");
                STATE.with_borrow_mut(|s| s.tear_down.push(id));
            }
            1
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser else { return };
            let id = browser.identifier();
            STATE.with_borrow_mut(|s| s.browsers.retain(|b| b.identifier() != id));
            tracing::info!("browser: closed {id}");
        }
    }
}

wrap_load_handler! {
    struct HerdrLoad;

    impl LoadHandler {
        fn on_load_end(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, http_status_code: c_int) {
            let Some(frame) = frame else { return };
            if frame.is_main() == 1 {
                let url = CefString::from(&frame.url()).to_string();
                tracing::info!("browser: load end {} status {http_status_code}", short(&url));
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
                tracing::warn!("browser: load error {} {error_code:?} {text}", short(&url));
            }
        }
    }
}
