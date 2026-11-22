//! Marionette's local store, the copy the package ships and the sync at every
//! app start. They run on their own thread so the window never waits: first the
//! shipped release is installed when the store has no intact copy at least as
//! new (a fresh install has Marionette offline too), then the sync follows the
//! latest release. A session opened before they finish uses the copy that was
//! current until then (or none).
use shinatama_engine::marionette::{self, SystemFetch};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

static STORE: OnceLock<PathBuf> = OnceLock::new();

/// Records `<home>/marionette` as the store, installs the shipped copy when the
/// store needs it and starts following the latest release, unless
/// `SHINATAMA_MARIONETTE=off`. Nothing is written until `accepted` says the data
/// folder passed Shinatama's storage checks: a folder they refuse (inside a
/// project, on a network share) must stay untouched.
pub(crate) fn start(home: &Path, accepted: impl FnOnce() -> bool + Send + 'static) {
    let store = home.join(marionette::STORE);
    let _ = STORE.set(store.clone());
    if marionette::switched_off() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("marionette-sync".into())
        .spawn(move || {
            if !accepted() {
                crate::app_log::write(
                    "marionette",
                    "Not installed: the data folder was not accepted.",
                );
                return;
            }
            if let Some(bundle) = bundle() {
                match marionette::seed(&store, &bundle, &SystemFetch) {
                    Ok(outcome) => {
                        crate::app_log::write("marionette", &format!("Bundled copy: {outcome:?}"))
                    }
                    Err(error) => crate::app_log::write(
                        "marionette",
                        &format!("Bundled copy not installed: {error}."),
                    ),
                }
            }
            match marionette::sync(&store, &SystemFetch) {
                Ok(outcome) => crate::app_log::write("marionette", &format!("{outcome:?}")),
                Err(error) => {
                    crate::app_log::write("marionette", &format!("Sync skipped: {error}."))
                }
            }
        });
}

/// `marionette\` beside the executable, where a package carries its pinned
/// release (`build-release.bat`); a development build has none.
fn bundle() -> Option<PathBuf> {
    let folder = std::env::current_exe()
        .ok()?
        .parent()?
        .join(marionette::BUNDLE);
    folder.is_dir().then_some(folder)
}

/// The store, once [`start`] ran.
pub(crate) fn store() -> Option<&'static Path> {
    STORE.get().map(PathBuf::as_path)
}
