//! The webview can request these project operations, never a generic filesystem dialog API.
use crate::storage::Services;
use serde::{Deserialize, Serialize};
use shinatama_state::cancellation::MutationToken;
use shinatama_state::database::ProjectRecord;
use shinatama_state::projects::{
    CatalogEngine, CatalogRequest, CatalogResponse, DatabaseLoader, RootsConfig, RootsSetRequest,
    canonical_root_path,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tauri::{Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

const RESERVED_LIFETIME: Duration = Duration::from_secs(30);

pub struct ProjectCatalog {
    services: Arc<Services>,
    pub engine: Arc<CatalogEngine>,
    active_mutation: Mutex<Option<ActiveMutation>>,
}

struct ActiveMutation {
    id: String,
    token: Arc<MutationToken>,
    claimed: bool,
    created: Instant,
}

fn expire_unclaimed(active: &mut Option<ActiveMutation>) {
    if active.as_ref().is_some_and(|operation| {
        !operation.claimed && operation.created.elapsed() >= RESERVED_LIFETIME
    }) && let Some(operation) = active.take()
    {
        operation.token.cancel();
    }
}
fn valid_request_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

impl ProjectCatalog {
    pub fn new(services: Arc<Services>) -> Self {
        Self {
            services,
            engine: Arc::new(CatalogEngine::new()),
            active_mutation: Mutex::new(None),
        }
    }
    fn loader(&self) -> DatabaseLoader {
        let services = self.services.clone();
        Arc::new(move || services.load().map(|services| services.store.clone()))
    }

    pub fn reserve(
        self: &Arc<Self>,
        request_id: Option<String>,
    ) -> Result<MutationReservation, String> {
        static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);
        if request_id
            .as_deref()
            .is_some_and(|id| !valid_request_id(id))
        {
            return Err("Identificatore dell'operazione non valido.".into());
        }
        let mut active = self
            .active_mutation
            .lock()
            .map_err(|_| "Il coordinamento delle cartelle non è disponibile.".to_owned())?;
        expire_unclaimed(&mut active);
        if let Some(id) = request_id {
            let operation = active
                .as_mut()
                .filter(|operation| operation.id == id && !operation.claimed)
                .ok_or_else(|| {
                    "Operazione non registrata, annullata oppure già avviata.".to_owned()
                })?;
            operation.claimed = true;
            return Ok(MutationReservation {
                catalog: self.clone(),
                id,
                token: operation.token.clone(),
            });
        }
        if active.is_some() {
            return Err("Un'operazione sulle cartelle attende ancora il disco. Non è stato avviato un altro tentativo.".into());
        }
        let id = format!("internal-{}", NEXT_REQUEST.fetch_add(1, Ordering::Relaxed));
        let token = Arc::new(MutationToken::new());
        *active = Some(ActiveMutation {
            id: id.clone(),
            token: token.clone(),
            claimed: true,
            created: Instant::now(),
        });
        Ok(MutationReservation {
            catalog: self.clone(),
            id,
            token,
        })
    }

    pub fn begin(&self, request_id: String) -> Result<(), String> {
        if !valid_request_id(&request_id) {
            return Err("Identificatore dell'operazione non valido.".into());
        }
        let mut active = self
            .active_mutation
            .lock()
            .map_err(|_| "Il coordinamento delle cartelle non è disponibile.".to_owned())?;
        expire_unclaimed(&mut active);
        if active.is_some() {
            return Err("Un'operazione sulle cartelle attende ancora il disco. Non è stato avviato un altro tentativo.".into());
        }
        *active = Some(ActiveMutation {
            id: request_id,
            token: Arc::new(MutationToken::new()),
            claimed: false,
            created: Instant::now(),
        });
        Ok(())
    }

    fn cancel_request(&self, request_id: &str) -> CancelAck {
        let mut active = self
            .active_mutation
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let cancelled = active
            .as_ref()
            .filter(|operation| operation.id == request_id)
            .is_some_and(|operation| operation.token.cancel());
        if cancelled && active.as_ref().is_some_and(|operation| !operation.claimed) {
            *active = None;
        }
        CancelAck {
            status: if cancelled {
                CancelStatus::Cancelled
            } else {
                CancelStatus::AlreadyCompleted
            },
        }
    }
}

pub struct MutationReservation {
    catalog: Arc<ProjectCatalog>,
    id: String,
    token: Arc<MutationToken>,
}
impl MutationReservation {
    pub fn token(&self) -> &MutationToken {
        &self.token
    }
}
impl Drop for MutationReservation {
    fn drop(&mut self) {
        let mut active = self
            .catalog
            .active_mutation
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if active.as_ref().is_some_and(|operation| {
            operation.id == self.id && Arc::ptr_eq(&operation.token, &self.token)
        }) {
            *active = None;
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelStatus {
    Cancelled,
    AlreadyCompleted,
}
#[derive(Serialize)]
pub struct CancelAck {
    pub status: CancelStatus,
}

#[tauri::command(async)]
pub fn catalog_begin(
    window: WebviewWindow,
    catalog: State<'_, Arc<ProjectCatalog>>,
    request_id: String,
) -> Result<(), String> {
    ensure_main(&window)?;
    catalog.begin(request_id)
}

#[tauri::command(async)]
pub fn catalog_cancel(
    window: WebviewWindow,
    catalog: State<'_, Arc<ProjectCatalog>>,
    request_id: String,
) -> Result<CancelAck, String> {
    ensure_main(&window)?;
    if request_id.len() > 64 {
        return Err("Identificatore dell'operazione non valido.".into());
    }
    Ok(catalog.cancel_request(&request_id))
}

fn ensure_main(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("Operazione non disponibile in questa finestra.".into())
    }
}

#[tauri::command]
pub async fn catalog_read(
    window: WebviewWindow,
    catalog: State<'_, Arc<ProjectCatalog>>,
    request: CatalogRequest,
) -> Result<CatalogResponse, String> {
    ensure_main(&window)?;
    let catalog = catalog.inner().clone();
    tauri::async_runtime::spawn_blocking(move || catalog.engine.read(catalog.loader(), request))
        .await
        .map_err(|_| "Il catalogo progetti non è disponibile.".into())
}

#[tauri::command]
pub async fn catalog_roots_set(
    window: WebviewWindow,
    catalog: State<'_, Arc<ProjectCatalog>>,
    request: RootsSetRequest,
    request_id: Option<String>,
) -> Result<RootsConfig, String> {
    ensure_main(&window)?;
    let catalog = catalog.inner().clone();
    let reservation = catalog.reserve(request_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        if reservation.token().is_cancelled() {
            return Err("Operazione annullata.".into());
        }
        let services = catalog.services.load()?;
        let config = services
            .store
            .project_roots_set_if_not_cancelled(request, reservation.token())
            .map_err(|error| error.to_string())?;
        catalog.engine.configure(config.clone());
        Ok(config)
    })
    .await
    .map_err(|_| "Impossibile aggiornare le radici dei progetti.".to_owned())?
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowsePurpose {
    Project,
    Root,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowseRequest {
    pub purpose: BrowsePurpose,
    pub initial_root_id: Option<String>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrowseResult {
    Cancelled,
    Project { project: ProjectRecord },
    Root { path: String },
}

#[tauri::command]
pub async fn catalog_browse(
    window: WebviewWindow,
    catalog: State<'_, Arc<ProjectCatalog>>,
    request: BrowseRequest,
    request_id: Option<String>,
) -> Result<BrowseResult, String> {
    ensure_main(&window)?;
    let catalog = catalog.inner().clone();
    let reservation = catalog.reserve(request_id)?;
    let title = match request.purpose {
        BrowsePurpose::Project => "Apri una cartella come progetto",
        BrowsePurpose::Root => "Aggiungi una radice dei progetti",
    };
    let mut dialog = window
        .app_handle()
        .dialog()
        .file()
        .set_parent(&window)
        .set_title(title);
    // A missing/offline default is retained in preferences, but never sent to the OS dialog.
    if let Some(path) = request
        .initial_root_id
        .as_deref()
        .and_then(|id| catalog.engine.initial_root(id))
    {
        dialog = dialog.set_directory(path);
    }
    let (sender, mut receiver) = tauri::async_runtime::channel(1);
    dialog.pick_folder(move |path| {
        let _ = sender.try_send(path);
    });
    let Some(path) = receiver
        .recv()
        .await
        .ok_or_else(|| "Il selettore cartelle non ha risposto.".to_owned())?
    else {
        return Ok(BrowseResult::Cancelled);
    };
    let path = path.into_path().map_err(|_| {
        "Il selettore non ha restituito una cartella locale o di rete supportata.".to_owned()
    })?;
    tauri::async_runtime::spawn_blocking(move || {
        if reservation.token().is_cancelled() {
            return Ok(BrowseResult::Cancelled);
        }
        match request.purpose {
            BrowsePurpose::Root => {
                let path = canonical_root_path(&path).map_err(|error| error.to_string())?;
                let permit = reservation
                    .token()
                    .begin_commit()
                    .map_err(|error| error.to_string())?;
                permit.finish();
                Ok(BrowseResult::Root { path })
            }
            BrowsePurpose::Project => {
                let services = catalog.services.load()?;
                let project = services
                    .store
                    .project_manager_select_if_not_cancelled(&path, reservation.token())
                    .map_err(|error| error.to_string())?;
                if let Ok(config) = services.store.project_manager_read() {
                    catalog.engine.configure(config);
                }
                Ok(BrowseResult::Project { project })
            }
        }
    })
    .await
    .map_err(|_| "Impossibile aprire la cartella scelta.".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_worker_keeps_its_slot_until_it_really_returns() {
        let catalog = Arc::new(ProjectCatalog::new(Arc::new(Services::new(Err(
            "unused".into()
        )))));
        catalog.begin("first".into()).unwrap();
        let first = catalog.reserve(Some("first".into())).unwrap();
        assert!(matches!(
            catalog.cancel_request("first").status,
            CancelStatus::Cancelled
        ));
        assert!(catalog.reserve(Some("second".into())).is_err());
        drop(first);
        catalog.begin("second".into()).unwrap();
        assert!(catalog.reserve(Some("second".into())).is_ok());
    }

    #[test]
    fn a_commit_in_progress_never_claims_to_be_undone_by_cancel() {
        let catalog = Arc::new(ProjectCatalog::new(Arc::new(Services::new(Err(
            "unused".into()
        )))));
        catalog.begin("request".into()).unwrap();
        let reservation = catalog.reserve(Some("request".into())).unwrap();
        let permit = reservation.token().begin_commit().unwrap();
        assert!(matches!(
            catalog.cancel_request("request").status,
            CancelStatus::AlreadyCompleted
        ));
        permit.finish();
        assert!(matches!(
            catalog.cancel_request("request").status,
            CancelStatus::AlreadyCompleted
        ));
    }

    #[test]
    fn cancel_before_the_async_body_runs_prevents_any_late_claim() {
        let catalog = Arc::new(ProjectCatalog::new(Arc::new(Services::new(Err(
            "unused".into()
        )))));
        catalog.begin("old".into()).unwrap();
        assert!(matches!(
            catalog.cancel_request("old").status,
            CancelStatus::Cancelled
        ));
        assert!(catalog.reserve(Some("old".into())).is_err());
        catalog.begin("new".into()).unwrap();
        assert!(catalog.reserve(Some("old".into())).is_err());
        let current = catalog.reserve(Some("new".into())).unwrap();
        assert!(!current.token().is_cancelled());
        assert!(catalog.reserve(Some("new".into())).is_err());
    }

    #[test]
    fn unknown_ids_never_create_a_mutating_worker() {
        let catalog = Arc::new(ProjectCatalog::new(Arc::new(Services::new(Err(
            "unused".into()
        )))));
        assert!(catalog.reserve(Some("unknown".into())).is_err());
        assert!(catalog.active_mutation.lock().unwrap().is_none());
        assert!(matches!(
            catalog.cancel_request("unknown").status,
            CancelStatus::AlreadyCompleted
        ));
        assert!(catalog.reserve(None).is_ok());
    }

    #[test]
    fn orphaned_reservation_expires_but_a_running_worker_never_expires() {
        let catalog = Arc::new(ProjectCatalog::new(Arc::new(Services::new(Err(
            "unused".into()
        )))));
        catalog.begin("orphan".into()).unwrap();
        catalog
            .active_mutation
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .created = Instant::now() - RESERVED_LIFETIME;
        catalog.begin("new".into()).unwrap();
        assert!(catalog.reserve(Some("orphan".into())).is_err());
        let running = catalog.reserve(Some("new".into())).unwrap();
        catalog
            .active_mutation
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .created = Instant::now() - RESERVED_LIFETIME;
        assert!(catalog.begin("replacement".into()).is_err());
        assert!(!running.token().is_cancelled());
    }
}
