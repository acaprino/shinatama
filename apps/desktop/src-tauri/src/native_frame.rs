//! What makes a Shinatama window behave as a Windows application rather than a web page.
//! Every window gets it from `webview_recovery::build`; the main window also gets its frame.
//!
//! - Navigation: a webview shows only the app's own pages. Any other navigation is refused,
//!   and no page opens a browser window of its own (Ctrl+click, middle click, `window.open`).
//! - Browser commands: a release build turns WebView2's browser accelerators off (F5, Ctrl+R,
//!   Ctrl+P, Ctrl+F, F12, Alt+Left...). Editing keys and every key the page handles keep
//!   working; a development build keeps them for reload and DevTools.
//! - Snap Layouts: the title bar is HTML and WebView2 covers the whole client area, so Windows
//!   never asks this window what lies under the cursor. A child window that never paints lies
//!   over the HTML maximize button and answers `HTMAXBUTTON`: Windows 11 offers its layouts on
//!   hover, and a click maximizes or restores. It tells the page about hover and press.
//! - Reload: the window menu (Alt+Space, Shift+right click on the taskbar button) offers
//!   "Reload interface", which works when the page is blank or hung.
use std::{
    sync::{
        Mutex, Once, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, Url, WebviewWindow};
use windows_sys::{
    Win32::{
        Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::{GetStockObject, NULL_BRUSH, ValidateRect},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
            Input::KeyboardAndMouse::{TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent},
            Shell::{DefSubclassProc, SetWindowSubclass},
            WindowsAndMessaging::{
                AppendMenuW, CreateWindowExW, DefWindowProcW, GetParent, GetPropW, GetSystemMenu,
                HTMAXBUTTON, HWND_TOP, IDC_ARROW, IsWindow, IsZoomed, LoadCursorW, MF_SEPARATOR,
                MF_STRING, RegisterClassExW, RemovePropW, SC_MAXIMIZE, SC_RESTORE, SM_CYFRAME,
                SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SendMessageW, SetCursor, SetPropW,
                SetWindowPos, ShowWindow, WM_DESTROY, WM_ERASEBKGND, WM_NCHITTEST,
                WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_PAINT,
                WM_SETCURSOR, WM_SYSCOMMAND, WNDCLASSEXW, WS_CHILD, WS_CLIPSIBLINGS, WS_VISIBLE,
            },
        },
    },
    core::w,
};

static APP: OnceLock<AppHandle> = OnceLock::new();

/// The navigation every window builder registers: the app's own origin, and in a development
/// build the dev server's. Scheme, host and port are compared exactly, never by prefix.
pub fn navigation_policy(app: &AppHandle) -> impl Fn(&Url) -> bool + Send + 'static {
    let dev = if tauri::is_dev() { app.config().build.dev_url.clone() } else { None };
    move |url| {
        let own = matches!(url.scheme(), "http" | "https")
            && url.host_str() == Some("tauri.localhost")
            && url.port().is_none();
        own || dev.as_ref().is_some_and(|dev| {
            dev.scheme() == url.scheme()
                && dev.host_str() == url.host_str()
                && dev.port_or_known_default() == url.port_or_known_default()
        })
    }
}

/// Applied to every window right after it is built.
pub fn prepare(window: &WebviewWindow) {
    let _ = APP.set(window.app_handle().clone());
    if !tauri::is_dev() {
        eprintln!("[native_frame] prepare {} accelerators off", window.label());
        let _ = window.with_webview(|webview| {
            use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
            use windows_core::Interface;
            // SAFETY: run by Tauri on the thread that owns the webview, on live COM objects.
            unsafe {
                let Ok(core) = webview.controller().CoreWebView2() else { return };
                let Ok(settings) = core.Settings() else { return };
                // WebView2's default is TRUE. Copy, cut, paste and the other editing keys stay.
                if let Ok(settings) = settings.cast::<ICoreWebView2Settings3>() {
                    let r = settings.SetAreBrowserAcceleratorKeysEnabled(false);
                    eprintln!("[native_frame] SetAreBrowserAcceleratorKeysEnabled {r:?}");
                } else { eprintln!("[native_frame] no Settings3"); }
            }
        });
    }
    if window.label() == "main"
        && let Ok(hwnd) = window.hwnd()
    {
        let hwnd = hwnd.0 as isize;
        let _ = window.run_on_main_thread(move || {
            // SAFETY: on the thread that owns the window, which is checked to be alive.
            unsafe { add_reload_command(hwnd as HWND) };
        });
    }
}

