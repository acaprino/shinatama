//! What an open the user is waiting on is doing now. A first open after Shinatama starts waits
//! for the model proxy, checks the harness and prepares its launch; each step is reported here,
//! so the window can say what it waits for instead of looking stuck, and the log keeps it.
use std::sync::OnceLock;

/// The steps, in the order an open passes them.
pub(crate) const STEPS: [&str; 6] = ["proxy", "proxy_resources", "check", "reading", "prepare", "start"];

type Sink = Box<dyn Fn(&str, &'static str) + Send + Sync>;
static SINK: OnceLock<Sink> = OnceLock::new();

/// Where the steps go (the main window); before this runs they go nowhere.
pub(crate) fn install(sink: Sink) {
    let _ = SINK.set(sink);
}

/// `harness` is a registered harness id; `step` one of [`STEPS`].
pub(crate) fn report(harness: &str, step: &'static str) {
    debug_assert!(STEPS.contains(&step));
    if let Some(sink) = SINK.get() {
        sink(harness, step);
    }
}
