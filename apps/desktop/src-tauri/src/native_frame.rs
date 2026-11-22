//! Native chrome for the main window. Imago owns the Windows frame and WebView2 controls.
//! Shinatama asks the page to save its workspace before an interface reload.
use imago_shell::{
    SnapLayouts, disable_browser_accelerators, install_reload_menu, install_snap_layouts,
};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tauri::{Emitter, WebviewWindow};

/// A rebuilt main window gets an overlay for its new HWND.
static MAIN_OVERLAY: Mutex<Option<SnapLayouts>> = Mutex::new(None);

pub fn prepare(window: &WebviewWindow) {
    if !tauri::is_dev() {
        let _ = disable_browser_accelerators(window);
    }
    if window.label() == "main" {
        let overlay = install_snap_layouts(window).ok();
        *MAIN_OVERLAY
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = overlay;
        let main = window.clone();
        let _ = install_reload_menu(window, "Reload interface", move || {
            request_reload(main.clone())
        });
    }
}

/// The reload the page has not answered yet: the page or the timeout reloads, never both.
static PENDING_RELOAD: Mutex<Option<u64>> = Mutex::new(None);
static RELOAD_TOKEN: AtomicU64 = AtomicU64::new(0);

/// A live page may hold a layout not saved yet: it is asked to save first. A dead or hung
/// page does not answer, and is reloaded after the timeout.
fn request_reload(main: WebviewWindow) {
    let token = RELOAD_TOKEN.fetch_add(1, Ordering::AcqRel) + 1;
    *PENDING_RELOAD
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(token);
    let _ = main.emit_to("main", "interface-reload-requested", token);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        if take_pending(token) {
            let _ = main.reload();
        }
    });
}

fn take_pending(token: u64) -> bool {
    let mut pending = PENDING_RELOAD
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if *pending == Some(token) {
        *pending = None;
        return true;
    }
    false
}

/// The page saved what it could and is ready to be reloaded.
#[tauri::command]
pub fn interface_reload_ready(window: WebviewWindow, token: u64) {
    if window.label() != "main" || !take_pending(token) {
        return;
    }
    // After this command has answered.
    std::thread::spawn(move || {
        let _ = window.reload();
    });
}

/// Receives the HTML maximize button's bounds in physical client pixels.
#[tauri::command]
pub fn window_caption_maximize(
    window: WebviewWindow,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Only the main window has a title bar".into());
    }
    let overlay = MAIN_OVERLAY
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
        .ok_or("The main window's caption overlay is unavailable")?;
    overlay.set_rect(x, y, width, height);
    Ok(())
}
