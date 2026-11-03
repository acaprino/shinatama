//! C25.1 durable metadata only. No transcript, native-store write, resume or
//! process-liveness claim. Every mutation rechecks schema inside BEGIN IMMEDIATE.
#[cfg(all(test, windows))]
mod tests;
use crate::{
    database::{Database, DatabaseError},
    records::{
        CanonicalIdKind, MAX_SAFE_REVISION, NativeSessionKey, ThreadKind, ThreadRelations,
        validate_id,
    },
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug)]
pub enum ThreadError {
    Invalid,
    Conflict,
    OwnedElsewhere,
    NotFound,
    RootUnavailable,
    Storage,
}
impl fmt::Display for ThreadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invalid => "Thread metadata is invalid.", Self::Conflict => "The thread changed; reload before saving.",
            Self::OwnedElsewhere => "An opening is already recorded for this thread or native session; no takeover was attempted.",
            Self::NotFound => "The thread record is unavailable.", Self::RootUnavailable => "The exact native store binding is unavailable or read-only.",
            Self::Storage => "Thread storage is unavailable; existing records are preserved.",
        })
    }
}
impl std::error::Error for ThreadError {}
impl From<DatabaseError> for ThreadError {
    fn from(_: DatabaseError) -> Self {
        Self::Storage
    }
}
impl From<rusqlite::Error> for ThreadError {
    fn from(_: rusqlite::Error) -> Self {
        Self::Storage
    }
}
pub type Result<T> = std::result::Result<T, ThreadError>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ThreadRecord {
    pub id: String,
    pub project_id: String,
    pub kind: ThreadKind,
    pub parent_thread_id: Option<String>,
    pub companion_worker_thread_id: Option<String>,
    pub title: Option<String>,
    pub archived: bool,
    pub revision: u64,
    pub created_at: i64,
    pub updated_at: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SegmentRecord {
    pub id: String,
    pub thread_id: String,
    pub ordinal: u32,
    pub harness: String,
    pub native: NativeSessionKey,
    pub archived: bool,
    pub revision: u64,
    pub created_at: i64,
}
/// Exact historical identity recorded by the owning runtime. Never a claim that
/// the PID is alive, a native permit or permission to reclaim a crashed writer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ActivationOwner {
    pub instance_id: String,
    pub pid: u32,
    pub created_at_100ns: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ActivationRecord {
    pub owner: ActivationOwner,
    pub thread_id: String,
    pub segment_id: String,
    pub native: NativeSessionKey,
    pub revision: u64,
    pub started_at: i64,
    pub ended_at: Option<i64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadStatus {
    Unbound,
    Inactive,
    RecordedOpen,
    Archived,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ThreadSummary {
    pub id: String,
    pub project_id: String,
    pub kind: ThreadKind,
    pub title: Option<String>,
    pub revision: u64,
    pub updated_at: i64,
    pub harness: Option<String>,
    pub native_available: Option<bool>,
    pub status: ThreadStatus,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ThreadCursor {
    pub updated_at: i64,
    pub thread_id: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadListPage {
    pub rows: Vec<ThreadSummary>,
    pub next: Option<ThreadCursor>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SidebarState {
    pub version: u8,
    pub width: u16,
    pub collapsed: bool,
}
impl Default for SidebarState {
    fn default() -> Self {
        Self {
            version: 1,
            width: 264,
            collapsed: false,
        }
    }
}
impl SidebarState {
    fn validate(&self) -> Result<()> {
        if self.version != 1 || !(180..=480).contains(&self.width) {
            Err(ThreadError::Invalid)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SidebarSnapshot {
    pub revision: u64,
    pub settings: SidebarState,
}
fn sidebar_snapshot(connection: &Connection) -> Result<SidebarSnapshot> {
    let row: Option<Option<String>> = connection.query_row("SELECT CASE WHEN octet_length(value)<=2048 THEN value END FROM setting WHERE key='ui.thread_sidebar.v1'", [], |row| row.get(0)).optional()?;
    match row {
        None => Ok(SidebarSnapshot {
            revision: 0,
            settings: SidebarState::default(),
        }),
        Some(None) => Err(ThreadError::Invalid),
        Some(Some(value)) => {
            let snapshot: SidebarSnapshot =
                serde_json::from_str(&value).map_err(|_| ThreadError::Invalid)?;
            revision(snapshot.revision)?;
            snapshot.settings.validate()?;
            Ok(snapshot)
        }
    }
}

fn id(value: &str, kind: CanonicalIdKind) -> Result<()> {
    validate_id(value, kind).map_err(|_| ThreadError::Invalid)
}
fn revision(value: u64) -> Result<i64> {
    if !(1..=MAX_SAFE_REVISION).contains(&value) {
        return Err(ThreadError::Invalid);
    }
    Ok(value as i64)
}
fn millis(value: i64) -> Result<()> {
    if value < 0 || value as u64 > MAX_SAFE_REVISION {
        Err(ThreadError::Invalid)
    } else {
        Ok(())
    }
}
fn title(value: Option<&str>) -> Result<()> {
    if value.is_some_and(|value| {
        value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
    }) {
        Err(ThreadError::Invalid)
    } else {
        Ok(())
    }
}
fn kind_name(kind: ThreadKind) -> &'static str {
    match kind {
        ThreadKind::Work => "work",
        ThreadKind::Roundtable => "roundtable",
        ThreadKind::Companion => "companion",
    }
}
fn parse_kind(value: String) -> rusqlite::Result<ThreadKind> {
    match value.as_str() {
        "work" => Ok(ThreadKind::Work),
        "roundtable" => Ok(ThreadKind::Roundtable),
        "companion" => Ok(ThreadKind::Companion),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}
fn rev_row(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(column)?;
    if value <= 0 || value as u64 > MAX_SAFE_REVISION {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(value as u64)
}
impl ThreadRecord {
    fn validate(&self) -> Result<()> {
        ThreadRelations {
            id: self.id.clone(),
            kind: self.kind,
            parent_thread_id: self.parent_thread_id.clone(),
            companion_worker_thread_id: self.companion_worker_thread_id.clone(),
        }
        .validate()
        .map_err(|_| ThreadError::Invalid)?;
        id(&self.project_id, CanonicalIdKind::Project)?;
        revision(self.revision)?;
        title(self.title.as_deref())?;
        millis(self.created_at)?;
        millis(self.updated_at)?;
        if self.updated_at < self.created_at {
            return Err(ThreadError::Invalid);
        }
        Ok(())
    }
}
impl ActivationOwner {
    fn validate(&self) -> Result<()> {
        id(&self.instance_id, CanonicalIdKind::Instance)?;
        if self.pid == 0 || self.created_at_100ns == 0 || self.created_at_100ns > i64::MAX as u64 {
            return Err(ThreadError::Invalid);
        }
        Ok(())
    }
}
const THREAD_COLUMNS: &str = "id,project_id,kind,parent_thread_id,companion_worker_thread_id,title,archived,revision,created_at,updated_at";
fn thread_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ThreadRecord> {
    Ok(ThreadRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        kind: parse_kind(row.get(2)?)?,
        parent_thread_id: row.get(3)?,
        companion_worker_thread_id: row.get(4)?,
        title: row.get(5)?,
        archived: row.get(6)?,
        revision: rev_row(row, 7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}
fn load_thread(connection: &Connection, key: &str) -> Result<ThreadRecord> {
    let record = connection
        .query_row(
            &format!("SELECT {THREAD_COLUMNS} FROM thread WHERE id=?1"),
            [key],
            thread_row,
        )
        .optional()?
        .ok_or(ThreadError::NotFound)?;
    record.validate()?;
    Ok(record)
}
fn touch(connection: &Connection, thread: &str, at: i64) -> Result<()> {
    if connection.execute("UPDATE thread SET revision=revision+1,updated_at=max(updated_at,?2) WHERE id=?1 AND revision<9007199254740991",params![thread,at])?!=1 { return Err(ThreadError::Conflict); }
    Ok(())
}
fn segment_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SegmentRecord> {
    Ok(SegmentRecord {
        id: row.get(0)?,
        thread_id: row.get(1)?,
        ordinal: row.get(2)?,
        harness: row.get(3)?,
        native: NativeSessionKey {
            store_root_id: row.get(4)?,
            store_revision: rev_row(row, 5)?,
            native_id: row.get(6)?,
        },
        archived: row.get(7)?,
        revision: rev_row(row, 8)?,
        created_at: row.get(9)?,
    })
}
fn segment(connection: &Connection, key: &str) -> Result<SegmentRecord> {
    connection.query_row("SELECT id,thread_id,ordinal,harness,store_root_id,store_revision,native_id,archived,revision,created_at FROM segment WHERE id=?1",[key],segment_row).optional()?.ok_or(ThreadError::NotFound)
}
fn native_binding(connection: &Connection, record: &SegmentRecord, writable: bool) -> Result<()> {
    let found: Option<(String, u64, String, bool)> = connection
        .query_row(
            "SELECT harness,config_revision,access,available FROM store_root WHERE id=?1",
            [&record.native.store_root_id],
            |row| Ok((row.get(0)?, rev_row(row, 1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    if !found.is_some_and(|(agent, revision, access, available)| {
        agent == record.harness
            && revision == record.native.store_revision
            && available
            && (!writable || access == "read_write")
    }) {
        return Err(ThreadError::RootUnavailable);
    }
    Ok(())
}
fn activation_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ActivationRecord> {
    let created: i64 = row.get(2)?;
    if created <= 0 {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(ActivationRecord {
        owner: ActivationOwner {
            instance_id: row.get(0)?,
            pid: row.get(1)?,
            created_at_100ns: created as u64,
        },
        thread_id: row.get(3)?,
        segment_id: row.get(4)?,
        native: NativeSessionKey {
            store_root_id: row.get(5)?,
            store_revision: rev_row(row, 6)?,
            native_id: row.get(7)?,
        },
        revision: rev_row(row, 8)?,
        started_at: row.get(9)?,
        ended_at: row.get(10)?,
    })
}
fn activation(connection: &Connection, key: &str) -> Result<ActivationRecord> {
    connection.query_row("SELECT instance_id,owner_pid,owner_created_at_100ns,thread_id,segment_id,store_root_id,store_revision,native_id,revision,started_at,ended_at FROM activation WHERE instance_id=?1",[key],activation_row).optional()?.ok_or(ThreadError::NotFound)
}

impl Database {
    pub fn thread_sidebar_snapshot(&self) -> Result<SidebarSnapshot> {
        let connection = self.lock()?;
        sidebar_snapshot(&connection)
    }
    pub fn save_thread_sidebar(
        &self,
        expected_revision: u64,
        settings: SidebarState,
    ) -> Result<SidebarSnapshot> {
        settings.validate()?;
        if expected_revision >= MAX_SAFE_REVISION {
            return Err(ThreadError::Invalid);
        }
        self.require_writable()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&transaction)?;
        if sidebar_snapshot(&transaction)?.revision != expected_revision {
            return Err(ThreadError::Conflict);
        }
        let snapshot = SidebarSnapshot {
            revision: expected_revision + 1,
            settings,
        };
        let json = serde_json::to_string(&snapshot).map_err(|_| ThreadError::Invalid)?;
        transaction.execute("INSERT INTO setting(key,value) VALUES('ui.thread_sidebar.v1',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[json])?;
        transaction.commit()?;
        Ok(snapshot)
    }
    pub fn create_thread(&self, record: &ThreadRecord) -> Result<ThreadRecord> {
        record.validate()?;
        if record.revision != 1 || record.archived {
            return Err(ThreadError::Invalid);
        }
        self.require_writable()?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&tx)?;
        for relation in [&record.parent_thread_id, &record.companion_worker_thread_id]
            .into_iter()
            .flatten()
        {
            if load_thread(&tx, relation)?.project_id != record.project_id {
                return Err(ThreadError::Invalid);
            }
        }
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM thread WHERE id=?1)",
            [&record.id],
            |row| row.get(0),
        )?;
        if exists {
            return Err(ThreadError::Conflict);
        }
        tx.execute("INSERT INTO thread(id,project_id,kind,parent_thread_id,companion_worker_thread_id,title,archived,revision,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,0,1,?7,?8)",params![record.id,record.project_id,kind_name(record.kind),record.parent_thread_id,record.companion_worker_thread_id,record.title,record.created_at,record.updated_at])?;
        tx.commit()?;
        Ok(record.clone())
    }
    pub fn thread_record(&self, key: &str) -> Result<Option<ThreadRecord>> {
        id(key, CanonicalIdKind::Thread)?;
        let connection = self.lock()?;
        match load_thread(&connection, key) {
            Ok(record) => Ok(Some(record)),
            Err(ThreadError::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }
    pub fn update_thread_metadata(
        &self,
        key: &str,
        expected_revision: u64,
        new_title: Option<&str>,
        archived: bool,
        at: i64,
    ) -> Result<ThreadRecord> {
        id(key, CanonicalIdKind::Thread)?;
        revision(expected_revision)?;
        title(new_title)?;
        millis(at)?;
        self.require_writable()?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&tx)?;
        let current = load_thread(&tx, key)?;
        if current.revision != expected_revision {
            return Err(ThreadError::Conflict);
        }
        if archived
            && tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM activation WHERE thread_id=?1 AND ended_at IS NULL)",
                [key],
                |row| row.get::<_, bool>(0),
            )?
        {
            return Err(ThreadError::OwnedElsewhere);
        }
        if tx.execute("UPDATE thread SET title=?2,archived=?3,revision=revision+1,updated_at=max(updated_at,?4) WHERE id=?1 AND revision=?5 AND revision<9007199254740991",params![key,new_title,archived,at,revision(expected_revision)?])?!=1 { return Err(ThreadError::Conflict); }
        let result = load_thread(&tx, key)?;
        tx.commit()?;
        Ok(result)
    }
    /// Attach ONLY a real adapter-provided native ID. No placeholder session is
    /// fabricated to make a launch appear resumable or available.
    pub fn attach_thread_segment(
        &self,
        record: &SegmentRecord,
        expected_thread_revision: u64,
    ) -> Result<SegmentRecord> {
        id(&record.id, CanonicalIdKind::Segment)?;
        id(&record.thread_id, CanonicalIdKind::Thread)?;
        record.native.validate().map_err(|_| ThreadError::Invalid)?;
        millis(record.created_at)?;
        revision(expected_thread_revision)?;
        if record.revision != 1
            || record.archived
            || record.ordinal == 0
            || record.harness.is_empty()
            || record.harness.len() > 64
        {
            return Err(ThreadError::Invalid);
        }
        self.require_writable()?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&tx)?;
        let thread = load_thread(&tx, &record.thread_id)?;
        if thread.archived
            || thread.revision != expected_thread_revision
            || record.created_at < thread.created_at
        {
            return Err(ThreadError::Conflict);
        }
        native_binding(&tx, record, false)?;
        let next: i64 = tx.query_row(
            "SELECT coalesce(max(ordinal),0)+1 FROM segment WHERE thread_id=?1",
            [&record.thread_id],
            |row| row.get(0),
        )?;
        if next != i64::from(record.ordinal) {
            return Err(ThreadError::Conflict);
        }
        let duplicate:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM segment WHERE id=?1 OR (store_root_id=?2 AND store_revision=?3 AND native_id=?4))",params![record.id,record.native.store_root_id,revision(record.native.store_revision)?,record.native.native_id],|row|row.get(0))?;
        if duplicate {
            return Err(ThreadError::Conflict);
        }
        tx.execute("INSERT INTO segment(id,thread_id,ordinal,harness,store_root_id,store_revision,native_id,archived,revision,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,0,1,?8)",params![record.id,record.thread_id,record.ordinal,record.harness,record.native.store_root_id,revision(record.native.store_revision)?,record.native.native_id,record.created_at])?;
        touch(&tx, &record.thread_id, record.created_at)?;
        tx.commit()?;
        Ok(record.clone())
    }
    pub fn segment_record(&self, key: &str) -> Result<Option<SegmentRecord>> {
        id(key, CanonicalIdKind::Segment)?;
        let connection = self.lock()?;
        match segment(&connection, key) {
            Ok(record) => Ok(Some(record)),
            Err(ThreadError::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }
    /// Persisted exclusive opening, not a claim of native liveness. No expiry or
    /// PID-only stealing. A crashed owner needs a separate exact recovery proof.
    pub fn activate_thread_segment(
        &self,
        segment_id: &str,
        expected_revision: u64,
        owner: &ActivationOwner,
        at: i64,
    ) -> Result<ActivationRecord> {
        id(segment_id, CanonicalIdKind::Segment)?;
        revision(expected_revision)?;
        owner.validate()?;
        millis(at)?;
        self.require_writable()?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&tx)?;
        let segment = segment(&tx, segment_id)?;
        if segment.revision != expected_revision
            || segment.archived
            || at < segment.created_at
            || load_thread(&tx, &segment.thread_id)?.archived
        {
            return Err(ThreadError::Conflict);
        }
        native_binding(&tx, &segment, true)?;
        let busy:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM activation WHERE (ended_at IS NULL AND (thread_id=?1 OR segment_id=?2 OR (store_root_id=?3 AND store_revision=?4 AND native_id=?5))) OR instance_id=?6)",params![segment.thread_id,segment.id,segment.native.store_root_id,revision(segment.native.store_revision)?,segment.native.native_id,owner.instance_id],|row|row.get(0))?;
        if busy {
            return Err(ThreadError::OwnedElsewhere);
        }
        tx.execute("INSERT INTO activation(instance_id,owner_pid,owner_created_at_100ns,thread_id,segment_id,store_root_id,store_revision,native_id,revision,started_at,ended_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,1,?9,NULL)",params![owner.instance_id,owner.pid,owner.created_at_100ns as i64,segment.thread_id,segment.id,segment.native.store_root_id,revision(segment.native.store_revision)?,segment.native.native_id,at])?;
        if tx.execute("UPDATE segment SET revision=revision+1 WHERE id=?1 AND revision=?2 AND revision<9007199254740991",params![segment_id,revision(expected_revision)?])?!=1 {return Err(ThreadError::Conflict);}
        touch(&tx, &segment.thread_id, at)?;
        let result = activation(&tx, &owner.instance_id)?;
        tx.commit()?;
        Ok(result)
    }
    /// The trusted runtime calls this only after its own native cleanup. This
    /// metadata operation does not terminate, inspect or release a native job.
    pub fn end_thread_activation(
        &self,
        owner: &ActivationOwner,
        expected_revision: u64,
        at: i64,
    ) -> Result<ActivationRecord> {
        owner.validate()?;
        revision(expected_revision)?;
        millis(at)?;
        self.require_writable()?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&tx)?;
        let current = activation(&tx, &owner.instance_id)?;
        if current.owner != *owner {
            return Err(ThreadError::OwnedElsewhere);
        }
        if current.revision != expected_revision
            || current.ended_at.is_some()
            || at < current.started_at
        {
            return Err(ThreadError::Conflict);
        }
        if tx.execute("UPDATE activation SET ended_at=?2,revision=revision+1 WHERE instance_id=?1 AND revision=?3 AND ended_at IS NULL AND revision<9007199254740991",params![owner.instance_id,at,revision(expected_revision)?])?!=1 {return Err(ThreadError::Conflict);}
        if tx.execute(
            "UPDATE segment SET revision=revision+1 WHERE id=?1 AND revision<9007199254740991",
            [&current.segment_id],
        )? != 1
        {
            return Err(ThreadError::Conflict);
        }
        touch(&tx, &current.thread_id, at)?;
        let result = activation(&tx, &owner.instance_id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn thread_activation(&self, instance_id: &str) -> Result<Option<ActivationRecord>> {
        id(instance_id, CanonicalIdKind::Instance)?;
        let connection = self.lock()?;
        match activation(&connection, instance_id) {
            Ok(record) => Ok(Some(record)),
            Err(ThreadError::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }
    pub fn list_threads(
        &self,
        project_id: &str,
        cursor: Option<&ThreadCursor>,
        limit: usize,
    ) -> Result<ThreadListPage> {
        id(project_id, CanonicalIdKind::Project)?;
        if !(1..=50).contains(&limit) {
            return Err(ThreadError::Invalid);
        }
        if let Some(cursor) = cursor {
            id(&cursor.thread_id, CanonicalIdKind::Thread)?;
            millis(cursor.updated_at)?;
        }
        let connection = self.lock()?;
        if !connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM project WHERE id=?1)",
            [project_id],
            |row| row.get::<_, bool>(0),
        )? {
            return Err(ThreadError::NotFound);
        }
        let mut statement=connection.prepare("SELECT t.id,t.project_id,t.kind,t.title,t.revision,t.updated_at,t.archived,s.harness,r.available,EXISTS(SELECT 1 FROM activation a WHERE a.thread_id=t.id AND a.ended_at IS NULL) FROM thread t LEFT JOIN segment s ON s.id=(SELECT id FROM segment WHERE thread_id=t.id ORDER BY ordinal DESC LIMIT 1) LEFT JOIN store_root r ON r.id=s.store_root_id WHERE t.project_id=?1 AND (?2 IS NULL OR t.updated_at<?2 OR (t.updated_at=?2 AND t.id>?3)) ORDER BY t.updated_at DESC,t.id LIMIT ?4")?;
        let mut rows = statement
            .query_map(
                params![
                    project_id,
                    cursor.map(|cursor| cursor.updated_at),
                    cursor.map(|cursor| cursor.thread_id.as_str()),
                    (limit + 1) as i64
                ],
                |row| {
                    let archived: bool = row.get(6)?;
                    let harness: Option<String> = row.get(7)?;
                    let opened: bool = row.get(9)?;
                    let status = if archived {
                        ThreadStatus::Archived
                    } else if opened {
                        ThreadStatus::RecordedOpen
                    } else if harness.is_some() {
                        ThreadStatus::Inactive
                    } else {
                        ThreadStatus::Unbound
                    };
                    Ok(ThreadSummary {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        kind: parse_kind(row.get(2)?)?,
                        title: row.get(3)?,
                        revision: rev_row(row, 4)?,
                        updated_at: row.get(5)?,
                        harness,
                        native_available: row.get(8)?,
                        status,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let more = rows.len() > limit;
        rows.truncate(limit);
        let next = if more {
            rows.last().map(|row| ThreadCursor {
                updated_at: row.updated_at,
                thread_id: row.id.clone(),
            })
        } else {
            None
        };
        Ok(ThreadListPage { rows, next })
    }
}

pub(crate) mod schema {
    pub const THREAD:&str="CREATE TABLE thread (
 id TEXT PRIMARY KEY NOT NULL CHECK(length(id)=14 AND substr(id,1,2)='t_' AND substr(id,3) NOT GLOB '*[^a-z2-7]*'),
 project_id TEXT NOT NULL REFERENCES project(id),kind TEXT NOT NULL CHECK(kind IN ('work','roundtable','companion')),
 parent_thread_id TEXT REFERENCES thread(id),companion_worker_thread_id TEXT REFERENCES thread(id),
 title TEXT CHECK(title IS NULL OR length(CAST(title AS BLOB)) BETWEEN 1 AND 256),
 archived INTEGER NOT NULL CHECK(archived IN (0,1)),revision INTEGER NOT NULL CHECK(revision BETWEEN 1 AND 9007199254740991),
 created_at INTEGER NOT NULL CHECK(created_at BETWEEN 0 AND 9007199254740991),updated_at INTEGER NOT NULL CHECK(updated_at BETWEEN created_at AND 9007199254740991),
 CHECK(parent_thread_id IS NULL OR parent_thread_id!=id),CHECK(companion_worker_thread_id IS NULL OR companion_worker_thread_id!=id),
 CHECK((kind='companion')=(companion_worker_thread_id IS NOT NULL))
) STRICT";
    pub const THREAD_RECENT: &str =
        "CREATE INDEX thread_project_recent ON thread(project_id,updated_at DESC,id)";
    pub const ROOT_KEY: &str =
        "CREATE UNIQUE INDEX store_root_revision_key ON store_root(id,config_revision)";
    pub const SEGMENT:&str="CREATE TABLE segment (
 id TEXT PRIMARY KEY NOT NULL CHECK(length(id)=14 AND substr(id,1,2)='s_' AND substr(id,3) NOT GLOB '*[^a-z2-7]*'),
 thread_id TEXT NOT NULL REFERENCES thread(id),ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 1 AND 4294967295),
 harness TEXT NOT NULL CHECK(length(harness) BETWEEN 1 AND 64),store_root_id TEXT NOT NULL,store_revision INTEGER NOT NULL CHECK(store_revision BETWEEN 1 AND 9007199254740991),
 native_id TEXT NOT NULL CHECK(length(CAST(native_id AS BLOB)) BETWEEN 1 AND 256),
 archived INTEGER NOT NULL CHECK(archived IN (0,1)),revision INTEGER NOT NULL CHECK(revision BETWEEN 1 AND 9007199254740991),created_at INTEGER NOT NULL CHECK(created_at BETWEEN 0 AND 9007199254740991),
 UNIQUE(thread_id,ordinal),UNIQUE(store_root_id,store_revision,native_id),UNIQUE(id,thread_id,store_root_id,store_revision,native_id),
 FOREIGN KEY(store_root_id,store_revision) REFERENCES store_root(id,config_revision)
) STRICT";
    pub const ACTIVATION:&str="CREATE TABLE activation (
 instance_id TEXT PRIMARY KEY NOT NULL CHECK(length(instance_id)=14 AND substr(instance_id,1,2)='i_' AND substr(instance_id,3) NOT GLOB '*[^a-z2-7]*'),
 owner_pid INTEGER NOT NULL CHECK(owner_pid BETWEEN 1 AND 4294967295),owner_created_at_100ns INTEGER NOT NULL CHECK(owner_created_at_100ns>0),
 thread_id TEXT NOT NULL REFERENCES thread(id),segment_id TEXT NOT NULL,store_root_id TEXT NOT NULL,store_revision INTEGER NOT NULL,native_id TEXT NOT NULL,
 revision INTEGER NOT NULL CHECK(revision BETWEEN 1 AND 9007199254740991),started_at INTEGER NOT NULL CHECK(started_at BETWEEN 0 AND 9007199254740991),
 ended_at INTEGER CHECK(ended_at BETWEEN started_at AND 9007199254740991),
 FOREIGN KEY(segment_id,thread_id,store_root_id,store_revision,native_id) REFERENCES segment(id,thread_id,store_root_id,store_revision,native_id)
) STRICT";
    pub const THREAD_OPEN: &str = "CREATE UNIQUE INDEX activation_thread_open ON activation(thread_id) WHERE ended_at IS NULL";
    pub const SEGMENT_OPEN: &str = "CREATE UNIQUE INDEX activation_segment_open ON activation(segment_id) WHERE ended_at IS NULL";
    pub const NATIVE_OPEN: &str = "CREATE UNIQUE INDEX activation_native_open ON activation(store_root_id,store_revision,native_id) WHERE ended_at IS NULL";
    pub const OBJECTS: &[(&str, &str)] = &[
        ("thread", THREAD),
        ("thread_project_recent", THREAD_RECENT),
        ("store_root_revision_key", ROOT_KEY),
        ("segment", SEGMENT),
        ("activation", ACTIVATION),
        ("activation_thread_open", THREAD_OPEN),
        ("activation_segment_open", SEGMENT_OPEN),
        ("activation_native_open", NATIVE_OPEN),
    ];
    pub const MIGRATION_V4: &[&str] = &[
        THREAD,
        THREAD_RECENT,
        ROOT_KEY,
        SEGMENT,
        ACTIVATION,
        THREAD_OPEN,
        SEGMENT_OPEN,
        NATIVE_OPEN,
    ];
}
