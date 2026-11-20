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
/// The key of the list of template ids; each template is the setting `<key>.<id>`.
const TEMPLATES_INDEX: &str = "team.templates.v1";
/// The readable templates by id, and the ids of those the index lists that are not readable as
/// templates.
type Templates = (Vec<(String, serde_json::Value)>, Vec<String>);
/// How many team templates are kept.
pub const MAX_TEMPLATES: usize = 64;

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

    /// C36 pinned projects (FRD-0004 R07): project ids, newest pin first, at most 100.
    pub fn pinned_projects(&self) -> Result<Vec<String>> {
        let connection = self.lock()?;
        let saved: Option<String> = connection
            .query_row(
                "SELECT value FROM setting WHERE key='projects.pinned.v1' AND octet_length(value)<=8192",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(saved
            .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|id| id.len() <= 64 && !id.chars().any(char::is_control))
            .take(100)
            .collect())
    }

    /// Pins or unpins one recorded project; returns the new pin list.
    pub fn set_project_pinned(&self, project_id: &str, pinned: bool) -> Result<Vec<String>> {
        self.require_writable()?;
        if project_id.is_empty() || project_id.len() > 64 || project_id.chars().any(char::is_control) {
            return Err(DatabaseError::InvalidProject("id"));
        }
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        schema::require_write_version(&transaction)?;
        let known: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM project WHERE id=?1)", [project_id], |row| row.get(0))?;
        if pinned && !known {
            return Err(DatabaseError::InvalidProject("unknown project"));
        }
        let saved: Option<String> = transaction
            .query_row("SELECT value FROM setting WHERE key='projects.pinned.v1' AND octet_length(value)<=8192", [], |row| row.get(0))
            .optional()?;
        let mut pins: Vec<String> = saved.and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default();
        pins.retain(|id| id != project_id);
        if pinned {
            pins.insert(0, project_id.to_owned());
        }
        pins.truncate(100);
        let json = serde_json::to_string(&pins).map_err(|_| DatabaseError::InvalidProject("pins"))?;
        transaction.execute(
            "INSERT INTO setting(key,value) VALUES('projects.pinned.v1',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [json],
        )?;
        transaction.commit()?;
        Ok(pins)
    }

    fn setting_json<T: serde::de::DeserializeOwned + Default>(&self, key: &str) -> Result<T> {
        let connection = self.lock()?;
        let saved: Option<String> = connection
            .query_row("SELECT value FROM setting WHERE key=?1 AND octet_length(value)<=65536", [key], |row| row.get(0))
            .optional()?;
        Ok(saved.and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default())
    }

    fn set_setting_json<T: serde::Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.require_writable()?;
        let json = serde_json::to_string(value).map_err(|_| DatabaseError::InvalidProject("setting"))?;
        if json.len() > 65536 {
            return Err(DatabaseError::InvalidProject("setting too large"));
        }
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        schema::require_write_version(&transaction)?;
        transaction.execute("INSERT INTO setting(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, json])?;
        transaction.commit()?;
        Ok(())
    }

    /// F06 titles switch; on by default (the verbatim titler sends nothing anywhere).
    pub fn titles_enabled(&self) -> Result<bool> {
        Ok(self.setting_json::<Option<bool>>("titles.enabled.v1")?.unwrap_or(true))
    }
    pub fn set_titles_enabled(&self, enabled: bool) -> Result<()> {
        self.set_setting_json("titles.enabled.v1", &Some(enabled))
    }

    /// Threads whose transcript is never read for a title or summary (FRD-0010 R22).
    pub fn summaries_excluded(&self) -> Result<Vec<String>> {
        Ok(self.setting_json::<Vec<String>>("threads.summaries_excluded.v1")?.into_iter().take(1000).collect())
    }
    pub fn set_summaries_excluded(&self, thread_id: &str, excluded: bool) -> Result<Vec<String>> {
        if thread_id.is_empty() || thread_id.len() > 64 || thread_id.chars().any(char::is_control) {
            return Err(DatabaseError::InvalidProject("thread id"));
        }
        let mut list = self.summaries_excluded()?;
        list.retain(|id| id != thread_id);
        if excluded {
            list.insert(0, thread_id.to_owned());
        }
        list.truncate(1000);
        self.set_setting_json("threads.summaries_excluded.v1", &list)?;
        Ok(list)
    }

    /// Threads (worksets, ADR-0055) whose sessions talk to each other through the
    /// thread's Mukade room (ADR-0057). Off unless listed.
    pub fn chat_threads(&self) -> Result<Vec<String>> {
        Ok(self
            .setting_json::<Vec<String>>("threads.chat_enabled.v1")?
            .into_iter()
            .filter(|id| crate::records::validate_id(id, crate::records::CanonicalIdKind::Workset).is_ok())
            .take(1000)
            .collect())
    }
    pub fn thread_chat_enabled(&self, workset_id: &str) -> Result<bool> {
        Ok(self.chat_threads()?.iter().any(|id| id == workset_id))
    }
    pub fn set_thread_chat(&self, workset_id: &str, enabled: bool) -> Result<Vec<String>> {
        crate::records::validate_id(workset_id, crate::records::CanonicalIdKind::Workset)
            .map_err(|_| DatabaseError::InvalidProject("thread id"))?;
        let mut list = self.chat_threads()?;
        list.retain(|id| id != workset_id);
        if enabled {
            list.insert(0, workset_id.to_owned());
        }
        list.truncate(1000);
        self.set_setting_json("threads.chat_enabled.v1", &list)?;
        Ok(list)
    }

    /// What a thread keeps of its team map: the applied formation and, per agent, what it was last
    /// told (`{ "formation": …, "told": { "<agent id>": { "revision", "member" } } }`). The shape
    /// belongs to the engine; this only keeps it. One setting per thread, because a setting holds
    /// 64 KiB and a full formation takes more than half of that.
    pub fn thread_team(&self, workset_id: &str) -> Result<Option<serde_json::Value>> {
        let key = Self::team_key(workset_id)?;
        let saved: Option<serde_json::Value> = self.setting_json(&key)?;
        Ok(saved.filter(|value| !value.is_null()))
    }

    /// `None` removes the thread's team.
    pub fn set_thread_team(&self, workset_id: &str, record: Option<&serde_json::Value>) -> Result<()> {
        let key = Self::team_key(workset_id)?;
        match record {
            Some(record) => self.set_setting_json(&key, record),
            None => {
                self.require_writable()?;
                let mut connection = self.lock()?;
                let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
                schema::require_write_version(&transaction)?;
                transaction.execute("DELETE FROM setting WHERE key=?1", params![key])?;
                transaction.commit()?;
                Ok(())
            }
        }
    }

    fn team_key(workset_id: &str) -> Result<String> {
        crate::records::validate_id(workset_id, crate::records::CanonicalIdKind::Workset)
            .map_err(|_| DatabaseError::InvalidProject("thread id"))?;
        Ok(format!("threads.team.v1.{workset_id}"))
    }

    /// The team templates, global to every thread and project, in the order the user keeps them.
    /// One setting per template (`team.templates.v1.<id>`) and an index (`team.templates.v1`, the
    /// ids in order): a setting holds 64 KiB and a full template takes more than half of that, so
    /// 64 of them could never share one. A template the index lists and the store lost, or that is
    /// not an object carrying its own id, is left out.
    pub fn team_templates(&self) -> Result<Vec<serde_json::Value>> {
        let connection = self.lock()?;
        Ok(Self::read_templates(&connection)?.0.into_iter().map(|(_, value)| value).collect())
    }

    /// Replace the whole list (see `edit_team_templates`).
    pub fn set_team_templates(&self, list: &[serde_json::Value]) -> Result<()> {
        self.edit_team_templates::<std::convert::Infallible>(|kept| {
            *kept = list.to_vec();
            Ok(())
        })?
        .map(|_| ())
        .map_err(|never| match never {})
    }

    /// Read the list, let `edit` change it and store the result, all in one write transaction, so
    /// that a check made by `edit` (at most 64 templates, one name once) holds when the list is
    /// stored even with two windows saving at once. An `edit` that refuses stores nothing and its
    /// refusal is the inner result. Only the templates that changed are written.
    pub fn edit_team_templates<E>(
        &self,
        edit: impl FnOnce(&mut Vec<serde_json::Value>) -> std::result::Result<(), E>,
    ) -> Result<std::result::Result<Vec<serde_json::Value>, E>> {
        self.require_writable()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        schema::require_write_version(&transaction)?;
        let (before, unreadable) = Self::read_templates(&transaction)?;
        let mut list: Vec<serde_json::Value> = before.iter().map(|(_, value)| value.clone()).collect();
        if let Err(refusal) = edit(&mut list) {
            return Ok(Err(refusal));
        }
        if list.len() > MAX_TEMPLATES {
            return Err(DatabaseError::InvalidProject("too many templates"));
        }
        let mut ids = Vec::with_capacity(list.len());
        let mut texts = Vec::with_capacity(list.len());
        for value in &list {
            let id = value.get("id").and_then(serde_json::Value::as_str).ok_or(DatabaseError::InvalidProject("template id"))?;
            Self::template_key(id)?;
            if ids.iter().any(|seen: &String| seen == id) {
                return Err(DatabaseError::InvalidProject("template id"));
            }
            let text = serde_json::to_string(value).map_err(|_| DatabaseError::InvalidProject("setting"))?;
            if text.len() > 65536 {
                return Err(DatabaseError::InvalidProject("setting too large"));
            }
            ids.push(id.to_owned());
            texts.push(text);
        }
        for (id, text) in ids.iter().zip(&texts) {
            let unchanged = before.iter().any(|(old, value)| old == id && serde_json::to_string(value).is_ok_and(|was| &was == text));
            if !unchanged {
                transaction.execute(
                    "INSERT INTO setting(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                    params![Self::template_key(id)?, text],
                )?;
            }
        }
        for (old, _) in &before {
            if !ids.contains(old) {
                transaction.execute("DELETE FROM setting WHERE key=?1", params![Self::template_key(old)?])?;
            }
        }
        // What this build cannot read stays listed (after the others) and its setting untouched,
        // as long as the list keeps within its bound: a format change must not lose templates.
        let mut listed = ids.clone();
        for kept in unreadable {
            if listed.len() < MAX_TEMPLATES && !listed.contains(&kept) {
                listed.push(kept);
            }
        }
        let index = serde_json::to_string(&listed).map_err(|_| DatabaseError::InvalidProject("setting"))?;
        transaction.execute(
            "INSERT INTO setting(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![TEMPLATES_INDEX, index],
        )?;
        transaction.commit()?;
        Ok(Ok(list))
    }

    fn template_key(id: &str) -> Result<String> {
        if id.is_empty() || id.len() > 40 || !id.bytes().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_')) {
            return Err(DatabaseError::InvalidProject("template id"));
        }
        Ok(format!("{TEMPLATES_INDEX}.{id}"))
    }

    /// The readable templates by id, and the ids of those the index lists whose setting exists but
    /// is not a template carrying its own id (a format this build does not know): an edit leaves
    /// those alone.
    fn read_templates(connection: &rusqlite::Connection) -> Result<Templates> {
        let read = |key: &str| -> Result<Option<String>> {
            Ok(connection
                .query_row("SELECT value FROM setting WHERE key=?1 AND octet_length(value)<=65536", [key], |row| row.get(0))
                .optional()?)
        };
        let ids: Vec<String> = read(TEMPLATES_INDEX)?.and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default();
        let mut found: Vec<(String, serde_json::Value)> = Vec::new();
        let mut unreadable: Vec<String> = Vec::new();
        for id in ids.into_iter().take(MAX_TEMPLATES) {
            let Ok(key) = Self::template_key(&id) else { continue };
            if found.iter().any(|(seen, _)| *seen == id) || unreadable.contains(&id) {
                continue;
            }
            let stored = read(&key)?;
            let value: Option<serde_json::Value> = stored.as_ref().and_then(|json| serde_json::from_str(json).ok());
            match value.filter(|value| value.get("id").and_then(serde_json::Value::as_str) == Some(id.as_str())) {
                Some(value) => found.push((id, value)),
                None if stored.is_some() => unreadable.push(id),
                None => {}
            }
        }
        Ok((found, unreadable))
    }

    /// C08's sole settings namespace. Reads never allocate an oversized value.
    pub fn presentation_json(&self) -> Result<Option<String>> {
        let connection = self.lock()?;
        let saved: Option<Option<String>> = connection.query_row(
            "SELECT CASE WHEN octet_length(value)<=8192 THEN value ELSE NULL END FROM setting WHERE key='ui.presentation.v1'",
            [], |row| row.get(0),
        ).optional()?;
        match saved {
            Some(Some(json)) => Ok(Some(json)),
            None => Ok(None),
            Some(None) => Err(DatabaseError::InvalidAppearance),
        }
    }

    /// Typed adapters validate the presentation DTO. This fixed-key boundary
    /// additionally bounds JSON and atomically compares its monotonic revision.
    pub fn set_presentation_json(&self, expected_revision: u64, json: &str) -> Result<()> {
        self.require_writable()?;
        if json.len() > 8192 || expected_revision >= 9_007_199_254_740_991 {
            return Err(DatabaseError::InvalidAppearance);
        }
        let incoming: serde_json::Value =
            serde_json::from_str(json).map_err(|_| DatabaseError::InvalidAppearance)?;
        if incoming.as_object().is_none_or(|object| object.len() != 2)
            || incoming
                .get("settings")
                .is_none_or(|value| !value.is_object())
            || incoming
                .get("settings")
                .and_then(|value| value.get("version"))
                .and_then(serde_json::Value::as_u64)
                != Some(1)
            || incoming.get("revision").and_then(serde_json::Value::as_u64)
                != Some(expected_revision + 1)
        {
            return Err(DatabaseError::InvalidAppearance);
        }
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        schema::require_write_version(&transaction)?;
        let current: Option<Option<String>>=transaction.query_row(
            "SELECT CASE WHEN octet_length(value)<=8192 THEN value ELSE NULL END FROM setting WHERE key='ui.presentation.v1'", [], |row|row.get(0),
        ).optional()?;
        let revision = match current {
            None => 0,
            Some(None) => return Err(DatabaseError::InvalidAppearance),
            Some(Some(json)) => {
                let value = serde_json::from_str::<serde_json::Value>(&json)
                    .map_err(|_| DatabaseError::InvalidAppearance)?;
                if value
                    .get("settings")
                    .and_then(|value| value.get("version"))
                    .and_then(serde_json::Value::as_u64)
                    != Some(1)
                {
                    return Err(DatabaseError::InvalidAppearance);
                }
                value
                    .get("revision")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or(DatabaseError::InvalidAppearance)?
            }
        };
        if revision != expected_revision {
            return Err(DatabaseError::Busy(
                "Presentation preferences changed; reload before saving",
            ));
        }
        transaction.execute("INSERT INTO setting(key,value) VALUES('ui.presentation.v1',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[json])?;
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
        let root = crate::test_scratch::root()
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

#[cfg(test)]
mod team_tests {
    use super::*;
    use crate::location::LocationRequest;
    use serde_json::json;

    fn database() -> Database {
        let root = crate::test_scratch::root()
            .join("shinatama-team-tests")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(root.join("projects")).unwrap();
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: root.clone(),
            home_override: Some(root.join("data")),
            project_roots: vec![root.join("projects")],
            synchronized_roots: Vec::new(),
        })
        .unwrap();
        Database::open(&location, None).unwrap()
    }

    const ONE: &str = "w_aaaaaaaaaaaa";
    const OTHER: &str = "w_bbbbbbbbbbbb";

    #[test]
    fn a_thread_s_team_is_kept_and_removed() {
        let database = database();
        assert_eq!(database.thread_team(ONE).unwrap(), None);
        let record = json!({"formation": {"revision": 2, "agents": [], "links": []}, "told": {}});
        database.set_thread_team(ONE, Some(&record)).unwrap();
        assert_eq!(database.thread_team(ONE).unwrap(), Some(record));
        database.set_thread_team(ONE, None).unwrap();
        assert_eq!(database.thread_team(ONE).unwrap(), None);
    }

    #[test]
    fn an_invalid_thread_id_is_refused() {
        let database = database();
        for bad in ["", "x_aaaaaaaaaaaa", "w_short", "w_AAAAAAAAAAAA", "w_aaaaaaaaaaaa/../x"] {
            assert!(database.set_thread_team(bad, Some(&json!({}))).is_err(), "{bad}");
            assert!(database.thread_team(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn another_thread_s_team_is_untouched() {
        let database = database();
        let first = json!({"formation": {"revision": 1}, "told": {}});
        let second = json!({"formation": {"revision": 9}, "told": {"a_aaaaaaaaaaaa": {"revision": 9, "member": "m"}}});
        database.set_thread_team(ONE, Some(&first)).unwrap();
        database.set_thread_team(OTHER, Some(&second)).unwrap();
        database.set_thread_team(ONE, None).unwrap();
        assert_eq!(database.thread_team(OTHER).unwrap(), Some(second));
    }

    #[test]
    fn a_full_formation_fits_in_its_own_record() {
        let database = database();
        let agents: Vec<_> = (0..24)
            .map(|i| json!({"id": format!("a_{i:012}"), "name": "n".repeat(40), "notes": "k".repeat(1024)}))
            .collect();
        let links: Vec<_> = (0..64).map(|i| json!({"from": "a", "to": "b", "words": "w".repeat(120), "i": i})).collect();
        let record = json!({"formation": {"revision": 1, "agents": agents, "links": links}, "told": {}});
        database.set_thread_team(ONE, Some(&record)).unwrap();
        database.set_thread_team(OTHER, Some(&record)).unwrap();
        assert_eq!(database.thread_team(OTHER).unwrap(), Some(record));
    }

    fn template(id: &str, name: &str) -> serde_json::Value {
        json!({"id": id, "name": name, "agents": [], "links": []})
    }

    #[test]
    fn templates_outlive_a_thread() {
        let database = database();
        assert_eq!(database.team_templates().unwrap(), Vec::<serde_json::Value>::new());
        let kept = vec![template("tm_aaaaaaaaaaaa", "Review"), template("tm_bbbbbbbbbbbb", "Ship")];
        database.set_team_templates(&kept).unwrap();
        database.set_thread_team(ONE, Some(&json!({"formation": {"revision": 1}, "told": {}}))).unwrap();
        database.set_thread_team(ONE, None).unwrap();
        assert_eq!(database.team_templates().unwrap(), kept);
        // Another thread, another handle on the same store: the list is global.
        database.set_thread_team(OTHER, Some(&json!({}))).unwrap();
        assert_eq!(database.team_templates().unwrap(), kept);
        // Removing one keeps the order of the others.
        database.set_team_templates(&kept[1..]).unwrap();
        assert_eq!(database.team_templates().unwrap(), kept[1..].to_vec());
    }

    #[test]
    fn sixty_four_templates_of_fifty_kib_each_are_kept_in_their_own_settings() {
        let database = database();
        let big = |i: usize| {
            let agents: Vec<_> = (0..24).map(|k| json!({"id": format!("a_{k:012}"), "notes": "k".repeat(1024)})).collect();
            json!({"id": format!("tm_{i:012}"), "name": format!("Team {i}"), "agents": agents, "links": [], "pad": "p".repeat(24_000)})
        };
        let list: Vec<_> = (0..64).map(big).collect();
        assert!(serde_json::to_string(&list).unwrap().len() > 65536);
        database.set_team_templates(&list).unwrap();
        assert_eq!(database.team_templates().unwrap(), list);
        // A template over a setting's size is refused and the stored list stays whole.
        let mut too_big = list.clone();
        too_big[3]["pad"] = json!("p".repeat(70_000));
        assert!(database.set_team_templates(&too_big).is_err());
        assert_eq!(database.team_templates().unwrap(), list);
        // A sixty-fifth one is refused by the store itself.
        let mut more = list.clone();
        more.push(big(64));
        assert!(database.set_team_templates(&more).is_err());
        assert_eq!(database.team_templates().unwrap().len(), 64);
    }

    #[test]
    fn an_edit_that_refuses_stores_nothing_and_one_that_adds_sees_the_stored_list() {
        let database = database();
        database.set_team_templates(&[template("tm_aaaaaaaaaaaa", "Review")]).unwrap();
        let refused = database
            .edit_team_templates(|list| {
                list.push(template("tm_bbbbbbbbbbbb", "Ship"));
                Err("no")
            })
            .unwrap();
        assert_eq!(refused, Err("no"));
        assert_eq!(database.team_templates().unwrap().len(), 1);
        let added: std::result::Result<_, ()> = database
            .edit_team_templates(|list| {
                assert_eq!(list.len(), 1);
                list.insert(0, template("tm_bbbbbbbbbbbb", "Ship"));
                Ok(())
            })
            .unwrap();
        assert_eq!(added.unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect::<Vec<_>>(), ["Ship", "Review"]);
        assert_eq!(database.team_templates().unwrap()[0]["id"], "tm_bbbbbbbbbbbb");
    }


    #[test]
    fn editing_the_templates_keeps_one_that_cannot_be_read() {
        let database = database();
        database.set_team_templates(&[template("tm_aaaaaaaaaaaa", "A"), template("tm_bbbbbbbbbbbb", "B")]).unwrap();
        database.lock().unwrap().execute("UPDATE setting SET value='7' WHERE key='team.templates.v1.tm_aaaaaaaaaaaa'", []).unwrap();
        assert_eq!(database.team_templates().unwrap().len(), 1, "the odd one is not listed");
        database.set_team_templates(&[template("tm_bbbbbbbbbbbb", "B"), template("tm_cccccccccccc", "C")]).unwrap();
        let connection = database.lock().unwrap();
        let text = |key: &str| -> Option<String> {
            connection.query_row("SELECT value FROM setting WHERE key=?1", [key], |row| row.get(0)).ok()
        };
        assert_eq!(text("team.templates.v1.tm_aaaaaaaaaaaa").as_deref(), Some("7"), "its setting is untouched");
        let index: Vec<String> = serde_json::from_str(&text("team.templates.v1").unwrap()).unwrap();
        assert_eq!(index, ["tm_bbbbbbbbbbbb", "tm_cccccccccccc", "tm_aaaaaaaaaaaa"], "and the index still names it, after the others");
    }
    #[test]
    fn a_template_without_a_usable_id_is_refused_and_a_lost_one_is_skipped() {
        let database = database();
        for bad in ["", "tm/../x", "TM_AAAAAAAAAAAA", "tm_a b"] {
            assert!(database.set_team_templates(&[template(bad, "x")]).is_err(), "{bad}");
        }
        assert!(database.set_team_templates(&[json!({"name": "no id"})]).is_err());
        assert!(database.set_team_templates(&[template("tm_aaaaaaaaaaaa", "A"), template("tm_aaaaaaaaaaaa", "B")]).is_err());
        assert!(database.team_templates().unwrap().is_empty());
        database.set_team_templates(&[template("tm_aaaaaaaaaaaa", "A"), template("tm_bbbbbbbbbbbb", "B")]).unwrap();
        // The store loses one setting: the other is still listed.
        database.lock().unwrap().execute("DELETE FROM setting WHERE key='team.templates.v1.tm_aaaaaaaaaaaa'", []).unwrap();
        let left = database.team_templates().unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0]["id"], "tm_bbbbbbbbbbbb");
    }
}
