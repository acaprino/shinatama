//! Cancellation and a database mutation have one atomic point of no return.
//! Filesystem work holds no commit permit, so cancel never waits for a slow disk.

use crate::database::{DatabaseError, Result};
use std::sync::atomic::{AtomicU8, Ordering};

const PENDING: u8 = 0;
const COMMITTING: u8 = 1;
const CANCELLED: u8 = 2;
const DONE: u8 = 3;

pub struct MutationToken {
    phase: AtomicU8,
}

impl Default for MutationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl MutationToken {
    pub fn new() -> Self {
        Self {
            phase: AtomicU8::new(PENDING),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.phase.load(Ordering::Acquire) == CANCELLED
    }

    /// True acknowledges cancellation before the mutation's point of no return.
    /// False means committing/completed; it never promises to undo a committed write.
    pub fn cancel(&self) -> bool {
        match self
            .phase
            .compare_exchange(PENDING, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) | Err(CANCELLED) => true,
            Err(_) => false,
        }
    }

    pub fn begin_commit(&self) -> Result<CommitPermit<'_>> {
        match self
            .phase
            .compare_exchange(PENDING, COMMITTING, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => Ok(CommitPermit { token: self }),
            Err(CANCELLED) => Err(DatabaseError::Cancelled),
            Err(_) => Err(DatabaseError::InvalidProject(
                "l'operazione ha già raggiunto il salvataggio",
            )),
        }
    }
}

pub struct CommitPermit<'a> {
    token: &'a MutationToken,
}

impl CommitPermit<'_> {
    pub fn finish(self) {
        drop(self);
    }
}

impl Drop for CommitPermit<'_> {
    fn drop(&mut self) {
        self.token.phase.store(DONE, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acknowledged_cancellation_never_grants_a_commit_permit() {
        let token = MutationToken::new();
        assert!(token.cancel());
        assert!(token.cancel());
        assert!(token.is_cancelled());
        assert!(token.begin_commit().is_err());
    }

    #[test]
    fn committing_and_completed_operations_never_claim_to_be_cancelled() {
        let token = MutationToken::new();
        let permit = token.begin_commit().unwrap();
        assert!(!token.cancel());
        assert!(!token.is_cancelled());
        permit.finish();
        assert!(!token.cancel());
        assert!(token.begin_commit().is_err());
    }

    #[test]
    fn concurrent_cancel_and_commit_have_exactly_one_winner() {
        use std::sync::{Arc, Barrier};
        for _ in 0..16 {
            let token = Arc::new(MutationToken::new());
            let start = Arc::new(Barrier::new(3));
            std::thread::scope(|scope| {
                let cancel_token = token.clone();
                let cancel_start = start.clone();
                let cancel = scope.spawn(move || {
                    cancel_start.wait();
                    cancel_token.cancel()
                });
                let commit_token = token.clone();
                let commit_start = start.clone();
                let commit = scope.spawn(move || {
                    commit_start.wait();
                    commit_token.begin_commit().is_ok()
                });
                start.wait();
                let cancelled = cancel.join().unwrap();
                let committed = commit.join().unwrap();
                assert_ne!(cancelled, committed);
            });
        }
    }

    #[test]
    #[cfg(windows)]
    fn database_selection_cancelled_while_waiting_for_a_writer_never_changes_history() {
        use crate::database::Database;
        use crate::location::{LocationRequest, StorageLocation};
        use std::{fs, sync::Arc, time::Duration};
        let root = std::env::temp_dir().join(format!(
            "shinatama-cancel-selection-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("project")).unwrap();
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: root.clone(),
            home_override: Some(root.join("state")),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        })
        .unwrap();
        let database = Arc::new(Database::open(&location, None).unwrap());
        let blocker = rusqlite::Connection::open(location.database_path()).unwrap();
        blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
        let token = Arc::new(MutationToken::new());
        let worker_token = token.clone();
        let worker_database = database.clone();
        let path = root.join("project");
        let worker = std::thread::spawn(move || {
            worker_database.select_if_not_cancelled(&path, &worker_token)
        });
        std::thread::sleep(Duration::from_millis(50));
        assert!(token.cancel());
        blocker.execute_batch("ROLLBACK").unwrap();
        assert!(matches!(
            worker.join().unwrap(),
            Err(DatabaseError::Cancelled)
        ));
        assert!(database.recent().unwrap().is_empty());
        let token = MutationToken::new();
        assert!(token.cancel());
        assert!(matches!(
            database.select_if_not_cancelled(
                std::path::Path::new(r"\\offline-server\share\project"),
                &token
            ),
            Err(DatabaseError::Cancelled)
        ));
        drop(blocker);
        drop(database);
        drop(location);
        fs::remove_dir_all(root).unwrap();
    }
}
