#![cfg(windows)]

use rusqlite::{Connection, ErrorCode};
use shinatama_state::database::{AppearanceSettings, Database, DatabaseAccess, DatabaseError};
use shinatama_state::location::{LocationRequest, StorageLocation};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("LOCALAPPDATA"))
            .join("shinatama-state-tests")
            .join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(root.join("projects")).unwrap();
        Self { root }
    }

    fn location(&self) -> StorageLocation {
        location_for(&self.root)
    }

    fn project(&self, name: &str) -> PathBuf {
        let path = self.root.join("projects").join(name);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn legacy(&self) -> PathBuf {
        let directory = self.root.join("legacy");
        fs::create_dir_all(&directory).unwrap();
        directory.join("projects.json")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.root) {
            eprintln!("Synthetic database fixture cleanup: {error}");
        }
    }
}

fn location_for(root: &Path) -> StorageLocation {
    StorageLocation::resolve(LocationRequest {
        local_data_directory: root.to_owned(),
        home_override: Some(root.join("private-state")),
        project_roots: vec![root.join("projects")],
        synchronized_roots: Vec::new(),
    })
    .unwrap()
}

#[test]
fn bundled_fts5_pragmas_and_private_files_are_effective() {
    let fixture = Fixture::new();
    let location = fixture.location();
    let database = Database::open(&location, None).unwrap();
    let diagnostic = database.diagnostics().unwrap();
    assert_eq!(diagnostic.path, location.database_path());
    assert_eq!(diagnostic.schema_version, 5);
    assert_eq!(diagnostic.journal_mode, "wal");
    assert_eq!(diagnostic.synchronous, 1);
    assert!(diagnostic.foreign_keys);
    assert_eq!(diagnostic.busy_timeout_ms, 5000);
    assert!(diagnostic.fts5);
    assert_eq!(database.access(), &DatabaseAccess::ReadWrite);
    location.verify_private_directory().unwrap();
    for name in ["shinatama.db", "shinatama.db-wal", "shinatama.db-shm"] {
        let path = location.directory().join(name);
        if path.exists() {
            location.verify_private_file(&path).unwrap();
        }
    }
    database.integrity_check().unwrap();
    let raw = Connection::open(location.database_path()).unwrap();
    let names: Vec<String> = raw.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
        .query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert_eq!(names, ["activation", "project", "segment", "setting", "store_root", "thread", "workspace"]);
}

#[test]
fn project_identity_recent_order_and_missing_roots_survive_restart() {
    let fixture = Fixture::new();
    let first_path = fixture.project("first");
    let second_path = fixture.project("second");
    let database = Database::open(&fixture.location(), None).unwrap();
    let first = database.select(&first_path).unwrap();
    assert!(first.id.starts_with("p_") && first.id.len() == 14);
    assert!(
        first.id.as_bytes()[2..]
            .iter()
            .all(|byte| b"abcdefghijklmnopqrstuvwxyz234567".contains(byte))
    );
    let second = database.select(&second_path).unwrap();
    let alias = first_path.join(".");
    assert_eq!(database.select(&alias).unwrap().id, first.id);
    assert_eq!(
        database
            .recent()
            .unwrap()
            .iter()
            .map(|entry| &entry.id)
            .collect::<Vec<_>>(),
        [&first.id, &second.id]
    );
    assert_eq!(fs::read_dir(&first_path).unwrap().count(), 0);
    fs::remove_dir(&first_path).unwrap();
    drop(database);
    let reopened = Database::open(&fixture.location(), None).unwrap();
    let recent = reopened.recent().unwrap();
    assert_eq!(recent[0].id, first.id);
    assert!(!recent[0].available);
    assert!(recent[1].available);
    assert!(matches!(
        reopened.select(&first_path),
        Err(DatabaseError::InvalidProject(_))
    ));
}

