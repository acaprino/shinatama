//! Typed local state. Callers schedule these blocking operations outside the UI thread.

use crate::location::{LocationError, StorageLocation};
use crate::migrations;
use crate::schema;
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

const HISTORY_LIMIT: usize = 10;
const MAX_LEGACY_BYTES: u64 = 256 * 1024;
const IMPORT_MARKER: &str = "migration.projects_json.v1";
const APPEARANCE_KEY: &str = "ui.appearance.v1";

#[derive(Debug)]
pub enum DatabaseError {
    Location(LocationError),
    Sqlite(rusqlite::Error),
    Io(io::Error),
    InvalidLegacyPreferences,
    InvalidProject(&'static str),
    InvalidAppearance,
    NewerSchema { found: u32, supported: u32 },
    Unavailable,
    IntegrityFailed,
    UnsupportedConfiguration,
    UnsupportedSchema,
    Busy(&'static str),
    BackupBudget(&'static str),
    CapacityBudget(&'static str),
    Cancelled,
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Location(error) => error.fmt(f),
            Self::Sqlite(error) => write!(f, "Errore del database locale: {error}"),
            Self::Io(error) => write!(f, "Impossibile accedere ai dati locali: {error}"),
            Self::InvalidLegacyPreferences => write!(
                f,
                "Cronologia progetti non valida o di una versione non supportata. Il file originale è conservato."
            ),
            Self::InvalidProject(reason) => {
                write!(f, "Cartella progetto non utilizzabile: {reason}")
            }
            Self::InvalidAppearance => write!(f, "Le preferenze dell'interfaccia non sono valide."),
            Self::NewerSchema { found, supported } => write!(
                f,
                "Database versione {found}, più recente di quella supportata ({supported}). Aperto in sola lettura; aggiorna Shinatama per modificarlo."
            ),
            Self::Unavailable => write!(f, "Il servizio di stato locale non è disponibile."),
            Self::IntegrityFailed => write!(
                f,
                "La verifica di integrità del database non è superata. I dati sono conservati."
            ),
            Self::UnsupportedConfiguration => write!(
                f,
                "SQLite non ha attivato la configurazione locale richiesta. I dati sono conservati."
            ),
            Self::UnsupportedSchema => write!(
                f,
                "Lo schema del database non corrisponde alla versione supportata. I dati sono conservati."
            ),
            Self::BackupBudget(reason) => write!(
                f,
                "Non è stato possibile completare la copia di sicurezza. {reason} I dati e le copie esistenti sono conservati. Dopo la correzione, chiudi e riapri Shinatama."
            ),
            Self::CapacityBudget(reason) => write!(
                f,
                "Salvataggio rinviato: {reason}. Il lavoro già salvato è conservato; libera spazio o archivia dati esplicitamente prima di riprovare."
            ),
            Self::Cancelled => write!(f, "Operazione annullata. I dati non sono stati modificati."),
            Self::Busy(operation) => write!(
                f,
                "{operation} in corso in un'altra finestra. Riprova dopo il completamento."
            ),
        }
    }
}

impl std::error::Error for DatabaseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Location(error) => Some(error),
            Self::Sqlite(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for DatabaseError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl From<LocationError> for DatabaseError {
    fn from(error: LocationError) -> Self {
        Self::Location(error)
    }
}

pub type Result<T> = std::result::Result<T, DatabaseError>;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProjectRecord {
    pub id: String,
    pub path: String,
    pub name: String,
    pub last_opened: i64,
    pub available: bool,
}

/// Filesystem validation is complete before a caller enters its SQL transaction.
pub(crate) struct PreparedProject {
    path: String,
    name: String,
    key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AppearanceSettings {
    pub terminal_font_size: u8,
    pub reduced_motion: bool,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            terminal_font_size: 14,
            reduced_motion: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum DatabaseAccess {
    ReadWrite,
    ReadOnlyNewer { found: u32, supported: u32 },
}

#[derive(Debug, Clone, Serialize)]
pub struct DatabaseDiagnostics {
    pub path: PathBuf,
    pub access: DatabaseAccess,
    pub schema_version: u32,
    pub sqlite_version: String,
    pub journal_mode: String,
    pub synchronous: i64,
    pub foreign_keys: bool,
    pub busy_timeout_ms: i64,
    pub fts5: bool,
}

/// Each process owns its connection. There is no resident broker and no public SQL API.
pub struct Database {
    location: StorageLocation,
    connection: Mutex<Connection>,
    access: DatabaseAccess,
}

impl Database {
    /// Legacy preferences are imported atomically once, never rewritten or deleted.
    /// Later edits by an older JSON-only binary are retained there, never replayed silently.
    /// Unknown/corrupt databases fail without any automatic recreation.
    pub fn open(location: &StorageLocation, legacy_preferences: Option<&Path>) -> Result<Self> {
        location.prepare_private_directory()?;
        // Cooperating versions serialize startup through this private, crash-released lock.
        // This gate is not a claim about external programs that ignore the protocol.
        let _startup_guard =
            acquire_private_lock(location, "schema.lock", "Apertura del database", true)?;
        secure_location_files(location)?;
        let path = location.database_path();
        let exists = path.try_exists().map_err(DatabaseError::Io)?;
        if exists {
            location.secure_private_file(&path)?;
            let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            connection.busy_timeout(Duration::from_millis(5000))?;
            let version: u32 =
                connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
            if version > schema::CURRENT_VERSION {
                connection.pragma_update(None, "foreign_keys", true)?;
                connection.pragma_update(None, "query_only", true)?;
                secure_location_files(location)?;
                return Ok(Self {
                    location: location.clone(),
                    connection: Mutex::new(connection),
                    access: DatabaseAccess::ReadOnlyNewer {
                        found: version,
                        supported: schema::CURRENT_VERSION,
                    },
                });
            }
            schema::validate_version(&connection, version).map_err(schema_error)?;
        }
        let mut connection = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        location.secure_private_file(&path)?;
        connection.busy_timeout(Duration::from_millis(5000))?;
        let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > schema::CURRENT_VERSION {
            drop(connection);
            // Retry through the read-only branch, while retaining the startup gate.
            let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            connection.busy_timeout(Duration::from_millis(5000))?;
            connection.pragma_update(None, "foreign_keys", true)?;
            connection.pragma_update(None, "query_only", true)?;
            return Ok(Self {
                location: location.clone(),
                connection: Mutex::new(connection),
                access: DatabaseAccess::ReadOnlyNewer {
                    found: version,
                    supported: schema::CURRENT_VERSION,
                },
            });
        }
        schema::validate_version(&connection, version).map_err(schema_error)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.pragma_update(None, "foreign_keys", true)?;
        verify_configuration(&connection)?;
        verify_fts5(&connection)?;
        migrations::initialize(&mut connection, location)?;
        let database = Self {
            location: location.clone(),
            connection: Mutex::new(connection),
            access: DatabaseAccess::ReadWrite,
        };
        database.secure_files()?;
        if let Some(path) = legacy_preferences {
            database.import_legacy(path)?;
        }
        Ok(database)
    }

    pub fn access(&self) -> &DatabaseAccess {
        &self.access
    }

    pub(crate) fn location(&self) -> &StorageLocation {
        &self.location
    }

    /// Uses its own connection so optional work never holds the UI service lock.
    pub fn maintain(
        &self,
        policy: crate::maintenance::Policy,
    ) -> Result<crate::maintenance::Report> {
        crate::maintenance::run_database(
            self.location(),
            matches!(self.access, DatabaseAccess::ReadWrite),
            policy,
        )
    }

    pub fn diagnostics(&self) -> Result<DatabaseDiagnostics> {
        let connection = self.lock()?;
        Ok(DatabaseDiagnostics {
            path: self.location.database_path(),
            access: self.access.clone(),
            schema_version: connection
                .pragma_query_value(None, "user_version", |row| row.get(0))?,
            sqlite_version: connection
                .query_row("SELECT sqlite_version()", [], |row| row.get(0))?,
            journal_mode: connection.pragma_query_value(None, "journal_mode", |row| row.get(0))?,
            synchronous: connection.pragma_query_value(None, "synchronous", |row| row.get(0))?,
            foreign_keys: connection.pragma_query_value(None, "foreign_keys", |row| row.get(0))?,
            busy_timeout_ms: connection
                .pragma_query_value(None, "busy_timeout", |row| row.get(0))?,
            fts5: connection.query_row(
                "SELECT sqlite_compileoption_used('ENABLE_FTS5')",
                [],
                |row| row.get(0),
            )?,
        })
    }

    /// Unavailable project roots remain visible; they never cause an implicit fallback.
    pub fn recent(&self) -> Result<Vec<ProjectRecord>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id,path,name,last_opened FROM project ORDER BY last_opened DESC,recent_sequence DESC LIMIT ?1",
        )?;
        let mut entries = statement
            .query_map([HISTORY_LIMIT as i64], |row| {
                Ok(ProjectRecord {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    name: row.get(2)?,
                    last_opened: row.get(3)?,
                    available: false,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        drop(connection);
        for entry in &mut entries {
            entry.available = Path::new(&entry.path).is_dir();
        }
        Ok(entries)
    }

    /// Select an existing folder without creating or modifying anything in it.
    pub fn select(&self, path: &Path) -> Result<ProjectRecord> {
        self.select_if_not_cancelled(path, &crate::cancellation::MutationToken::new())
    }

    pub fn select_if_not_cancelled(
        &self,
        path: &Path,
        cancellation: &crate::cancellation::MutationToken,
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
        let permit = cancellation.begin_commit()?;
        let selected = self.record_selection(&transaction, &prepared)?;
        transaction.commit()?;
        permit.finish();
        Ok(selected)
    }

    pub(crate) fn prepare_project(&self, path: &Path) -> Result<PreparedProject> {
        self.require_writable()?;
        self.location.validate_project(path)?;
        let canonical = canonical_project(path)?;
        let name = Path::new(&canonical)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or(&canonical)
            .to_owned();
        let key = path_key(&canonical);
        Ok(PreparedProject {
            path: canonical,
            name,
            key,
        })
    }

    pub(crate) fn record_selection(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        prepared: &PreparedProject,
    ) -> Result<ProjectRecord> {
        crate::schema::require_write_version(transaction)?;
        let (previous_time, previous_sequence): (i64, i64) = transaction.query_row(
            "SELECT coalesce(max(last_opened),0),coalesce(max(recent_sequence),0) FROM project",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let timestamp = chrono::Utc::now()
            .timestamp_millis()
            .max(previous_time.saturating_add(1));
        let id = transaction.query_row(
            "INSERT INTO project(id,path,path_key,name,last_opened,recent_sequence) VALUES(?1,?2,?3,?4,?5,?6)
             ON CONFLICT(path_key) DO UPDATE SET path=excluded.path,name=excluded.name,last_opened=excluded.last_opened,recent_sequence=excluded.recent_sequence
             RETURNING id",
            params![project_id(),prepared.path,prepared.key,prepared.name,timestamp,previous_sequence.saturating_add(1)], |row| row.get(0),
        )?;
        Ok(ProjectRecord {
            id,
            path: prepared.path.clone(),
            name: prepared.name.clone(),
            last_opened: timestamp,
            available: true,
        })
    }

    pub fn appearance(&self) -> Result<AppearanceSettings> {
        let connection = self.lock()?;
        let saved: Option<String> = connection
            .query_row(
                "SELECT value FROM setting WHERE key=?1",
                [APPEARANCE_KEY],
                |row| row.get(0),
            )
            .optional()?;
        let settings = match saved {
            Some(json) => {
                serde_json::from_str(&json).map_err(|_| DatabaseError::InvalidAppearance)?
            }
            None => AppearanceSettings::default(),
        };
        validate_appearance(&settings)?;
        Ok(settings)
    }

    pub fn set_appearance(&self, settings: &AppearanceSettings) -> Result<()> {
        self.require_writable()?;
        validate_appearance(settings)?;
        let json = serde_json::to_string(settings).map_err(|_| DatabaseError::InvalidAppearance)?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        schema::require_write_version(&transaction)?;
        transaction.execute("INSERT INTO setting(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![APPEARANCE_KEY,json])?;
        transaction.commit()?;
        Ok(())
    }

    /// This check never rebuilds or deletes authoritative data.
    pub fn integrity_check(&self) -> Result<()> {
        let connection = self.lock()?;
        let mut statement = connection.prepare("PRAGMA integrity_check")?;
        let results = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if results == ["ok"] {
            Ok(())
        } else {
            Err(DatabaseError::IntegrityFailed)
        }
    }

    pub(crate) fn lock(&self) -> Result<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| DatabaseError::Unavailable)
    }

    pub(crate) fn require_writable(&self) -> Result<()> {
        match self.access {
            DatabaseAccess::ReadWrite => crate::maintenance::admit_write(self.location()),
            DatabaseAccess::ReadOnlyNewer { found, supported } => {
                Err(DatabaseError::NewerSchema { found, supported })
            }
        }
    }

    /// Check current schema and storage admission before an external side effect.
    /// This does not reserve a future SQL commit; typed writes still recheck in
    /// their transaction. No database lock is held during filesystem admission.
    pub fn check_write_admission(&self) -> Result<()> {
        {
            let connection = self.lock()?;
            schema::require_write_version(&connection)?;
        }
        self.require_writable()
    }

    fn secure_files(&self) -> Result<()> {
        secure_location_files(&self.location)
    }

    fn import_legacy(&self, path: &Path) -> Result<()> {
        {
            let connection = self.lock()?;
            if import_completed(&connection)? {
                return Ok(());
            }
        }
        self.require_writable()?;
        if path.file_name() != Some(std::ffi::OsStr::new("projects.json")) {
            return Err(DatabaseError::InvalidLegacyPreferences);
        }
        let parent = path
            .parent()
            .ok_or(DatabaseError::InvalidLegacyPreferences)?;
        let legacy_location = self.location.resolve_related_directory(parent)?;
        legacy_location.prepare_private_directory()?;
        let legacy_path = legacy_location.directory().join("projects.json");
        // This is the very same persistent lock used by B02's atomic JSON writer.
        // Keep it until both imported rows and their completion marker are committed.
        let _legacy_guard = acquire_private_lock(
            &legacy_location,
            "projects.lock",
            "Aggiornamento della cronologia progetti",
            false,
        )?;
        if legacy_path.try_exists().map_err(DatabaseError::Io)? {
            legacy_location.secure_private_file(&legacy_path)?;
        }
        // Bounded file IO and validation happen before taking a write transaction.
        let projects = read_legacy(&legacy_path)?;
        for project in &projects {
            self.location.validate_project(Path::new(&project.path))?;
        }
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if import_completed(&transaction)? {
            return Ok(());
        }
        schema::require_write_version(&transaction)?;
        let previous_sequence: i64 = transaction.query_row(
            "SELECT coalesce(max(recent_sequence),0) FROM project",
            [],
            |row| row.get(0),
        )?;
        for (index, project) in projects.iter().enumerate() {
            transaction.execute(
                "INSERT INTO project(id,path,path_key,name,last_opened,recent_sequence) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(path_key) DO NOTHING",
                params![project_id(),project.path,path_key(&project.path),project.name,project.last_opened,previous_sequence.saturating_add((projects.len()-index) as i64)],
            )?;
        }
        transaction.execute(
            "INSERT INTO setting(key,value) VALUES(?1,?2)",
            params![IMPORT_MARKER, "{\"completed\":true}"],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

fn schema_error(error: rusqlite::Error) -> DatabaseError {
    if matches!(error, rusqlite::Error::InvalidQuery) {
        DatabaseError::UnsupportedSchema
    } else {
        DatabaseError::Sqlite(error)
    }
}

fn acquire_private_lock(
    location: &StorageLocation,
    name: &str,
    operation: &'static str,
    wait: bool,
) -> Result<File> {
    let path = location.directory().join(name);
    let options = |create_new| {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(create_new);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
            };
            options
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
        options
    };
    let file = match options(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            options(false).open(&path).map_err(DatabaseError::Io)?
        }
        Err(error) => return Err(DatabaseError::Io(error)),
    };
    // The no-follow, no-delete-sharing handle above keeps the inspected name stable.
    // The location guard checks ownership, reparse points and hard links before ACL changes.
    location.secure_private_file(&path)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::WouldBlock) if wait && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(TryLockError::WouldBlock) => return Err(DatabaseError::Busy(operation)),
            Err(TryLockError::Error(error)) => return Err(DatabaseError::Io(error)),
        }
    }
}

fn secure_location_files(location: &StorageLocation) -> Result<()> {
    let database = location.database_path();
    if database.try_exists().map_err(DatabaseError::Io)? {
        location.secure_private_file(&database)?;
    }
    for name in ["shinatama.db-wal", "shinatama.db-shm"] {
        let path = location.directory().join(name);
        // SQLite's last connection may delete these independently of schema.lock.
        // Regenerated sidecars inherit the already-verified private directory ACL;
        // existing sidecars still receive the full no-follow/ownership inspection.
        location.secure_private_file_if_present(&path)?;
    }
    Ok(())
}

fn verify_configuration(connection: &Connection) -> Result<()> {
    let journal: String = connection.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    let synchronous: u32 = connection.pragma_query_value(None, "synchronous", |row| row.get(0))?;
    let foreign_keys: bool =
        connection.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
    let timeout: u32 = connection.pragma_query_value(None, "busy_timeout", |row| row.get(0))?;
    if journal == "wal" && synchronous == 1 && foreign_keys && timeout == 5000 {
        Ok(())
    } else {
        Err(DatabaseError::UnsupportedConfiguration)
    }
}

fn verify_fts5(connection: &Connection) -> Result<()> {
    connection.execute_batch("CREATE VIRTUAL TABLE temp.shinatama_fts_probe USING fts5(text); INSERT INTO shinatama_fts_probe(text) VALUES('shinatama');")?;
    let found: u32 = connection.query_row(
        "SELECT count(*) FROM shinatama_fts_probe WHERE shinatama_fts_probe MATCH 'shinatama'",
        [],
        |row| row.get(0),
    )?;
    connection.execute_batch("DROP TABLE temp.shinatama_fts_probe")?;
    if found == 1 {
        Ok(())
    } else {
        Err(DatabaseError::UnsupportedConfiguration)
    }
}

fn import_completed(connection: &Connection) -> Result<bool> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM setting WHERE key=?1)",
        [IMPORT_MARKER],
        |row| row.get(0),
    )?)
}

fn validate_appearance(settings: &AppearanceSettings) -> Result<()> {
    if (8..=40).contains(&settings.terminal_font_size) {
        Ok(())
    } else {
        Err(DatabaseError::InvalidAppearance)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyPreferences {
    schema_version: u32,
    projects: Vec<LegacyProject>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyProject {
    path: String,
    name: String,
    last_opened: i64,
}

fn read_legacy(path: &Path) -> Result<Vec<LegacyProject>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(DatabaseError::Io(error)),
    };
    let mut bytes = Vec::new();
    file.take(MAX_LEGACY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(DatabaseError::Io)?;
    if bytes.len() as u64 > MAX_LEGACY_BYTES {
        return Err(DatabaseError::InvalidLegacyPreferences);
    }
    let mut saved: LegacyPreferences =
        serde_json::from_slice(&bytes).map_err(|_| DatabaseError::InvalidLegacyPreferences)?;
    if saved.schema_version != 1 || saved.projects.len() > HISTORY_LIMIT {
        return Err(DatabaseError::InvalidLegacyPreferences);
    }
    let mut keys = HashSet::new();
    for project in &mut saved.projects {
        if !Path::new(&project.path).is_absolute()
            || project.name.is_empty()
            || project.last_opened < 0
        {
            return Err(DatabaseError::InvalidLegacyPreferences);
        }
        project.path = match canonical_project(Path::new(&project.path)) {
            Ok(path) => path,
            Err(_) => normalize_path(&project.path),
        };
        if !keys.insert(path_key(&project.path)) {
            return Err(DatabaseError::InvalidLegacyPreferences);
        }
    }
    // Stable sort retains JSON order when timestamps are tied.
    saved
        .projects
        .sort_by_key(|project| std::cmp::Reverse(project.last_opened));
    Ok(saved.projects)
}

fn canonical_project(path: &Path) -> Result<String> {
    if !path.is_absolute() {
        return Err(DatabaseError::InvalidProject("scegli un percorso assoluto"));
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| DatabaseError::InvalidProject("la cartella non esiste o non è accessibile"))?;
    if !canonical.is_dir() {
        return Err(DatabaseError::InvalidProject(
            "scegli una cartella, non un file",
        ));
    }
    let text = canonical.to_str().ok_or(DatabaseError::InvalidProject(
        "il percorso non è Unicode valido",
    ))?;
    Ok(normalize_path(text))
}

fn normalize_path(path: &str) -> String {
    #[cfg(windows)]
    {
        let mut normal = if let Some(unc) = path.strip_prefix("\\\\?\\UNC\\") {
            format!("\\\\{unc}")
        } else {
            path.strip_prefix("\\\\?\\").unwrap_or(path).to_owned()
        };
        if normal.as_bytes().get(1) == Some(&b':') {
            normal.replace_range(..1, &normal[..1].to_ascii_uppercase());
        }
        normal
    }
    #[cfg(not(windows))]
    {
        path.to_owned()
    }
}

fn path_key(path: &str) -> String {
    #[cfg(windows)]
    {
        path.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        path.to_owned()
    }
}

fn project_id() -> String {
    const BASE32: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let random = uuid::Uuid::new_v4();
    let bytes = random.as_bytes();
    // Skip the UUID version and variant bytes, retaining 60 independent random bits.
    let mut value = u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[7], bytes[9],
    ]);
    let mut id = String::from("p_");
    for _ in 0..12 {
        id.push(BASE32[(value & 31) as usize] as char);
        value >>= 5;
    }
    id
}

#[cfg(all(test, windows))]
mod fault_tests {
    use super::*;
    use crate::location::LocationRequest;
    use std::fs;