// ---- Reload from the window menu -----------------------------------------------------------

/// Below 0xF000, the range of Windows' own commands, with the low four bits clear.
const RELOAD_COMMAND: usize = 0x5350;
const FRAME_SUBCLASS: usize = 0x5348_4e46; // "SHNF"
const MENU_PROP: *const u16 = w!("ShinatamaReloadCommand");

unsafe fn add_reload_command(hwnd: HWND) {
    unsafe {
        if IsWindow(hwnd) == 0 || !GetPropW(hwnd, MENU_PROP).is_null() {
            return;
        }
        let menu = GetSystemMenu(hwnd, 0);
        if menu.is_null() {
            return;
        }
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, RELOAD_COMMAND, w!("Reload interface"));
        SetWindowSubclass(hwnd, Some(frame_proc), FRAME_SUBCLASS, 0);
        SetPropW(hwnd, MENU_PROP, 1 as HANDLE);
    }
}

unsafe extern "system" fn frame_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_SYSCOMMAND && wparam & 0xFFF0 == RELOAD_COMMAND {
        request_reload();
        return 0;
    }
    if msg == WM_DESTROY {
        // SAFETY: the window is still alive while it handles its own destruction.
        unsafe { RemovePropW(hwnd, MENU_PROP) };
    }
    // SAFETY: forwards the message to the next procedure of this live window.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// The reload the page has not answered yet: the page or the timeout reloads, never both.
static PENDING_RELOAD: Mutex<Option<u64>> = Mutex::new(None);
static RELOAD_TOKEN: AtomicU64 = AtomicU64::new(0);

/// A live page may hold a layout not saved yet: it is asked to save first. A dead or hung
/// page does not answer, and is reloaded after the timeout.
fn request_reload() {
    let Some(main) = APP.get().and_then(|app| app.get_webview_window("main")) else { return };
    let token = RELOAD_TOKEN.fetch_add(1, Ordering::AcqRel) + 1;
    *PENDING_RELOAD.lock().unwrap_or_else(|error| error.into_inner()) = Some(token);
    let _ = main.emit_to("main", "interface-reload-requested", token);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        if take_pending(token) {
            eprintln!("[native_frame] reload after timeout");
            let _ = main.reload();
        }
    });
}

fn take_pending(token: u64) -> bool {
    let mut pending = PENDING_RELOAD.lock().unwrap_or_else(|error| error.into_inner());
    if *pending == Some(token) {
        *pending = None;
        return true;
    }
    false
}

/// The page saved what it could and is ready to be reloaded.
#[tauri::command]
pub fn interface_reload_ready(window: WebviewWindow, token: u64) {
    eprintln!("[native_frame] reload ready {token}");
    if window.label() != "main" || !take_pending(token) {
        return;
    }
    // After this command has answered.
    std::thread::spawn(move || {
        let _ = window.reload();
    });
}

// ---- Snap Layouts on the HTML maximize button ----------------------------------------------

const OVERLAY_CLASS: *const u16 = w!("ShinatamaMaximizeOverlay");
const OVERLAY_PROP: *const u16 = w!("ShinatamaMaximizeOverlay");
static OVERLAY_CLASS_REGISTERED: Once = Once::new();
static HOVER: AtomicBool = AtomicBool::new(false);
static PRESSED: AtomicBool = AtomicBool::new(false);

/// The page reports where its maximize button is, in physical pixels of the client area,
/// on every layout change; an empty rectangle removes the button from Windows' hit test.
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
    let hwnd = window.hwnd().map_err(|error| error.to_string())?.0 as isize;
    window
        .run_on_main_thread(move || {
            // SAFETY: on the thread that owns the window, which is checked to be alive.
            unsafe { place_overlay(hwnd as HWND, x, y, width, height) };
        })
        .map_err(|error| error.to_string())
}