#[test]
fn settings_are_typed_bounded_and_persisted() {
    let fixture = Fixture::new();
    let database = Database::open(&fixture.location(), None).unwrap();
    assert_eq!(
        database.appearance().unwrap(),
        AppearanceSettings::default()
    );
    let value = AppearanceSettings {
        terminal_font_size: 18,
        reduced_motion: true,
    };
    database.set_appearance(&value).unwrap();
    assert!(matches!(
        database.set_appearance(&AppearanceSettings {
            terminal_font_size: 255,
            reduced_motion: false
        }),
        Err(DatabaseError::InvalidAppearance)
    ));
    drop(database);
    assert_eq!(
        Database::open(&fixture.location(), None)
            .unwrap()
            .appearance()
            .unwrap(),
        value
    );
}

#[test]
fn legacy_import_preserves_original_timestamps_order_and_unavailable_folders() {
    let fixture = Fixture::new();
    let available = fixture.project("available");
    let missing = fixture.root.join("projects").join("missing");
    let legacy = fixture.legacy();
    let bytes = serde_json::to_vec(&serde_json::json!({"schema_version":1,"projects":[
        {"path":missing,"name":"missing","last_opened":1234},
        {"path":available,"name":"available","last_opened":1234}
    ]}))
    .unwrap();
    fs::write(&legacy, &bytes).unwrap();
    let database = Database::open(&fixture.location(), Some(&legacy)).unwrap();
    let entries = database.recent().unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["missing", "available"]
    );
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.last_opened)
            .collect::<Vec<_>>(),
        [1234, 1234]
    );
    assert!(!entries[0].available);
    assert!(entries[1].available);
    assert_eq!(fs::read(&legacy).unwrap(), bytes);
    let ids = entries
        .iter()
        .map(|entry| entry.id.clone())
        .collect::<Vec<_>>();
    drop(database);
    fs::write(&legacy, b"not valid JSON after completed migration").unwrap();
    let reopened = Database::open(&fixture.location(), Some(&legacy)).unwrap();
    assert_eq!(
        reopened
            .recent()
            .unwrap()
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<Vec<_>>(),
        ids
    );
    assert_eq!(
        fs::read(&legacy).unwrap(),
        b"not valid JSON after completed migration"
    );
}

#[test]
fn malformed_future_and_oversize_legacy_files_do_not_partially_import() {
    for bytes in [
        b"{invalid".to_vec(),
        b"{\"schema_version\":999,\"projects\":[]}".to_vec(),
        vec![b'x'; 256 * 1024 + 1],
    ] {
        let fixture = Fixture::new();
        let legacy = fixture.legacy();
        fs::write(&legacy, &bytes).unwrap();
        assert!(matches!(
            Database::open(&fixture.location(), Some(&legacy)),
            Err(DatabaseError::InvalidLegacyPreferences)
        ));
        assert_eq!(fs::read(&legacy).unwrap(), bytes);
        let database = Database::open(&fixture.location(), None).unwrap();
        assert!(database.recent().unwrap().is_empty());
        database.integrity_check().unwrap();
    }
}

#[test]
fn concurrent_legacy_import_has_one_marker_and_no_duplicate_projects() {
    // Repeated last-connection closes delete WAL/SHM while the next startup
    // inspects them. Each round also independently proves one-time import.
    for _ in 0..8 {
        let fixture = Fixture::new();
        let project = fixture.project("imported");
        let legacy = fixture.legacy();
        fs::write(&legacy, serde_json::to_vec(&serde_json::json!({"schema_version":1,"projects":[{"path":project,"name":"imported","last_opened":1234}]})).unwrap()).unwrap();
        // Initialize once, then contend for the same one-time migration.
        drop(Database::open(&fixture.location(), None).unwrap());
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        let handles = (0..4)
            .map(|_| {
                let location = fixture.location();
                let legacy = legacy.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    Database::open(&location, Some(&legacy))
                        .unwrap()
                        .recent()
                        .unwrap()
                })
            })
            .collect::<Vec<_>>();
        let results = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        assert!(
            results
                .iter()
                .all(|entries| entries.len() == 1 && entries[0].id == results[0][0].id)
        );
        let raw = Connection::open(fixture.location().database_path()).unwrap();
        assert_eq!(
            raw.query_row::<i64, _, _>(
                "SELECT count(*) FROM setting WHERE key='migration.projects_json.v1'",
                [],
                |row| row.get(0)
            )
            .unwrap(),
            1
        );
    }
}

