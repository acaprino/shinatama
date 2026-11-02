//! Ordered migrations with a verified SQLite snapshot before each write transaction.
//! Online Backup includes committed WAL pages; copying only the .db file does not.

use crate::database::{DatabaseError, Result};
use crate::location::StorageLocation;
use crate::schema;
use rusqlite::backup::{Backup, StepResult};
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::time::{Duration, Instant};

// Bounds SQLite backup-step retries/copying. Integrity/schema scans, filesystem
// metadata, flushes and OS calls are blocking local operations, not a hard deadline.
// Callers keep all of this work outside the window/UI thread.
const BACKUP_DEADLINE: Duration = Duration::from_secs(30);
const MAX_BACKUP_FILES: usize = 8;
const MAX_BACKUP_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MIN_FREE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 256;
const COPY_PAGES: u64 = 64;

struct BackupAdmission {
    remaining_bytes: u64,
    burst_bytes: u64,
}

pub(crate) const SETTING_JSON_INSERT_TRIGGER: &str = "CREATE TRIGGER setting_json_insert
BEFORE INSERT ON setting WHEN NOT json_valid(NEW.value)
BEGIN SELECT RAISE(ABORT, 'setting value must be JSON'); END";

pub(crate) const SETTING_JSON_UPDATE_TRIGGER: &str = "CREATE TRIGGER setting_json_update
BEFORE UPDATE OF value ON setting WHEN NOT json_valid(NEW.value)
BEGIN SELECT RAISE(ABORT, 'setting value must be JSON'); END";

struct Migration {
    from: u32,
    to: u32,
    sql: &'static [&'static str],
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        from: 0,
        to: 1,
        sql: &[schema::CREATE_V1],
    },
    Migration {
        from: 1,
        to: 2,
        sql: &[SETTING_JSON_INSERT_TRIGGER, SETTING_JSON_UPDATE_TRIGGER],
    },
    Migration {
        from: 2,
        to: 3,
        sql: crate::records::schema::MIGRATION_V3,
    },
];

/// The caller holds the private crash-released schema.lock across this operation.
pub(crate) fn initialize(connection: &mut Connection, location: &StorageLocation) -> Result<()> {
    initialize_impl(connection, location, |_, _| Ok(()), |_, _| Ok(()))
}

fn initialize_impl(
    connection: &mut Connection,
    location: &StorageLocation,
    after_backup: impl Fn(u32, &Connection) -> Result<()>,
    after_sql: impl Fn(u32, &Connection) -> Result<()>,
) -> Result<()> {
    if MIGRATIONS.last().map(|step| step.to) != Some(schema::CURRENT_VERSION)
        || MIGRATIONS
            .iter()
            .enumerate()
            .any(|(index, step)| step.from != index as u32 || step.to != step.from + 1)
    {
        return Err(DatabaseError::UnsupportedSchema);
    }
    loop {
        let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > schema::CURRENT_VERSION {
            return Err(DatabaseError::NewerSchema {
                found: version,
                supported: schema::CURRENT_VERSION,
            });
        }
        schema::validate_version(connection, version).map_err(schema_error)?;
        if version == schema::CURRENT_VERSION {
            return Ok(());
        }
        let migration = MIGRATIONS
            .iter()
            .find(|step| step.from == version)
            .ok_or(DatabaseError::UnsupportedSchema)?;
        let generation: i64 =
            connection.pragma_query_value(None, "data_version", |row| row.get(0))?;
        verified_backup(location, migration.from, migration.to, BACKUP_DEADLINE)?;
        after_backup(migration.to, connection)?;

        // Backup happens before the write transaction. Detect any intervening commit,
        // including changes by an older still-open window, rather than migrating a
        // revision for which the verified backup would no longer be complete.
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: u32 =
            transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let current_generation: i64 =
            transaction.pragma_query_value(None, "data_version", |row| row.get(0))?;
        if current != version || current_generation != generation {
            return Err(DatabaseError::Busy("Aggiornamento dello stato locale"));
        }
        schema::validate_version(&transaction, current).map_err(schema_error)?;
        if migration.from == 1 {
            let invalid_json: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM setting WHERE NOT json_valid(value))",
                [],
                |row| row.get(0),
            )?;
            if invalid_json {
                return Err(DatabaseError::UnsupportedSchema);
            }
        }
        for sql in migration.sql {
            transaction.execute_batch(sql)?;
        }
        transaction.pragma_update(None, "user_version", migration.to)?;
        schema::validate_version(&transaction, migration.to).map_err(schema_error)?;
        ensure_integrity(&transaction)?;
        after_sql(migration.to, &transaction)?;
        transaction.commit()?;
    }
}

