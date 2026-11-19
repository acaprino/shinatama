//! The control window comes before everything Shinatama starts.
//!
//! - The thread that runs the window (messages, input, IPC dispatch) runs above the
//!   process's other threads, and Windows never slows this process to save power.
//!   Agents, shells and the model proxy start below normal priority (see the terminal
//!   helper and the contained workload launcher), so a busy agent waits for the CPU and
//!   the window does not.
//! - A webview nobody sees gives its memory back: a hidden window and a minimized main
//!   window ask WebView2 for its low memory target; shown again, the normal one.
//!
//! Every call here is advisory. A refusal leaves Windows' defaults in place and is not an
//! error: the window works the same, it only loses this preference.
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::WebviewWindow;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    ICoreWebView2_19,
};
use windows_core::Interface;

/// Called once, first thing on the thread that will run the event loop.
pub fn prioritize_control_thread() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentThread, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE,
        ProcessPowerThrottling, SetProcessInformation, SetThreadPriority,
        THREAD_PRIORITY_ABOVE_NORMAL,
    };
    // SAFETY: pseudo-handles of this process and thread; the record outlives the call.
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL);
        // ControlMask set and StateMask clear: never EcoQoS, even minimized, so the window
        // answers at full speed the moment it is shown again.
        let state = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: 0,
        };
        SetProcessInformation(
            GetCurrentProcess(),
            ProcessPowerThrottling,
            (&state as *const PROCESS_POWER_THROTTLING_STATE).cast(),
            size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );
    }
}

/// Low while nobody can see the window, normal while somebody can.
pub fn set_memory_target(window: &WebviewWindow, visible: bool) {
    let _ = window.with_webview(move |webview| {
        // SAFETY: run by Tauri on the thread that owns the webview, on live COM objects.
        unsafe {
            let Ok(core) = webview.controller().CoreWebView2() else { return };
            // Older WebView2 runtimes lack the interface: their default stays.
            let Ok(core) = core.cast::<ICoreWebView2_19>() else { return };
            let _ = core.SetMemoryUsageTargetLevel(if visible {
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
            } else {
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
            });
        }
    });
}

/// The main window's last known state, so a resize asks WebView2 only on a change.
static MAIN_MINIMIZED: AtomicBool = AtomicBool::new(false);

/// On every resize of the main window: minimizing and restoring change the target.
pub fn main_resized(window: &WebviewWindow) {
    let Ok(minimized) = window.is_minimized() else { return };
    if MAIN_MINIMIZED.swap(minimized, Ordering::AcqRel) != minimized {
        set_memory_target(window, !minimized);
    }
}

/// A rebuilt main window starts shown with WebView2's normal target.
pub fn main_rebuilt() {
    MAIN_MINIMIZED.store(false, Ordering::Release);
}
