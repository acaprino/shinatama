//! C28.1: the hop journal (FRD-0005 R02, R35, R42-R43; ADR-0018).
//!
//! Every hop is a row in `hop` with its state; every arrow of the state machine is ONE SQLite
//! transaction that updates `hop.state` and inserts a `hop_event`. External effects (a spawn, a
//! seed delivery, closing the source) happen between transactions and are journaled as events
//! under the hop id first. A partial unique index keeps at most one unfinished hop per thread
//! (invariant I1): when two processes plan a hop of the same thread exactly one wins.
//!
//! The journal has its own SQLite file next to the authoritative database (`hops.sqlite`), so
//! a hop never needs an authoritative schema migration; thread ids are the database's own.
use crate::location::StorageLocation;
use rusqlite::{Connection, ErrorCode, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

pub const FILE_NAME: &str = "hops.sqlite";
pub const SCHEMA_VERSION: u32 = 1;
/// R38/R42: a destination that gave no evidence within this time fails.
pub const READY_TIMEOUT_MS: i64 = 90_000;
const MAX_DETAIL_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HopState {
    Planned,
    Previewed,
    Confirmed,
    Preparing,
    Ready,
    Switching,
    Committed,
    Aborted,
    Failed,
    RolledBack,
}
impl HopState {
    pub const ALL: [HopState; 10] = [
        HopState::Planned,
        HopState::Previewed,
        HopState::Confirmed,
        HopState::Preparing,
        HopState::Ready,
        HopState::Switching,
        HopState::Committed,
        HopState::Aborted,
        HopState::Failed,
        HopState::RolledBack,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            HopState::Planned => "planned",
            HopState::Previewed => "previewed",
            HopState::Confirmed => "confirmed",
            HopState::Preparing => "preparing",
            HopState::Ready => "ready",
            HopState::Switching => "switching",
            HopState::Committed => "committed",
            HopState::Aborted => "aborted",
            HopState::Failed => "failed",
            HopState::RolledBack => "rolled_back",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.as_str() == value)
    }
    pub fn terminal(self) -> bool {
        matches!(self, HopState::Committed | HopState::Aborted | HopState::Failed | HopState::RolledBack)
    }
    /// The arrows of R35. Abort is allowed from every pre-commit state; FAILED from the states
    /// where an external effect may have happened; UNDO rolls a committed close hop back.
    pub fn can_move_to(self, next: HopState) -> bool {
        use HopState::*;
        matches!(
            (self, next),
            (Planned, Previewed)
                | (Previewed, Previewed)
                | (Previewed, Confirmed)
                | (Confirmed, Preparing)
                | (Preparing, Ready)
                | (Ready, Switching)
                | (Switching, Committed)
                | (Planned | Previewed | Confirmed | Preparing | Ready | Switching, Aborted)
                | (Preparing | Ready | Switching, Failed)
                | (Committed, RolledBack)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HopMode {
    Close,
    Fork,
}
impl HopMode {
    pub fn as_str(self) -> &'static str {
        match self {
            HopMode::Close => "close",
            HopMode::Fork => "fork",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HopStrategy {
    NativeResume,
    SeedPrompt,
}
impl HopStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            HopStrategy::NativeResume => "native_resume",
            HopStrategy::SeedPrompt => "seed_prompt",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HopPlan {
    /// 32 lowercase hex characters.
    pub id: String,
    pub thread_id: String,
    /// The thread a fork creates (may be recorded later); always `None` for close.
    pub destination_thread_id: Option<String>,
    pub mode: HopMode,
    pub strategy: HopStrategy,
    pub source_agent: String,
    pub source_native_id: String,
    /// Live source instance; `None` for a hop from a historical session (C28.4).
    pub source_instance: Option<String>,
    pub destination_agent: String,
    pub nonce: String,
    pub snapshot_sha256: String,
    pub bootstrap_sha256: String,
    pub seed_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HopRecord {
    pub plan: HopPlan,
    pub state: HopState,
    pub revision: u64,
    pub destination_instance: Option<String>,
    pub destination_native_id: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HopEvent {
    pub sequence: u64,
    pub hop_id: String,
    pub kind: String,
    pub from_state: Option<HopState>,
    pub to_state: Option<HopState>,
    pub detail: Option<String>,
    pub at_ms: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum HopError {
    /// Another unfinished hop already holds this thread (I1).
    ThreadBusy { hop_id: String },
    /// The hop moved since the caller read it; nothing changed.
    Stale { current: HopState, revision: u64 },
    InvalidTransition { from: HopState, to: HopState },
    NotFound,
    Invalid(&'static str),
    /// SQLITE_BUSY / SQLITE_LOCKED after retries: nothing was journaled, so nothing may run.
    Busy,
    /// SQLITE_FULL: nothing was journaled, so nothing may run.
    Full,
    Storage(String),
}
impl std::fmt::Display for HopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HopError::ThreadBusy { .. } => write!(f, "Another hop of this thread is already running; it must finish first."),
            HopError::Stale { current, .. } => write!(f, "The hop changed meanwhile (now {}); nothing was done.", current.as_str()),
            HopError::InvalidTransition { from, to } => write!(f, "A hop cannot go from {} to {}.", from.as_str(), to.as_str()),
            HopError::NotFound => write!(f, "The hop is not in the journal."),
            HopError::Invalid(what) => write!(f, "Invalid hop field: {what}."),
            HopError::Busy => write!(f, "The hop journal is busy; nothing was started. Retry in a moment."),
            HopError::Full => write!(f, "The disk is full; the hop journal cannot record the hop, so nothing was started."),
            HopError::Storage(message) => write!(f, "Hop journal: {message}"),
        }
    }
}
impl std::error::Error for HopError {}
pub type Result<T> = std::result::Result<T, HopError>;

fn storage(error: rusqlite::Error) -> HopError {
    match error.sqlite_error_code() {
        Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => HopError::Busy,
        Some(ErrorCode::DiskFull) => HopError::Full,
        _ => HopError::Storage(error.to_string()),
    }
}

fn hex32(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}
fn sha(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}
fn short(value: &str, cap: usize) -> bool {
    !value.is_empty() && value.len() <= cap && !value.chars().any(char::is_control)
}

impl HopPlan {
    fn validate(&self) -> Result<()> {
        if !hex32(&self.id) {
            return Err(HopError::Invalid("id"));
        }
        if !short(&self.thread_id, 64) || self.destination_thread_id.as_deref().is_some_and(|t| !short(t, 64) || t == self.thread_id) {
            return Err(HopError::Invalid("thread"));
        }
        // Close continues the same thread; a fork may learn its new thread only after launch.
        if self.mode == HopMode::Close && self.destination_thread_id.is_some() {
            return Err(HopError::Invalid("mode"));
        }
        if !short(&self.source_agent, 64) || !short(&self.destination_agent, 64) || !short(&self.source_native_id, 256) {
            return Err(HopError::Invalid("agent"));
        }
        if self.source_instance.as_deref().is_some_and(|i| !short(i, 64)) {
            return Err(HopError::Invalid("instance"));
        }
        if !short(&self.nonce, 64) || !sha(&self.snapshot_sha256) || !sha(&self.bootstrap_sha256) || !short(&self.seed_path, 4096) {
            return Err(HopError::Invalid("snapshot"));
        }
        Ok(())
    }
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS hop(
  id TEXT PRIMARY KEY NOT NULL CHECK(length(id)=32),
  thread_id TEXT NOT NULL,
  destination_thread_id TEXT,
  mode TEXT NOT NULL CHECK(mode IN ('close','fork')),
  strategy TEXT NOT NULL CHECK(strategy IN ('native_resume','seed_prompt')),
  source_agent TEXT NOT NULL,
  source_native_id TEXT NOT NULL,
  source_instance TEXT,
  destination_agent TEXT NOT NULL,
  nonce TEXT NOT NULL,
  snapshot_sha256 TEXT NOT NULL CHECK(length(snapshot_sha256)=64),
  bootstrap_sha256 TEXT NOT NULL CHECK(length(bootstrap_sha256)=64),
  seed_path TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('planned','previewed','confirmed','preparing','ready','switching','committed','aborted','failed','rolled_back')),
  finished INTEGER NOT NULL CHECK(finished IN (0,1)),
  revision INTEGER NOT NULL CHECK(revision >= 1),
  destination_instance TEXT,
  destination_native_id TEXT,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  CHECK(mode='fork' OR destination_thread_id IS NULL),
  CHECK(finished=(state IN ('committed','aborted','failed','rolled_back')))
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS hop_one_unfinished_per_thread ON hop(thread_id) WHERE finished=0;
CREATE INDEX IF NOT EXISTS hop_by_state ON hop(finished,state);
CREATE TABLE IF NOT EXISTS hop_event(
  sequence INTEGER PRIMARY KEY AUTOINCREMENT,
  hop_id TEXT NOT NULL REFERENCES hop(id),
  kind TEXT NOT NULL CHECK(length(kind) BETWEEN 1 AND 64),
  from_state TEXT,
  to_state TEXT,
  detail TEXT CHECK(detail IS NULL OR length(CAST(detail AS BLOB)) <= 16384),
  at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS hop_event_by_hop ON hop_event(hop_id,sequence);
";

pub struct HopJournal {
    connection: Mutex<Connection>,
}

/// What recovery did to one hop (R42).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recovered {
    pub hop_id: String,
    pub from: HopState,
    pub to: Option<HopState>,
    pub reason: &'static str,
}

/// Recovery questions only the runtime can answer.
pub trait RecoveryProbe {
    /// The destination instance this hop started is alive.
    fn destination_alive(&mut self, hop: &HopRecord) -> bool;
}

impl HopJournal {
    pub fn open(location: &StorageLocation) -> Result<Self> {
        let path = location.directory().join(FILE_NAME);
        let journal = Self::open_path(&path)?;
        location.secure_private_file(&path).map_err(|e| HopError::Storage(e.to_string()))?;
        Ok(journal)
    }

    /// For tests and for a second process on the same file.
    pub fn open_path(path: &Path) -> Result<Self> {
        let connection = Connection::open(path).map_err(storage)?;
        Self::prepare(connection)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::prepare(Connection::open_in_memory().map_err(storage)?)
    }

    fn prepare(connection: Connection) -> Result<Self> {
        connection.busy_timeout(Duration::from_millis(2_000)).map_err(storage)?;
        connection.pragma_update(None, "foreign_keys", true).map_err(storage)?;
        let _ = connection.pragma_update(None, "journal_mode", "WAL");
        connection.pragma_update(None, "synchronous", "FULL").map_err(storage)?;
        let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0)).map_err(storage)?;
        if version > SCHEMA_VERSION {
            return Err(HopError::Storage("the hop journal was written by a newer Shinatama".into()));
        }
        if version < SCHEMA_VERSION {
            connection
                .execute_batch(&format!("BEGIN IMMEDIATE;{SCHEMA}PRAGMA user_version={SCHEMA_VERSION};COMMIT;"))
                .map_err(storage)?;
        }
        Ok(Self { connection: Mutex::new(connection) })
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.connection.lock().map_err(|_| HopError::Storage("journal lock poisoned".into()))
    }

    /// R43: retry busy/locked with backoff; never proceed without the journal.
    fn with_retry<T>(&self, mut work: impl FnMut(&mut Connection) -> Result<T>) -> Result<T> {
        let mut connection = self.lock()?;
        let mut wait = Duration::from_millis(25);
        for attempt in 0..6 {
            match work(&mut connection) {
                Err(HopError::Busy) if attempt < 5 => {
                    std::thread::sleep(wait);
                    wait = (wait * 2).min(Duration::from_millis(800));
                }
                other => return other,
            }
        }
        Err(HopError::Busy)
    }

    /// PLANNED: takes the thread's uniqueness (I1) and records the frozen snapshot hashes.
    pub fn plan(&self, plan: &HopPlan, now_ms: i64) -> Result<HopRecord> {
        plan.validate()?;
        self.with_retry(|connection| {
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(storage)?;
            if let Some(holder) = tx
                .query_row("SELECT id FROM hop WHERE thread_id=?1 AND finished=0", [&plan.thread_id], |row| row.get::<_, String>(0))
                .optional()
                .map_err(storage)?
            {
                return Err(HopError::ThreadBusy { hop_id: holder });
            }
            let inserted = tx.execute(
                "INSERT INTO hop(id,thread_id,destination_thread_id,mode,strategy,source_agent,source_native_id,source_instance,
                   destination_agent,nonce,snapshot_sha256,bootstrap_sha256,seed_path,state,finished,revision,created_at_ms,updated_at_ms)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,'planned',0,1,?14,?14)",
                params![
                    plan.id, plan.thread_id, plan.destination_thread_id, plan.mode.as_str(), plan.strategy.as_str(),
                    plan.source_agent, plan.source_native_id, plan.source_instance, plan.destination_agent, plan.nonce,
                    plan.snapshot_sha256, plan.bootstrap_sha256, plan.seed_path, now_ms
                ],
            );
            match inserted {
                Ok(_) => {}
                Err(error) if error.sqlite_error_code() == Some(ErrorCode::ConstraintViolation) => {
                    let holder = tx
                        .query_row("SELECT id FROM hop WHERE thread_id=?1 AND finished=0", [&plan.thread_id], |row| row.get::<_, String>(0))
                        .optional()
                        .map_err(storage)?;
                    return Err(match holder {
                        Some(hop_id) => HopError::ThreadBusy { hop_id },
                        None => HopError::Invalid("duplicate hop id"),
                    });
                }
                Err(error) => return Err(storage(error)),
            }
            event(&tx, &plan.id, "planned", None, Some(HopState::Planned), None, now_ms)?;
            tx.commit().map_err(storage)?;
            Ok(())
        })?;
        self.hop(&plan.id)?.ok_or(HopError::NotFound)
    }

    /// One arrow of the state machine: compare-and-set on (state, revision), plus its event.
    pub fn transition(&self, hop_id: &str, expected: HopState, expected_revision: u64, next: HopState, detail: Option<&str>, now_ms: i64) -> Result<HopRecord> {
        if !expected.can_move_to(next) {
            return Err(HopError::InvalidTransition { from: expected, to: next });
        }
        if detail.is_some_and(|d| d.len() > MAX_DETAIL_BYTES) {
            return Err(HopError::Invalid("detail"));
        }
        self.with_retry(|connection| {
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(storage)?;
            let current = current(&tx, hop_id)?;
            if current.0 != expected || current.1 != expected_revision {
                return Err(HopError::Stale { current: current.0, revision: current.1 });
            }
            tx.execute(
                "UPDATE hop SET state=?2,finished=?3,revision=revision+1,updated_at_ms=?4 WHERE id=?1",
                params![hop_id, next.as_str(), next.terminal(), now_ms],
            )
            .map_err(storage)?;
            event(&tx, hop_id, next.as_str(), Some(expected), Some(next), detail, now_ms)?;
            tx.commit().map_err(storage)?;
            Ok(())
        })?;
        self.hop(hop_id)?.ok_or(HopError::NotFound)
    }

    /// Journals an external effect under the hop BEFORE (or as evidence after) it happens,
    /// without changing the state. Refused once the hop is finished.
    pub fn record_effect(&self, hop_id: &str, kind: &str, detail: Option<&str>, now_ms: i64) -> Result<()> {
        if !short(kind, 64) || detail.is_some_and(|d| d.len() > MAX_DETAIL_BYTES) {
            return Err(HopError::Invalid("effect"));
        }
        self.with_retry(|connection| {
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(storage)?;
            let (state, _) = current(&tx, hop_id)?;
            if state.terminal() {
                return Err(HopError::InvalidTransition { from: state, to: state });
            }
            event(&tx, hop_id, kind, None, None, detail, now_ms)?;
            tx.commit().map_err(storage)?;
            Ok(())
        })
    }

    /// Records which destination instance and native session the hop produced.
    pub fn set_destination(&self, hop_id: &str, instance: Option<&str>, native_id: Option<&str>, now_ms: i64) -> Result<()> {
        self.set_destination_with_thread(hop_id, instance, native_id, None, now_ms)
    }

    /// As [`Self::set_destination`], also recording a fork's new thread.
    pub fn set_destination_with_thread(&self, hop_id: &str, instance: Option<&str>, native_id: Option<&str>, thread: Option<&str>, now_ms: i64) -> Result<()> {
        if instance.is_some_and(|i| !short(i, 64)) || native_id.is_some_and(|n| !short(n, 256)) || thread.is_some_and(|t| !short(t, 64)) {
            return Err(HopError::Invalid("destination"));
        }
        self.with_retry(|connection| {
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(storage)?;
            let (state, _) = current(&tx, hop_id)?;
            if state.terminal() {
                return Err(HopError::InvalidTransition { from: state, to: state });
            }
            if thread.is_some() && tx.query_row("SELECT mode FROM hop WHERE id=?1", [hop_id], |row| row.get::<_, String>(0)).map_err(storage)? != "fork" {
                return Err(HopError::Invalid("only a fork has a new thread"));
            }
            tx.execute(
                "UPDATE hop SET destination_instance=coalesce(?2,destination_instance),destination_native_id=coalesce(?3,destination_native_id),
                   destination_thread_id=coalesce(?5,destination_thread_id),updated_at_ms=?4 WHERE id=?1",
                params![hop_id, instance, native_id, now_ms, thread],
            )
            .map_err(storage)?;
            event(&tx, hop_id, "destination_recorded", None, None, native_id.or(instance), now_ms)?;
            tx.commit().map_err(storage)?;
            Ok(())
        })
    }

    /// `PRAGMA integrity_check` on the journal file.
    pub fn integrity_check(&self) -> Result<bool> {
        let connection = self.lock()?;
        let result: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0)).map_err(storage)?;
        Ok(result == "ok")
    }

    pub fn hop(&self, hop_id: &str) -> Result<Option<HopRecord>> {
        let connection = self.lock()?;
        read_hop(&connection, hop_id)
    }

    pub fn active_for_thread(&self, thread_id: &str) -> Result<Option<HopRecord>> {
        let connection = self.lock()?;
        let id = connection
            .query_row("SELECT id FROM hop WHERE thread_id=?1 AND finished=0", [thread_id], |row| row.get::<_, String>(0))
            .optional()
            .map_err(storage)?;
        match id {
            Some(id) => read_hop(&connection, &id),
            None => Ok(None),
        }
    }

    pub fn events(&self, hop_id: &str) -> Result<Vec<HopEvent>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT sequence,hop_id,kind,from_state,to_state,detail,at_ms FROM hop_event WHERE hop_id=?1 ORDER BY sequence LIMIT 1000")
            .map_err(storage)?;
        let rows = statement
            .query_map([hop_id], |row| {
                Ok(HopEvent {
                    sequence: row.get::<_, i64>(0)? as u64,
                    hop_id: row.get(1)?,
                    kind: row.get(2)?,
                    from_state: row.get::<_, Option<String>>(3)?.and_then(|s| HopState::parse(&s)),
                    to_state: row.get::<_, Option<String>>(4)?.and_then(|s| HopState::parse(&s)),
                    detail: row.get(5)?,
                    at_ms: row.get(6)?,
                })
            })
            .map_err(storage)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(storage)
    }

    /// The newest hops, finished or not, for the overview timeline.
    pub fn recent(&self, limit: usize) -> Result<Vec<HopRecord>> {
        let ids: Vec<String> = {
            let connection = self.lock()?;
            let mut statement = connection.prepare("SELECT id FROM hop ORDER BY updated_at_ms DESC LIMIT ?1").map_err(storage)?;
            let rows = statement.query_map([limit.min(100) as i64], |row| row.get::<_, String>(0)).map_err(storage)?;
            rows.collect::<std::result::Result<Vec<_>, _>>().map_err(storage)?
        };
        ids.iter().filter_map(|id| self.hop(id).transpose()).collect()
    }

    pub fn unfinished(&self) -> Result<Vec<HopRecord>> {
        let ids: Vec<String> = {
            let connection = self.lock()?;
            let mut statement = connection.prepare("SELECT id FROM hop WHERE finished=0 ORDER BY created_at_ms LIMIT 256").map_err(storage)?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0)).map_err(storage)?;
            rows.collect::<std::result::Result<Vec<_>, _>>().map_err(storage)?
        };
        ids.iter().filter_map(|id| self.hop(id).transpose()).collect()
    }

    /// Startup recovery (R42), safe to run from any process; a concurrent recovery loses the
    /// compare-and-set and skips the hop. Nothing an agent owns is deleted here.
    pub fn recover(&self, probe: &mut dyn RecoveryProbe, now_ms: i64) -> Result<Vec<Recovered>> {
        let mut done = Vec::new();
        for hop in self.unfinished()? {
            let events = self.events(&hop.plan.id)?;
            let attempted = events.iter().any(|e| e.kind == "spawn_attempted");
            let observed = events.iter().any(|e| e.kind == "spawn_observed");
            let (to, reason) = match hop.state {
                HopState::Planned | HopState::Previewed | HopState::Confirmed => (Some(HopState::Aborted), "nothing ran in the destination"),
                HopState::Ready => (Some(HopState::Aborted), "the switch never started"),
                HopState::Preparing if attempted && !observed && now_ms - hop.updated_at_ms < READY_TIMEOUT_MS => (None, "uncertain spawn: observe only, never relaunch"),
                HopState::Preparing if attempted && !observed => (Some(HopState::Failed), "uncertain spawn timed out; retry manually"),
                HopState::Preparing if !attempted => (Some(HopState::Aborted), "no spawn was attempted"),
                HopState::Preparing => (Some(HopState::Failed), "the destination never became ready"),
                HopState::Switching if probe.destination_alive(&hop) => (Some(HopState::Committed), "the destination is alive"),
                HopState::Switching => (Some(HopState::Aborted), "the destination is gone; reopen the source"),
                _ => (None, "finished"),
            };
            if let Some(next) = to {
                match self.transition(&hop.plan.id, hop.state, hop.revision, next, Some(&format!("recovery: {reason}")), now_ms) {
                    Ok(_) => {}
                    Err(HopError::Stale { .. }) => continue,
                    Err(error) => return Err(error),
                }
            }
            done.push(Recovered { hop_id: hop.plan.id.clone(), from: hop.state, to, reason });
        }
        Ok(done)
    }
}