fn schema_error(error: rusqlite::Error) -> DatabaseError {
    if matches!(error, rusqlite::Error::InvalidQuery) {
        DatabaseError::UnsupportedSchema
    } else {
        DatabaseError::Sqlite(error)
    }
}

fn ensure_integrity(connection: &Connection) -> Result<()> {
    let result: String = connection.query_row("PRAGMA integrity_check(1)", [], |row| row.get(0))?;
    if result == "ok" {
        Ok(())
    } else {
        Err(DatabaseError::IntegrityFailed)
    }
}

/// Incomplete attempts are retained with an explicit .incomplete name. Only a completed,
/// integrity-checked, version-checked, flushed copy is renamed to .backup.db.
fn verified_backup(
    location: &StorageLocation,
    from: u32,
    to: u32,
    deadline: Duration,
) -> Result<PathBuf> {
    location.verify_private_directory()?;
    let source =
        Connection::open_with_flags(location.database_path(), OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    source.busy_timeout(Duration::ZERO)?;
    let pages: i64 = source
        .pragma_query_value(None, "page_count", |row| row.get(0))
        .map_err(backup_error)?;
    let page_size: i64 = source
        .pragma_query_value(None, "page_size", |row| row.get(0))
        .map_err(backup_error)?;
    let admission = admit_backup(
        location,
        pages
            .try_into()
            .map_err(|_| DatabaseError::UnsupportedConfiguration)?,
        page_size
            .try_into()
            .map_err(|_| DatabaseError::UnsupportedConfiguration)?,
    )?;
    let unique = uuid::Uuid::new_v4();
    let stem = format!("shinatama-before-v{from}-to-v{to}-{unique}");
    let partial = location.directory().join(format!("{stem}.incomplete.db"));
    let completed = location.directory().join(format!("{stem}.backup.db"));
    let reservation = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)
        .map_err(DatabaseError::Io)?;
    drop(reservation);
    location.secure_private_file(&partial)?;
    let mut destination = Connection::open_with_flags(&partial, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    destination.busy_timeout(Duration::ZERO)?;
    destination.pragma_update(None, "journal_mode", "DELETE")?;
    destination.pragma_update(None, "synchronous", "FULL")?;
    {
        let backup = Backup::new(&source, &mut destination)?;
        let started = Instant::now();
        loop {
            if started.elapsed() >= deadline {
                return Err(DatabaseError::Busy("Copia di sicurezza del database"));
            }
            let current_bytes = fs::metadata(&partial).map_err(DatabaseError::Io)?.len();
            if current_bytes.saturating_add(admission.burst_bytes) > admission.remaining_bytes {
                return Err(DatabaseError::BackupBudget(
                    "Le copie locali raggiungono il limite di 2 GiB. Archivia manualmente alcune copie in un'altra cartella privata e riprova.",
                ));
            }
            require_disk_headroom(location, admission.burst_bytes)?;
            match backup.step(COPY_PAGES as i32)? {
                StepResult::Done => break,
                StepResult::More => std::thread::yield_now(),
                StepResult::Busy | StepResult::Locked => std::thread::sleep(
                    Duration::from_millis(10).min(deadline.saturating_sub(started.elapsed())),
                ),
                _ => return Err(DatabaseError::UnsupportedConfiguration),
            }
        }
        // rusqlite calls sqlite3_backup_finish exactly once when this handle drops.
    }
    destination.pragma_update(None, "journal_mode", "DELETE")?;
    ensure_integrity(&destination)?;
    let version: u32 = destination.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version != from {
        return Err(DatabaseError::Busy("Aggiornamento dello stato locale"));
    }
    schema::validate_version(&destination, from).map_err(schema_error)?;
    drop(destination);
    drop(source);
    location.secure_private_file(&partial)?;
    OpenOptions::new()
        .write(true)
        .open(&partial)
        .and_then(|file| file.sync_all())
        .map_err(DatabaseError::Io)?;
    complete_backup(&partial, &completed)?;
    location.verify_private_file(&completed)?;
    Ok(completed)
}

fn backup_error(error: rusqlite::Error) -> DatabaseError {
    if let rusqlite::Error::SqliteFailure(code, _) = &error
        && matches!(
            code.code,
            rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
        )
    {
        return DatabaseError::Busy("Copia di sicurezza del database");
    }
    DatabaseError::Sqlite(error)
}

fn admit_backup(location: &StorageLocation, pages: u64, page_size: u64) -> Result<BackupAdmission> {
    if !(512..=65536).contains(&page_size) || !page_size.is_power_of_two() {
        return Err(DatabaseError::UnsupportedConfiguration);
    }
    let mut count = 0;
    let mut bytes = 0u64;
    for (index, entry) in fs::read_dir(location.directory())
        .map_err(DatabaseError::Io)?
        .enumerate()
    {
        if index >= MAX_DIRECTORY_ENTRIES {
            return Err(DatabaseError::BackupBudget(
                "La cartella dati contiene più di 256 elementi. Riordina manualmente le copie prima di riprovare.",
            ));
        }
        let entry = entry.map_err(DatabaseError::Io)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("shinatama-before-v") {
            continue;
        }
        let info = fs::symlink_metadata(entry.path()).map_err(DatabaseError::Io)?;
        if !info.is_file() || info.file_type().is_symlink() {
            return Err(DatabaseError::BackupBudget(
                "Una copia esistente usa un collegamento o non è un file regolare. Verifica la cartella dati senza eliminare il database.",
            ));
        }
        if name.ends_with(".db") {
            count += 1;
        }
        bytes = bytes.saturating_add(info.len());
    }
    if count >= MAX_BACKUP_FILES {
        return Err(DatabaseError::BackupBudget(
            "Sono già conservate 8 copie complete o incomplete. Archivia manualmente alcune copie in un'altra cartella privata e riprova.",
        ));
    }
    let remaining_bytes = MAX_BACKUP_BYTES.saturating_sub(bytes);
    // Reserve the maximum SQLite page size plus a possible pending-byte-page skip,
    // even if an external connection changes page size while the snapshot restarts.
    let burst_bytes = 65536 * (COPY_PAGES + 1);
    let estimated_bytes = pages
        .saturating_mul(page_size)
        .saturating_add(burst_bytes.saturating_mul(2))
        .saturating_add(1024 * 1024);
    if estimated_bytes > remaining_bytes {
        return Err(DatabaseError::BackupBudget(
            "La copia richiede più del budget locale di 2 GiB. Archivia le copie precedenti; se il database da solo supera il budget, serve una politica di backup con un limite maggiore.",
        ));
    }
    require_disk_headroom(location, estimated_bytes)?;
    Ok(BackupAdmission {
        remaining_bytes,
        burst_bytes,
    })
}

fn require_disk_headroom(location: &StorageLocation, required: u64) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        let path: Vec<_> = location
            .directory()
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let mut available = 0;
        if unsafe {
            GetDiskFreeSpaceExW(
                path.as_ptr(),
                &mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(DatabaseError::Io(std::io::Error::last_os_error()));
        }
        if available < MIN_FREE_BYTES.saturating_add(required) {
            return Err(DatabaseError::BackupBudget(
                "Lo spazio disponibile non lascia almeno 1 GiB libero oltre alla copia. Libera spazio su questo disco prima di riprovare.",
            ));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (location, required);
        Err(DatabaseError::UnsupportedConfiguration)
    }
}

fn complete_backup(partial: &std::path::Path, completed: &std::path::Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};
        let partial: Vec<_> = partial.as_os_str().encode_wide().chain(Some(0)).collect();
        let completed: Vec<_> = completed.as_os_str().encode_wide().chain(Some(0)).collect();
        // Same-directory move, with no overwrite flag and durable rename completion.
        if unsafe { MoveFileExW(partial.as_ptr(), completed.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0
        {
            return Err(DatabaseError::Io(std::io::Error::last_os_error()));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    fs::rename(partial, completed).map_err(DatabaseError::Io)
}

#[cfg(all(test, windows))]
mod fault_tests {
    use super::*;
    use crate::location::LocationRequest;
    use std::path::Path;
    use std::process::{Child, Command, Stdio};

    fn location(root: &Path) -> StorageLocation {
        StorageLocation::resolve(LocationRequest {
            local_data_directory: root.to_path_buf(),
            home_override: Some(root.join("state")),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        })
        .unwrap()
    }

    fn root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "shinatama-migration-fault-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&root).unwrap();
        root
    }

    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn migration_helper() {
        let Some(root) = std::env::var_os("SHINATAMA_TEST_MIGRATION_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let location = location(&root);
        location.prepare_private_directory().unwrap();
        let mut connection = Connection::open(location.database_path()).unwrap();
        connection.busy_timeout(Duration::from_secs(5)).unwrap();
        initialize_impl(
            &mut connection,
            &location,
            |_, _| Ok(()),
            |version, _| {
                if version == 2 {
                    fs::write(root.join("migration-ready"), "uncommitted version 2").unwrap();
                    // This pause exists only in the test binary. The product has no env hook.
                    loop {
                        std::thread::park();
                    }
                }
                Ok(())
            },
        )
        .unwrap();
    }

    #[test]
    fn fs03_ddl_failure_rolls_back_to_v2_and_retry_keeps_the_verified_backup() {
        let root = root();
        let location = location(&root);
        location.prepare_private_directory().unwrap();
        let mut connection = Connection::open(location.database_path()).unwrap();
        connection.execute_batch(schema::CREATE_V1).unwrap();
        connection
            .execute_batch(SETTING_JSON_INSERT_TRIGGER)
            .unwrap();
        connection
            .execute_batch(SETTING_JSON_UPDATE_TRIGGER)
            .unwrap();
        connection
            .execute("INSERT INTO setting VALUES('saved','\"before FS03\"')", [])
            .unwrap();
        connection.pragma_update(None, "user_version", 2).unwrap();
        let result = initialize_impl(
            &mut connection,
            &location,
            |_, _| Ok(()),
            |version, _| {
                assert_eq!(version, 3);
                Err(DatabaseError::Io(std::io::Error::other(
                    "synthetic FS03 fault",
                )))
            },
        );
        assert!(matches!(result, Err(DatabaseError::Io(_))));
        schema::validate_version(&connection, 2).unwrap();
        assert_eq!(
            connection
                .pragma_query_value::<u32, _>(None, "user_version", |row| row.get(0))
                .unwrap(),
            2
        );
        assert_eq!(
            connection
                .query_row::<String, _, _>(
                    "SELECT value FROM setting WHERE key='saved'",
                    [],
                    |row| row.get(0)
                )
                .unwrap(),
            "\"before FS03\""
        );
        let backups = fs::read_dir(location.directory())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.to_string_lossy().ends_with(".backup.db"))
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        let backup =
            Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        schema::validate_version(&backup, 2).unwrap();
        ensure_integrity(&backup).unwrap();
        drop(backup);
        initialize(&mut connection, &location).unwrap();
        schema::validate_version(&connection, 3).unwrap();
        ensure_integrity(&connection).unwrap();
        assert!(backups[0].is_file());
        drop(connection);
        drop(location);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_commit_after_backup_defers_the_migration_without_losing_new_work() {
        let root = root();
        let location = location(&root);
        location.prepare_private_directory().unwrap();
        let mut connection = Connection::open(location.database_path()).unwrap();
        connection.execute_batch(schema::CREATE_V1).unwrap();
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
        let other = Connection::open(location.database_path()).unwrap();
        let result = initialize_impl(
            &mut connection,
            &location,
            |_, _| {
                other.execute(
                    "INSERT INTO setting VALUES('new-work', '\"committed after snapshot\"')",
                    [],
                )?;
                Ok(())
            },
            |_, _| Ok(()),
        );
        assert!(matches!(result, Err(DatabaseError::Busy(_))));
        assert_eq!(
            connection
                .pragma_query_value::<u32, _>(None, "user_version", |row| row.get(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row::<String, _, _>(
                    "SELECT value FROM setting WHERE key='new-work'",
                    [],
                    |row| row.get(0)
                )
                .unwrap(),
            "\"committed after snapshot\""
        );
        ensure_integrity(&connection).unwrap();
        drop(other);
        initialize(&mut connection, &location).unwrap();
        ensure_integrity(&connection).unwrap();
        drop(connection);
        drop(location);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_locked_source_has_a_finite_backup_deadline_and_never_a_verified_partial() {
        let root = root();
        let location = location(&root);
        location.prepare_private_directory().unwrap();
        let connection = Connection::open(location.database_path()).unwrap();
        connection.execute_batch(schema::CREATE_V1).unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
        connection.execute_batch("BEGIN EXCLUSIVE").unwrap();
        let start = Instant::now();
        let result = verified_backup(&location, 1, 2, Duration::from_millis(100));
        assert!(matches!(result, Err(DatabaseError::Busy(_))));
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(!fs::read_dir(location.directory()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".backup.db")
        }));
        connection.execute_batch("ROLLBACK").unwrap();
        ensure_integrity(&connection).unwrap();
        drop(connection);
        drop(location);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn backup_admission_counts_incomplete_attempts_without_creating_another_file() {
        let root = root();
        let location = location(&root);
        location.prepare_private_directory().unwrap();
        for index in 0..MAX_BACKUP_FILES {
            fs::write(
                location
                    .directory()
                    .join(format!("shinatama-before-v1-to-v2-{index}.incomplete.db")),
                b"",
            )
            .unwrap();
        }
        assert!(matches!(
            admit_backup(&location, 1, 4096),
            Err(DatabaseError::BackupBudget(_))
        ));
        assert_eq!(
            fs::read_dir(location.directory()).unwrap().count(),
            MAX_BACKUP_FILES
        );
        drop(location);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn oversized_state_and_excess_directory_entries_are_refused_without_reservation() {
        let root = root();
        let location = location(&root);
        location.prepare_private_directory().unwrap();
        assert!(matches!(
            admit_backup(&location, MAX_BACKUP_BYTES / 4096 + 1, 4096),
            Err(DatabaseError::BackupBudget(_))
        ));
        assert_eq!(fs::read_dir(location.directory()).unwrap().count(), 0);
        for index in 0..=MAX_DIRECTORY_ENTRIES {
            fs::write(location.directory().join(format!("unrelated-{index}")), b"").unwrap();
        }
        assert!(matches!(
            admit_backup(&location, 1, 4096),
            Err(DatabaseError::BackupBudget(_))
        ));
        assert_eq!(
            fs::read_dir(location.directory()).unwrap().count(),
            MAX_DIRECTORY_ENTRIES + 1
        );
        drop(location);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn kill_during_migration_rolls_back_and_retains_the_verified_backup() {
        let root = root();
        let location = location(&root);
        location.prepare_private_directory().unwrap();
        let mut connection = Connection::open(location.database_path()).unwrap();
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .unwrap();
        connection.execute_batch(schema::CREATE_V1).unwrap();
        connection
            .execute("INSERT INTO setting VALUES('saved', '\"saved work\"')", [])
            .unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "migrations::fault_tests::migration_helper",
                "--nocapture",
            ])
            .env("SHINATAMA_TEST_MIGRATION_ROOT", &root)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut child = OwnedChild(child);
        let start = Instant::now();
        while !root.join("migration-ready").exists() {
            assert!(
                start.elapsed() < Duration::from_secs(15),
                "migration child did not reach its uncommitted transaction"
            );
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "migration helper exited before its fault point"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let recovered = Connection::open(location.database_path()).unwrap();
        assert_eq!(
            recovered
                .pragma_query_value::<u32, _>(None, "user_version", |row| row.get(0))
                .unwrap(),
            1
        );
        ensure_integrity(&recovered).unwrap();
        drop(recovered);
        assert_eq!(
            connection
                .pragma_query_value::<u32, _>(None, "user_version", |row| row.get(0))
                .unwrap(),
            1
        );
        ensure_integrity(&connection).unwrap();
        assert_eq!(
            connection
                .query_row::<String, _, _>(
                    "SELECT value FROM setting WHERE key='saved'",
                    [],
                    |row| row.get(0)
                )
                .unwrap(),
            "\"saved work\""
        );
        assert_eq!(
            connection
                .query_row::<u32, _, _>(
                    "SELECT count(*) FROM sqlite_schema WHERE type='trigger'",
                    [],
                    |row| row.get(0)
                )
                .unwrap(),
            0
        );
        let backups = fs::read_dir(location.directory())
            .unwrap()
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.to_string_lossy().ends_with(".backup.db"))
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        let backup =
            Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        ensure_integrity(&backup).unwrap();
        assert_eq!(
            backup
                .query_row::<String, _, _>(
                    "SELECT value FROM setting WHERE key='saved'",
                    [],
                    |row| row.get(0)
                )
                .unwrap(),
            "\"saved work\""
        );
        drop(backup);
        initialize(&mut connection, &location).unwrap();
        assert_eq!(
            connection
                .pragma_query_value::<u32, _>(None, "user_version", |row| row.get(0))
                .unwrap(),
            schema::CURRENT_VERSION
        );
        ensure_integrity(&connection).unwrap();
        drop(connection);
        drop(location);
        fs::remove_dir_all(root).unwrap();
    }
}
