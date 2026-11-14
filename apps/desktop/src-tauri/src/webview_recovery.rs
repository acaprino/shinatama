//! A window never stays an empty shell. When WebView2 loses the process that draws a window,
//! the window is transparent glass with nothing in it and takes no input. The terminals live
//! in this process and survive; the page reattaches them when it loads (`terminal_list`).
//!
//! - The browser process exits: every webview of the application is dead. All configured
//!   windows are destroyed and built again, the main one where it was.
//! - A render process exits: only that page is dead. It is reloaded.
//!
//! Other WebView2 processes (GPU, utility) are restarted by WebView2 itself.
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};
use webview2_com::{
    Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
    },
    ProcessFailedEventHandler,
};

/// True from the loss of the browser process until the windows are built again. While it is
/// set, the destruction of the windows is not the end of the application.
static RECOVERING: AtomicBool = AtomicBool::new(false);

pub fn recovering() -> bool {
    RECOVERING.load(Ordering::Acquire)
}

/// Builds one configured window and watches its webview.
pub fn build(app: &AppHandle, config: &tauri::utils::config::WindowConfig) -> tauri::Result<WebviewWindow> {
    let window = tauri::WebviewWindowBuilder::from_config(app, config)?.build()?;
    watch(&window);
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
    let _ = window.with_webview(move |webview| {
        // SAFETY: called on the thread that owns the webview, on live COM objects.
        unsafe {
            let Ok(core) = webview.controller().CoreWebView2() else { return };
            let handler = ProcessFailedEventHandler::create(Box::new(move |sender, args| {
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
                } else if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
                    && let Some(sender) = sender
                {
                    sender.Reload()?;
                }
                Ok(())
            }));
            let mut token = 0i64;
            let _ = core.add_ProcessFailed(&handler, &mut token);
        }
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
