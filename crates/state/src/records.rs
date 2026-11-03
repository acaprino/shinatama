//! FS03 typed records. Schema activation belongs to the verified-backup migration.
//! These operations restore data and never start a process or qualify an adapter.

mod database;
pub(crate) use database::read_store_root;
pub mod recovery;
pub(crate) mod schema;

use crate::database::{Database, DatabaseError};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

pub const MAX_RECORD_PAGE: usize = 100;
pub const MAX_LAYOUT_BYTES: usize = 256 * 1024;
pub const MAX_SAFE_REVISION: u64 = 9_007_199_254_740_991;

pub enum RecordsError {
    Invalid(&'static str),
    Database(DatabaseError),
    Sqlite(rusqlite::Error),
    Conflict,
    OwnedElsewhere,
    NotFound,
    UnsupportedLayout,
}
impl fmt::Debug for RecordsError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, output)
    }
}
impl From<DatabaseError> for RecordsError {
    fn from(value: DatabaseError) -> Self {
        Self::Database(value)
    }
}
impl From<rusqlite::Error> for RecordsError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}
impl fmt::Display for RecordsError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(reason) => write!(output, "Record non valido: {reason}."),
            Self::Database(_) | Self::Sqlite(_) => output.write_str("Impossibile completare il salvataggio dei record; i dati precedenti sono conservati."),
            Self::Conflict => output.write_str("Il record è cambiato in un'altra finestra; ricaricalo prima di salvare."),
            Self::OwnedElsewhere => output.write_str("Lo spazio di lavoro appartiene a un'altra finestra."),
            Self::NotFound => output.write_str("Il record richiesto non è disponibile."),
            Self::UnsupportedLayout => output.write_str("Questa versione del layout non è supportata; il documento salvato è conservato."),
        }
    }
}
impl std::error::Error for RecordsError {}
pub type Result<T> = std::result::Result<T, RecordsError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableAuthority {
    Authoritative,
    Operational,
    DerivedDeferred,
    Unknown,
}
pub fn table_authority(name: &str) -> TableAuthority {
    if schema::AUTHORITATIVE_NAMES.contains(&name) {
        TableAuthority::Authoritative
    } else if schema::OPERATIONAL_NAMES.contains(&name) {
        TableAuthority::Operational
    } else if schema::DERIVED_DEFERRED_NAMES.contains(&name) {
        TableAuthority::DerivedDeferred
    } else {
        TableAuthority::Unknown
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanonicalIdKind {
    Project,
    Thread,
    Segment,
    Hop,
    Instance,
    Roundtable,
    Message,
    BrowserSession,
}
impl CanonicalIdKind {
    fn prefix(self) -> &'static str {
        match self {
            Self::Project => "p_",
            Self::Thread => "t_",
            Self::Segment => "s_",
            Self::Hop => "h_",
            Self::Instance => "i_",
            Self::Roundtable => "r_",
            Self::Message => "m_",
            Self::BrowserSession => "bs_",
        }
    }
}

pub fn validate_id(value: &str, kind: CanonicalIdKind) -> Result<()> {
    let suffix = value
        .strip_prefix(kind.prefix())
        .ok_or(RecordsError::Invalid("id"))?;
    if suffix.len() != 12
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || (b'2'..=b'7').contains(&byte))
    {
        return Err(RecordsError::Invalid("id"));
    }
    Ok(())
}

pub fn new_id(kind: CanonicalIdKind) -> String {
    const BASE32: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let random = uuid::Uuid::new_v4();
    let bytes = random.as_bytes();
    let mut bits = u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[7], bytes[9],
    ]);
    let mut id = String::from(kind.prefix());
    for _ in 0..12 {
        id.push(BASE32[(bits & 31) as usize] as char);
        bits >>= 5;
    }
    id
}

