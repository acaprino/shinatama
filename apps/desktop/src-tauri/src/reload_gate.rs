//! What the page saves before the shell reloads it.
//!
//! The shell (`imago-shell`) reloads the page when the person asks (the item of the window menu) and when WebView2 lost the
//! process that draws it. A page the person asked to reload may hold a layout not saved yet: it is asked to save first
//! (`interface-reload-requested`, with a token) and answers with `interface_reload_ready`. A dead or hung page does not
//! answer, so the reload goes ahead after the timeout; a page that failed by itself is reloaded at once.
use imago_shell::ProcessFailure;
use std::{
    sync::Mutex,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, WebviewWindow};

/// How long a live page has to save what it holds.
const WAIT: Duration = Duration::from_secs(3);

static TOKEN: AtomicU64 = AtomicU64::new(0);
/// The reload the page has not answered yet.
static PENDING: Mutex<Option<u64>> = Mutex::new(None);

/// Takes the pending request when `token` is its own. False for any other token (a late answer to an earlier request).
fn answered(pending: &Mutex<Option<u64>>, token: u64) -> bool {
    let mut pending = pending.lock().unwrap_or_else(|error| error.into_inner());
    if *pending == Some(token) {
        *pending = None;
        return true;
    }
    false
}

/// Called by the shell before each reload, on a thread that may wait: the answer is whether to reload, and it is always yes.
pub fn before_reload(app: &AppHandle, failure: Option<ProcessFailure>) -> bool {
    // A page that failed cannot be asked.
    if failure.is_some() {
        return true;
    }
    let token = TOKEN.fetch_add(1, Ordering::AcqRel) + 1;
    *PENDING.lock().unwrap_or_else(|error| error.into_inner()) = Some(token);
    let _ = app.emit_to("main", "interface-reload-requested", token);
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if PENDING
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_none_or(|pending| pending != token)
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    // The page did not answer in time: a late answer is not taken for the next request.
    answered(&PENDING, token);
    true
}

/// The page saved what it could and is ready to be reloaded.
#[tauri::command]
pub fn interface_reload_ready(window: WebviewWindow, token: u64) {
    if window.label() == "main" {
        answered(&PENDING, token);
    }
}

#[cfg(test)]
mod tests {
    use super::answered;
    use std::sync::Mutex;

    #[test]
    fn only_the_answer_to_the_pending_request_counts() {
        let pending = Mutex::new(Some(7));
        assert!(
            !answered(&pending, 6),
            "a late answer to an earlier request"
        );
        assert_eq!(*pending.lock().unwrap(), Some(7));
        assert!(answered(&pending, 7));
        assert_eq!(*pending.lock().unwrap(), None);
        assert!(!answered(&pending, 7), "an answer counts once");
    }
}