    #[test]
    fn sqlite_full_rolls_back_the_failed_write_and_preserves_history() {
        let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
            .join("shinatama-full-tests")
            .join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(root.join("projects")).unwrap();
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: root.clone(),
            home_override: Some(root.join("data")),
            project_roots: vec![root.join("projects")],
            synchronized_roots: Vec::new(),
        })
        .unwrap();
        let database = Database::open(&location, None).unwrap();
        let initial = root.join("projects").join("initial");
        fs::create_dir(&initial).unwrap();
        database.select(&initial).unwrap();
        {
            let connection = database.connection.lock().unwrap();
            let pages: u32 = connection
                .pragma_query_value(None, "page_count", |row| row.get(0))
                .unwrap();
            connection
                .pragma_update(None, "max_page_count", pages)
                .unwrap();
        }
        let mut saw_full = false;
        for index in 0..200 {
            let project = root
                .join("projects")
                .join(format!("{index:03}-{}", "x".repeat(96)));
            fs::create_dir(&project).unwrap();
            let before = database.recent().unwrap();
            match database.select(&project) {
                Ok(_) => {}
                Err(DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(error, _)))
                    if error.code == rusqlite::ErrorCode::DiskFull =>
                {
                    saw_full = true;
                    assert_eq!(database.recent().unwrap(), before);
                    break;
                }
                Err(error) => panic!("Expected SQLite FULL, got {error}"),
            }
        }
        assert!(
            saw_full,
            "bounded page count must cause a real SQLITE_FULL result"
        );
        database.integrity_check().unwrap();
        drop(database);
        drop(location);
        fs::remove_dir_all(root).unwrap();
    }
}
