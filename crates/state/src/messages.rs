//! C31.1: instance-to-instance messages (FRD-0007 R01-R02, R11-R13).
//!
//! Operational, short-lived records in their own file (`messages.sqlite`) beside the
//! authoritative database: every message expires within one hour and is never delivered
//! afterwards. Rows are written only by the messaging engine, which validates scope, redacts
//! bodies and enforces limits before calling this store.
use crate::location::StorageLocation;
use rusqlite::{Connection, ErrorCode, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

pub const FILE_NAME: &str = "messages.sqlite";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageState {
    Queued,
    Delivering,
    Delivered,
    Acked,
    Failed,
    Expired,
}
impl MessageState {
    pub fn as_str(self) -> &'static str {
        match self {
            MessageState::Queued => "queued",
            MessageState::Delivering => "delivering",
            MessageState::Delivered => "delivered",
            MessageState::Acked => "acked",
            MessageState::Failed => "failed",
            MessageState::Expired => "expired",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        [Self::Queued, Self::Delivering, Self::Delivered, Self::Acked, Self::Failed, Self::Expired].into_iter().find(|s| s.as_str() == value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: String,
    pub kind: String,
    pub sender_instance: String,
    pub sender_agent: String,
    pub sender_project: String,
    pub sender_thread: Option<String>,
    pub recipient_instance: String,
    pub recipient_project: String,
    pub group_id: Option<String>,
    pub body: String,
    pub depth: u32,
    pub in_reply_to: Option<String>,
    pub dedupe_key: String,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
    pub state: MessageState,
    pub attempts: u32,
    pub next_attempt_at_ms: i64,
    pub channel: Option<String>,
    pub failure: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alert {
    pub id: i64,
    pub kind: String,
    pub detail: String,
    pub at_ms: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StoreError {
    Duplicate { existing_id: String },
    NotFound,
    Busy,
    Full,
    Storage(String),
}
impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Duplicate { .. } => write!(f, "This message was already sent."),
            StoreError::NotFound => write!(f, "The message does not exist."),
            StoreError::Busy => write!(f, "The message store is busy; retry."),
            StoreError::Full => write!(f, "The disk is full; the message was not stored."),
            StoreError::Storage(message) => write!(f, "Message store: {message}"),
        }
    }
}
pub type Result<T> = std::result::Result<T, StoreError>;

fn storage(error: rusqlite::Error) -> StoreError {
    match error.sqlite_error_code() {
        Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => StoreError::Busy,
        Some(ErrorCode::DiskFull) => StoreError::Full,
        _ => StoreError::Storage(error.to_string()),
    }
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS message(
  id TEXT PRIMARY KEY NOT NULL CHECK(length(id)=32),
  kind TEXT NOT NULL CHECK(kind IN ('note','ask','reply','catch_up','roundtable_prompt','roundtable_answer','alert')),
  sender_instance TEXT NOT NULL, sender_agent TEXT NOT NULL, sender_project TEXT NOT NULL, sender_thread TEXT,
  recipient_instance TEXT NOT NULL, recipient_project TEXT NOT NULL, group_id TEXT,
  body TEXT NOT NULL CHECK(length(CAST(body AS BLOB)) <= 16384),
  depth INTEGER NOT NULL CHECK(depth BETWEEN 0 AND 64),
  in_reply_to TEXT,
  dedupe_key TEXT NOT NULL UNIQUE,
  created_at_ms INTEGER NOT NULL,
  expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms > created_at_ms AND expires_at_ms <= created_at_ms + 3600000),
  state TEXT NOT NULL CHECK(state IN ('queued','delivering','delivered','acked','failed','expired')),
  attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts BETWEEN 0 AND 6),
  next_attempt_at_ms INTEGER NOT NULL,
  channel TEXT, failure TEXT
) STRICT;
CREATE INDEX IF NOT EXISTS message_due ON message(state, next_attempt_at_ms);
CREATE INDEX IF NOT EXISTS message_recipient ON message(recipient_instance, state);
CREATE INDEX IF NOT EXISTS message_pair ON message(sender_instance, recipient_instance, created_at_ms);
CREATE TABLE IF NOT EXISTS alert(
  id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, detail TEXT NOT NULL, at_ms INTEGER NOT NULL
) STRICT;
";

pub struct MessageStore {
    connection: Mutex<Connection>,
}

const COLUMNS: &str = "id,kind,sender_instance,sender_agent,sender_project,sender_thread,recipient_instance,recipient_project,group_id,body,depth,in_reply_to,dedupe_key,created_at_ms,expires_at_ms,state,attempts,next_attempt_at_ms,channel,failure";

fn row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredMessage> {
    Ok(StoredMessage {
        id: row.get(0)?,
        kind: row.get(1)?,
        sender_instance: row.get(2)?,
        sender_agent: row.get(3)?,
        sender_project: row.get(4)?,
        sender_thread: row.get(5)?,
        recipient_instance: row.get(6)?,
        recipient_project: row.get(7)?,
        group_id: row.get(8)?,
        body: row.get(9)?,
        depth: row.get::<_, i64>(10)? as u32,
        in_reply_to: row.get(11)?,
        dedupe_key: row.get(12)?,
        created_at_ms: row.get(13)?,
        expires_at_ms: row.get(14)?,
        state: MessageState::parse(&row.get::<_, String>(15)?).unwrap_or(MessageState::Failed),
        attempts: row.get::<_, i64>(16)? as u32,
        next_attempt_at_ms: row.get(17)?,
        channel: row.get(18)?,
        failure: row.get(19)?,
    })
}

impl MessageStore {
    pub fn open(location: &StorageLocation) -> Result<Self> {
        let path = location.directory().join(FILE_NAME);
        let store = Self::open_path(&path)?;
        location.secure_private_file(&path).map_err(|e| StoreError::Storage(e.to_string()))?;
        Ok(store)
    }
    pub fn open_path(path: &Path) -> Result<Self> {
        Self::prepare(Connection::open(path).map_err(storage)?)
    }
    pub fn open_in_memory() -> Result<Self> {
        Self::prepare(Connection::open_in_memory().map_err(storage)?)
    }
    fn prepare(connection: Connection) -> Result<Self> {
        connection.busy_timeout(Duration::from_millis(2_000)).map_err(storage)?;
        let _ = connection.pragma_update(None, "journal_mode", "WAL");
        let version: u32 = connection.pragma_query_value(None, "user_version", |r| r.get(0)).map_err(storage)?;
        if version > SCHEMA_VERSION {
            return Err(StoreError::Storage("written by a newer Shinatama".into()));
        }
        if version < SCHEMA_VERSION {
            connection.execute_batch(&format!("BEGIN IMMEDIATE;{SCHEMA}PRAGMA user_version={SCHEMA_VERSION};COMMIT;")).map_err(storage)?;
        }
        Ok(Self { connection: Mutex::new(connection) })
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.connection.lock().map_err(|_| StoreError::Storage("lock poisoned".into()))
    }

    /// Inserts a validated message. A repeated dedupe key returns the existing id (R12).
    pub fn insert(&self, message: &StoredMessage) -> Result<()> {
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(storage)?;
        if let Some(existing_id) = tx.query_row("SELECT id FROM message WHERE dedupe_key=?1", [&message.dedupe_key], |r| r.get::<_, String>(0)).optional().map_err(storage)? {
            return Err(StoreError::Duplicate { existing_id });
        }
        tx.execute(
            &format!("INSERT INTO message({COLUMNS}) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)"),
            params![
                message.id, message.kind, message.sender_instance, message.sender_agent, message.sender_project, message.sender_thread,
                message.recipient_instance, message.recipient_project, message.group_id, message.body, message.depth, message.in_reply_to,
                message.dedupe_key, message.created_at_ms, message.expires_at_ms, message.state.as_str(), message.attempts,
                message.next_attempt_at_ms, message.channel, message.failure
            ],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)
    }

    pub fn get(&self, id: &str) -> Result<Option<StoredMessage>> {
        self.lock()?.query_row(&format!("SELECT {COLUMNS} FROM message WHERE id=?1"), [id], row).optional().map_err(storage)
    }

    fn select(&self, sql: &str, args: &[&dyn rusqlite::ToSql]) -> Result<Vec<StoredMessage>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(&format!("SELECT {COLUMNS} FROM message {sql}")).map_err(storage)?;
        let rows = statement.query_map(args, row).map_err(storage)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(storage)
    }

    /// Queued messages whose next attempt is due and that have not expired.
    pub fn due(&self, now_ms: i64, limit: usize) -> Result<Vec<StoredMessage>> {
        self.select("WHERE state='queued' AND next_attempt_at_ms<=?1 AND expires_at_ms>?1 ORDER BY created_at_ms LIMIT ?2", &[&now_ms, &(limit as i64)])
    }

    /// Delivered or queued messages for one recipient (MCP pull), oldest first.
    pub fn for_recipient(&self, instance: &str, now_ms: i64, limit: usize) -> Result<Vec<StoredMessage>> {
        self.select("WHERE recipient_instance=?1 AND state IN ('queued','delivered') AND expires_at_ms>?2 ORDER BY created_at_ms LIMIT ?3", &[&instance, &now_ms, &(limit as i64)])
    }

    /// Messages between a pair since a time, for rate and depth limits.
    pub fn between(&self, from: &str, to: &str, since_ms: i64) -> Result<Vec<StoredMessage>> {
        self.select("WHERE sender_instance=?1 AND recipient_instance=?2 AND created_at_ms>=?3 ORDER BY created_at_ms", &[&from, &to, &since_ms])
    }

    pub fn count_thread_since(&self, thread: &str, since_ms: i64) -> Result<u64> {
        self.lock()?
            .query_row("SELECT count(*) FROM message WHERE sender_thread=?1 AND created_at_ms>=?2", params![thread, since_ms], |r| r.get::<_, i64>(0))
            .map(|n| n as u64)
            .map_err(storage)
    }

    /// Compare-and-set on the state; returns whether this caller won.
    #[allow(clippy::too_many_arguments)]
    pub fn update_state(&self, id: &str, from: MessageState, to: MessageState, attempts: u32, next_attempt_at_ms: i64, channel: Option<&str>, failure: Option<&str>) -> Result<bool> {
        let changed = self
            .lock()?
            .execute(
                "UPDATE message SET state=?3,attempts=?4,next_attempt_at_ms=?5,channel=coalesce(?6,channel),failure=?7 WHERE id=?1 AND state=?2",
                params![id, from.as_str(), to.as_str(), attempts, next_attempt_at_ms, channel, failure],
            )
            .map_err(storage)?;
        Ok(changed == 1)
    }

    /// Marks every unfinished message past its expiry as expired; returns how many.
    pub fn expire(&self, now_ms: i64) -> Result<u64> {
        self.lock()?
            .execute("UPDATE message SET state='expired' WHERE state IN ('queued','delivering') AND expires_at_ms<=?1", [now_ms])
            .map(|n| n as u64)
            .map_err(storage)
    }

    /// Removes finished rows an hour after their expiry; no mailbox outlives that (R02).
    pub fn purge(&self, now_ms: i64) -> Result<u64> {
        self.lock()?.execute("DELETE FROM message WHERE expires_at_ms<=?1", [now_ms - 3_600_000]).map(|n| n as u64).map_err(storage)
    }

    pub fn raise_alert(&self, kind: &str, detail: &str, now_ms: i64) -> Result<()> {
        self.lock()?.execute("INSERT INTO alert(kind,detail,at_ms) VALUES(?1,?2,?3)", params![kind, detail, now_ms]).map(|_| ()).map_err(storage)
    }

    pub fn alerts_since(&self, since_ms: i64) -> Result<Vec<Alert>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare("SELECT id,kind,detail,at_ms FROM alert WHERE at_ms>=?1 ORDER BY id").map_err(storage)?;
        let rows = statement.query_map([since_ms], |r| Ok(Alert { id: r.get(0)?, kind: r.get(1)?, detail: r.get(2)?, at_ms: r.get(3)? })).map_err(storage)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(storage)
    }
}
