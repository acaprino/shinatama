//! C08's fixed settings capability. No generic setting key, font path or asset URL.
use crate::storage::Services;
use serde::{Deserialize, Serialize};
use shinatama_state::database::{DatabaseAccess, DatabaseError};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::{State, WebviewWindow};

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
    fn load(&self) -> Result<Snapshot, Error> {
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
        if self.load()?.revision != request.expected_revision {
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
