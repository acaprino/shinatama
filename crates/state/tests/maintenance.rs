#![cfg(windows)]

use rusqlite::{Connection, OpenFlags};
use shinatama_state::database::{Database, DatabaseError};
use shinatama_state::location::{LocationRequest, StorageLocation};
use shinatama_state::maintenance::{self, Capacity, Check, Checkpoint, Outcome, Policy};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

struct Fixture {
    root: PathBuf,
    location: Option<StorageLocation>,
}

impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
            .join("shinatama-maintenance-tests")
            .join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(root.join("projects")).unwrap();
        let location = location_for(&root);
        Self {
            root,
            location: Some(location),
        }
    }
    fn location(&self) -> &StorageLocation {
        self.location.as_ref().unwrap()
    }
    fn open(&self) -> (Database, Connection) {
        let database = Database::open(self.location(), None).unwrap();
        let connection = Connection::open(self.location().database_path()).unwrap();
        connection
            .busy_timeout(Duration::from_millis(5000))
            .unwrap();
        (database, connection)
    }
    fn project(&self, name: &str) -> PathBuf {
        let path = self.root.join("projects").join(name);
        fs::create_dir_all(&path).unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        drop(self.location.take());
        if let Err(error) = fs::remove_dir_all(&self.root) {
            eprintln!("Maintenance fixture cleanup: {error}");
        }
    }
}

fn location_for(root: &Path) -> StorageLocation {
    StorageLocation::resolve(LocationRequest {
        local_data_directory: root.to_path_buf(),
        home_override: Some(root.join("state")),
        project_roots: vec![root.join("projects")],
        synchronized_roots: Vec::new(),
    })
    .unwrap()
}

fn validation_only() -> Policy {
    Policy {
        quick_check: false,
        optimize: false,
        incremental_vacuum_pages: 0,
        ..Policy::default()
    }
}

