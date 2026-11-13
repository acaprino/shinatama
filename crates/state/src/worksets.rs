//! ADR-0055: a workset is what the interface calls a thread: a named set of sessions of one
//! project. A session is a `thread` record, unchanged; every one belongs to exactly one workset.
//! The layout of a workset is the project `workspace` record whose id is the workset id.
use crate::{
    database::Database,
    records::{CanonicalIdKind, MAX_SAFE_REVISION, new_id},
    threads::{
        Result, SUMMARY_SELECT, ThreadError, ThreadRecord, ThreadSummary, id, millis, rev_row,
        revision, summary_row, title,
    },
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

pub const MAX_LISTED: usize = 200;
pub const MAX_MEMBERS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WorksetRecord {
    pub id: String,
    pub project_id: String,
    pub title: Option<String>,
    pub archived: bool,
    pub revision: u64,
    pub created_at: i64,
    pub updated_at: i64,
}
/// The oldest session of a workset; it names a workset the user has not named.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WorksetLead {
    pub thread_id: String,
    pub title: Option<String>,
    pub harness: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WorksetSummary {
    pub id: String,
    pub project_id: String,
    pub title: Option<String>,
    pub archived: bool,
    pub revision: u64,
    /// The later of the workset's own change and its sessions' last use.
    pub used_at: i64,
    pub sessions: u32,
    /// Sessions with a recorded opening; a recorded opening is not proof of a live process.
    pub open: u32,
    pub lead: Option<WorksetLead>,
}

const COLUMNS: &str = "id,project_id,title,archived,revision,created_at,updated_at";
fn record_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorksetRecord> {
    Ok(WorksetRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        title: row.get(2)?,
        archived: row.get(3)?,
        revision: rev_row(row, 4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}
fn load(connection: &Connection, key: &str) -> Result<WorksetRecord> {
    connection
        .query_row(&format!("SELECT {COLUMNS} FROM workset WHERE id=?1"), [key], record_row)
        .optional()?
        .ok_or(ThreadError::NotFound)
}
fn of_thread(connection: &Connection, thread: &str) -> Result<Option<String>> {
    Ok(connection
        .query_row("SELECT workset_id FROM workset_member WHERE thread_id=?1", [thread], |row| row.get(0))
        .optional()?)
}
fn insert(connection: &Connection, record: &WorksetRecord) -> Result<()> {
    connection.execute(
        "INSERT INTO workset(id,project_id,title,archived,revision,created_at,updated_at) VALUES(?1,?2,?3,0,1,?4,?4)",
        params![record.id, record.project_id, record.title, record.created_at],
    )?;
    Ok(())
}
/// Called inside the transaction that inserts `record`. A session that comes from another one
/// (fork, companion) stays with it; otherwise it joins the named workset or gets its own.
pub(crate) fn join(connection: &Connection, record: &ThreadRecord, workset: Option<&str>) -> Result<()> {
    let source = record.companion_worker_thread_id.as_deref().or(record.parent_thread_id.as_deref());
    let target = match (source, workset) {
        (Some(source), _) => of_thread(connection, source)?.ok_or(ThreadError::NotFound)?,
        (None, Some(workset)) => {
            id(workset, CanonicalIdKind::Workset)?;
            let found = load(connection, workset)?;
            if found.project_id != record.project_id || found.archived {
                return Err(ThreadError::Invalid);
            }
            found.id
        }
        (None, None) => {
            let own = format!("w_{}", &record.id[2..]);
            if connection.query_row("SELECT EXISTS(SELECT 1 FROM workset WHERE id=?1)", [&own], |row| row.get::<_, bool>(0))? {
                return Err(ThreadError::Conflict);
            }
            insert(connection, &WorksetRecord {
                id: own.clone(), project_id: record.project_id.clone(), title: None, archived: false,
                revision: 1, created_at: record.created_at, updated_at: record.created_at,
            })?;
            own
        }
    };
    connection.execute("INSERT INTO workset_member(thread_id,workset_id) VALUES(?1,?2)", params![record.id, target])?;
    Ok(())
}
fn bump(connection: &Connection, key: &str, at: i64) -> Result<()> {
    if connection.execute(
        "UPDATE workset SET revision=revision+1,updated_at=max(updated_at,?2) WHERE id=?1 AND revision<9007199254740991",
        params![key, at],
    )? != 1 {
        return Err(ThreadError::Conflict);
    }
    Ok(())
}

impl Database {
    pub fn create_workset(&self, project_id: &str, name: Option<&str>, at: i64) -> Result<WorksetRecord> {
        id(project_id, CanonicalIdKind::Project)?;
        title(name)?;
        millis(at)?;
        self.require_writable()?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&tx)?;
        if !tx.query_row("SELECT EXISTS(SELECT 1 FROM project WHERE id=?1)", [project_id], |row| row.get::<_, bool>(0))? {
            return Err(ThreadError::NotFound);
        }
        let record = WorksetRecord {
            id: new_id(CanonicalIdKind::Workset), project_id: project_id.to_owned(),
            title: name.map(str::to_owned), archived: false, revision: 1, created_at: at, updated_at: at,
        };
        insert(&tx, &record)?;
        tx.commit()?;
        Ok(record)
    }
    pub fn workset_record(&self, key: &str) -> Result<Option<WorksetRecord>> {
        id(key, CanonicalIdKind::Workset)?;
        let connection = self.lock()?;
        match load(&connection, key) {
            Ok(record) => Ok(Some(record)),
            Err(ThreadError::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }
    pub fn workset_of_thread(&self, thread_id: &str) -> Result<Option<String>> {
        id(thread_id, CanonicalIdKind::Thread)?;
        let connection = self.lock()?;
        of_thread(&connection, thread_id)
    }
    /// Most recently used first. Bounded; `more` says the project has worksets beyond the bound.
    pub fn list_worksets(&self, project_id: &str, archived: bool) -> Result<(Vec<WorksetSummary>, bool)> {
        id(project_id, CanonicalIdKind::Project)?;
        let connection = self.lock()?;
        if !connection.query_row("SELECT EXISTS(SELECT 1 FROM project WHERE id=?1)", [project_id], |row| row.get::<_, bool>(0))? {
            return Err(ThreadError::NotFound);
        }
        let mut statement = connection.prepare(
            "SELECT w.id,w.project_id,w.title,w.archived,w.revision,
 max(w.updated_at,coalesce((SELECT max(t.updated_at) FROM workset_member m JOIN thread t ON t.id=m.thread_id WHERE m.workset_id=w.id),0)) AS used,
 (SELECT count(*) FROM workset_member m WHERE m.workset_id=w.id),
 (SELECT count(*) FROM workset_member m JOIN activation a ON a.thread_id=m.thread_id AND a.ended_at IS NULL WHERE m.workset_id=w.id),
 l.id,l.title,(SELECT harness FROM segment WHERE thread_id=l.id ORDER BY ordinal DESC LIMIT 1)
 FROM workset w LEFT JOIN thread l ON l.id=(SELECT t.id FROM workset_member m JOIN thread t ON t.id=m.thread_id WHERE m.workset_id=w.id ORDER BY t.created_at,t.id LIMIT 1)
 WHERE w.project_id=?1 AND w.archived=?2 ORDER BY used DESC,w.id LIMIT ?3",
        )?;
        let mut rows = statement
            .query_map(params![project_id, archived, (MAX_LISTED + 1) as i64], |row| {
                let lead: Option<String> = row.get(8)?;
                Ok(WorksetSummary {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    title: row.get(2)?,
                    archived: row.get(3)?,
                    revision: rev_row(row, 4)?,
                    used_at: row.get(5)?,
                    sessions: row.get(6)?,
                    open: row.get(7)?,
                    lead: match lead {
                        Some(thread_id) => Some(WorksetLead { thread_id, title: row.get(9)?, harness: row.get(10)? }),
                        None => None,
                    },
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let more = rows.len() > MAX_LISTED;
        rows.truncate(MAX_LISTED);
        Ok((rows, more))
    }
    /// The sessions of one workset, oldest first.
    pub fn workset_threads(&self, key: &str) -> Result<Vec<ThreadSummary>> {
        id(key, CanonicalIdKind::Workset)?;
        let connection = self.lock()?;
        load(&connection, key)?;
        let mut statement = connection.prepare(&format!(
            "{SUMMARY_SELECT} JOIN workset_member m ON m.thread_id=t.id WHERE m.workset_id=?1 ORDER BY t.created_at,t.id LIMIT ?2"
        ))?;
        let rows = statement
            .query_map(params![key, MAX_MEMBERS as i64], summary_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
    /// `name`: `Some(None)` clears the title, `None` leaves it.
    pub fn update_workset(
        &self,
        key: &str,
        expected_revision: u64,
        name: Option<Option<&str>>,
        archived: Option<bool>,
        at: i64,
    ) -> Result<WorksetRecord> {
        id(key, CanonicalIdKind::Workset)?;
        revision(expected_revision)?;
        millis(at)?;
        if let Some(name) = name {
            title(name)?;
        }
        if expected_revision >= MAX_SAFE_REVISION || (name.is_none() && archived.is_none()) {
            return Err(ThreadError::Invalid);
        }
        self.require_writable()?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&tx)?;
        let current = load(&tx, key)?;
        if current.revision != expected_revision {
            return Err(ThreadError::Conflict);
        }
        let name = match name {
            Some(name) => name.map(str::to_owned),
            None => current.title.clone(),
        };
        tx.execute(
            "UPDATE workset SET title=?2,archived=?3,revision=revision+1,updated_at=max(updated_at,?4) WHERE id=?1",
            params![key, name, archived.unwrap_or(current.archived), at],
        )?;
        let saved = load(&tx, key)?;
        tx.commit()?;
        Ok(saved)
    }
    /// An explicit user action only; membership is never inferred.
    pub fn move_thread_to_workset(&self, thread_id: &str, key: &str, at: i64) -> Result<()> {
        id(thread_id, CanonicalIdKind::Thread)?;
        id(key, CanonicalIdKind::Workset)?;
        millis(at)?;
        self.require_writable()?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&tx)?;
        let thread = crate::threads::load_thread(&tx, thread_id)?;
        let target = load(&tx, key)?;
        if target.project_id != thread.project_id || target.archived {
            return Err(ThreadError::Invalid);
        }
        let previous = of_thread(&tx, thread_id)?.ok_or(ThreadError::NotFound)?;
        if previous != key {
            tx.execute("UPDATE workset_member SET workset_id=?2 WHERE thread_id=?1", params![thread_id, key])?;
            bump(&tx, &previous, at)?;
            bump(&tx, key, at)?;
        }
        tx.commit()?;
        Ok(())
    }
    /// Only an empty workset is deleted here; sessions are forgotten one by one (FRD-0014 R27).
    pub fn delete_empty_workset(&self, key: &str, expected_revision: u64) -> Result<()> {
        id(key, CanonicalIdKind::Workset)?;
        revision(expected_revision)?;
        self.require_writable()?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&tx)?;
        let current = load(&tx, key)?;
        if current.revision != expected_revision {
            return Err(ThreadError::Conflict);
        }
        if tx.query_row("SELECT EXISTS(SELECT 1 FROM workset_member WHERE workset_id=?1)", [key], |row| row.get::<_, bool>(0))? {
            return Err(ThreadError::OwnedElsewhere);
        }
        tx.execute("DELETE FROM workspace WHERE project_id=?1 AND workspace_id=?2", params![current.project_id, key])?;
        tx.execute("DELETE FROM workset WHERE id=?1", [key])?;
        tx.commit()?;
        Ok(())
    }
}

pub(crate) mod schema {
    pub const WORKSET: &str = "CREATE TABLE workset (
 id TEXT PRIMARY KEY NOT NULL CHECK(length(id)=14 AND substr(id,1,2)='w_' AND substr(id,3) NOT GLOB '*[^a-z2-7]*'),
 project_id TEXT NOT NULL REFERENCES project(id),
 title TEXT CHECK(title IS NULL OR length(CAST(title AS BLOB)) BETWEEN 1 AND 256),
 archived INTEGER NOT NULL CHECK(archived IN (0,1)),revision INTEGER NOT NULL CHECK(revision BETWEEN 1 AND 9007199254740991),
 created_at INTEGER NOT NULL CHECK(created_at BETWEEN 0 AND 9007199254740991),updated_at INTEGER NOT NULL CHECK(updated_at BETWEEN created_at AND 9007199254740991)
) STRICT";
    pub const WORKSET_RECENT: &str =
        "CREATE INDEX workset_project_recent ON workset(project_id,updated_at DESC,id)";
    /// The primary key is the invariant: a session is in one workset. Forgetting a session
    /// deletes its `thread` row and, through the cascade, its membership.
    pub const WORKSET_MEMBER: &str = "CREATE TABLE workset_member (
 thread_id TEXT PRIMARY KEY NOT NULL REFERENCES thread(id) ON DELETE CASCADE,
 workset_id TEXT NOT NULL REFERENCES workset(id)
) STRICT";
    pub const WORKSET_MEMBER_SET: &str =
        "CREATE INDEX workset_member_set ON workset_member(workset_id)";
    pub const OBJECTS: &[(&str, &str)] = &[
        ("workset", WORKSET),
        ("workset_project_recent", WORKSET_RECENT),
        ("workset_member", WORKSET_MEMBER),
        ("workset_member_set", WORKSET_MEMBER_SET),
    ];
    /// Every session without a source gets a workset of its own, with the same suffix as its
    /// id; forks and companions join the workset of the session they descend from. The last two
    /// statements cover a session whose source chain does not reach a root.
    pub const MIGRATION_V6: &[&str] = &[
        WORKSET,
        WORKSET_RECENT,
        WORKSET_MEMBER,
        WORKSET_MEMBER_SET,
        "INSERT INTO workset(id,project_id,title,archived,revision,created_at,updated_at) SELECT 'w_'||substr(id,3),project_id,NULL,archived,1,created_at,updated_at FROM thread WHERE parent_thread_id IS NULL AND companion_worker_thread_id IS NULL",
        "WITH RECURSIVE family(id,root) AS (SELECT id,id FROM thread WHERE parent_thread_id IS NULL AND companion_worker_thread_id IS NULL UNION ALL SELECT t.id,family.root FROM thread t JOIN family ON coalesce(t.companion_worker_thread_id,t.parent_thread_id)=family.id) INSERT INTO workset_member(thread_id,workset_id) SELECT id,'w_'||substr(root,3) FROM family",
        "INSERT INTO workset(id,project_id,title,archived,revision,created_at,updated_at) SELECT 'w_'||substr(id,3),project_id,NULL,archived,1,created_at,updated_at FROM thread WHERE id NOT IN (SELECT thread_id FROM workset_member)",
        "INSERT INTO workset_member(thread_id,workset_id) SELECT id,'w_'||substr(id,3) FROM thread WHERE id NOT IN (SELECT thread_id FROM workset_member)",
    ];
}

#[cfg(all(test, windows))]
mod tests {
    use rusqlite::Connection;

    fn thread(connection: &Connection, id: &str, parent: Option<&str>, worker: Option<&str>) {
        connection.execute(
            "INSERT INTO thread(id,project_id,kind,parent_thread_id,companion_worker_thread_id,title,archived,revision,created_at,updated_at) VALUES(?1,'p_aaaaaaaaaaaa',?2,?3,?4,NULL,0,1,10,20)",
            rusqlite::params![id, if worker.is_some() { "companion" } else { "work" }, parent, worker],
        ).unwrap();
    }
    /// ADR-0055 EX-004: a fork family becomes one workset; INV-001 holds for every thread.
    #[test]
    fn migration_groups_a_family_and_leaves_no_session_outside_a_workset() {
        let connection = Connection::open_in_memory().unwrap();
        connection.pragma_update(None, "foreign_keys", true).unwrap();
        connection.execute_batch(crate::schema::CREATE_V1).unwrap();
        for sql in crate::records::schema::MIGRATION_V3.iter().chain(crate::threads::schema::MIGRATION_V4).chain(crate::threads::schema::MIGRATION_V5) {
            connection.execute_batch(sql).unwrap();
        }
        connection.execute("INSERT INTO project(id,path,path_key,name,last_opened,recent_sequence) VALUES('p_aaaaaaaaaaaa','x','x','x',0,0)", []).unwrap();
        thread(&connection, "t_aaaaaaaaaaaa", None, None);
        thread(&connection, "t_bbbbbbbbbbbb", Some("t_aaaaaaaaaaaa"), None);
        thread(&connection, "t_cccccccccccc", Some("t_bbbbbbbbbbbb"), None);
        thread(&connection, "t_dddddddddddd", None, Some("t_cccccccccccc"));
        thread(&connection, "t_eeeeeeeeeeee", None, None);
        for sql in super::schema::MIGRATION_V6 {
            connection.execute_batch(sql).unwrap();
        }
        let sets: Vec<(String, i64)> = connection
            .prepare("SELECT workset_id,count(*) FROM workset_member GROUP BY workset_id ORDER BY workset_id").unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
        assert_eq!(sets, vec![("w_aaaaaaaaaaaa".to_owned(), 4), ("w_eeeeeeeeeeee".to_owned(), 1)]);
        let worksets: i64 = connection.query_row("SELECT count(*) FROM workset", [], |row| row.get(0)).unwrap();
        assert_eq!(worksets, 2);
        // Forgetting a session removes its membership with it.
        connection.execute("DELETE FROM thread WHERE id='t_dddddddddddd'", []).unwrap();
        let left: i64 = connection.query_row("SELECT count(*) FROM workset_member", [], |row| row.get(0)).unwrap();
        assert_eq!(left, 4);
    }
}