fn event(tx: &rusqlite::Transaction<'_>, hop_id: &str, kind: &str, from: Option<HopState>, to: Option<HopState>, detail: Option<&str>, now_ms: i64) -> Result<()> {
    tx.execute(
        "INSERT INTO hop_event(hop_id,kind,from_state,to_state,detail,at_ms) VALUES(?1,?2,?3,?4,?5,?6)",
        params![hop_id, kind, from.map(HopState::as_str), to.map(HopState::as_str), detail, now_ms],
    )
    .map_err(storage)?;
    Ok(())
}

fn current(tx: &rusqlite::Transaction<'_>, hop_id: &str) -> Result<(HopState, u64)> {
    let row = tx
        .query_row("SELECT state,revision FROM hop WHERE id=?1", [hop_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
        .optional()
        .map_err(storage)?
        .ok_or(HopError::NotFound)?;
    Ok((HopState::parse(&row.0).ok_or(HopError::Storage("unknown state".into()))?, row.1 as u64))
}

fn read_hop(connection: &Connection, hop_id: &str) -> Result<Option<HopRecord>> {
    connection
        .query_row(
            "SELECT id,thread_id,destination_thread_id,mode,strategy,source_agent,source_native_id,source_instance,destination_agent,
               nonce,snapshot_sha256,bootstrap_sha256,seed_path,state,revision,destination_instance,destination_native_id,created_at_ms,updated_at_ms
             FROM hop WHERE id=?1",
            [hop_id],
            |row| {
                let mode: String = row.get(3)?;
                let strategy: String = row.get(4)?;
                let state: String = row.get(13)?;
                Ok(HopRecord {
                    plan: HopPlan {
                        id: row.get(0)?,
                        thread_id: row.get(1)?,
                        destination_thread_id: row.get(2)?,
                        mode: if mode == "fork" { HopMode::Fork } else { HopMode::Close },
                        strategy: if strategy == "native_resume" { HopStrategy::NativeResume } else { HopStrategy::SeedPrompt },
                        source_agent: row.get(5)?,
                        source_native_id: row.get(6)?,
                        source_instance: row.get(7)?,
                        destination_agent: row.get(8)?,
                        nonce: row.get(9)?,
                        snapshot_sha256: row.get(10)?,
                        bootstrap_sha256: row.get(11)?,
                        seed_path: row.get(12)?,
                    },
                    state: HopState::parse(&state).unwrap_or(HopState::Failed),
                    revision: row.get::<_, i64>(14)? as u64,
                    destination_instance: row.get(15)?,
                    destination_native_id: row.get(16)?,
                    created_at_ms: row.get(17)?,
                    updated_at_ms: row.get(18)?,
                })
            },
        )
        .optional()
        .map_err(storage)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn plan(id: char, thread: &str, mode: HopMode) -> HopPlan {
        HopPlan {
            id: std::iter::repeat_n(id, 32).collect(),
            thread_id: thread.into(),
            destination_thread_id: (mode == HopMode::Fork).then(|| format!("{thread}-fork")),
            mode,
            strategy: HopStrategy::SeedPrompt,
            source_agent: "claude_code".into(),
            source_native_id: "native-1".into(),
            source_instance: Some("i_source".into()),
            destination_agent: "codex".into(),
            nonce: "abc123".into(),
            snapshot_sha256: "a".repeat(64),
            bootstrap_sha256: "b".repeat(64),
            seed_path: r"C:\x\seed.md".into(),
        }
    }

    #[test]
    fn walks_the_happy_path_with_one_event_per_arrow() {
        let journal = HopJournal::open_in_memory().unwrap();
        let mut hop = journal.plan(&plan('1', "t_a", HopMode::Close), 1).unwrap();
        for next in [HopState::Previewed, HopState::Confirmed, HopState::Preparing, HopState::Ready, HopState::Switching, HopState::Committed] {
            hop = journal.transition(&hop.plan.id, hop.state, hop.revision, next, None, 2).unwrap();
        }
        assert_eq!(hop.state, HopState::Committed);
        assert_eq!(hop.revision, 7);
        let kinds: Vec<_> = journal.events(&hop.plan.id).unwrap().into_iter().map(|e| e.kind).collect();
        assert_eq!(kinds, ["planned", "previewed", "confirmed", "preparing", "ready", "switching", "committed"]);
        assert!(journal.active_for_thread("t_a").unwrap().is_none(), "a finished hop frees the thread");
        journal.plan(&plan('2', "t_a", HopMode::Close), 3).unwrap();
    }

    #[test]
    fn invalid_or_stale_transitions_change_nothing() {
        let journal = HopJournal::open_in_memory().unwrap();
        let hop = journal.plan(&plan('1', "t_a", HopMode::Fork), 1).unwrap();
        assert_eq!(
            journal.transition(&hop.plan.id, HopState::Planned, 1, HopState::Preparing, None, 2),
            Err(HopError::InvalidTransition { from: HopState::Planned, to: HopState::Preparing })
        );
        assert!(matches!(journal.transition(&hop.plan.id, HopState::Previewed, 1, HopState::Confirmed, None, 2), Err(HopError::Stale { .. })));
        assert!(matches!(journal.transition(&hop.plan.id, HopState::Planned, 9, HopState::Previewed, None, 2), Err(HopError::Stale { .. })));
        let after = journal.hop(&hop.plan.id).unwrap().unwrap();
        assert_eq!((after.state, after.revision), (HopState::Planned, 1));
        assert_eq!(journal.events(&hop.plan.id).unwrap().len(), 1);
        let mut bad = plan('3', "t_b", HopMode::Close);
        bad.destination_thread_id = Some("t_other".into());
        assert_eq!(journal.plan(&bad, 1), Err(HopError::Invalid("mode")));
        let mut late = plan('4', "t_c", HopMode::Fork);
        late.destination_thread_id = None;
        let late = journal.plan(&late, 1).unwrap();
        journal.set_destination_with_thread(&late.plan.id, Some("i_x"), None, Some("t_new"), 2).unwrap();
        assert_eq!(journal.hop(&late.plan.id).unwrap().unwrap().plan.destination_thread_id.as_deref(), Some("t_new"));
        assert!(journal.set_destination_with_thread(&hop.plan.id, None, None, Some("t_new"), 2).is_ok(), "hop 1 is a fork too");
    }

    #[test]
    fn two_processes_planning_one_thread_produce_exactly_one_winner() {
        let dir = std::env::temp_dir().join(format!("shinatama-hops-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        HopJournal::open_path(&path).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let handles: Vec<_> = (0..8u8)
            .map(|n| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let journal = HopJournal::open_path(&path).unwrap();
                    barrier.wait();
                    journal.plan(&plan(char::from_digit(u32::from(n), 16).unwrap(), "t_race", HopMode::Close), 1)
                })
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let winners = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(winners, 1, "{results:?}");
        let winner = results.iter().find_map(|r| r.as_ref().ok()).unwrap().plan.id.clone();
        for result in &results {
            if let Err(error) = result {
                assert_eq!(error, &HopError::ThreadBusy { hop_id: winner.clone() });
            }
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    struct Alive(bool);
    impl RecoveryProbe for Alive {
        fn destination_alive(&mut self, _: &HopRecord) -> bool {
            self.0
        }
    }

    #[test]
    fn a_kill_inside_a_transaction_leaves_the_previous_state() {
        let dir = std::env::temp_dir().join(format!("shinatama-hops-tx-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        let journal = HopJournal::open_path(&path).unwrap();
        let hop = journal.plan(&plan('1', "t_k", HopMode::Close), 1).unwrap();
        drop(journal);
        {
            let raw = Connection::open(&path).unwrap();
            raw.execute_batch("BEGIN IMMEDIATE; UPDATE hop SET state='ready',revision=9; INSERT INTO hop_event(hop_id,kind,at_ms) VALUES('11111111111111111111111111111111','half',0);").unwrap();
            // Dropped without COMMIT: the process died mid-transaction.
        }
        let journal = HopJournal::open_path(&path).unwrap();
        assert!(journal.integrity_check().unwrap());
        let after = journal.hop(&hop.plan.id).unwrap().unwrap();
        assert_eq!((after.state, after.revision), (HopState::Planned, 1));
        assert_eq!(journal.events(&hop.plan.id).unwrap().len(), 1);
        drop(journal);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn recovery_follows_r42_per_state() {
        let journal = HopJournal::open_in_memory().unwrap();
        let advance = |id: char, thread: &str, to: &[HopState]| {
            let mut hop = journal.plan(&plan(id, thread, HopMode::Close), 0).unwrap();
            for next in to {
                hop = journal.transition(&hop.plan.id, hop.state, hop.revision, *next, None, 0).unwrap();
            }
            hop
        };
        use HopState::*;
        let confirmed = advance('1', "t_1", &[Previewed, Confirmed]);
        let fresh_spawn = advance('2', "t_2", &[Previewed, Confirmed, Preparing]);
        journal.record_effect(&fresh_spawn.plan.id, "spawn_attempted", None, 0).unwrap();
        let switching = advance('3', "t_3", &[Previewed, Confirmed, Preparing, Ready, Switching]);
        let quiet = advance('4', "t_4", &[Previewed, Confirmed, Preparing]);
        let early = journal.recover(&mut Alive(true), 1_000).unwrap();
        let state = |id: &str| journal.hop(id).unwrap().unwrap().state;
        assert_eq!(state(&confirmed.plan.id), Aborted);
        assert_eq!(state(&fresh_spawn.plan.id), Preparing, "an uncertain spawn is only observed");
        assert_eq!(state(&switching.plan.id), Committed);
        assert_eq!(state(&quiet.plan.id), Aborted);
        assert_eq!(early.len(), 4);
        journal.recover(&mut Alive(false), READY_TIMEOUT_MS + 1).unwrap();
        assert_eq!(state(&fresh_spawn.plan.id), Failed);
        assert!(journal.record_effect(&fresh_spawn.plan.id, "late", None, 0).is_err(), "no effects after a terminal state");
    }
}
