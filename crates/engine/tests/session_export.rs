#![cfg(all(windows, feature = "session-reader"))]
//! C26 native proof: one export through the real contained session helper, written by the
//! parent with the atomic writer. Synthetic archive only; the source bytes must not change.
use shinatama_engine::{
    resources::admission::{AdmissionCoordinator, PrepareRequest},
    session_detail::DetailRequest,
    session_export::ExportRequest,
    session_roots::{PreparedSessionList, RootSelector, metadata_worker_budget},
};
use shinatama_export::{Format, Mode, output, stx};
use shinatama_state::{
    database::Database,
    location::{LocationRequest, StorageLocation},
    records::{PreparedStoreRoot, RootAccess, RootKind, RootOrigin, RootScope, StoreRootRecord},
    resource_reservations::{PolicyStamp, ReservationPolicy},
};
use std::{fs, path::Path, time::{SystemTime, UNIX_EPOCH}};

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
}

#[test]
#[ignore = "explicit native proof with the contained session helper; synthetic archive only"]
fn contained_export_writes_redacted_stx_and_keeps_the_source_unchanged() {
    let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
    let fixture = parent.join(format!("shinatama-export-native-{}", uuid::Uuid::new_v4()));
    let location = StorageLocation::resolve(LocationRequest {
        local_data_directory: parent.clone(), home_override: Some(fixture.clone()),
        project_roots: vec![], synchronized_roots: vec![],
    }).unwrap();
    let database = Database::open(&location, None).unwrap();
    let budget = metadata_worker_budget();
    let stamp = PolicyStamp { version: 1, id: "session-export-local-proof-v1".into() };
    database.initialize_resource_policy(&ReservationPolicy {
        stamp: stamp.clone(), aggregate_ceiling: budget, per_slot_ceiling: budget,
        max_slots: 1, max_lease_ms: 15_000,
    }).unwrap();
    let coordinator = AdmissionCoordinator::new(1).unwrap();
    let source = concat!(
        "{\"type\":\"user\",\"uuid\":\"u1\",\"parentUuid\":null,\"sessionId\":\"export-native-id\",\"cwd\":\"C:/synthetic\",\"timestamp\":\"2026-11-09T10:00:00Z\",",
        "\"message\":{\"role\":\"user\",\"content\":\"deploy with ghp_0123456789abcdefABCDEF0123456789abcd please\"}}\n",
        "{\"type\":\"assistant\",\"uuid\":\"a1\",\"parentUuid\":\"u1\",\"sessionId\":\"export-native-id\",\"cwd\":\"C:/synthetic\",\"timestamp\":\"2026-11-09T10:00:05Z\",",
        "\"message\":{\"id\":\"msg-1\",\"role\":\"assistant\",\"model\":\"m\",\"stop_reason\":\"end_turn\",\"content\":[{\"type\":\"thinking\",\"thinking\":\"private-thought\",\"signature\":\"sig\"},{\"type\":\"text\",\"text\":\"Done.\"}]}}\n"
    ).as_bytes();
    let archive = fixture.join("archive");
    let file = archive.join("projects/fixture-project/export-native-id.jsonl");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, source).unwrap();
    let root = PreparedStoreRoot::resolve(StoreRootRecord {
        id: "export-root".into(), harness: "claude_code".into(),
        kind: RootKind::System, scope: RootScope::Additional,
        resolved_path: archive.to_string_lossy().trim_start_matches(r"\?\").into(),
        origin: RootOrigin::Configured, config_revision: 1,
        access: RootAccess::ReadOnly, available: true,
    }).unwrap();
    database.register_additional_system_root(&root).unwrap();
    let selector = RootSelector { store_root_id: "export-root".into(), configuration_revision: 1 };
    let request = ExportRequest {
        detail: DetailRequest { root: selector, native_id: "export-native-id".into(), cursor: None, limit: 1 },
        format: Format::Stx, mode: Mode::Full, with_attachments: false, thread: None, home: None,
    };
    let prepared = PreparedSessionList::resolve_export(&database, &location, request).unwrap();
    let at = now();
    let revision = database.resource_reservations().unwrap().unwrap().revision;
    let workload = coordinator.prepare(&database, &stamp, revision, PrepareRequest {
        budget, process_commit_bytes: 768 * 1024 * 1024, now_ms: at, lease_expires_at_ms: at + 15_000,
    }).unwrap();
    let receipt = prepared.run_contained(&database,
        Path::new(env!("CARGO_BIN_EXE_shinatama-session-helper")), &location, workload);
    assert!(!receipt.cleanup_retained(), "worker cleanup retained: {:?}", receipt.issue);
    assert!(receipt.issue.is_none(), "contained export refused: {:?}", receipt.issue);
    let reply = receipt.export().unwrap();
    assert!(reply.issue.is_none(), "export issue: {:?}", reply.issue);
    let payload = reply.payload.as_ref().unwrap();
    assert!(!payload.content.contains("ghp_0123"));
    assert!(!payload.content.contains("private-thought"));
    let out = fixture.join("out.stx.jsonl");
    output::write_export(&out, output::Overwrite::Refuse, payload.content.as_bytes(), &[]).unwrap();
    let document = stx::read(&fs::read(&out).unwrap()).unwrap();
    assert_eq!(document.header.source.native_session_id, "export-native-id");
    assert!(document.items().any(|item| item.text.contains("[REDACTED:github_token]")));
    assert_eq!(document.header.omitted.reasoning, 1);
    assert_eq!(fs::read(&file).unwrap(), source, "the native source must not change");
    assert_eq!(database.resource_reservations().unwrap().unwrap().reserved_total().unwrap().memory_bytes, 0);
    drop(database);
    fs::remove_dir_all(fixture).unwrap();
}