unsafe fn place_overlay(parent: HWND, x: i32, y: i32, width: i32, height: i32) {
    unsafe {
        eprintln!("[native_frame] place_overlay {x} {y} {width} {height} alive={}", IsWindow(parent));
        if IsWindow(parent) == 0 {
            return;
        }
        let mut overlay = GetPropW(parent, OVERLAY_PROP) as HWND;
        if width <= 0 || height <= 0 {
            if !overlay.is_null() {
                ShowWindow(overlay, SW_HIDE);
            }
            return;
        }
        if overlay.is_null() {
            let instance = GetModuleHandleW(std::ptr::null());
            OVERLAY_CLASS_REGISTERED.call_once(|| {
                let class = WNDCLASSEXW {
                    cbSize: size_of::<WNDCLASSEXW>() as u32,
                    lpfnWndProc: Some(overlay_proc),
                    hInstance: instance,
                    hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
                    // Never painted: the HTML button shows through.
                    hbrBackground: GetStockObject(NULL_BRUSH),
                    lpszClassName: OVERLAY_CLASS,
                    ..std::mem::zeroed()
                };
                RegisterClassExW(&class);
            });
            // The window text is the name UI Automation reads for the hit-test area.
            overlay = CreateWindowExW(
                0,
                OVERLAY_CLASS,
                w!("Maximize"),
                WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
                x,
                y,
                width,
                height,
                parent,
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            );
            if overlay.is_null() {
                eprintln!("[native_frame] overlay CreateWindowExW failed: {}", windows_sys::Win32::Foundation::GetLastError());
                return;
            }
            SetPropW(parent, OVERLAY_PROP, overlay as HANDLE);
        }
        // A restored window keeps its top border for resizing, as Windows' own buttons do.
        let top = if IsZoomed(parent) != 0 {
            y
        } else {
            y.max(GetSystemMetricsForDpi(SM_CYFRAME, GetDpiForWindow(parent)))
        };
        // Above the webview and Tauri's resize border, both created before it.
        SetWindowPos(
            overlay,
            HWND_TOP,
            x,
            top,
            width,
            (y + height - top).max(1),
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

fn report_caption() {
    let state = serde_json::json!({
        "hover": HOVER.load(Ordering::Acquire),
        "pressed": PRESSED.load(Ordering::Acquire),
    });
    if let Some(app) = APP.get() {
        let _ = app.emit_to("main", "caption-maximize", state);
    }
}

unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: every call below acts on this live window or its parent.
    unsafe {
        match msg {
            WM_NCHITTEST => {
                if !HOVER.swap(true, Ordering::AcqRel) {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE | TME_NONCLIENT,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    TrackMouseEvent(&mut track);
                    report_caption();
                }
                HTMAXBUTTON as LRESULT
            }
            WM_NCMOUSELEAVE => {
                HOVER.store(false, Ordering::Release);
                PRESSED.store(false, Ordering::Release);
                report_caption();
                0
            }
            WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK if wparam == HTMAXBUTTON as WPARAM => {
                PRESSED.store(true, Ordering::Release);
                report_caption();
                0
            }
            WM_NCLBUTTONUP if wparam == HTMAXBUTTON as WPARAM => {
                // Released where it was pressed: leaving the button first cancels, as in Windows.
                if PRESSED.swap(false, Ordering::AcqRel) {
                    report_caption();
                    let parent = GetParent(hwnd);
                    let command = if IsZoomed(parent) != 0 { SC_RESTORE } else { SC_MAXIMIZE };
                    SendMessageW(parent, WM_SYSCOMMAND, command as WPARAM, 0);
                }
                0
            }
            WM_SETCURSOR => {
                SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_ARROW));
                1
            }
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                ValidateRect(hwnd, std::ptr::null());
                0
            }
            WM_DESTROY => {
                let parent = GetParent(hwnd);
                if !parent.is_null() && GetPropW(parent, OVERLAY_PROP) as HWND == hwnd {
                    RemovePropW(parent, OVERLAY_PROP);
                }
                HOVER.store(false, Ordering::Release);
                PRESSED.store(false, Ordering::Release);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