#[test]
fn legacy_writer_lock_prevents_import_of_an_obsolete_selection() {
    let fixture = Fixture::new();
    let first = fixture.project("old-selection");
    let second = fixture.project("new-selection");
    let legacy = fixture.legacy();
    let bytes = serde_json::to_vec(&serde_json::json!({"schema_version":1,"projects":[{"path":first,"name":"old-selection","last_opened":100}]})).unwrap();
    fs::write(&legacy, &bytes).unwrap();
    drop(Database::open(&fixture.location(), None).unwrap());
    let legacy_location = fixture
        .location()
        .resolve_related_directory(legacy.parent().unwrap())
        .unwrap();
    legacy_location.prepare_private_directory().unwrap();
    let lock_path = legacy.parent().unwrap().join("projects.lock");
    let writer = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .unwrap();
    writer.try_lock().unwrap();
    assert!(matches!(
        Database::open(&fixture.location(), Some(&legacy)),
        Err(DatabaseError::Busy(_))
    ));
    assert_eq!(fs::read(&legacy).unwrap(), bytes);
    let raw = Connection::open(fixture.location().database_path()).unwrap();
    assert_eq!(
        raw.query_row::<i64, _, _>(
            "SELECT count(*) FROM setting WHERE key='migration.projects_json.v1'",
            [],
            |row| row.get(0)
        )
        .unwrap(),
        0
    );
    drop(raw);
    let updated = serde_json::to_vec(&serde_json::json!({"schema_version":1,"projects":[{"path":second,"name":"new-selection","last_opened":200}]})).unwrap();
    fs::write(&legacy, &updated).unwrap();
    drop(writer);
    let database = Database::open(&fixture.location(), Some(&legacy)).unwrap();
    assert_eq!(database.recent().unwrap()[0].name, "new-selection");
    assert_eq!(fs::read(&legacy).unwrap(), updated);
}

#[test]
fn legacy_file_and_lock_hard_links_are_refused_before_import() {
    for name in ["projects.json", "projects.lock"] {
        let fixture = Fixture::new();
        let legacy = fixture.legacy();
        let outside = fixture.root.join("outside.json");
        let bytes = b"{\"schema_version\":1,\"projects\":[]}";
        fs::write(&outside, bytes).unwrap();
        if name == "projects.json" {
            fs::hard_link(&outside, &legacy).unwrap();
        } else {
            fs::write(&legacy, bytes).unwrap();
            fs::hard_link(&outside, legacy.parent().unwrap().join(name)).unwrap();
        }
        assert!(matches!(
            Database::open(&fixture.location(), Some(&legacy)),
            Err(DatabaseError::Location(_))
        ));
        assert_eq!(fs::read(&outside).unwrap(), bytes);
    }
}

#[test]
fn busy_write_is_reported_and_does_not_change_history() {
    let fixture = Fixture::new();
    let project = fixture.project("blocked");
    let database = Database::open(&fixture.location(), None).unwrap();
    let blocker = Connection::open(fixture.location().database_path()).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = Instant::now();
    let error = database.select(&project).unwrap_err();
    assert!(started.elapsed() >= Duration::from_millis(4500));
    assert!(
        matches!(error, DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(failure, _)) if failure.code == ErrorCode::DatabaseBusy)
    );
    assert!(database.recent().unwrap().is_empty());
    blocker.execute_batch("ROLLBACK").unwrap();
    database.select(&project).unwrap();
    database.integrity_check().unwrap();
}