fn authored_rows(connection: &Connection) -> (Vec<String>, Vec<String>) {
    let projects=connection.prepare("SELECT json_array(id,path,path_key,name,last_opened,recent_sequence) FROM project ORDER BY id").unwrap().query_map([],|row| row.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    let settings=connection.prepare("SELECT json_array(key,value) FROM setting WHERE key NOT IN ('maintenance.lease.v1','maintenance.state.v1') ORDER BY key").unwrap().query_map([],|row| row.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    (projects, settings)
}

#[test]
fn real_idle_job_checks_sqlite_and_preserves_saved_work() {
    let fixture = Fixture::new();
    let (database, mut connection) = fixture.open();
    database.select(&fixture.project("saved")).unwrap();
    let before = authored_rows(&connection);
    let report = maintenance::run(&mut connection, fixture.location(), Policy::default()).unwrap();
    assert_eq!(report.outcome, Outcome::Completed);
    assert_eq!(report.observed_rows, 1);
    assert_eq!(report.quick_check, Check::Passed);
    assert!(report.optimized);
    assert_eq!(
        report.vacuum_pages_requested, 0,
        "auto_vacuum must not be converted"
    );
    assert!(report.lease_released);
    assert!(matches!(report.checkpoint, Checkpoint::Done { .. }));
    assert_eq!(authored_rows(&connection), before);
    assert_eq!(
        connection
            .pragma_query_value::<u32, _>(None, "busy_timeout", |row| row.get(0))
            .unwrap(),
        5000
    );
    database.integrity_check().unwrap();
}

#[test]
fn failed_quick_check_stops_optimization_and_preserves_the_malformed_work_record() {
    let fixture = Fixture::new();
    let (_database, mut connection) = fixture.open();
    connection
        .execute_batch("PRAGMA ignore_check_constraints=ON")
        .unwrap();
    connection
        .execute(
            "INSERT INTO project VALUES('invalid','relative','relative','retain this',0,0)",
            [],
        )
        .unwrap();
    connection
        .execute_batch("PRAGMA ignore_check_constraints=OFF")
        .unwrap();
    let before = authored_rows(&connection);
    let report = maintenance::run(&mut connection, fixture.location(), Policy::default()).unwrap();
    assert_eq!(report.outcome, Outcome::CorruptionDetected);
    assert_eq!(report.quick_check, Check::Corrupt);
    assert!(!report.optimized);
    assert!(
        !report.lease_released,
        "after detecting corruption no further metadata writes are attempted"
    );
    assert_eq!(report.checkpoint, Checkpoint::NotRun);
    assert_eq!(authored_rows(&connection), before);
    assert_eq!(
        connection
            .query_row::<i64, _, _>(
                "SELECT count(*) FROM setting WHERE key='maintenance.state.v1'",
                [],
                |row| row.get(0)
            )
            .unwrap(),
        1
    );
}

fn many_valid_projects(connection: &Connection, count: usize) {
    let transaction = connection.unchecked_transaction().unwrap();
    for index in 0..count {
        let mut number = index;
        let mut id = String::from("p_");
        for _ in 0..12 {
            id.push(b"abcdefghijklmnopqrstuvwxyz234567"[number & 31] as char);
            number >>= 5;
        }
        let path = format!("C:\\synthetic-projects\\item-{index}");
        transaction
            .execute(
                "INSERT INTO project VALUES(?1,?2,?3,?4,0,?5)",
                rusqlite::params![
                    id,
                    path,
                    path.to_lowercase(),
                    "saved-project-name-for-resource-test".repeat(2),
                    index as i64
                ],
            )
            .unwrap();
    }
    transaction.commit().unwrap();
}

#[test]
fn optional_quick_check_cannot_roll_back_completed_scan_frontiers() {
    let fixture = Fixture::new();
    let (database, mut connection) = fixture.open();
    many_valid_projects(&connection, 5000);
    connection
        .execute(
            "INSERT INTO setting VALUES('migration.projects_json.v1','{}')",
            [],
        )
        .unwrap();
    let pages = connection
        .pragma_query_value::<u32, _>(None, "page_count", |row| row.get(0))
        .unwrap();
    assert!(pages < 4096);
    let mut previous = 0;
    let mut saw_optional_interrupt = false;
    let mut saw_settings = false;
    let mut saved_ticks = 0;
    let base = chrono::Utc::now().timestamp_millis();
    for tick in 0..8 {
        // New idle slots occur after the finite lease expiry. This does not
        // alter the real cooperative deadline or the VM quota of any run.
        let report = maintenance::run(
            &mut connection,
            fixture.location(),
            Policy {
                now_millis: base + tick * 31_000,
                ..Policy::default()
            },
        )
        .unwrap();
        assert!(report.vm_steps_approx <= 50_000, "{report:?}");
        assert!(
            matches!(report.outcome, Outcome::Completed | Outcome::DeferredBudget),
            "{report:?}"
        );
        saved_ticks += usize::from(report.scan_progress_saved);
        let cursor=connection.query_row::<i64,_,_>("SELECT coalesce((SELECT json_extract(value,'$.project_cursor') FROM setting WHERE key='maintenance.state.v1'),0)",[],|row| row.get(0)).unwrap();
        assert!(cursor >= previous);
        previous = cursor;
        saw_optional_interrupt |= report.quick_check == Check::DeferredBudget;
        saw_settings |= report.scan_progress_saved
            && report
                .anomalies
                .iter()
                .any(|item| item.reason == "import_marker_shape");
    }
    assert!(previous > 128 && saw_settings && saw_optional_interrupt && saved_ticks >= 2);
    assert_eq!(
        connection
            .query_row::<i64, _, _>("SELECT count(*) FROM project", [], |row| row.get(0))
            .unwrap(),
        5000
    );
    database.integrity_check().unwrap();
}

#[test]
fn tight_work_budget_reserves_durable_progress_and_lease_cleanup() {
    let fixture = Fixture::new();
    let (_database, mut connection) = fixture.open();
    many_valid_projects(&connection, 5000);
    let mut previous = 0;
    let mut completed_rows = 0;
    let mut saved_ticks = 0;
    let mut released_ticks = 0;
    let before = authored_rows(&connection);
    let base = chrono::Utc::now().timestamp_millis();
    for tick in 0..10 {
        let report = maintenance::run(
            &mut connection,
            fixture.location(),
            Policy {
                max_vm_steps: 2000,
                now_millis: base + tick * 31_000,
                ..validation_only()
            },
        )
        .unwrap();
        assert!(report.vm_steps_approx <= 2000, "{report:?}");
        assert!(
            matches!(report.outcome, Outcome::Completed | Outcome::DeferredBudget),
            "{report:?}"
        );
        saved_ticks += usize::from(report.scan_progress_saved);
        released_ticks += usize::from(report.lease_released);
        let cursor=connection.query_row::<i64,_,_>("SELECT coalesce((SELECT json_extract(value,'$.project_cursor') FROM setting WHERE key='maintenance.state.v1'),0)",[],|row| row.get(0)).unwrap();
        assert!(cursor >= previous);
        previous = cursor;
        if report.scan_progress_saved {
            completed_rows += report.observed_rows;
        }
    }
    assert!(previous > 1 && completed_rows > 1 && saved_ticks >= 2 && released_ticks >= 2);
    assert_eq!(authored_rows(&connection), before);
}

#[test]
fn dedicated_facade_defers_a_busy_worker_without_locking_foreground_state() {
    let fixture = Fixture::new();
    let (database, connection) = fixture.open();
    let selected = database
        .select(&fixture.project("foreground-still-works"))
        .unwrap();
    connection.execute_batch("BEGIN IMMEDIATE").unwrap();
    let report = database.maintain(validation_only()).unwrap();
    assert!(matches!(
        report.outcome,
        Outcome::DeferredBusy | Outcome::DeferredBudget
    ));
    assert_eq!(database.recent().unwrap()[0].id, selected.id);
    connection.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        database.maintain(validation_only()).unwrap().outcome,
        Outcome::Completed
    );
}

#[test]
fn finite_batches_resume_and_never_delete_project_records() {
    let fixture = Fixture::new();
    let (database, mut connection) = fixture.open();
    for index in 0..12 {
        database
            .select(&fixture.project(&format!("project-{index}")))
            .unwrap();
    }
    let before = authored_rows(&connection);
    let report = maintenance::run(
        &mut connection,
        fixture.location(),
        Policy {
            max_rows: 2,
            ..validation_only()
        },
    )
    .unwrap();
    assert_eq!(report.outcome, Outcome::Completed);
    assert_eq!(report.observed_rows, 2);
    assert!(report.more_rows);
    for _ in 0..20 {
        let report = maintenance::run(
            &mut connection,
            fixture.location(),
            Policy {
                max_rows: 2,
                ..validation_only()
            },
        )
        .unwrap();
        assert!(report.observed_rows <= 2);
    }
    assert_eq!(authored_rows(&connection), before);
    assert_eq!(
        connection
            .query_row::<i64, _, _>("SELECT count(*) FROM project", [], |row| row.get(0))
            .unwrap(),
        12
    );
}

#[test]
fn a_row_larger_than_the_whole_byte_quota_does_not_starve_later_records() {
    for (payload_size, quota) in [(200usize, 64usize), (65_536, 65_536)] {
        let fixture = Fixture::new();
        let (_database, mut connection) = fixture.open();
        let value = format!("\"{}\"", "x".repeat(payload_size - 2));
        connection
            .execute("INSERT INTO setting VALUES('a',?1)", [value])
            .unwrap();
        connection
            .execute(
                "INSERT INTO setting VALUES('migration.projects_json.v1','{}')",
                [],
            )
            .unwrap();
        let before = authored_rows(&connection);
        let mut saw_reference = false;
        let mut saw_later_anomaly = false;
        for _ in 0..8 {
            let report = maintenance::run(
                &mut connection,
                fixture.location(),
                Policy {
                    max_rows: 1,
                    max_payload_bytes: quota,
                    ..validation_only()
                },
            )
            .unwrap();
            assert!(matches!(
                report.outcome,
                Outcome::Completed | Outcome::DeferredBudget
            ));
            assert!(report.inspected_payload_bytes <= quota);
            saw_reference |= report
                .deferred_rows
                .iter()
                .any(|row| row.table == "setting" && row.row_id == 1);
            assert!(
                !report
                    .anomalies
                    .iter()
                    .any(|row| row.table == "setting" && row.row_id == 1),
                "a legitimate quota-deferred row is not malformed"
            );
            saw_later_anomaly |= report
                .anomalies
                .iter()
                .any(|row| row.reason == "import_marker_shape");
        }
        assert!(
            saw_reference && saw_later_anomaly,
            "cursor must move past the unverified first row"
        );
        assert_eq!(authored_rows(&connection), before);
    }
}

#[test]
fn remaining_byte_budget_defers_the_next_row_without_labeling_it_malformed() {
    let fixture = Fixture::new();
    let (_database, mut connection) = fixture.open();
    for key in ["a", "b"] {
        connection
            .execute(
                "INSERT INTO setting VALUES(?1,?2)",
                rusqlite::params![key, format!("\"{}\"", "x".repeat(38))],
            )
            .unwrap();
    }
    connection
        .execute(
            "INSERT INTO setting VALUES('migration.projects_json.v1','{}')",
            [],
        )
        .unwrap();
    let mut saw_later = false;
    for _ in 0..8 {
        let report = maintenance::run(
            &mut connection,
            fixture.location(),
            Policy {
                max_payload_bytes: 64,
                ..validation_only()
            },
        )
        .unwrap();
        // A cooperative work deadline may defer a tick under local IO pressure;
        // it must still preserve the byte quota and make progress on later ticks.
        assert!(
            matches!(report.outcome, Outcome::Completed | Outcome::DeferredBudget),
            "{report:?}"
        );
        assert!(report.inspected_payload_bytes <= 64);
        assert!(
            report.deferred_rows.is_empty(),
            "a later batch can admit these whole records"
        );
        assert!(!report.anomalies.iter().any(|row| row.row_id <= 2));
        saw_later |= report
            .anomalies
            .iter()
            .any(|row| row.reason == "import_marker_shape");
    }
    assert!(saw_later);
}

#[test]
fn malformed_authoritative_records_are_reported_without_copying_or_repairing_payloads() {
    let fixture = Fixture::new();
    let (database, mut connection) = fixture.open();
    connection.execute("INSERT INTO project VALUES('p_aaaaaaaaaaaa','relative-sensitive-path','relative-sensitive-key','sensitive-name',0,0)",[]).unwrap();
    connection.execute("INSERT INTO setting VALUES('ui.appearance.v1','{\"terminal_font_size\":255,\"reduced_motion\":false}')",[]).unwrap();
    let before = authored_rows(&connection);
    let mut repeated = false;
    for _ in 0..12 {
        let report =
            maintenance::run(&mut connection, fixture.location(), validation_only()).unwrap();
        assert_eq!(report.outcome, Outcome::Completed);
        assert!(report.anomalies.len() <= 2);
        repeated |= report.anomalies.iter().any(|item| item.occurrences > 1);
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("sensitive-name") && !json.contains("relative-sensitive"));
    }
    assert!(repeated);
    assert_eq!(authored_rows(&connection), before);
    let metadata: u32 = connection
        .query_row(
            "SELECT octet_length(value) FROM setting WHERE key='maintenance.state.v1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(metadata <= 64 * 1024);
    database.integrity_check().unwrap();
}

#[test]
fn anomaly_store_has_fixed_bounds_under_repeated_malformed_input() {
    let fixture = Fixture::new();
    let (_database, mut connection) = fixture.open();
    for index in 0..300 {
        connection
            .execute(
                "INSERT INTO project VALUES(?1,?2,?2,'bad',0,?3)",
                rusqlite::params![format!("p_{index:012}"), format!("relative-{index}"), index],
            )
            .unwrap();
    }
    let before = authored_rows(&connection);
    let mut omitted = 0;
    for _ in 0..14 {
        let report =
            maintenance::run(&mut connection, fixture.location(), validation_only()).unwrap();
        assert_eq!(report.outcome, Outcome::Completed);
        assert!(report.anomalies.len() <= 128);
        omitted = omitted.max(report.unrecorded_anomalies);
    }
    assert!(omitted > 0);
    assert_eq!(authored_rows(&connection), before);
    assert_eq!(
        connection
            .query_row::<i64, _, _>("SELECT count(*) FROM setting", [], |row| row.get(0))
            .unwrap(),
        2
    );
    assert!(
        connection
            .query_row::<u32, _, _>(
                "SELECT octet_length(value) FROM setting WHERE key='maintenance.state.v1'",
                [],
                |row| row.get(0)
            )
            .unwrap()
            <= 64 * 1024
    );
}

#[test]
fn interrupted_work_is_deferred_and_does_not_claim_corruption() {
    let fixture = Fixture::new();
    let (database, mut connection) = fixture.open();
    for index in 0..10 {
        database
            .select(&fixture.project(&format!("project-{index}")))
            .unwrap();
    }
    connection
        .busy_timeout(Duration::from_millis(1234))
        .unwrap();
    let before = authored_rows(&connection);
    let report = maintenance::run(
        &mut connection,
        fixture.location(),
        Policy {
            max_vm_steps: 100,
            ..Policy::default()
        },
    )
    .unwrap();
    assert_eq!(report.outcome, Outcome::DeferredBudget);
    assert_ne!(report.quick_check, Check::Corrupt);
    assert!(report.anomalies.is_empty());
    assert_eq!(authored_rows(&connection), before);
    assert_eq!(
        connection
            .pragma_query_value::<u32, _>(None, "busy_timeout", |row| row.get(0))
            .unwrap(),
        1234
    );
    database.integrity_check().unwrap();
    assert_eq!(
        maintenance::run(&mut connection, fixture.location(), validation_only())
            .unwrap()
            .outcome,
        Outcome::Completed
    );
}

#[test]
fn pressure_and_read_only_schemas_do_not_write_job_metadata_or_create_locks() {
    let fixture = Fixture::new();
    let (_database, mut connection) = fixture.open();
    let before = authored_rows(&connection);
    let report = maintenance::run(
        &mut connection,
        fixture.location(),
        Policy {
            pressure: true,
            ..Policy::default()
        },
    )
    .unwrap();
    assert_eq!(report.outcome, Outcome::DeferredPressure);
    assert!(
        !fixture
            .location()
            .directory()
            .join("maintenance.lock")
            .exists()
    );
    connection.pragma_update(None, "user_version", 999).unwrap();
    assert_eq!(
        maintenance::run(&mut connection, fixture.location(), Policy::default())
            .unwrap()
            .outcome,
        Outcome::ReadOnly
    );
    assert_eq!(authored_rows(&connection), before);
    assert!(
        !fixture
            .location()
            .directory()
            .join("maintenance.lock")
            .exists()
    );
    let mut readonly = Connection::open_with_flags(
        fixture.location().database_path(),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        maintenance::run(&mut readonly, fixture.location(), Policy::default())
            .unwrap()
            .outcome,
        Outcome::ReadOnly
    );
}

#[test]
fn growth_limits_report_and_defer_without_erasing_history() {
    let fixture = Fixture::new();
    let (database, mut connection) = fixture.open();
    database.select(&fixture.project("retained")).unwrap();
    let before = authored_rows(&connection);
    let report = maintenance::run(
        &mut connection,
        fixture.location(),
        Policy {
            database_soft_limit_bytes: 1,
            database_hard_limit_bytes: 2,
            ..validation_only()
        },
    )
    .unwrap();
    assert_eq!(report.outcome, Outcome::DeferredCapacity);
    assert_eq!(report.growth.unwrap().capacity, Capacity::HardLimit);
    assert_eq!(authored_rows(&connection), before);
    assert_eq!(
        connection
            .query_row::<i64, _, _>("SELECT count(*) FROM setting", [], |row| row.get(0))
            .unwrap(),
        0
    );
}

#[test]
fn malformed_operational_metadata_is_bounded_and_never_silently_rewritten() {
    let fixture = Fixture::new();
    let (_database, mut connection) = fixture.open();
    let malformed = "{\"unexpected\":\"do-not-copy-sensitive-metadata\"}";
    connection
        .execute(
            "INSERT INTO setting VALUES('maintenance.state.v1',?1)",
            [malformed],
        )
        .unwrap();
    let report = maintenance::run(&mut connection, fixture.location(), validation_only()).unwrap();
    assert_eq!(report.outcome, Outcome::InvalidMetadata);
    assert!(
        !serde_json::to_string(&report)
            .unwrap()
            .contains("do-not-copy-sensitive-metadata")
    );
    assert_eq!(
        connection
            .query_row::<String, _, _>(
                "SELECT value FROM setting WHERE key='maintenance.state.v1'",
                [],
                |row| row.get(0)
            )
            .unwrap(),
        malformed
    );
}

#[test]
fn passive_checkpoint_is_admitted_by_wal_size_and_never_forces_truncation() {
    let fixture = Fixture::new();
    let (_database, mut connection) = fixture.open();
    let report = maintenance::run(
        &mut connection,
        fixture.location(),
        Policy {
            wal_checkpoint_max_bytes: 0,
            ..validation_only()
        },
    )
    .unwrap();
    assert_eq!(report.outcome, Outcome::Completed);
    assert_eq!(report.checkpoint, Checkpoint::SkippedLargeWal);
}

#[test]
fn active_transaction_and_cancelled_jobs_leave_saved_work_alone() {
    let fixture = Fixture::new();
    let (_database, mut connection) = fixture.open();
    connection.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert_eq!(
        maintenance::run(&mut connection, fixture.location(), Policy::default())
            .unwrap()
            .outcome,
        Outcome::InTransaction
    );
    connection.execute_batch("ROLLBACK").unwrap();
    let cancel = Arc::new(AtomicBool::new(true));
    assert_eq!(
        maintenance::run(
            &mut connection,
            fixture.location(),
            Policy {
                cancel: cancel.clone(),
                ..Policy::default()
            }
        )
        .unwrap()
        .outcome,
        Outcome::DeferredPressure
    );
    cancel.store(false, Ordering::Relaxed);
    assert_eq!(
        maintenance::run(&mut connection, fixture.location(), validation_only())
            .unwrap()
            .outcome,
        Outcome::Completed
    );
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

#[test]
fn process_lock_prevents_overlap_and_crashed_owner_recovers_after_finite_expiry() {
    let fixture = Fixture::new();
    let (database, mut connection) = fixture.open();
    database
        .select(&fixture.project("saved-during-crash"))
        .unwrap();
    let before = authored_rows(&connection);
    let mut child = OwnedChild(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "maintenance_child", "--nocapture"])
            .env("SHINATAMA_MAINTENANCE_TEST_ROOT", &fixture.root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while !fixture.root.join("owner-ready").exists() {
        assert!(child.0.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        maintenance::run(&mut connection, fixture.location(), validation_only())
            .unwrap()
            .outcome,
        Outcome::DeferredBusy
    );
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    // Killing a process can temporarily make local filesystem calls slow. Each
    // retry here represents a new, explicitly admitted idle tick with unchanged
    // quotas; the core never restarts its own deferred work.
    let mut report = maintenance::run(
        &mut connection,
        fixture.location(),
        Policy {
            now_millis: chrono::Utc::now().timestamp_millis() + 31_000,
            ..validation_only()
        },
    )
    .unwrap();
    for _ in 0..2 {
        if report.outcome != Outcome::DeferredBudget {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
        report = maintenance::run(
            &mut connection,
            fixture.location(),
            Policy {
                now_millis: chrono::Utc::now().timestamp_millis() + 31_000,
                ..validation_only()
            },
        )
        .unwrap();
    }
    assert_eq!(report.outcome, Outcome::Completed, "{report:?}");
    assert!(report.lease_released);
    assert_eq!(authored_rows(&connection), before);
    database.integrity_check().unwrap();
}

#[test]
fn maintenance_child() {
    let Some(root) = std::env::var_os("SHINATAMA_MAINTENANCE_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let location = location_for(&root);
    location.prepare_private_directory().unwrap();
    let connection = Connection::open(location.database_path()).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(location.directory().join("maintenance.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let mut created: FILETIME = unsafe { std::mem::zeroed() };
    let mut exited = created;
    let mut kernel = created;
    let mut user = created;
    assert_ne!(
        unsafe {
            GetProcessTimes(
                GetCurrentProcess(),
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
        },
        0
    );
    let identity = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
    let lease = serde_json::json!({"owner":uuid::Uuid::new_v4().to_string(),"pid":std::process::id(),"process_started":identity,"expires_at":chrono::Utc::now().timestamp_millis()+30_000});
    connection.execute("INSERT INTO setting VALUES('maintenance.lease.v1',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[lease.to_string()]).unwrap();
    fs::write(root.join("owner-ready"), []).unwrap();
    std::thread::sleep(Duration::from_secs(30));
}

#[test]
fn policy_overrides_cannot_raise_safety_ceilings() {
    let fixture = Fixture::new();
    let (_database, mut connection) = fixture.open();
    assert!(matches!(
        maintenance::run(
            &mut connection,
            fixture.location(),
            Policy {
                max_rows: 129,
                ..Policy::default()
            }
        ),
        Err(DatabaseError::UnsupportedConfiguration)
    ));
    assert!(matches!(
        maintenance::run(
            &mut connection,
            fixture.location(),
            Policy {
                cooperative_time: Duration::from_secs(1),
                ..Policy::default()
            }
        ),
        Err(DatabaseError::UnsupportedConfiguration)
    ));
}
