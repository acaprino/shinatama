//! The window of the attention widget: a small page that floats over the desktop, always on top, and never takes the focus.
//!
//! It is not the main window, so the shell (`imago-shell`) does not open or watch it: this file builds it with Tauri's builder
//! from its entry in `tauri.conf.json` (`"create": false`), hidden, with the arguments of the main window's browser (WebView2
//! refuses a second webview on the same data folder with other arguments) and with the same navigation policy: it shows its own
//! page and nothing else. The shell rebuilds only the main window when WebView2 loses its browser process, and that takes the
//! widget's page with it, so the widget is destroyed with the main window and built again where it is needed (`ensure`).
use std::sync::{Mutex, PoisonError};
use tauri::{
    AppHandle, Manager, Url, WebviewWindow, WebviewWindowBuilder, webview::NewWindowResponse,
};

/// The label of the widget in `tauri.conf.json`.
pub const LABEL: &str = "attention";

/// Whether `url` is a page of the app: its embedded frontend, or the dev server in development.
fn own_page(url: &Url, embedded: &[&str], dev: Option<&Url>) -> bool {
    let origin = url.origin().ascii_serialization();
    embedded.contains(&origin.as_str()) || dev.is_some_and(|dev| dev.origin() == url.origin())
}

/// The origins the embedded frontend is served from: Tauri's protocol, mapped to http(s)://tauri.localhost on Windows.
fn embedded_origins(use_https: bool) -> Vec<&'static str> {
    if cfg!(any(windows, target_os = "android")) {
        vec![if use_https {
            "https://tauri.localhost"
        } else {
            "http://tauri.localhost"
        }]
    } else {
        vec!["tauri://localhost"]
    }
}

/// Builds the widget, hidden.
pub fn build(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let windows = &app.config().app.windows;
    let mut config = windows
        .iter()
        .find(|window| window.label == LABEL)
        .cloned()
        .ok_or(tauri::Error::WindowNotFound)?;
    if let Some(main) = windows.iter().find(|window| window.label == "main") {
        config.additional_browser_args = Some(crate::webview_args::shared(main));
    }
    let https = config.use_https_scheme;
    let dev = tauri::is_dev()
        .then(|| app.config().build.dev_url.clone())
        .flatten();
    let embedded = embedded_origins(https);
    let window = WebviewWindowBuilder::from_config(app, &config)?
        .on_navigation(move |url| own_page(url, &embedded, dev.as_ref()))
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .build()?;
    // A webview nobody sees gives its memory back.
    #[cfg(windows)]
    crate::responsiveness::set_memory_target(&window, false);
    Ok(window)
}

/// Held while the widget is built: two alerts that arrive together would both find it missing, and the second build would fail
/// on the label of the first and leave its alert without a window.
static BUILDING: Mutex<()> = Mutex::new(());

/// The widget, built when it is not there and the main window is (not while the application closes).
pub fn ensure(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(widget) = app.get_webview_window(LABEL) {
        return Some(widget);
    }
    let _building = BUILDING.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(widget) = app.get_webview_window(LABEL) {
        return Some(widget);
    }
    app.get_webview_window("main")?;
    build(app).ok()
}

/// Where the widget goes: the bottom right corner of the work area (the screen without the taskbar) of the monitor the main window
/// is on, or the primary one when the main window has none (it is minimized), in physical pixels, negative coordinates and
/// scaling included. Its size is the window's own, from `tauri.conf.json`, read as it is now.
pub fn corner(app: &AppHandle, widget: &WebviewWindow) -> Option<(i32, i32)> {
    let monitor = app
        .get_webview_window("main")
        .and_then(|main| main.current_monitor().ok().flatten())
        .or_else(|| widget.primary_monitor().ok().flatten())?;
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let (width, height) = widget
        .outer_size()
        .ok()
        .filter(|size| size.width > 0 && size.height > 0)
        .map_or(
            ((440.0 * scale).ceil() as i32, (190.0 * scale).ceil() as i32),
            |size| (size.width as i32, size.height as i32),
        );
    let margin = (16.0 * scale).ceil() as i32;
    Some((
        area.position.x + (area.size.width as i32 - width - margin).max(0),
        area.position.y + (area.size.height as i32 - height - margin).max(0),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    #[test]
    fn the_widget_shows_its_own_page_and_the_dev_server_only() {
        let embedded = ["http://tauri.localhost"];
        let dev = url("http://localhost:1420/");
        assert!(own_page(
            &url("http://tauri.localhost/widget.html"),
            &embedded,
            None
        ));
        assert!(!own_page(&url("https://tauri.localhost/"), &embedded, None));
        assert!(!own_page(&url("https://example.com/"), &embedded, None));
        assert!(!own_page(
            &url("http://localhost:1420/widget.html"),
            &embedded,
            None
        ));
        assert!(own_page(
            &url("http://localhost:1420/widget.html"),
            &embedded,
            Some(&dev)
        ));
        assert!(!own_page(
            &url("http://localhost:1421/"),
            &embedded,
            Some(&dev)
        ));
    }
}