#[test]
fn newer_database_opens_read_only_without_rewriting_content() {
    let fixture = Fixture::new();
    let project = fixture.project("retained");
    let database = Database::open(&fixture.location(), None).unwrap();
    let selected = database.select(&project).unwrap();
    drop(database);
    let path = fixture.location().database_path();
    let raw = Connection::open(&path).unwrap();
    raw.pragma_update(None, "user_version", 999).unwrap();
    raw.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    raw.pragma_update(None, "journal_mode", "DELETE").unwrap();
    drop(raw);
    let bytes = fs::read(&path).unwrap();
    let reopened = Database::open(&fixture.location(), None).unwrap();
    assert_eq!(
        reopened.access(),
        &DatabaseAccess::ReadOnlyNewer {
            found: 999,
            supported: 5
        }
    );
    assert_eq!(reopened.recent().unwrap()[0].id, selected.id);
    assert!(matches!(
        reopened.select(&project),
        Err(DatabaseError::NewerSchema {
            found: 999,
            supported: 5
        })
    ));
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn an_already_open_writer_cannot_modify_a_database_upgraded_by_another_connection() {
    let fixture = Fixture::new();
    let project = fixture.project("retained-before-upgrade");
    let candidate = fixture.project("not-saved-after-upgrade");
    let location = fixture.location();
    let database = Database::open(&location, None).unwrap();
    database.select(&project).unwrap();
    database.check_write_admission().unwrap();
    let before = database.recent().unwrap();
    let appearance = database.appearance().unwrap();
    let external = Connection::open(location.database_path()).unwrap();
    external.pragma_update(None, "user_version", 999).unwrap();
    assert!(matches!(
        database.check_write_admission(),
        Err(DatabaseError::NewerSchema { found: 999, .. })
    ));
    assert!(matches!(
        database.select(&candidate),
        Err(DatabaseError::NewerSchema { found: 999, .. })
    ));
    assert!(matches!(
        database.set_appearance(&AppearanceSettings {
            terminal_font_size: 19,
            reduced_motion: true
        }),
        Err(DatabaseError::NewerSchema { found: 999, .. })
    ));
    assert_eq!(database.recent().unwrap(), before);
    assert_eq!(database.appearance().unwrap(), appearance);
    assert_eq!(
        external
            .pragma_query_value::<u32, _>(None, "user_version", |row| row.get(0))
            .unwrap(),
        999
    );
}

#[test]
fn claimed_current_version_requires_the_embedded_schema_before_any_wal_change() {
    for sql in [
        "PRAGMA user_version=2;",
        "CREATE TABLE foreign_data(value TEXT); INSERT INTO foreign_data VALUES('retain'); PRAGMA user_version=2;",
        "CREATE TABLE project(id TEXT PRIMARY KEY); CREATE TABLE setting(key TEXT PRIMARY KEY,value TEXT); PRAGMA user_version=2;",
    ] {
        let fixture = Fixture::new();
        let location = fixture.location();
        location.prepare_private_directory().unwrap();
        let path = location.database_path();
        let raw = Connection::open(&path).unwrap();
        raw.execute_batch(sql).unwrap();
        drop(raw);
        let original = fs::read(&path).unwrap();
        assert!(matches!(
            Database::open(&location, None),
            Err(DatabaseError::UnsupportedSchema)
        ));
        assert_eq!(fs::read(&path).unwrap(), original);
        let raw = Connection::open(&path).unwrap();
        assert_eq!(
            raw.pragma_query_value::<String, _>(None, "journal_mode", |row| row.get(0))
                .unwrap(),
            "delete"
        );
    }
}

#[test]
fn corrupt_and_unversioned_databases_are_never_recreated() {
    let fixture = Fixture::new();
    let location = fixture.location();
    location.prepare_private_directory().unwrap();
    let bytes = b"this is not an SQLite database";
    fs::write(location.database_path(), bytes).unwrap();
    assert!(Database::open(&location, None).is_err());
    assert_eq!(fs::read(location.database_path()).unwrap(), bytes);
    fs::remove_file(location.database_path()).unwrap();
    let raw = Connection::open(location.database_path()).unwrap();
    raw.execute_batch(
        "CREATE TABLE unknown_data(value TEXT); INSERT INTO unknown_data VALUES('keep');",
    )
    .unwrap();
    drop(raw);
    assert!(Database::open(&location, None).is_err());
    let raw = Connection::open(location.database_path()).unwrap();
    assert_eq!(
        raw.query_row::<String, _, _>("SELECT value FROM unknown_data", [], |row| row.get(0))
            .unwrap(),
        "keep"
    );
}

struct ChildProcesses(Vec<Child>);

impl Drop for ChildProcesses {
    fn drop(&mut self) {
        for child in &mut self.0 {
            if let Ok(None) = child.try_wait() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

#[test]
fn four_reader_and_four_writer_processes_preserve_integrity() {
    let fixture = Fixture::new();
    for index in 0..4 {
        fixture.project(&format!("writer-{index}"));
    }
    drop(Database::open(&fixture.location(), None).unwrap());
    let mut children = ChildProcesses(Vec::new());
    for role in ["reader", "writer"] {
        for index in 0..4 {
            children.0.push(
                Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "database_child", "--nocapture"])
                    .env("SHINATAMA_DATABASE_TEST_ROOT", &fixture.root)
                    .env("SHINATAMA_DATABASE_TEST_ROLE", role)
                    .env("SHINATAMA_DATABASE_TEST_INDEX", index.to_string())
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .unwrap(),
            );
        }
    }
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        let ready = fs::read_dir(&fixture.root)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("ready-"))
            .count();
        if ready == 8 {
            break;
        }
        assert!(Instant::now() < deadline, "worker startup deadline");
        std::thread::sleep(Duration::from_millis(20));
    }
    fs::write(fixture.root.join("go"), []).unwrap();
    for child in &mut children.0 {
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "database worker failed: {status}");
                break;
            }
            assert!(
                Instant::now() < deadline,
                "concurrent reader/writer deadline"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    let database = Database::open(&fixture.location(), None).unwrap();
    let projects = database.recent().unwrap();
    assert_eq!(projects.len(), 4);
    let mut identities = projects
        .iter()
        .map(|project| &project.id)
        .collect::<Vec<_>>();
    identities.sort();
    identities.dedup();
    assert_eq!(identities.len(), 4);
    database.integrity_check().unwrap();
}

