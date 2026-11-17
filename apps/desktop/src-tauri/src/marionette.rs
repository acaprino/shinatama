//! Marionette's local store and its sync at every app start. The sync runs on
//! its own thread so the window never waits for the network; a session opened
//! before it finishes uses the copy that was current until then (or none).
use shinatama_engine::marionette::{self, SystemFetch};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

static STORE: OnceLock<PathBuf> = OnceLock::new();

/// Records `<home>/marionette` as the store and starts following the latest
/// release, unless `SHINATAMA_MARIONETTE=off`.
pub(crate) fn start(home: &Path) {
    let store = home.join(marionette::STORE);
    let _ = STORE.set(store.clone());
    if marionette::switched_off() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("marionette-sync".into())
        .spawn(move || match marionette::sync(&store, &SystemFetch) {
            Ok(outcome) => crate::app_log::write("marionette", &format!("{outcome:?}")),
            Err(error) => crate::app_log::write("marionette", &format!("Sync skipped: {error}.")),
        });
}

/// The store, once [`start`] ran.
pub(crate) fn store() -> Option<&'static Path> {
    STORE.get().map(PathBuf::as_path)
}