/// Prefixes not assigned by the spec (root/workspace/window/activation) are not
/// fabricated here. Those keys are supplied by their owning feature.
fn validate_key(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        Err(RecordsError::Invalid("identificativo"))
    } else {
        Ok(())
    }
}
fn validate_revision(value: u64) -> Result<()> {
    if value == 0 || value > MAX_SAFE_REVISION {
        Err(RecordsError::Invalid("revisione"))
    } else {
        Ok(())
    }
}
fn validate_millis(value: i64) -> Result<()> {
    if value < 0 {
        Err(RecordsError::Invalid("data UTC"))
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootKind {
    Box,
    System,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootScope {
    Primary,
    Additional,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootOrigin {
    Configured,
    Env,
    Catalog,
    Scan,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootAccess {
    ReadOnly,
    ReadWrite,
}
impl RootKind {
    fn sql(self) -> &'static str {
        match self {
            Self::Box => "box",
            Self::System => "system",
        }
    }
}
impl RootScope {
    fn sql(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Additional => "additional",
        }
    }
}
impl RootOrigin {
    fn sql(self) -> &'static str {
        match self {
            Self::Configured => "configured",
            Self::Env => "env",
            Self::Catalog => "catalog",
            Self::Scan => "scan",
        }
    }
}
impl RootAccess {
    fn sql(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::ReadWrite => "read_write",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreRootRecord {
    pub id: String,
    pub harness: String,
    pub kind: RootKind,
    pub scope: RootScope,
    pub resolved_path: String,
    pub origin: RootOrigin,
    pub config_revision: u64,
    pub access: RootAccess,
    pub available: bool,
}
impl StoreRootRecord {
    /// Canonicalization/held C11 grants are prepared by the caller before SQL.
    /// This structural validation never claims that a string proves an ACL/home.
    pub fn validate(&self) -> Result<()> {
        validate_key(&self.id)?;
        validate_revision(self.config_revision)?;
        if self.harness.is_empty()
            || self.harness.len() > 64
            || !self
                .harness
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(RecordsError::Invalid("agente"));
        }
        if self.resolved_path.is_empty()
            || self.resolved_path.len() > 4096
            || self.resolved_path.chars().any(char::is_control)
            || !Path::new(&self.resolved_path).is_absolute()
            || self.resolved_path.starts_with(r"\\?\")
            || Path::new(&self.resolved_path)
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(RecordsError::Invalid("percorso canonico"));
        }
        if self.scope == RootScope::Additional
            && (self.kind != RootKind::System || self.access != RootAccess::ReadOnly)
        {
            return Err(RecordsError::Invalid("radice aggiuntiva in sola lettura"));
        }
        Ok(())
    }
    pub fn validate_additional_system_root(&self) -> Result<()> {
        self.validate()?;
        if self.scope != RootScope::Additional
            || self.kind != RootKind::System
            || self.access != RootAccess::ReadOnly
        {
            return Err(RecordsError::Invalid("radice aggiuntiva in sola lettura"));
        }
        Ok(())
    }
}

/// Filesystem resolution precedes admission/SQL. A C11 boxed-home grant must be
/// held by the launch caller independently; registration is not home qualification.
#[derive(Clone, Debug)]
pub struct PreparedStoreRoot {
    record: StoreRootRecord,
}
impl PreparedStoreRoot {
    pub fn resolve(mut record: StoreRootRecord) -> Result<Self> {
        validate_key(&record.id)?;
        validate_revision(record.config_revision)?;
        if record.resolved_path.len() > 4096 || record.resolved_path.chars().any(char::is_control) {
            return Err(RecordsError::Invalid("percorso"));
        }
        record.resolved_path =
            crate::projects::canonical_root_path(Path::new(&record.resolved_path))?;
        record.validate()?;
        if !record.available {
            return Err(RecordsError::Invalid("nuova radice non disponibile"));
        }
        Ok(Self { record })
    }
    pub fn record(&self) -> &StoreRootRecord {
        &self.record
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSessionKey {
    pub store_root_id: String,
    pub store_revision: u64,
    pub native_id: String,
}
impl NativeSessionKey {
    pub fn validate(&self) -> Result<()> {
        validate_key(&self.store_root_id)?;
        validate_revision(self.store_revision)?;
        validate_key(&self.native_id)
    }
    pub fn matches_root(&self, root: &StoreRootRecord) -> bool {
        self.store_root_id == root.id && self.store_revision == root.config_revision
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadKind {
    Work,
    Roundtable,
    Companion,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadRelations {
    pub id: String,
    pub kind: ThreadKind,
    pub parent_thread_id: Option<String>,
    pub companion_worker_thread_id: Option<String>,
}
impl ThreadRelations {
    pub fn validate(&self) -> Result<()> {
        validate_id(&self.id, CanonicalIdKind::Thread)?;
        for reference in [&self.parent_thread_id, &self.companion_worker_thread_id]
            .into_iter()
            .flatten()
        {
            validate_id(reference, CanonicalIdKind::Thread)?;
            if reference == &self.id {
                return Err(RecordsError::Invalid("relazione ciclica"));
            }
        }
        if (self.kind == ThreadKind::Companion && self.companion_worker_thread_id.is_none())
            || (self.kind != ThreadKind::Companion && self.companion_worker_thread_id.is_some())
        {
            return Err(RecordsError::Invalid(
                "legame companion distinto dalla discendenza hop",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceOwner {
    pub window_id: String,
    pub pid: u32,
    pub start_time: String,
}
impl WorkspaceOwner {
    pub fn validate(&self) -> Result<()> {
        validate_key(&self.window_id)?;
        validate_key(&self.start_time)?;
        if self.pid == 0 {
            return Err(RecordsError::Invalid("proprietario"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceRecord {
    pub project_id: String,
    pub workspace_id: String,
    pub revision: u64,
    pub schema_version: u32,
    pub layout_json: String,
    pub owner: Option<WorkspaceOwner>,
    pub updated_at: i64,
}

/// Data-only restoration. There is deliberately no launch/resume/process handle,
/// owner PID or executable in this return type. C06 validates its tree separately.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceRestore {
    pub project_id: String,
    pub workspace_id: String,
    pub revision: u64,
    pub schema_version: u32,
    pub layout_json: String,
}

impl WorkspaceRecord {
    pub fn validate(&self) -> Result<()> {
        validate_id(&self.project_id, CanonicalIdKind::Project)?;
        validate_key(&self.workspace_id)?;
        validate_revision(self.revision)?;
        validate_millis(self.updated_at)?;
        validate_layout_json(self.schema_version, &self.layout_json)?;
        if let Some(owner) = &self.owner {
            owner.validate()?;
        }
        Ok(())
    }
    pub fn restore_descriptors(&self) -> Result<WorkspaceRestore> {
        self.validate()?;
        Ok(WorkspaceRestore {
            project_id: self.project_id.clone(),
            workspace_id: self.workspace_id.clone(),
            revision: self.revision,
            schema_version: self.schema_version,
            layout_json: self.layout_json.clone(),
        })
    }
}

pub(crate) fn validate_layout_json(schema_version: u32, layout_json: &str) -> Result<()> {
    if schema_version != 1 { return Err(RecordsError::UnsupportedLayout); }
    if layout_json.len() > MAX_LAYOUT_BYTES { return Err(RecordsError::Invalid("dimensione layout")); }
    let value: serde_json::Value = serde_json::from_str(layout_json).map_err(|_| RecordsError::Invalid("JSON layout"))?;
    if !value.is_object() { return Err(RecordsError::Invalid("documento layout")); }
    reject_runtime_payload(&value, 0, &mut 0)
}

fn reject_runtime_payload(
    value: &serde_json::Value,
    depth: usize,
    nodes: &mut usize,
) -> Result<()> {
    *nodes += 1;
    if depth > 32 || *nodes > 8192 {
        return Err(RecordsError::Invalid("complessità layout"));
    }
    match value {
        serde_json::Value::Object(values) => {
            for (key, child) in values {
                if matches!(
                    key.to_ascii_lowercase().as_str(),
                    "pid"
                        | "owner_pid"
                        | "pty"
                        | "pty_handle"
                        | "handle"
                        | "command"
                        | "argv"
                        | "environment"
                        | "credentials"
                        | "token"
                        | "password"
                        | "dirty_buffer"
                        | "rendered_content"
                ) {
                    return Err(RecordsError::Invalid("contenuto runtime nel layout"));
                }
                reject_runtime_payload(child, depth + 1, nodes)?;
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                reject_runtime_payload(child, depth + 1, nodes)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests;
