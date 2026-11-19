//! A window never stays an empty shell. When WebView2 loses the process that draws a window,
//! the window is transparent glass with nothing in it and takes no input. The terminals live
//! in this process and survive; the page reattaches them when it loads (`terminal_list`).
//!
//! - The browser process exits: every webview of the application is dead. All configured
//!   windows are destroyed and built again, the main one where it was.
//! - A render process exits: only that page is dead. It is reloaded.
//! - The page fails to load: it is loaded again after a second.
//! - A render process stops responding: the main window asks whether to reload it.
//!
//! Other WebView2 processes (GPU, utility) are restarted by WebView2 itself. WebView2's own
//! error page ("can't reach this page") is off: it has no way back into the app. A page is
//! reloaded at most three times a minute; past that the main window asks the user, so a page
//! that fails at once cannot reload forever.
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use webview2_com::{
    Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
        COREWEBVIEW2_WEB_ERROR_STATUS, COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED,
    },
    NavigationCompletedEventHandler, ProcessFailedEventHandler,
};

/// True from the loss of the browser process until the windows are built again. While it is
/// set, the destruction of the windows is not the end of the application.
static RECOVERING: AtomicBool = AtomicBool::new(false);

pub fn recovering() -> bool {
    RECOVERING.load(Ordering::Acquire)
}

/// Builds one configured window and watches its webview.
pub fn build(app: &AppHandle, config: &tauri::utils::config::WindowConfig) -> tauri::Result<WebviewWindow> {
    // The main window opens on the backdrop of the look its page chose last (presentation.rs).
    let mut config = config.clone();
    if config.label == "main"
        && let Some(theme) = crate::presentation::saved_window_theme(app)
    {
        config.theme = theme;
    }
    let config = &config;
    let window = tauri::WebviewWindowBuilder::from_config(app, config)?
        .on_navigation(crate::native_frame::navigation_policy(app))
        .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
        .build()?;
    watch(&window);
    crate::native_frame::prepare(&window);
    // A window built hidden (the alert widget) holds only what it needs until shown.
    if config.visible {
        if config.label == "main" {
            crate::responsiveness::main_rebuilt();
        }
    } else {
        crate::responsiveness::set_memory_target(&window, false);
    }
    Ok(window)
}

fn watch(window: &WebviewWindow) {
    let app = window.app_handle().clone();
    let failed = window.clone();
    let loaded = window.clone();
    let _ = window.with_webview(move |webview| {
        // SAFETY: called on the thread that owns the webview, on live COM objects.
        unsafe {
            let Ok(core) = webview.controller().CoreWebView2() else { return };
            // WebView2's default is TRUE. From the next navigation a failed load shows a blank
            // page, and NavigationCompleted below takes over.
            if let Ok(settings) = core.Settings() {
                let _ = settings.SetIsBuiltInErrorPageEnabled(false);
            }
            let handler = ProcessFailedEventHandler::create(Box::new(move |_, args| {
                let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                if let Some(args) = args {
                    args.ProcessFailedKind(&mut kind)?;
                }
                if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED {
                    // Each webview reports the same loss; the first report rebuilds them all.
                    if !RECOVERING.swap(true, Ordering::AcqRel) {
                        let app = app.clone();
                        // Windows are destroyed and built off the thread that runs this callback.
                        std::thread::spawn(move || rebuild(&app));
                    }
                } else if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED {
                    reload_or_ask(&failed, Failure::Stopped);
                } else if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE {
                    ask(&failed, Failure::Unresponsive);
                }
                Ok(())
            }));
            let mut token = 0i64;
            let _ = core.add_ProcessFailed(&handler, &mut token);
            let handler = NavigationCompletedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut success = windows_core::BOOL(1);
                args.IsSuccess(&mut success)?;
                let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                args.WebErrorStatus(&mut status)?;
                // Cancelled is not failed: a navigation the policy refused, or one a newer replaced.
                if !success.as_bool() && status != COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED && !recovering() {
                    reload_or_ask(&loaded, Failure::NotLoaded(status.0));
                }
                Ok(())
            }));
            let _ = core.add_NavigationCompleted(&handler, &mut token);
        }
    });
}

