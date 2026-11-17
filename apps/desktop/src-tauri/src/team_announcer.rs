//! The worker that tells a thread's sessions their place in its team (spec section 5). `wake` is
//! what `team_apply` calls so the worker checks at once and not at its next turn.
use std::sync::{Condvar, Mutex, PoisonError};

static WAKE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

/// Ask the worker to check the talking threads now.
pub(crate) fn wake() {
    *WAKE.0.lock().unwrap_or_else(PoisonError::into_inner) = true;
    WAKE.1.notify_all();
}
