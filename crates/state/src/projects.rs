//! Project roots are preferences; listing a folder never creates a project database row.
//! All filesystem discovery runs in bounded workers, independently of snapshots and cancel.

use crate::cancellation::MutationToken;
use crate::database::{Database, DatabaseError, ProjectRecord, Result};
use crate::location::{StorageLocation, is_within, validate_project_shape};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, UNIX_EPOCH};

const SETTING_KEY: &str = "projects.manager.v1";
const MAX_ROOTS: usize = 16;
const MAX_PATH_BYTES: usize = 4096;
const MAX_CONFIG_BYTES: usize = 64 * 1024;
const MAX_CANDIDATES: usize = 1000;
const MAX_SCAN_ENTRIES: usize = 4000;
const SCAN_DEADLINE: Duration = Duration::from_secs(5);
const MAX_CACHE_BYTES: usize = 8 * 1024 * 1024;
const MAX_ROOT_CACHE_BYTES: usize = 2 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const RECORD_COLUMNS: &str = "id,CASE WHEN length(CAST(path AS BLOB))<=131072 THEN path ELSE '' END,CASE WHEN length(CAST(name AS BLOB))<=1024 THEN name ELSE '[nome troppo lungo]' END,last_opened";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectRoot {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RootsConfig {
    pub version: u32,
    pub revision: u64,
    pub roots: Vec<ProjectRoot>,
    pub default_root_id: Option<String>,
    pub non_standard_history: Vec<String>,
}

impl Default for RootsConfig {
    fn default() -> Self {
        Self {
            version: 1,
            revision: 0,
            roots: Vec::new(),
            default_root_id: None,
            non_standard_history: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RootInput {
    pub id: Option<String>,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RootsSetRequest {
    pub expected_revision: u64,
    pub roots: Vec<RootInput>,
    pub default_root_index: Option<usize>,
}

fn invalid(reason: &'static str) -> DatabaseError {
    DatabaseError::InvalidProject(reason)
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}
fn next_revision(revision: u64) -> Result<u64> {
    revision
        .checked_add(1)
        .filter(|value| *value <= 9_007_199_254_740_991)
        .ok_or(invalid(
            "la revisione delle radici ha raggiunto il limite supportato",
        ))
}
fn same_reference(left: &str, right: &str) -> bool {
    let left = PathBuf::from(normalize_reference(left));
    let right = PathBuf::from(normalize_reference(right));
    is_within(&left, &right) && is_within(&right, &left)
}
fn under_root(path: &str, config: &RootsConfig) -> bool {
    config
        .roots
        .iter()
        .any(|root| is_within(Path::new(path), Path::new(&root.path)))
}

fn normalize_reference(path: &str) -> String {
    #[cfg(windows)]
    {
        let mut path = if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{unc}")
        } else {
            path.strip_prefix(r"\\?\").unwrap_or(path).to_owned()
        };
        if path.as_bytes().get(1) == Some(&b':') {
            path.replace_range(..1, &path[..1].to_ascii_uppercase());
        }
        path
    }
    #[cfg(not(windows))]
    path.to_owned()
}

/// Project folders may live on UNC/network/cloud drives. Only shared-state storage is local.
pub fn canonical_root_path(path: &Path) -> Result<String> {
    validate_project_shape(path)?;
    if path.as_os_str().len() > MAX_PATH_BYTES {
        return Err(invalid("il percorso supera il limite supportato"));
    }
    let canonical = fs::canonicalize(path).map_err(DatabaseError::Io)?;
    if !canonical.is_dir() {
        return Err(invalid("scegli una cartella esistente"));
    }
    let text = canonical.to_str().ok_or(invalid(
        "il percorso non è rappresentabile nell'interfaccia",
    ))?;
    let text = normalize_reference(text);
    if text.len() > MAX_PATH_BYTES {
        return Err(invalid("il percorso supera il limite supportato"));
    }
    Ok(text)
}

fn read_config(connection: &Connection) -> Result<RootsConfig> {
    let value: Option<Option<String>> = connection.query_row(
        "SELECT CASE WHEN length(CAST(value AS BLOB))<=?2 THEN value ELSE NULL END FROM setting WHERE key=?1",
        params![SETTING_KEY, MAX_CONFIG_BYTES as i64], |row| row.get(0),
    ).optional()?;
    let Some(value) = value else {
        return Ok(RootsConfig::default());
    };
    let value = value.ok_or(invalid(
        "la configurazione delle radici è troppo grande; i dati sono conservati",
    ))?;
    let config: RootsConfig = serde_json::from_str(&value).map_err(|_| {
        invalid("la configurazione delle radici non è valida; i dati sono conservati")
    })?;
    if config.version != 1
        || config.revision > 9_007_199_254_740_991
        || config.roots.len() > MAX_ROOTS
        || config.non_standard_history.len() > 10
    {
        return Err(invalid(
            "la configurazione delle radici non è supportata; i dati sono conservati",
        ));
    }
    let mut ids = HashSet::new();
    for root in &config.roots {
        if !valid_id(&root.id)
            || !ids.insert(root.id.clone())
            || root.path.len() > MAX_PATH_BYTES
            || validate_project_shape(Path::new(&root.path)).is_err()
        {
            return Err(invalid(
                "la configurazione contiene una radice non valida; i dati sono conservati",
            ));
        }
    }
    if config
        .default_root_id
        .as_ref()
        .is_some_and(|id| !ids.contains(id))
        || config.non_standard_history.iter().any(|id| !valid_id(id))
        || config
            .non_standard_history
            .iter()
            .collect::<HashSet<_>>()
            .len()
            != config.non_standard_history.len()
    {
        return Err(invalid(
            "la configurazione contiene riferimenti non validi; i dati sono conservati",
        ));
    }
    Ok(config)
}

fn write_config(connection: &Connection, config: &RootsConfig) -> Result<()> {
    let json =
        serde_json::to_string(config).map_err(|_| invalid("impossibile salvare le radici"))?;
    if json.len() > MAX_CONFIG_BYTES {
        return Err(invalid("la configurazione delle radici è troppo grande"));
    }
    connection.execute("INSERT INTO setting(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![SETTING_KEY, json])?;
    Ok(())
}

fn project_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectRecord> {
    Ok(ProjectRecord {
        id: row.get(0)?,
        path: row.get(1)?,
        name: row.get(2)?,
        last_opened: row.get(3)?,
        available: false,
    })
}

impl Database {
    pub fn project_manager_read(&self) -> Result<RootsConfig> {
        let connection = self.lock()?;
        read_config(&connection)
    }

    pub fn project_roots_set(&self, request: RootsSetRequest) -> Result<RootsConfig> {
        self.project_roots_set_if_not_cancelled(request, &MutationToken::new())
    }

    pub fn project_roots_set_if_not_cancelled(
        &self,
        request: RootsSetRequest,
        cancellation: &MutationToken,
    ) -> Result<RootsConfig> {
        if cancellation.is_cancelled() {
            return Err(DatabaseError::Cancelled);
        }
        self.require_writable()?;
        if request.roots.len() > MAX_ROOTS
            || request
                .default_root_index
                .is_some_and(|index| index >= request.roots.len())
        {
            return Err(invalid(
                "sono supportate al massimo 16 radici e una predefinita esplicita",
            ));
        }
        let old = self.project_manager_read()?;
        if old.revision != request.expected_revision {
            return Err(invalid(
                "le radici sono cambiate in un'altra finestra; riapri le impostazioni prima di salvare",
            ));
        }
        let mut roots = Vec::<ProjectRoot>::new();
        let mut used_ids = HashSet::new();
        for input in request.roots {
            if input.path.len() > MAX_PATH_BYTES
                || input.id.as_ref().is_some_and(|id| id.len() > 64)
            {
                return Err(invalid(
                    "il percorso o l'identificatore della radice supera il limite supportato",
                ));
            }
            if cancellation.is_cancelled() {
                return Err(DatabaseError::Cancelled);
            }
            let existing = match &input.id {
                Some(id) => Some(
                    old.roots
                        .iter()
                        .find(|root| &root.id == id)
                        .ok_or(invalid("la radice da modificare non esiste più"))?,
                ),
                None => None,
            };
            // Keeping a configured but removed/offline drive requires no filesystem access.
            let path = if let Some(root) =
                existing.filter(|root| same_reference(&root.path, &input.path))
            {
                root.path.clone()
            } else {
                canonical_root_path(Path::new(&input.path))?
            };
            let id = existing
                .map(|root| root.id.clone())
                .unwrap_or_else(|| format!("pr_{}", uuid::Uuid::new_v4().simple()));
            if !used_ids.insert(id.clone())
                || roots.iter().any(|root| same_reference(&root.path, &path))
            {
                return Err(invalid(
                    "la stessa radice non può comparire due volte, neanche tramite un collegamento",
                ));
            }
            roots.push(ProjectRoot { id, path });
        }
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if cancellation.is_cancelled() {
            return Err(DatabaseError::Cancelled);
        }
        crate::schema::require_write_version(&transaction)?;
        let current = read_config(&transaction)?;
        if current.revision != request.expected_revision {
            return Err(invalid(
                "le radici sono cambiate in un'altra finestra; riapri le impostazioni prima di salvare",
            ));
        }
        let mut config = RootsConfig {
            version: 1,
            revision: next_revision(current.revision)?,
            default_root_id: request
                .default_root_index
                .map(|index| roots[index].id.clone()),
            roots,
            non_standard_history: Vec::new(),
        };
        for id in current.non_standard_history {
            let path: Option<String> = transaction
                .query_row("SELECT path FROM project WHERE id=?1", [&id], |row| {
                    row.get(0)
                })
                .optional()?;
            if path.as_ref().is_some_and(|path| !under_root(path, &config)) {
                config.non_standard_history.push(id);
            }
        }
        let permit = cancellation.begin_commit()?;
        write_config(&transaction, &config)?;
        transaction.commit()?;
        permit.finish();
        Ok(config)
    }

    /// Explicitly opening a folder records it once and maintains only the ten external roots.
    pub fn project_manager_select(&self, path: &Path) -> Result<ProjectRecord> {
        self.project_manager_select_if_not_cancelled(path, &MutationToken::new())
    }

    pub fn project_manager_select_if_not_cancelled(
        &self,
        path: &Path,
        cancellation: &MutationToken,
    ) -> Result<ProjectRecord> {
        if cancellation.is_cancelled() {
            return Err(DatabaseError::Cancelled);
        }
        let prepared = self.prepare_project(path)?;
        if cancellation.is_cancelled() {
            return Err(DatabaseError::Cancelled);
        }
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&transaction)?;
        let mut config = read_config(&transaction)?;
        let permit = cancellation.begin_commit()?;
        let selected = self.record_selection(&transaction, &prepared)?;
        config.non_standard_history.retain(|id| id != &selected.id);
        if !under_root(&selected.path, &config) {
            config.non_standard_history.insert(0, selected.id.clone());
        }
        config.non_standard_history.truncate(10);
        config.revision = next_revision(config.revision)?;
        write_config(&transaction, &config)?;
        transaction.commit()?;
        permit.finish();
        Ok(selected)
    }

    fn catalog_records(
        &self,
        config: &RootsConfig,
    ) -> Result<(Vec<ProjectRecord>, Vec<ProjectRecord>)> {
        let connection = self.lock()?;
        let mut recent_statement = connection.prepare(&format!("SELECT {RECORD_COLUMNS} FROM project ORDER BY last_opened DESC,recent_sequence DESC LIMIT 10"))?;
        let recent = recent_statement
            .query_map([], project_record)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut history = Vec::new();
        for id in &config.non_standard_history {
            if let Some(record) = connection
                .query_row(
                    &format!("SELECT {RECORD_COLUMNS} FROM project WHERE id=?1"),
                    [id],
                    project_record,
                )
                .optional()?
                && !under_root(&record.path, config)
            {
                history.push(record);
            }
        }
        Ok((recent, history))
    }

    fn fill_catalog_usage(&self, projects: &mut [ProjectCandidate]) -> Result<()> {
        let connection = self.lock()?;
        let mut statement =
            connection.prepare("SELECT id,last_opened FROM project WHERE path_key=?1")?;
        for project in projects {
            if let Some((id, opened)) = statement
                .query_row([project.path.to_lowercase()], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })
                .optional()?
            {
                project.id = Some(id);
                project.last_opened = Some(opened);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectCandidate {
    pub id: Option<String>,
    pub path: String,
    pub name: String,
    pub last_opened: Option<i64>,
    pub available: bool,
    #[serde(rename = "modifiedMs")]
    pub modified_ms: Option<i64>,
    #[serde(rename = "rootId")]
    pub root_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RootState {
    Ready,
    Scanning,
    Missing,
    Unavailable,
    TimedOut,
    Truncated,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRoot {
    pub id: String,
    pub path: String,
    pub state: RootState,
    pub message: Option<String>,
    pub project_count: usize,
    pub projects: Vec<ProjectCandidate>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum CatalogRequest {
    Refresh {
        #[serde(default)]
        filter: String,
        #[serde(default)]
        offset: usize,
        limit: Option<usize>,
    },
    Snapshot {
        #[serde(rename = "scanId")]
        scan_id: Option<String>,
        #[serde(default)]
        filter: String,
        #[serde(default)]
        offset: usize,
        limit: Option<usize>,
    },
    Cancel {
        #[serde(rename = "scanId")]
        scan_id: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogResponse {
    pub scan_id: String,
    pub revision: u64,
    pub roots_config: RootsConfig,
    pub roots: Vec<CatalogRoot>,
    pub projects: Vec<ProjectCandidate>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub history: Vec<ProjectRecord>,
    pub recent: Vec<ProjectRecord>,
    pub scanning: bool,
    pub config_loading: bool,
    pub error: Option<String>,
    pub truncated: bool,
}

struct MetadataCharge {
    used: Arc<AtomicUsize>,
    bytes: usize,
}
impl MetadataCharge {
    fn add(&mut self, bytes: usize) -> bool {
        if self.bytes.saturating_add(bytes) > MAX_ROOT_CACHE_BYTES {
            return false;
        }
        if self
            .used
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes)
                    .filter(|sum| *sum <= MAX_CACHE_BYTES)
            })
            .is_err()
        {
            return false;
        }
        self.bytes += bytes;
        true
    }
}
impl Drop for MetadataCharge {
    fn drop(&mut self) {
        self.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

// Field order frees candidate allocations before their shared budget is returned.
struct ProjectCache {
    projects: Vec<ProjectCandidate>,
    charge: MetadataCharge,
}
impl ProjectCache {
    fn empty(used: Arc<AtomicUsize>) -> Self {
        Self {
            projects: Vec::new(),
            charge: MetadataCharge { used, bytes: 0 },
        }
    }
    fn push(&mut self, project: ProjectCandidate, dedup_bytes: usize) -> bool {
        if self.projects.len() >= MAX_CANDIDATES {
            return false;
        }
        if self.projects.len() == self.projects.capacity() {
            let capacity = self.projects.capacity();
            let target = (capacity.max(8) * 2).min(MAX_CANDIDATES);
            if !self
                .charge
                .add((target - capacity) * std::mem::size_of::<ProjectCandidate>())
            {
                return false;
            }
            self.projects.reserve_exact(target - self.projects.len());
        }
        let bytes = project
            .path
            .capacity()
            .saturating_add(project.name.capacity())
            .saturating_add(project.root_id.capacity())
            .saturating_add(dedup_bytes)
            .saturating_add(256);
        if !self.charge.add(bytes) {
            return false;
        }
        self.projects.push(project);
        true
    }
}
impl std::ops::Deref for ProjectCache {
    type Target = [ProjectCandidate];
    fn deref(&self) -> &Self::Target {
        &self.projects
    }
}

#[derive(Clone)]
struct RootCache {
    root: ProjectRoot,
    state: RootState,
    message: Option<String>,
    projects: Arc<ProjectCache>,
}
struct Worker {
    root_id: String,
    path: String,
    cancel: Arc<AtomicBool>,
    started: Instant,
    handle: JoinHandle<()>,
}
struct CatalogData {
    scan_id: String,
    config: RootsConfig,
    roots: Vec<RootCache>,
    recent: Vec<ProjectRecord>,
    history: Vec<ProjectRecord>,
    loading: bool,
    config_loading: bool,
    error: Option<String>,
    cancelled: bool,
    coordinator: Option<Worker>,
    history_worker: Option<Worker>,
    workers: Vec<Worker>,
}

pub type DatabaseLoader = Arc<dyn Fn() -> std::result::Result<Arc<Database>, String> + Send + Sync>;

struct ScanResult {
    cache: ProjectCache,
    truncated: bool,
}
type Scanner = Arc<
    dyn Fn(
            &ProjectRoot,
            &StorageLocation,
            &AtomicBool,
            Instant,
            Duration,
            Arc<AtomicUsize>,
        ) -> Result<ScanResult>
        + Send
        + Sync,
>;

pub struct CatalogEngine {
    data: Mutex<CatalogData>,
    deadline: Duration,
    scanner: Scanner,
    cache_bytes: Arc<AtomicUsize>,
}

impl Default for CatalogEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl CatalogEngine {
    pub fn new() -> Self {
        Self::with_deadline(SCAN_DEADLINE)
    }

    fn with_deadline(deadline: Duration) -> Self {
        Self {
            deadline,
            scanner: Arc::new(scan_root),
            cache_bytes: Arc::new(AtomicUsize::new(0)),
            data: Mutex::new(CatalogData {
                scan_id: uuid::Uuid::new_v4().to_string(),
                config: RootsConfig::default(),
                roots: Vec::new(),
                recent: Vec::new(),
                history: Vec::new(),
                loading: false,
                config_loading: false,
                error: None,
                cancelled: false,
                coordinator: None,
                history_worker: None,
                workers: Vec::new(),
            }),
        }
    }

    pub fn read(
        self: &Arc<Self>,
        loader: DatabaseLoader,
        request: CatalogRequest,
    ) -> CatalogResponse {
        match request {
            CatalogRequest::Refresh {
                filter,
                offset,
                limit,
            } => {
                let assigned = self.refresh(loader);
                let mut response = self.snapshot(None, &filter, offset, limit.unwrap_or(100));
                if response.scan_id != assigned {
                    response.scan_id = assigned;
                    response.scanning = false;
                    response.projects.clear();
                    response.total = 0;
                    response.offset = 0;
                    response.error = Some(
                        "La scansione è stata sostituita da una richiesta più recente.".into(),
                    );
                }
                response
            }
            CatalogRequest::Snapshot {
                scan_id,
                filter,
                offset,
                limit,
            } => self.snapshot(scan_id.as_deref(), &filter, offset, limit.unwrap_or(100)),
            CatalogRequest::Cancel { scan_id } => {
                self.cancel(scan_id.as_deref());
                self.snapshot(None, "", 0, 100)
            }
        }
    }

    pub fn configure(&self, config: RootsConfig) {
        let mut data = self.data.lock().unwrap_or_else(|error| error.into_inner());
        if config.revision < data.config.revision {
            return;
        }
        if config.roots.len() > MAX_ROOTS
            || config.non_standard_history.len() > 10
            || config
                .roots
                .iter()
                .any(|root| root.path.len() > MAX_PATH_BYTES || root.id.len() > 64)
            || config.non_standard_history.iter().any(|id| id.len() > 64)
            || serde_json::to_vec(&config).map_or(true, |value| value.len() > MAX_CONFIG_BYTES)
        {
            data.error = Some("La configurazione supera il limite supportato; le preferenze precedenti sono conservate.".into());
            return;
        }
        cancel_workers(&mut data);
        data.scan_id = uuid::Uuid::new_v4().to_string();
        data.roots = config
            .roots
            .iter()
            .cloned()
            .map(|root| RootCache {
                root,
                state: RootState::Ready,
                message: None,
                projects: Arc::new(ProjectCache::empty(self.cache_bytes.clone())),
            })
            .collect();
        data.config = config;
        data.loading = false;
        data.config_loading = false;
        data.cancelled = false;
        data.error = None;
    }

    fn refresh(self: &Arc<Self>, loader: DatabaseLoader) -> String {
        let mut data = self.data.lock().unwrap_or_else(|error| error.into_inner());
        cancel_workers(&mut data);
        data.workers.retain(|worker| !worker.handle.is_finished());
        if data
            .history_worker
            .as_ref()
            .is_some_and(|worker| worker.handle.is_finished())
        {
            data.history_worker = None;
        }
        if data
            .coordinator
            .as_ref()
            .is_some_and(|worker| worker.handle.is_finished())
        {
            data.coordinator = None;
        }
        data.scan_id = uuid::Uuid::new_v4().to_string();
        data.cancelled = false;
        data.error = None;
        data.config_loading = true;
        if data.coordinator.is_some() {
            data.loading = false;
            data.error = Some("Il caricamento precedente è ancora in attesa del disco. Non è stata avviata un'altra scansione.".into());
            return data.scan_id.clone();
        }
        data.loading = true;
        let cancel = Arc::new(AtomicBool::new(false));
        let token = cancel.clone();
        let engine = self.clone();
        let scan_id = data.scan_id.clone();
        let started = Instant::now();
        let handle = thread::Builder::new()
            .name("project-config".into())
            .spawn(move || {
                let loaded = loader().and_then(|database| {
                    let config = database
                        .project_manager_read()
                        .map_err(|error| error.to_string())?;
                    let records = database
                        .catalog_records(&config)
                        .map_err(|error| error.to_string())?;
                    Ok((database, config, records))
                });
                if token.load(Ordering::Relaxed) {
                    return;
                }
                match loaded {
                    Ok((database, config, (recent, history))) => {
                        engine.start_roots(&scan_id, database, config, recent, history)
                    }
                    Err(error) => {
                        let mut data = engine
                            .data
                            .lock()
                            .unwrap_or_else(|error| error.into_inner());
                        if data.scan_id == scan_id {
                            data.loading = false;
                            data.config_loading = false;
                            data.error = Some(error);
                        }
                    }
                }
            });
        match handle {
            Ok(handle) => {
                data.coordinator = Some(Worker {
                    root_id: String::new(),
                    path: String::new(),
                    cancel,
                    started,
                    handle,
                })
            }
            Err(error) => {
                data.loading = false;
                data.config_loading = false;
                data.error = Some(format!("Impossibile avviare il catalogo: {error}"));
            }
        }
        data.scan_id.clone()
    }

    fn start_roots(
        self: &Arc<Self>,
        scan_id: &str,
        database: Arc<Database>,
        config: RootsConfig,
        recent: Vec<ProjectRecord>,
        history: Vec<ProjectRecord>,
    ) {
        let mut data = self.data.lock().unwrap_or_else(|error| error.into_inner());
        if data.scan_id != scan_id || data.cancelled {
            return;
        }
        if config.revision < data.config.revision {
            data.loading = false;
            data.config_loading = false;
            return;
        }
        let available = data
            .recent
            .iter()
            .chain(&data.history)
            .filter(|record| record.available)
            .map(|record| record.id.clone())
            .collect::<HashSet<_>>();
        data.loading = false;
        data.config_loading = false;
        data.config = config.clone();
        data.recent = recent;
        data.history = history;
        for record in &mut data.recent {
            record.available = available.contains(&record.id);
        }
        for record in &mut data.history {
            record.available = available.contains(&record.id);
        }
        data.roots.clear();
        data.workers.retain(|worker| !worker.handle.is_finished());
        for root in &config.roots {
            let occupied = data.workers.iter().any(|worker| {
                worker.root_id == root.id || same_reference(&worker.path, &root.path)
            });
            if occupied || data.workers.len() >= MAX_ROOTS {
                data.roots.push(RootCache { root: root.clone(), state: RootState::TimedOut, message: Some("Una scansione precedente è ancora in attesa del disco; nessun nuovo tentativo è stato aggiunto.".into()), projects: Arc::new(ProjectCache::empty(self.cache_bytes.clone())) });
                continue;
            }
            data.roots.push(RootCache {
                root: root.clone(),
                state: RootState::Scanning,
                message: None,
                projects: Arc::new(ProjectCache::empty(self.cache_bytes.clone())),
            });
            let engine = self.clone();
            let database = database.clone();
            let root = root.clone();
            let id = scan_id.to_owned();
            let revision = config.revision;
            let cancel = Arc::new(AtomicBool::new(false));
            let token = cancel.clone();
            let started = Instant::now();
            let worker_root = root.clone();
            let handle = thread::Builder::new()
                .name("project-root".into())
                .spawn(move || {
                    let result = (engine.scanner)(
                        &worker_root,
                        database.location(),
                        &token,
                        started,
                        engine.deadline,
                        engine.cache_bytes.clone(),
                    )
                    .and_then(|mut result| {
                        database.fill_catalog_usage(&mut result.cache.projects)?;
                        Ok(result)
                    });
                    if token.load(Ordering::Relaxed) {
                        return;
                    }
                    let mut data = engine
                        .data
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    if data.scan_id != id || data.config.revision != revision || data.cancelled {
                        return;
                    }
                    if let Some(cache) = data
                        .roots
                        .iter_mut()
                        .find(|cache| cache.root.id == worker_root.id)
                    {
                        match result {
                            Ok(result) => {
                                cache.state = if result.truncated {
                                    RootState::Truncated
                                } else {
                                    RootState::Ready
                                };
                                if result.truncated { cache.message = Some("Risultati parziali: alcune cartelle non sono leggibili oppure superano i limiti dell'elenco.".into()); }
                                cache.projects = Arc::new(result.cache);
                            }
                            Err(error) => {
                                cache.state = match &error {
                                    DatabaseError::Io(error)
                                        if error.kind() == std::io::ErrorKind::NotFound =>
                                    {
                                        RootState::Missing
                                    }
                                    DatabaseError::Busy(_) => RootState::TimedOut,
                                    _ => RootState::Unavailable,
                                };
                                cache.message = Some(error.to_string());
                            }
                        }
                    }
                });
            match handle {
                Ok(handle) => data.workers.push(Worker {
                    root_id: root.id.clone(),
                    path: root.path.clone(),
                    cancel,
                    started,
                    handle,
                }),
                Err(error) => {
                    if let Some(cache) =
                        data.roots.iter_mut().find(|cache| cache.root.id == root.id)
                    {
                        cache.state = RootState::Unavailable;
                        cache.message = Some(format!("Impossibile avviare la scansione: {error}"));
                    }
                }
            }
        }
        if data
            .history_worker
            .as_ref()
            .is_some_and(|worker| worker.handle.is_finished())
        {
            data.history_worker = None;
        }
        if data.history_worker.is_none() {
            let mut records = data
                .recent
                .iter()
                .chain(&data.history)
                .map(|record| (record.id.clone(), record.path.clone()))
                .collect::<Vec<_>>();
            records.sort_by_key(|(_, path)| path.starts_with(r"\\"));
            records.dedup_by(|a, b| a.0 == b.0);
            let engine = self.clone();
            let id = scan_id.to_owned();
            let cancel = Arc::new(AtomicBool::new(false));
            let token = cancel.clone();
            let started = Instant::now();
            let handle = thread::Builder::new()
                .name("project-history".into())
                .spawn(move || {
                    for (record_id, path) in records {
                        if token.load(Ordering::Relaxed) || started.elapsed() >= engine.deadline {
                            return;
                        }
                        let available = !path.is_empty() && Path::new(&path).is_dir();
                        let mut data = engine
                            .data
                            .lock()
                            .unwrap_or_else(|error| error.into_inner());
                        if data.scan_id != id {
                            return;
                        }
                        for record in data.recent.iter_mut() {
                            if record.id == record_id {
                                record.available = available;
                            }
                        }
                        for record in data.history.iter_mut() {
                            if record.id == record_id {
                                record.available = available;
                            }
                        }
                    }
                });
            match handle {
                Ok(handle) => {
                    data.history_worker = Some(Worker {
                        root_id: String::new(),
                        path: String::new(),
                        cancel,
                        started,
                        handle,
                    })
                }
                Err(error) => {
                    data.error = Some(format!(
                        "Impossibile controllare le cartelle recenti: {error}"
                    ))
                }
            }
        }
    }

    pub fn cancel(&self, scan_id: Option<&str>) {
        let mut data = self.data.lock().unwrap_or_else(|error| error.into_inner());
        if scan_id.is_some_and(|id| id != data.scan_id) {
            return;
        }
        cancel_workers(&mut data);
        data.cancelled = true;
        data.loading = false;
        data.config_loading = false;
        for root in &mut data.roots {
            if root.state == RootState::Scanning {
                root.state = RootState::Ready;
                root.message = Some("Scansione annullata.".into());
            }
        }
    }

    pub fn initial_root(&self, id: &str) -> Option<PathBuf> {
        let data = self.data.lock().unwrap_or_else(|error| error.into_inner());
        data.roots
            .iter()
            .find(|root| {
                root.root.id == id && matches!(root.state, RootState::Ready | RootState::Truncated)
            })
            .map(|root| PathBuf::from(&root.root.path))
    }

    pub fn snapshot(
        &self,
        _scan_id: Option<&str>,
        filter: &str,
        offset: usize,
        limit: usize,
    ) -> CatalogResponse {
        let mut data = self.data.lock().unwrap_or_else(|error| error.into_inner());
        let timed_out = data
            .workers
            .iter()
            .filter(|worker| {
                !worker.handle.is_finished() && worker.started.elapsed() >= self.deadline
            })
            .map(|worker| {
                worker.cancel.store(true, Ordering::Relaxed);
                worker.root_id.clone()
            })
            .collect::<HashSet<_>>();
        for root in &mut data.roots {
            if timed_out.contains(&root.root.id) && root.state == RootState::Scanning {
                root.state = RootState::TimedOut;
                root.message = Some(
                    "Il disco non ha risposto in tempo. Le altre radici restano disponibili."
                        .into(),
                );
            }
        }
        if let Some(worker) = &data.coordinator
            && !worker.handle.is_finished()
            && worker.started.elapsed() >= self.deadline
        {
            worker.cancel.store(true, Ordering::Relaxed);
            data.loading = false;
            data.error = Some("La configurazione attende ancora il disco. Nessun altro caricamento è stato avviato.".into());
        }
        if let Some(worker) = &data.history_worker
            && !worker.handle.is_finished()
            && worker.started.elapsed() >= self.deadline
        {
            worker.cancel.store(true, Ordering::Relaxed);
        }
        let scan_id = data.scan_id.clone();
        let config = data.config.clone();
        let roots = data.roots.clone();
        let recent = data.recent.clone();
        let history = data.history.clone();
        let error = data.error.clone();
        let scanning = data.loading
            || data
                .roots
                .iter()
                .any(|root| root.state == RootState::Scanning)
            || data.history_worker.as_ref().is_some_and(|worker| {
                !worker.handle.is_finished() && !worker.cancel.load(Ordering::Relaxed)
            });
        let config_loading = data.config_loading;
        drop(data);
        let filter = filter.chars().take(128).collect::<String>().to_lowercase();
        let mut projects = roots
            .iter()
            .flat_map(|root| root.projects.iter())
            .filter(|project| {
                filter.is_empty()
                    || project.name.to_lowercase().contains(&filter)
                    || project.path.to_lowercase().contains(&filter)
            })
            .collect::<Vec<_>>();
        projects.sort_by(|left, right| {
            right
                .last_opened
                .cmp(&left.last_opened)
                .then_with(|| {
                    if left.last_opened.is_none() && right.last_opened.is_none() {
                        right.modified_ms.cmp(&left.modified_ms)
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
                .then_with(|| left.path.cmp(&right.path))
        });
        // Overlapping roots may observe the same folder at different instants. Its
        // usage/mtime can therefore differ, so duplicate identities are not adjacent.
        let mut seen = HashSet::new();
        projects.retain(|project| seen.insert(project.path.to_lowercase()));
        let total = projects.len();
        let offset = offset.min(total);
        let limit = limit.clamp(1, 100);
        let page = projects
            .into_iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        let roots = roots
            .iter()
            .map(|root| CatalogRoot {
                id: root.root.id.clone(),
                path: bounded_display(&root.root.path, 1024),
                state: root.state,
                message: root
                    .message
                    .as_ref()
                    .map(|message| bounded_display(message, 512)),
                project_count: root.projects.len(),
                // Top-level projects is the ordered page; do not duplicate its strings.
                projects: Vec::new(),
            })
            .collect();
        bounded_response(CatalogResponse {
            scan_id,
            revision: config.revision,
            roots_config: config,
            roots,
            projects: page,
            total,
            offset,
            limit,
            history,
            recent,
            scanning,
            config_loading,
            error,
            truncated: false,
        })
    }
}

fn bounded_display(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let mut end = limit.saturating_sub(3).min(value.len());
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

fn bounded_response(mut response: CatalogResponse) -> CatalogResponse {
    struct SizeLimit(usize);
    impl std::io::Write for SizeLimit {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            if self.0 > MAX_RESPONSE_BYTES {
                return Err(std::io::Error::other("catalog payload budget"));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    response.error = response
        .error
        .as_ref()
        .map(|error| bounded_display(error, 512));
    while serde_json::to_writer(&mut SizeLimit(0), &response).is_err() {
        response.truncated = true;
        response.error = Some("Elenco parziale: raggiunto il limite di dimensione dei risultati. Usa il filtro o F3 per scegliere direttamente una cartella.".into());
        if response.projects.len() > 1 {
            response.projects.truncate(response.projects.len() / 2);
            response.limit = response.projects.len();
        } else if !response.recent.is_empty() {
            response.recent.pop();
        } else if !response.history.is_empty() {
            response.history.pop();
        } else if response.roots.iter().any(|root| root.message.is_some()) {
            for root in &mut response.roots {
                root.message = None;
            }
        } else {
            response.roots.clear();
            response.projects.clear();
            response.total = 0;
            response.offset = 0;
            response.limit = 1;
            // roots_config has its independent 64 KiB validated bound.
            break;
        }
    }
    response
}

fn cancel_workers(data: &mut CatalogData) {
    for worker in data
        .workers
        .iter()
        .chain(data.coordinator.iter())
        .chain(data.history_worker.iter())
    {
        worker.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for CatalogEngine {
    fn drop(&mut self) {
        if let Ok(data) = self.data.get_mut() {
            cancel_workers(data);
        }
    }
}

fn scan_root(
    root: &ProjectRoot,
    location: &StorageLocation,
    cancel: &AtomicBool,
    started: Instant,
    deadline: Duration,
    budget: Arc<AtomicUsize>,
) -> Result<ScanResult> {
    let mut cache = ProjectCache::empty(budget);
    let mut truncated = false;
    let directory = fs::read_dir(&root.path).map_err(DatabaseError::Io)?;
    let mut seen = HashSet::new();
    for (index, entry) in directory.enumerate() {
        if cancel.load(Ordering::Relaxed) || started.elapsed() >= deadline {
            return Err(DatabaseError::Busy("Scansione della radice"));
        }
        if index >= MAX_SCAN_ENTRIES || cache.len() >= MAX_CANDIDATES {
            truncated = true;
            break;
        }
        let entry = entry.map_err(DatabaseError::Io)?;
        let kind = entry.file_type().map_err(DatabaseError::Io)?;
        if !kind.is_dir() && !kind.is_symlink() {
            continue;
        }
        let path = match canonical_root_path(&entry.path()) {
            Ok(path) => path,
            Err(_) => {
                truncated = true;
                continue;
            }
        };
        let key = path.to_lowercase();
        if location.validate_project(Path::new(&path)).is_err() || seen.contains(&key) {
            continue;
        }
        let name = Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&path)
            .to_owned();
        if name.len() > 1024 {
            continue;
        }
        let modified_ms = fs::metadata(&path)
            .ok()
            .and_then(|info| info.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .and_then(|duration| i64::try_from(duration.as_millis()).ok());
        if !cache.push(
            ProjectCandidate {
                id: None,
                path,
                name,
                last_opened: None,
                available: true,
                modified_ms,
                root_id: root.id.clone(),
            },
            key.capacity(),
        ) {
            truncated = true;
            break;
        }
        seen.insert(key);
    }
    Ok(ScanResult { cache, truncated })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn network_is_a_project_location_but_never_shared_state_storage() {
        for path in [r"\\server\share\project", r"\\?\UNC\server\share\project"] {
            assert!(validate_project_shape(Path::new(path)).is_ok());
            assert!(StorageLocation::preflight(Path::new(path)).is_err());
        }
    }

    struct Gate(Arc<(Mutex<bool>, std::sync::Condvar)>);
    impl Drop for Gate {
        fn drop(&mut self) {
            let (lock, signal) = &*self.0;
            *lock.lock().unwrap() = true;
            signal.notify_all();
        }
    }

    fn test_database() -> (PathBuf, Arc<Database>) {
        use crate::location::LocationRequest;
        let root =
            std::env::temp_dir().join(format!("shinatama-catalog-fault-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: root.clone(),
            home_override: Some(root.join("state")),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        })
        .unwrap();
        (root, Arc::new(Database::open(&location, None).unwrap()))
    }

    fn wait_ready(engine: &CatalogEngine) -> CatalogResponse {
        let start = Instant::now();
        loop {
            let response = engine.snapshot(None, "", 0, 100);
            if !response.scanning {
                return response;
            }
            assert!(start.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn hung_root_is_not_respawned_and_a_healthy_root_keeps_responding() {
        use std::sync::atomic::AtomicUsize;
        let (directory, database) = test_database();
        let blocked = directory.join("blocked");
        let healthy = directory.join("healthy");
        fs::create_dir(&blocked).unwrap();
        fs::create_dir(&healthy).unwrap();
        fs::create_dir(healthy.join("usable-project")).unwrap();
        database
            .project_roots_set(RootsSetRequest {
                expected_revision: 0,
                roots: vec![
                    RootInput {
                        id: None,
                        path: blocked.to_string_lossy().into_owned(),
                    },
                    RootInput {
                        id: None,
                        path: healthy.to_string_lossy().into_owned(),
                    },
                ],
                default_root_index: Some(0),
            })
            .unwrap();
        let gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let release = Gate(gate.clone());
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let mut engine = CatalogEngine::with_deadline(Duration::from_millis(150));
        engine.scanner = Arc::new(move |root, location, cancel, started, deadline, budget| {
            if root.path.ends_with("blocked") {
                count.fetch_add(1, Ordering::SeqCst);
                let (lock, signal) = &*gate;
                let mut released = lock.lock().unwrap();
                while !*released {
                    released = signal.wait(released).unwrap();
                }
            }
            scan_root(root, location, cancel, started, deadline, budget)
        });
        let engine = Arc::new(engine);
        let source = database.clone();
        let loader: DatabaseLoader = Arc::new(move || Ok(source.clone()));
        for _ in 0..4 {
            let start = Instant::now();
            engine.read(
                loader.clone(),
                CatalogRequest::Refresh {
                    filter: String::new(),
                    offset: 0,
                    limit: Some(100),
                },
            );
            assert!(start.elapsed() < Duration::from_millis(250));
            let response = wait_ready(&engine);
            assert_eq!(response.roots[0].state, RootState::TimedOut);
            assert_eq!(response.roots[1].state, RootState::Ready);
            assert_eq!(response.projects[0].name, "usable-project");
            assert_eq!(calls.load(Ordering::SeqCst), 1);
        }
        let id = engine.snapshot(None, "", 0, 100).scan_id;
        let start = Instant::now();
        engine.cancel(Some(&id));
        assert!(start.elapsed() < Duration::from_millis(250));
        drop(release);
        let start = Instant::now();
        while engine
            .data
            .lock()
            .unwrap()
            .workers
            .iter()
            .any(|worker| !worker.handle.is_finished())
        {
            assert!(start.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(5));
        }
        drop(loader);
        drop(engine);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cancel_and_refresh_do_not_duplicate_a_blocked_configuration_loader() {
        use std::sync::atomic::AtomicUsize;
        let (directory, database) = test_database();
        let gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let release = Gate(gate.clone());
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let source = database.clone();
        let loader: DatabaseLoader = Arc::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
            let (lock, signal) = &*gate;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = signal.wait(released).unwrap();
            }
            Ok(source.clone())
        });
        let engine = Arc::new(CatalogEngine::with_deadline(Duration::from_millis(100)));
        let first = engine.read(
            loader.clone(),
            CatalogRequest::Refresh {
                filter: String::new(),
                offset: 0,
                limit: None,
            },
        );
        let start = Instant::now();
        while calls.load(Ordering::SeqCst) == 0 {
            assert!(start.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(5));
        }
        engine.cancel(Some(&first.scan_id));
        let second = engine.read(
            loader.clone(),
            CatalogRequest::Refresh {
                filter: String::new(),
                offset: 0,
                limit: None,
            },
        );
        assert_ne!(second.scan_id, first.scan_id);
        assert!(!second.scanning);
        assert!(second.error.is_some());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        drop(release);
        let start = Instant::now();
        while engine
            .data
            .lock()
            .unwrap()
            .coordinator
            .as_ref()
            .is_some_and(|worker| !worker.handle.is_finished())
        {
            assert!(start.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(engine.snapshot(None, "", 0, 100).scan_id, second.scan_id);
        drop(loader);
        drop(engine);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cache_budget_covers_workers_and_old_generation_references_until_last_drop() {
        let used = Arc::new(AtomicUsize::new(0));
        let mut caches = Vec::new();
        for root in 0..5 {
            let mut cache = ProjectCache::empty(used.clone());
            for index in 0..MAX_CANDIDATES {
                let project = ProjectCandidate {
                    id: None,
                    path: format!("C:\\{}\\{index}", "a".repeat(4000)),
                    name: "synthetic".into(),
                    last_opened: None,
                    available: true,
                    modified_ms: None,
                    root_id: root.to_string(),
                };
                let scratch = project.path.len();
                if !cache.push(project, scratch) {
                    break;
                }
            }
            assert!(cache.charge.bytes <= MAX_ROOT_CACHE_BYTES);
            assert!(used.load(Ordering::Acquire) <= MAX_CACHE_BYTES);
            caches.push(Arc::new(cache));
        }
        let old_snapshot = caches[0].clone();
        drop(caches);
        assert!(used.load(Ordering::Acquire) > 0);
        drop(old_snapshot);
        assert_eq!(used.load(Ordering::Acquire), 0);
    }

    #[test]
    fn payload_budget_reduces_page_without_skipping_the_next_candidate() {
        let projects = (0..100)
            .map(|index| ProjectCandidate {
                id: None,
                path: format!("C:\\{}\\{index:03}", "a".repeat(4000)),
                name: format!("project-{index}"),
                last_opened: None,
                available: true,
                modified_ms: None,
                root_id: "root".into(),
            })
            .collect();
        let response = bounded_response(CatalogResponse {
            scan_id: "scan".into(),
            revision: 0,
            roots_config: RootsConfig::default(),
            roots: Vec::new(),
            projects,
            total: 100,
            offset: 0,
            limit: 100,
            history: Vec::new(),
            recent: Vec::new(),
            scanning: false,
            config_loading: false,
            error: None,
            truncated: false,
        });
        assert!(response.truncated);
        assert_eq!(response.limit, response.projects.len());
        assert!(response.limit > 0 && response.limit < 100);
        assert!(serde_json::to_vec(&response).unwrap().len() <= MAX_RESPONSE_BYTES);
        assert_eq!(
            response.projects.last().unwrap().name,
            format!("project-{}", response.limit - 1)
        );
    }

    #[test]
    fn overlapping_roots_never_duplicate_a_folder_with_different_cached_usage() {
        let engine = CatalogEngine::new();
        engine.configure(RootsConfig {
            version: 1,
            revision: 0,
            roots: vec![
                ProjectRoot {
                    id: "a".into(),
                    path: r"C:\a".into(),
                },
                ProjectRoot {
                    id: "b".into(),
                    path: r"C:\b".into(),
                },
            ],
            default_root_id: None,
            non_standard_history: Vec::new(),
        });
        let candidate = |path: &str, root: &str, opened: Option<i64>| ProjectCandidate {
            id: opened.map(|_| "p_abcdefghijkl".into()),
            path: path.into(),
            name: path.into(),
            last_opened: opened,
            available: true,
            modified_ms: None,
            root_id: root.into(),
        };
        let mut first = ProjectCache::empty(engine.cache_bytes.clone());
        assert!(first.push(candidate(r"C:\shared", "a", Some(20)), 32));
        assert!(first.push(candidate(r"C:\other", "a", Some(10)), 32));
        let mut second = ProjectCache::empty(engine.cache_bytes.clone());
        assert!(second.push(candidate(r"C:\shared", "b", None), 32));
        {
            let mut data = engine.data.lock().unwrap();
            data.roots[0].projects = Arc::new(first);
            data.roots[1].projects = Arc::new(second);
        }
        let result = engine.snapshot(None, "", 0, 100);
        assert_eq!(result.total, 2);
        assert_eq!(
            result
                .projects
                .iter()
                .filter(|project| project.path == r"C:\shared")
                .count(),
            1
        );
        assert_eq!(result.projects[0].last_opened, Some(20));
    }

    #[test]
    fn cached_configuration_is_loading_until_the_database_revision_arrives_not_until_fs_finishes() {
        let (directory, database) = test_database();
        let root = directory.join("root");
        fs::create_dir(&root).unwrap();
        let config = database
            .project_roots_set(RootsSetRequest {
                expected_revision: 0,
                roots: vec![RootInput {
                    id: None,
                    path: root.to_string_lossy().into_owned(),
                }],
                default_root_index: Some(0),
            })
            .unwrap();
        let load_gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let release_load = Gate(load_gate.clone());
        let source = database.clone();
        let loader: DatabaseLoader = Arc::new(move || {
            let (lock, signal) = &*load_gate;
            let mut ready = lock.lock().unwrap();
            while !*ready {
                ready = signal.wait(ready).unwrap();
            }
            Ok(source.clone())
        });
        let fs_gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let release_fs = Gate(fs_gate.clone());
        let mut engine = CatalogEngine::with_deadline(Duration::from_secs(5));
        engine.scanner = Arc::new(move |root, location, cancel, started, deadline, budget| {
            let (lock, signal) = &*fs_gate;
            let mut ready = lock.lock().unwrap();
            while !*ready {
                ready = signal.wait(ready).unwrap();
            }
            scan_root(root, location, cancel, started, deadline, budget)
        });
        let engine = Arc::new(engine);
        let initial = engine.read(
            loader.clone(),
            CatalogRequest::Refresh {
                filter: String::new(),
                offset: 0,
                limit: None,
            },
        );
        assert_eq!(initial.revision, 0);
        assert!(initial.config_loading);
        assert_eq!(
            serde_json::to_value(&initial).unwrap()["configLoading"],
            true
        );
        drop(release_load);
        let start = Instant::now();
        loop {
            let snapshot = engine.snapshot(None, "", 0, 100);
            if !snapshot.config_loading {
                assert_eq!(snapshot.revision, config.revision);
                assert!(snapshot.scanning);
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(5));
        }
        drop(release_fs);
        let final_snapshot = wait_ready(&engine);
        assert!(!final_snapshot.config_loading);
        drop(loader);
        drop(engine);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn late_configure_and_loader_results_cannot_downgrade_a_committed_revision() {
        let (directory, database) = test_database();
        let engine = Arc::new(CatalogEngine::new());
        let latest = RootsConfig {
            revision: 2,
            ..RootsConfig::default()
        };
        let stale = RootsConfig {
            revision: 1,
            ..RootsConfig::default()
        };
        engine.configure(latest.clone());
        let id = engine.snapshot(None, "", 0, 100).scan_id;
        engine.configure(stale.clone());
        assert_eq!(engine.snapshot(None, "", 0, 100).scan_id, id);
        engine.start_roots(&id, database.clone(), stale, Vec::new(), Vec::new());
        let snapshot = engine.snapshot(None, "", 0, 100);
        assert_eq!(snapshot.roots_config, latest);
        assert!(!snapshot.config_loading);
        drop(engine);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }
}
