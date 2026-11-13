#![cfg(windows)]

use rusqlite::{Connection, OpenFlags, params};
use shinatama_state::database::{Database, DatabaseAccess, DatabaseError};
use shinatama_state::location::{LocationRequest, StorageLocation};
use std::fs;
use std::path::PathBuf;

// Exact v1 schema shipped by C02. Fixtures remain independent of current DDL.
const V1: &str = "CREATE TABLE project (
    id TEXT PRIMARY KEY NOT NULL CHECK(length(id)=14 AND substr(id,1,2)='p_'),
    path TEXT NOT NULL,
    path_key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL CHECK(length(name)>0),
    last_opened INTEGER NOT NULL CHECK(last_opened>=0),
    recent_sequence INTEGER NOT NULL CHECK(recent_sequence>=0)
) STRICT;
CREATE INDEX project_recent ON project(last_opened DESC, recent_sequence DESC);
CREATE TABLE setting (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) STRICT;";

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("shinatama-migrations-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn location(&self) -> StorageLocation {
        StorageLocation::resolve(LocationRequest {
            local_data_directory: self.0.clone(),
            home_override: Some(self.0.join("private-state")),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        })
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn backups(location: &StorageLocation) -> Vec<PathBuf> {
    let mut files = fs::read_dir(location.directory())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.to_string_lossy().ends_with(".backup.db"))
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn version(connection: &Connection) -> u32 {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}
fn integrity(connection: &Connection) {
    assert_eq!(
        connection
            .query_row::<String, _, _>("PRAGMA integrity_check", [], |row| row.get(0))
            .unwrap(),
        "ok"
    );
}

fn seed_v1(location: &StorageLocation, value: &str) -> Connection {
    location.prepare_private_directory().unwrap();
    let connection = Connection::open(location.database_path()).unwrap();
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .unwrap();
    connection
        .pragma_update(None, "wal_autocheckpoint", 0)
        .unwrap();
    connection.execute_batch(V1).unwrap();
    connection.pragma_update(None, "user_version", 1).unwrap();
    connection
        .execute("INSERT INTO setting VALUES('saved', ?1)", [value])
        .unwrap();
    connection
        .execute(
            "INSERT INTO project VALUES('p_abcdefghijkl', ?1, ?1, 'saved project', 100, 7)",
            [r"C:\synthetic-shinatama-project"],
        )
        .unwrap();
    connection
}

#[test]
fn new_database_creates_ordered_verified_backups_and_current_reopen_is_idempotent() {
    let fixture = Fixture::new();
    let location = fixture.location();
    let database = Database::open(&location, None).unwrap();
    assert_eq!(database.diagnostics().unwrap().schema_version, 6);
    let saved = backups(&location);
    assert_eq!(saved.len(), 6);
    let mut versions = Vec::new();
    for file in &saved {
        location.verify_private_file(file).unwrap();
        let backup = Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        integrity(&backup);
        versions.push(version(&backup));
    }
    versions.sort();
    assert_eq!(versions, [0, 1, 2, 3, 4, 5]);
    database.integrity_check().unwrap();
    drop(database);
    let reopened = Database::open(&location, None).unwrap();
    assert_eq!(reopened.diagnostics().unwrap().schema_version, 6);
    assert_eq!(backups(&location), saved);
}

#[test]
fn v1_migration_preserves_authoritative_rows_and_backs_up_uncheckpointed_wal() {
    let fixture = Fixture::new();
    let location = fixture.location();
    let writer = seed_v1(&location, r#"{"saved":"committed in WAL"}"#);
    assert!(
        fs::metadata(location.directory().join("shinatama.db-wal"))
            .unwrap()
            .len()
            > 32
    );
    let database = Database::open(&location, None).unwrap();
    assert_eq!(database.diagnostics().unwrap().schema_version, 6);
    let recent = database.recent().unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].id, "p_abcdefghijkl");
    assert_eq!(recent[0].last_opened, 100);
    let saved = backups(&location);
    assert_eq!(saved.len(), 5);
    let backup = Connection::open_with_flags(&saved[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(version(&backup), 1);
    assert_eq!(
        backup
            .query_row::<String, _, _>("SELECT value FROM setting WHERE key='saved'", [], |row| row
                .get(0))
            .unwrap(),
        r#"{"saved":"committed in WAL"}"#
    );
    assert_eq!(
        backup
            .query_row::<String, _, _>("SELECT id FROM project", [], |row| row.get(0))
            .unwrap(),
        "p_abcdefghijkl"
    );
    integrity(&backup);
    integrity(&writer);
    assert!(
        writer
            .execute("INSERT INTO setting VALUES('invalid-new', 'not JSON')", [])
            .is_err()
    );
    assert!(
        writer
            .execute("UPDATE setting SET value='not JSON' WHERE key='saved'", [])
            .is_err()
    );
    writer
        .execute(
            "INSERT INTO setting VALUES('valid-new', ?1)",
            params!["true"],
        )
        .unwrap();
}

#[test]
fn v2_to_v3_preserves_rows_and_verifies_a_complete_v2_backup_before_new_records() {
    let fixture = Fixture::new();
    let location = fixture.location();
    let writer = seed_v1(&location, r#"{"saved":"before FS03"}"#);
    writer
        .execute_batch(
            "CREATE TRIGGER setting_json_insert
BEFORE INSERT ON setting WHEN NOT json_valid(NEW.value)
BEGIN SELECT RAISE(ABORT, 'setting value must be JSON'); END;
CREATE TRIGGER setting_json_update
BEFORE UPDATE OF value ON setting WHEN NOT json_valid(NEW.value)
BEGIN SELECT RAISE(ABORT, 'setting value must be JSON'); END;",
        )
        .unwrap();
    writer.pragma_update(None, "user_version", 2).unwrap();

    let database = Database::open(&location, None).unwrap();
    assert_eq!(database.diagnostics().unwrap().schema_version, 6);
    assert_eq!(database.recent().unwrap()[0].id, "p_abcdefghijkl");
    let saved = backups(&location);
    assert_eq!(saved.len(), 4);
    let backup = Connection::open_with_flags(&saved[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(version(&backup), 2);
    integrity(&backup);
    assert_eq!(
        backup
            .query_row::<String, _, _>("SELECT value FROM setting WHERE key='saved'", [], |row| row
                .get(0))
            .unwrap(),
        r#"{"saved":"before FS03"}"#
    );
    assert_eq!(
        backup
            .query_row::<u32, _, _>(
                "SELECT count(*) FROM sqlite_schema WHERE name IN ('store_root','workspace')",
                [],
                |row| row.get(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        writer
            .query_row::<String, _, _>("SELECT value FROM setting WHERE key='saved'", [], |row| row
                .get(0))
            .unwrap(),
        r#"{"saved":"before FS03"}"#
    );
    assert_eq!(
        writer
            .query_row::<i64, _, _>(
                "SELECT recent_sequence FROM project WHERE id='p_abcdefghijkl'",
                [],
                |row| row.get(0)
            )
            .unwrap(),
        7
    );
    assert_eq!(
        writer
            .query_row::<u32, _, _>(
                "SELECT (SELECT count(*) FROM store_root)+(SELECT count(*) FROM workspace)",
                [],
                |row| row.get(0)
            )
            .unwrap(),
        0
    );
    assert!(
        writer
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query([])
            .unwrap()
            .next()
            .unwrap()
            .is_none()
    );
    drop(backup);
    drop(database);
    drop(writer);
    let reopened = Database::open(&location, None).unwrap();
    assert_eq!(reopened.diagnostics().unwrap().schema_version, 6);
    assert_eq!(backups(&location), saved);
}

#[test]
fn invalid_old_json_fails_without_repair_or_loss_and_keeps_its_backup() {
    let fixture = Fixture::new();
    let location = fixture.location();
    let original = seed_v1(&location, "old non-JSON value");
    assert!(matches!(
        Database::open(&location, None),
        Err(DatabaseError::UnsupportedSchema)
    ));
    assert_eq!(version(&original), 1);
    assert_eq!(
        original
            .query_row::<String, _, _>("SELECT value FROM setting WHERE key='saved'", [], |row| row
                .get(0))
            .unwrap(),
        "old non-JSON value"
    );
    assert_eq!(
        original
            .query_row::<u32, _, _>(
                "SELECT count(*) FROM sqlite_schema WHERE type='trigger'",
                [],
                |row| row.get(0)
            )
            .unwrap(),
        0
    );
    integrity(&original);
    let saved = backups(&location);
    assert_eq!(saved.len(), 1);
    let backup = Connection::open_with_flags(&saved[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(version(&backup), 1);
    integrity(&backup);
    assert_eq!(
        backup
            .query_row::<String, _, _>("SELECT value FROM setting WHERE key='saved'", [], |row| row
                .get(0))
            .unwrap(),
        "old non-JSON value"
    );
}

#[test]
fn repeated_failed_migrations_stop_before_creating_a_ninth_backup() {
    let fixture = Fixture::new();
    let location = fixture.location();
    let original = seed_v1(&location, "invalid old JSON, retained");
    for _ in 0..8 {
        assert!(matches!(
            Database::open(&location, None),
            Err(DatabaseError::UnsupportedSchema)
        ));
    }
    let saved = backups(&location);
    assert_eq!(saved.len(), 8);
    let snapshots = saved
        .iter()
        .map(|file| (file.clone(), fs::read(file).unwrap()))
        .collect::<Vec<_>>();
    let count = fs::read_dir(location.directory()).unwrap().count();
    let original_bytes = fs::read(location.database_path()).unwrap();
    let wal_bytes = fs::read(location.directory().join("shinatama.db-wal")).unwrap();
    assert!(matches!(
        Database::open(&location, None),
        Err(DatabaseError::BackupBudget(_))
    ));
    assert_eq!(fs::read_dir(location.directory()).unwrap().count(), count);
    assert_eq!(backups(&location), saved);
    assert_eq!(fs::read(location.database_path()).unwrap(), original_bytes);
    assert_eq!(
        fs::read(location.directory().join("shinatama.db-wal")).unwrap(),
        wal_bytes
    );
    for (file, bytes) in snapshots {
        assert_eq!(fs::read(file).unwrap(), bytes);
    }
    assert_eq!(version(&original), 1);
    assert_eq!(
        original
            .query_row::<String, _, _>("SELECT value FROM setting WHERE key='saved'", [], |row| row
                .get(0))
            .unwrap(),
        "invalid old JSON, retained"
    );
    integrity(&original);
}

#[test]
fn newer_database_is_readonly_without_schema_changes_or_backup() {
    let fixture = Fixture::new();
    let location = fixture.location();
    location.prepare_private_directory().unwrap();
    let original = Connection::open(location.database_path()).unwrap();
    original.execute_batch("CREATE TABLE future_history(body TEXT); INSERT INTO future_history VALUES('future saved work'); PRAGMA user_version=999;").unwrap();
    drop(original);
    let bytes = fs::read(location.database_path()).unwrap();
    let database = Database::open(&location, None).unwrap();
    assert_eq!(
        database.access(),
        &DatabaseAccess::ReadOnlyNewer {
            found: 999,
            supported: 6
        }
    );
    assert!(database.set_appearance(&Default::default()).is_err());
    assert!(backups(&location).is_empty());
    assert_eq!(fs::read(location.database_path()).unwrap(), bytes);
}

#[test]
fn corrupt_file_is_preserved_without_recreation_or_backup() {
    let fixture = Fixture::new();
    let location = fixture.location();
    location.prepare_private_directory().unwrap();
    let bytes = b"corrupt database, retained for explicit recovery";
    fs::write(location.database_path(), bytes).unwrap();
    assert!(Database::open(&location, None).is_err());
    assert_eq!(fs::read(location.database_path()).unwrap(), bytes);
    assert!(backups(&location).is_empty());
}

#[test]
fn unknown_unversioned_history_is_not_treated_as_an_empty_database() {
    let fixture = Fixture::new();
    let location = fixture.location();
    location.prepare_private_directory().unwrap();
    let original = Connection::open(location.database_path()).unwrap();
    original.execute_batch("CREATE TABLE saved_history(body TEXT); INSERT INTO saved_history VALUES('unrecognized but preserved');").unwrap();
    assert!(matches!(
        Database::open(&location, None),
        Err(DatabaseError::UnsupportedSchema)
    ));
    assert_eq!(version(&original), 0);
    assert_eq!(
        original
            .query_row::<String, _, _>("SELECT body FROM saved_history", [], |row| row.get(0))
            .unwrap(),
        "unrecognized but preserved"
    );
    assert!(backups(&location).is_empty());
}
