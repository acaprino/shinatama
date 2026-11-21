#![cfg(all(windows, feature = "session-reader"))]
//! Native reader proofs using only tiny synthetic archives.
use shinatama_engine::{
    resources::admission::{AdmissionCoordinator, PrepareRequest},
    session_detail::DetailRequest,
    session_roots::{PreparedSessionList, RootSelector, SessionReadIssue, metadata_worker_budget},
};
use shinatama_state::{
    database::Database,
    location::{LocationRequest, StorageLocation},
    records::{PreparedStoreRoot, RootAccess, RootKind, RootOrigin, RootScope, StoreRootRecord},
    resource_reservations::{PolicyStamp, ReservationPolicy},
};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

// The admission pool is process-wide; these proofs each reserve its only reader slot.
static NATIVE_READER: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[test]
#[ignore = "explicit contained resume-overview proof while another synthetic session is writing"]
fn resume_overview_succeeds_on_first_attempt_while_another_session_writes() {
    use std::{
        io::Write,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        time::Duration,
    };
    let _reader = NATIVE_READER.lock().unwrap();

    let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
    let fixture = parent.join(format!("shinatama-session-native-{}", uuid::Uuid::new_v4()));
    let location = StorageLocation::resolve(LocationRequest {
        local_data_directory: parent.clone(),
        home_override: Some(fixture.clone()),
        project_roots: vec![],
        synchronized_roots: vec![],
    })
    .unwrap();
    let database = Database::open(&location, None).unwrap();
    let budget = metadata_worker_budget();
    let stamp = PolicyStamp {
        version: 1,
        id: "session-resume-concurrent-proof-v1".into(),
    };
    database
        .initialize_resource_policy(&ReservationPolicy {
            stamp: stamp.clone(),
            aggregate_ceiling: budget,
            per_slot_ceiling: budget,
            max_slots: 1,
            max_lease_ms: 15_000,
        })
        .unwrap();
    let coordinator = AdmissionCoordinator::new(1).unwrap();
    let native_id = "01a17e07-93f7-7a73-85c7-9f0205836600";
    for harness in ["claude_code", "codex"] {
        let archive = fixture.join(harness);
        let (relative, body) = if harness == "codex" {
            (
                format!("sessions/2026/11/16/rollout-2026-11-16T12-00-00-{native_id}.jsonl"),
                serde_json::json!({"type":"session_meta","payload":{"id":native_id,"cwd":"C:\\synthetic-project"}}),
            )
        } else {
            (
                format!("projects/fixture-project/{native_id}.jsonl"),
                serde_json::json!({"type":"user","sessionId":native_id,"cwd":"C:\\synthetic-project","message":{"role":"user","content":"hello"}}),
            )
        };
        let source = format!("{body}\n");
        let file = archive.join(&relative);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, &source).unwrap();
        let other =
            archive.join(relative.replace(native_id, "01a17e07-93f7-7a73-85c7-9f0205836601"));
        fs::write(
            &other,
            source.replace(native_id, "01a17e07-93f7-7a73-85c7-9f0205836601"),
        )
        .unwrap();
        let root = PreparedStoreRoot::resolve(StoreRootRecord {
            id: format!("concurrent-root-{harness}"),
            harness: harness.into(),
            kind: RootKind::System,
            scope: RootScope::Additional,
            resolved_path: archive.to_string_lossy().trim_start_matches(r"\\?\").into(),
            origin: RootOrigin::Configured,
            config_revision: 1,
            access: RootAccess::ReadOnly,
            available: true,
        })
        .unwrap();
        database.register_additional_system_root(&root).unwrap();
        let selector = RootSelector {
            store_root_id: root.record().id.clone(),
            configuration_revision: 1,
        };
        let prepared = PreparedSessionList::resolve_detail(
            &database,
            &location,
            DetailRequest {
                root: selector.clone(),
                native_id: native_id.into(),
                cursor: None,
                limit: 1,
                overview_only: true,
            },
        )
        .unwrap();
        let at = now();
        let revision = database.resource_reservations().unwrap().unwrap().revision;
        let workload = coordinator
            .prepare(
                &database,
                &stamp,
                revision,
                PrepareRequest {
                    budget,
                    process_commit_bytes: 768 * 1024 * 1024,
                    now_ms: at,
                    lease_expires_at_ms: at + 15_000,
                },
            )
            .unwrap();
        let writing = Arc::new(AtomicBool::new(true));
        let writes = Arc::new(AtomicUsize::new(0));
        let flag = Arc::clone(&writing);
        let count = Arc::clone(&writes);
        let writer = std::thread::spawn(move || {
            let mut output = fs::OpenOptions::new().append(true).open(other).unwrap();
            while flag.load(Ordering::Acquire) {
                output.write_all(b"\n").unwrap();
                count.fetch_add(1, Ordering::Release);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        // Exactly one attempt, through the same contained overview used by Resume.
        let receipt = prepared.run_contained(
            &database,
            Path::new(env!("CARGO_BIN_EXE_shinatama-session-helper")),
            &location,
            workload,
        );
        writing.store(false, Ordering::Release);
        writer.join().unwrap();
        assert!(
            writes.load(Ordering::Acquire) > 1,
            "concurrent writer did not run"
        );
        assert!(!receipt.cleanup_retained());
        assert!(receipt.issue.is_none(), "{harness}: {:?}", receipt.issue);
        let reply = receipt.detail().unwrap();
        assert!(reply.issue.is_none(), "{harness}: {:?}", reply.issue);
        let page = reply.page.as_ref().unwrap();
        assert_eq!(page.root, selector);
        assert_eq!(page.native_id, native_id);
        assert_eq!(
            page.overview.working_directory.as_deref(),
            Some("C:\\synthetic-project")
        );
        assert_eq!(fs::read(&file).unwrap(), source.as_bytes());
        assert_eq!(
            database
                .resource_reservations()
                .unwrap()
                .unwrap()
                .reserved_total()
                .unwrap()
                .memory_bytes,
            0
        );
    }
    drop(database);
    assert_eq!(fixture.parent(), Some(parent.as_path()));
    assert!(
        fixture
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("shinatama-session-native-")
    );
    fs::remove_dir_all(fixture).unwrap();
}

#[test]
#[ignore = "explicit native proof with the packaged contained session helper; synthetic archives only"]
fn contained_reader_preserves_explicit_root_identity_and_source_bytes() {
    let _reader = NATIVE_READER.lock().unwrap();
    let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
    let fixture = parent.join(format!("shinatama-session-native-{}", uuid::Uuid::new_v4()));
    let location = StorageLocation::resolve(LocationRequest {
        local_data_directory: parent.clone(),
        home_override: Some(fixture.clone()),
        project_roots: vec![],
        synchronized_roots: vec![],
    })
    .unwrap();
    let database = Database::open(&location, None).unwrap();
    let budget = metadata_worker_budget();
    let stamp = PolicyStamp {
        version: 1,
        id: "session-reader-local-proof-v1".into(),
    };
    database
        .initialize_resource_policy(&ReservationPolicy {
            stamp: stamp.clone(),
            aggregate_ceiling: budget,
            per_slot_ceiling: budget,
            max_slots: 1,
            max_lease_ms: 15_000,
        })
        .unwrap();
    let coordinator = AdmissionCoordinator::new(1).unwrap();
    let source = b"{\"sessionId\":\"same-native-id\",\"cwd\":\"C:\\\\synthetic-project\"}\n";
    for index in 0..2 {
        let archive = fixture.join(format!("archive-{index}"));
        let file = archive.join("projects/fixture-project/same-native-id.jsonl");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, source).unwrap();
        let root = PreparedStoreRoot::resolve(StoreRootRecord {
            id: format!("synthetic-root-{index}"),
            harness: "claude_code".into(),
            kind: RootKind::System,
            scope: RootScope::Additional,
            resolved_path: archive.to_string_lossy().trim_start_matches(r"\\?\").into(),
            origin: RootOrigin::Configured,
            config_revision: 1,
            access: RootAccess::ReadOnly,
            available: true,
        })
        .unwrap();
        database.register_additional_system_root(&root).unwrap();
        let selector = RootSelector {
            store_root_id: root.record().id.clone(),
            configuration_revision: 1,
        };
        let prepared =
            PreparedSessionList::resolve(&database, &location, &selector, None, 50, true).unwrap();
        let at = now();
        let revision = database.resource_reservations().unwrap().unwrap().revision;
        let workload = coordinator
            .prepare(
                &database,
                &stamp,
                revision,
                PrepareRequest {
                    budget,
                    process_commit_bytes: 768 * 1024 * 1024,
                    now_ms: at,
                    lease_expires_at_ms: at + 15_000,
                },
            )
            .unwrap();
        let receipt = prepared.run_contained(
            &database,
            Path::new(env!("CARGO_BIN_EXE_shinatama-session-helper")),
            &location,
            workload,
        );
        assert!(
            !receipt.cleanup_retained(),
            "reader cleanup remains reserved: {:?}",
            receipt.issue
        );
        assert!(
            receipt.issue.is_none(),
            "contained reader refused: {:?}",
            receipt.issue
        );
        let page = receipt.page().unwrap();
        assert_eq!(page.root, selector);
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.rows[0].native_id, "same-native-id");
        assert_eq!(page.rows[0].title, None);
        assert_eq!(page.rows[0].turns, None);
        assert_eq!(fs::read(&file).unwrap(), source);
        assert_eq!(
            database
                .resource_reservations()
                .unwrap()
                .unwrap()
                .reserved_total()
                .unwrap()
                .memory_bytes,
            0
        );
        let stale = RootSelector {
            configuration_revision: 2,
            ..selector
        };
        assert!(matches!(
            PreparedSessionList::resolve(&database, &location, &stale, None, 50, true),
            Err(SessionReadIssue::RootChanged)
        ));
    }
    assert!(matches!(
        PreparedSessionList::resolve(
            &database,
            &location,
            &RootSelector {
                store_root_id: "missing-explicit-root".into(),
                configuration_revision: 1
            },
            None,
            50,
            true
        ),
        Err(SessionReadIssue::RootUnavailable)
    ));
    drop(database);
    assert_eq!(fixture.parent(), Some(parent.as_path()));
    assert!(
        fixture
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("shinatama-session-native-")
    );
    fs::remove_dir_all(fixture).unwrap();
}