#[derive(Clone, Copy)]
enum Failure {
    /// The render process exited.
    Stopped,
    /// The page did not load, with WebView2's error status.
    NotLoaded(i32),
    /// The render process stopped responding.
    Unresponsive,
}

impl Failure {
    fn message(self) -> String {
        let alive = "Your terminals and agents are still running.";
        match self {
            Failure::Stopped => format!("The Shinatama interface stopped three times in a minute. {alive}\n\nReload the interface?"),
            Failure::NotLoaded(status) => format!("The Shinatama interface could not be loaded (WebView2 error {status}). {alive}\n\nTry loading it again?"),
            Failure::Unresponsive => format!("The Shinatama interface is not responding. {alive}\n\nReload the interface now?"),
        }
    }
}

/// Reloads of each window in the last minute.
static RELOADS: Mutex<Vec<(String, Instant)>> = Mutex::new(Vec::new());
const RELOAD_LIMIT: usize = 3;
const RELOAD_PERIOD: Duration = Duration::from_secs(60);
/// One question at a time: several failures of one loss are one question.
static ASKING: AtomicBool = AtomicBool::new(false);

fn reload_or_ask(window: &WebviewWindow, failure: Failure) {
    let now = Instant::now();
    let allowed = {
        let mut reloads = RELOADS.lock().unwrap_or_else(|error| error.into_inner());
        reloads.retain(|(_, at)| now.duration_since(*at) < RELOAD_PERIOD);
        let label = window.label();
        let allowed = reloads.iter().filter(|(of, _)| of == label).count() < RELOAD_LIMIT;
        if allowed {
            reloads.push((label.to_owned(), now));
        }
        allowed
    };
    if !allowed {
        ask(window, failure);
        return;
    }
    let window = window.clone();
    // Never inside the WebView2 callback. A failed load waits a moment before it tries again.
    std::thread::spawn(move || {
        if matches!(failure, Failure::NotLoaded(_)) {
            std::thread::sleep(Duration::from_secs(1));
        }
        let _ = window.reload();
    });
}

/// The main window asks, owning the question; other windows stay as they are until they are
/// built again with the main one.
fn ask(window: &WebviewWindow, failure: Failure) {
    if window.label() != "main" || ASKING.swap(true, Ordering::AcqRel) {
        return;
    }
    let window = window.clone();
    std::thread::spawn(move || {
        let target = window.clone();
        window
            .app_handle()
            .dialog()
            .message(failure.message())
            .title("Shinatama")
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom("Reload".into(), "Not now".into()))
            .parent(&window)
            .show(move |reload| {
                if reload {
                    RELOADS
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .retain(|(of, _)| of != target.label());
                    let _ = target.reload();
                }
                ASKING.store(false, Ordering::Release);
            });
    });
}

struct Placement {
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    maximized: bool,
}

fn rebuild(app: &AppHandle) {
    let configs = app.config().app.windows.clone();
    let placement = app.get_webview_window("main").and_then(|main| {
        let maximized = main.is_maximized().ok()?;
        if main.is_minimized().ok()? {
            return None;
        }
        Some(Placement { position: main.outer_position().ok()?, size: main.inner_size().ok()?, maximized })
    });
    for config in &configs {
        if let Some(window) = app.get_webview_window(&config.label) {
            let _ = window.destroy();
        }
    }
    // A label is free only once its window is gone.
    let deadline = Instant::now() + Duration::from_secs(5);
    while configs.iter().any(|config| app.get_webview_window(&config.label).is_some()) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    for config in &configs {
        let Ok(window) = build(app, config) else { continue };
        if config.label != "main" {
            continue;
        }
        if let Some(placement) = &placement {
            if placement.maximized {
                let _ = window.maximize();
            } else {
                let _ = window.set_position(placement.position);
                let _ = window.set_size(placement.size);
            }
        }
        let _ = window.set_focus();
    }
    RECOVERING.store(false, Ordering::Release);
}