#[test]
fn database_child() {
    let Some(root) = std::env::var_os("SHINATAMA_DATABASE_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let role = std::env::var("SHINATAMA_DATABASE_TEST_ROLE").unwrap();
    let index = std::env::var("SHINATAMA_DATABASE_TEST_INDEX").unwrap();
    let database = Database::open(&location_for(&root), None).unwrap();
    fs::write(root.join(format!("ready-{role}-{index}")), []).unwrap();
    let deadline = Instant::now() + Duration::from_secs(35);
    while !root.join("go").exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    if role == "writer" {
        let path = root.join("projects").join(format!("writer-{index}"));
        let mut identity = None;
        for _ in 0..50 {
            let project = database.select(&path).unwrap();
            if let Some(previous) = identity.as_ref() {
                assert_eq!(previous, &project.id);
            }
            identity = Some(project.id);
        }
        fs::write(root.join(format!("done-{index}")), []).unwrap();
    } else {
        let mut reads = 0;
        loop {
            let projects = database.recent().unwrap();
            assert!(projects.len() <= 4);
            assert!(projects.iter().all(|project| project.available));
            reads += 1;
            let complete = (0..4).all(|writer| root.join(format!("done-{writer}")).exists());
            if complete && reads >= 100 {
                break;
            }
            assert!(Instant::now() < deadline, "reader deadline");
        }
    }
    database.integrity_check().unwrap();
}
