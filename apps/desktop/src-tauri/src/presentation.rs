//! C08's fixed settings capability. No generic setting key, font path or asset URL.
use crate::storage::Services;
use serde::{Deserialize, Serialize};
use shinatama_engine::terminal_palette::{self, HarnessPalette};
use shinatama_state::database::{DatabaseAccess, DatabaseError};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::{Manager, State, WebviewWindow};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Settings {
    pub version: u8,
    pub skin: String,
    pub follow_skin: bool,
    pub font_family: String,
    pub font_size: u8,
    pub line_height: u16,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            skin: "shinatama".into(),
            follow_skin: false,
            font_family: "profile".into(),
            font_size: 13,
            line_height: 115,
        }
    }
}
impl Settings {
    fn validate(&self) -> Result<(), Error> {
        if self.version != 1
            || !["shinatama", "neutral", "section9", "tctf", "sumi"].contains(&self.skin.as_str())
            || ![
                "profile",
                "Cascadia Mono",
                "Cascadia Code",
                "Consolas",
                "monospace",
            ]
            .contains(&self.font_family.as_str())
            || !(8..=32).contains(&self.font_size)
            || !(100..=180).contains(&self.line_height)
        {
            return Err(Error::new(
                "invalid",
                "Presentation preferences are invalid or unsupported; saved data was kept.",
            ));
        }
        Ok(())
    }
    /// Harnesses draw with the terminal's palette while the terminals follow
    /// the skin. Every skin paints a dark terminal (its `terminal` token in
    /// presentation.ts, checked by presentation.test.ts), so the dark variants.
    fn harness_palette(&self) -> HarnessPalette {
        if self.follow_skin {
            HarnessPalette::Terminal { light: false }
        } else {
            HarnessPalette::Own
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub revision: u64,
    pub settings: Settings,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Save {
    pub expected_revision: u64,
    pub settings: Settings,
}
#[derive(Debug, Serialize)]
pub struct Error {
    code: &'static str,
    message: &'static str,
}
impl Error {
    fn new(code: &'static str, message: &'static str) -> Self {
        Self { code, message }
    }
}
fn db_error(error: DatabaseError) -> Error {
    match error {
        DatabaseError::NewerSchema { .. } => Error::new(
            "read_only",
            "Update Shinatama before saving presentation preferences.",
        ),
        DatabaseError::Busy(_) => Error::new(
            "conflict",
            "Presentation preferences changed in another window. Reload them before saving.",
        ),
        DatabaseError::InvalidAppearance => Error::new(
            "invalid",
            "Saved presentation preferences are invalid or too large; their original data was kept.",
        ),
        _ => Error::new(
            "unavailable",
            "Presentation preferences could not be accessed; saved data was kept.",
        ),
    }
}
pub struct Controller {
    services: Arc<Services>,
    busy: AtomicBool,
}
impl Controller {
    pub fn new(services: Arc<Services>) -> Self {
        Self {
            services,
            busy: AtomicBool::new(false),
        }
    }
    fn reserve(self: &Arc<Self>) -> Result<Slot, Error> {
        self.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| {
                Error::new(
                    "busy",
                    "The previous presentation request is still returning.",
                )
            })?;
        Ok(Slot(self.clone()))
    }
    /// Loads the saved preferences and makes them the palette of later harness
    /// launches; harnesses keep their own themes when they cannot be read,
    /// just as the window then shows the profiles' colours.
    fn load(&self) -> Result<Snapshot, Error> {
        let loaded = self.read();
        terminal_palette::set_current(loaded.as_ref().map_or(HarnessPalette::Own, |snapshot| {
            snapshot.settings.harness_palette()
        }));
        loaded
    }
    fn read(&self) -> Result<Snapshot, Error> {
        let state = self
            .services
            .load()
            .map_err(|_| Error::new("unavailable", "Presentation storage is unavailable."))?;
        let saved = state.store.presentation_json().map_err(db_error)?;
        let snapshot = match saved {
            Some(json) => serde_json::from_str::<Snapshot>(&json).map_err(|_| {
                Error::new(
                    "invalid",
                    "Saved presentation preferences are unsupported; original data was kept.",
                )
            })?,
            None => Snapshot {
                revision: 0,
                settings: Settings::default(),
            },
        };
        if snapshot.revision > 9_007_199_254_740_991 {
            return Err(Error::new(
                "invalid",
                "Presentation revision is unsupported.",
            ));
        }
        snapshot.settings.validate()?;
        Ok(snapshot)
    }
    fn save(&self, request: Save) -> Result<Snapshot, Error> {
        request.settings.validate()?;
        if self.read()?.revision != request.expected_revision {
            return Err(Error::new(
                "conflict",
                "Presentation preferences changed; reload before saving.",
            ));
        }
        if request.expected_revision >= 9_007_199_254_740_991 {
            return Err(Error::new(
                "invalid",
                "Presentation revision is unsupported.",
            ));
        }
        let state = self
            .services
            .load()
            .map_err(|_| Error::new("unavailable", "Presentation storage is unavailable."))?;
        if !matches!(state.store.access(), DatabaseAccess::ReadWrite) {
            return Err(Error::new(
                "read_only",
                "Update Shinatama before saving presentation preferences.",
            ));
        }
        let snapshot = Snapshot {
            revision: request.expected_revision + 1,
            settings: request.settings,
        };
        let json = serde_json::to_string(&snapshot)
            .map_err(|_| Error::new("invalid", "Invalid presentation preferences."))?;
        state
            .store
            .set_presentation_json(request.expected_revision, &json)
            .map_err(db_error)?;
        terminal_palette::set_current(snapshot.settings.harness_palette());
        Ok(snapshot)
    }
}
struct Slot(Arc<Controller>);
impl Drop for Slot {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}
#[tauri::command]
pub async fn presentation_load(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
) -> Result<Snapshot, Error> {
    crate::local(&window).map_err(|_| {
        Error::new(
            "unavailable",
            "This window cannot access presentation preferences.",
        )
    })?;
    let slot = controller.inner().reserve()?;
    tauri::async_runtime::spawn_blocking(move || {
        let reservation = slot;
        reservation.0.load()
    })
    .await
    .map_err(|_| Error::new("unavailable", "Presentation worker did not return."))?
}
/// The main window's theme, kept in the data folder for the next start.
const WINDOW_THEME_FILE: &str = "window-theme";

/// `light`, `dark` or `system` (no theme of its own: the window follows Windows).
fn window_theme(value: &str) -> Option<Option<tauri::Theme>> {
    match value {
        "light" => Some(Some(tauri::Theme::Light)),
        "dark" => Some(Some(tauri::Theme::Dark)),
        "system" => Some(None),
        _ => None,
    }
}

/// The theme the main window opens with: the one its page chose last time, so the acrylic
/// behind the glass is already the look's own before the page draws.
pub(crate) fn saved_window_theme(app: &tauri::AppHandle) -> Option<Option<tauri::Theme>> {
    let home = crate::data_home(app).ok()?;
    window_theme(
        std::fs::read_to_string(home.join(WINDOW_THEME_FILE))
            .ok()?
            .trim(),
    )
}

/// The window takes the theme of the look the page shows (ONI MODE is always dark). On Windows 11
/// the acrylic backdrop and the page's `prefers-color-scheme` follow the window's theme, never a
/// tint: a dark backdrop under Smooth's light surfaces turns them grey.
#[tauri::command]
pub async fn presentation_window_theme(window: WebviewWindow, theme: String) -> Result<(), Error> {
    crate::local(&window)
        .map_err(|_| Error::new("unavailable", "This window cannot change its theme."))?;
    let wanted =
        window_theme(&theme).ok_or_else(|| Error::new("invalid", "Unknown window theme."))?;
    window
        .set_theme(wanted)
        .map_err(|_| Error::new("unavailable", "The window theme could not be changed."))?;
    if let Ok(home) = crate::data_home(window.app_handle()) {
        let _ = std::fs::write(home.join(WINDOW_THEME_FILE), &theme);
    }
    Ok(())
}
#[tauri::command]
pub async fn presentation_save(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: Save,
) -> Result<Snapshot, Error> {
    crate::local(&window).map_err(|_| {
        Error::new(
            "unavailable",
            "This window cannot save presentation preferences.",
        )
    })?;
    request.settings.validate()?;
    let slot = controller.inner().reserve()?;
    tauri::async_runtime::spawn_blocking(move || {
        let reservation = slot;
        reservation.0.save(request)
    })
    .await
    .map_err(|_| Error::new("unavailable", "Presentation worker did not return."))?
}

#[cfg(test)]
mod tests {
    use super::window_theme;

    #[test]
    fn window_theme_reads_only_its_three_words() {
        assert_eq!(window_theme("light"), Some(Some(tauri::Theme::Light)));
        assert_eq!(window_theme("dark"), Some(Some(tauri::Theme::Dark)));
        assert_eq!(window_theme("system"), Some(None));
        assert_eq!(window_theme("Dark"), None);
        assert_eq!(window_theme(""), None);
    }
}
