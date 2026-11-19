#![cfg(all(windows, feature = "session-reader"))]
//! C26 native proof: one export through the real contained session helper, framed over its pipe,
//! staged and committed by the parent with the atomic writer. Synthetic archive only; the source
//! bytes must not change.
use shinatama_engine::{
    resources::admission::{AdmissionCoordinator, PrepareRequest},
    session_detail::{DetailCursor, DetailRequest},
    session_find::FindRequest,
    session_export::{ExportRequest, commit_export},
    export_frames::Progress,
    session_roots::{Framed, PreparedSessionList, RootSelector, SessionReadIssue, metadata_worker_budget},
};
use shinatama_export::{Format, Mode, output, stx};
use shinatama_state::{
    database::Database,
    location::{LocationRequest, StorageLocation},
    records::{PreparedStoreRoot, RootAccess, RootKind, RootOrigin, RootScope, StoreRootRecord},
    resource_reservations::{PolicyStamp, ReservationPolicy},
};
use std::{
    cell::{Cell, RefCell},
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// One contained worker at a time: the admission coordinator is a process-wide singleton.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
}

/// What one contained export left behind.
struct Exported {
    fixture: PathBuf,
    out: PathBuf,
    content_bytes: u64,
    receipt_bytes: u64,
    source_unchanged: bool,
}

/// Runs one export of `source` (a Claude Code session file) through the real helper.
fn export_through_helper(source: &[u8], format: Format, file_name: &str) -> Exported {
    let _only = ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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
        max_slots: 1, max_lease_ms: 300_000,
    }).unwrap();
    let coordinator = AdmissionCoordinator::new(1).unwrap();
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
        detail: DetailRequest { root: selector, native_id: "export-native-id".into(), cursor: None, limit: 1, overview_only: false },
        format, mode: Mode::Full, with_attachments: false, thread: None, home: None,
    };
    let prepared = PreparedSessionList::resolve_export(&database, &location, request).unwrap();
    let at = now();
    let revision = database.resource_reservations().unwrap().unwrap().revision;
    let workload = coordinator.prepare(&database, &stamp, revision, PrepareRequest {
        budget, process_commit_bytes: 768 * 1024 * 1024, now_ms: at, lease_expires_at_ms: at + 15_000,
    }).unwrap();
    let out = fixture.join(file_name);
    let mut staged = Some(output::stage(&out, output::Overwrite::Refuse).unwrap());
    let mut sink = |bytes: &[u8]| staged.as_mut().unwrap().write(bytes);
    let receipt = prepared.run_contained_export(&database,
        Path::new(env!("CARGO_BIN_EXE_shinatama-session-helper")), &location, workload, &mut sink);
    assert!(!receipt.cleanup_retained(), "worker cleanup retained: {:?}", receipt.issue);
    assert!(receipt.issue.is_none(), "contained export refused: {:?}", receipt.issue);
    let reply = receipt.export().unwrap();
    assert!(reply.issue.is_none(), "export issue: {:?}", reply.issue);
    let payload = reply.payload.as_ref().unwrap();
    let written = commit_export(staged.take().unwrap(), payload).unwrap();
    assert_eq!(fs::read_dir(&fixture).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().starts_with('.')).count(), 0,
        "no staged file is left beside the output");
    assert_eq!(database.resource_reservations().unwrap().unwrap().reserved_total().unwrap().memory_bytes, 0);
    let exported = Exported {
        content_bytes: payload.report.content_bytes,
        receipt_bytes: written.bytes,
        source_unchanged: fs::read(&file).unwrap() == source,
        fixture: fixture.clone(),
        out,
    };
    drop(database);
    exported
}

#[test]
#[ignore = "explicit native proof with the contained session helper; synthetic archive only"]
fn contained_export_writes_redacted_stx_and_keeps_the_source_unchanged() {
    let source = concat!(
        "{\"type\":\"user\",\"uuid\":\"u1\",\"parentUuid\":null,\"sessionId\":\"export-native-id\",\"cwd\":\"C:/synthetic\",\"timestamp\":\"2026-11-09T10:00:00Z\",",
        "\"message\":{\"role\":\"user\",\"content\":\"deploy with ghp_0123456789abcdefABCDEF0123456789abcd please\"}}\n",
        "{\"type\":\"assistant\",\"uuid\":\"a1\",\"parentUuid\":\"u1\",\"sessionId\":\"export-native-id\",\"cwd\":\"C:/synthetic\",\"timestamp\":\"2026-11-09T10:00:05Z\",",
        "\"message\":{\"id\":\"msg-1\",\"role\":\"assistant\",\"model\":\"m\",\"stop_reason\":\"end_turn\",\"content\":[{\"type\":\"thinking\",\"thinking\":\"private-thought\",\"signature\":\"sig\"},{\"type\":\"text\",\"text\":\"Done.\"}]}}\n"
    ).as_bytes();
    let exported = export_through_helper(source, Format::Stx, "out.stx.jsonl");
    let content = fs::read_to_string(&exported.out).unwrap();
    assert_eq!(exported.receipt_bytes, content.len() as u64);
    assert_eq!(exported.content_bytes, content.len() as u64);
    assert!(!content.contains("ghp_0123"));
    assert!(!content.contains("private-thought"));
    let document = stx::read(content.as_bytes()).unwrap();
    assert_eq!(document.header.source.native_session_id, "export-native-id");
    assert!(document.items().any(|item| item.text.contains("[REDACTED:github_token]")));
    assert_eq!(document.header.omitted.reasoning, 1);
    assert!(exported.source_unchanged, "the native source must not change");
    fs::remove_dir_all(exported.fixture).unwrap();
}

#[test]
#[ignore = "explicit native proof with the contained session helper; synthetic archive only"]
fn contained_export_streams_an_output_past_the_old_24_mib_content_bound() {
    // One tool result of 24.5 MiB: the worker's reply used to be one bounded document and the
    // content stopped at 24 MiB. Frames of 1 MiB carry it, and the parent stages and commits it.
    let big = format!("{}END-MARKER", "x".repeat(24 * 1024 * 1024 + 512 * 1024));
    let rows = [
        serde_json::json!({"type":"user","uuid":"u1","parentUuid":null,"sessionId":"export-native-id","cwd":"C:/synthetic","message":{"role":"user","content":"read it"}}),
        serde_json::json!({"type":"assistant","uuid":"a1","parentUuid":"u1","sessionId":"export-native-id","message":{"id":"m1","role":"assistant","model":"m","stop_reason":"tool_use","content":[{"type":"tool_use","id":"c1","name":"Read","input":{"file_path":"C:/synthetic/big.txt"}}]}}),
        serde_json::json!({"type":"user","uuid":"r1","parentUuid":"a1","sessionId":"export-native-id","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"c1","content":big}]}}),
    ];
    let mut source = String::new();
    for row in rows {
        source.push_str(&serde_json::to_string(&row).unwrap());
        source.push('\n');
    }
    let exported = export_through_helper(source.as_bytes(), Format::Md, "big.md");
    let bytes = fs::metadata(&exported.out).unwrap().len();
    assert!(bytes > 24 * 1024 * 1024, "{bytes}");
    assert_eq!(exported.receipt_bytes, bytes);
    assert_eq!(exported.content_bytes, bytes);
    let content = fs::read_to_string(&exported.out).unwrap();
    assert!(content.contains("END-MARKER"));
    assert!(exported.source_unchanged);
    fs::remove_dir_all(exported.fixture).unwrap();
}

/// A session of `results` tool results of `bytes` each, as one Claude Code file.
fn big_session(results: usize, bytes: usize) -> String {
    let mut out = String::new();
    let mut parent = "u1".to_owned();
    out.push_str(&serde_json::to_string(&serde_json::json!({"type":"user","uuid":"u1","parentUuid":null,"sessionId":"export-native-id","cwd":"C:/synthetic","message":{"role":"user","content":"read them"}})).unwrap());
    out.push('\n');
    for index in 0..results {
        for row in [
            serde_json::json!({"type":"assistant","uuid":format!("a{index}"),"parentUuid":parent,"sessionId":"export-native-id","message":{"id":format!("m{index}"),"role":"assistant","model":"m","stop_reason":"tool_use","content":[{"type":"tool_use","id":format!("c{index}"),"name":"Read","input":{"file_path":"C:/synthetic/big.txt"}}]}}),
            serde_json::json!({"type":"user","uuid":format!("r{index}"),"parentUuid":format!("a{index}"),"sessionId":"export-native-id","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":format!("c{index}"),"content":format!("{index} {}", "y".repeat(bytes))}]}}),
        ] {
            out.push_str(&serde_json::to_string(&row).unwrap());
            out.push('\n');
        }
        parent = format!("r{index}");
    }
    out
}

/// What a framed export left behind.
struct Framed2 {
    issue: Option<SessionReadIssue>,
    retained: bool,
    progress: Vec<(Duration, Progress)>,
    elapsed: Duration,
    left_beside_output: Vec<String>,
    memory_reserved: u64,
    out_exists: bool,
    file_bytes: u64,
}

/// One export through the real helper with a progress listener, stopping (`cancel_after` body bytes)
/// when asked; the staged file is dropped unless the export completes and commits.
fn framed_export(source: &str, format: Format, cancel_after: Option<u64>) -> Framed2 {
    let _only = ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
    let fixture = parent.join(format!("shinatama-export-progress-{}", uuid::Uuid::new_v4()));
    let location = StorageLocation::resolve(LocationRequest {
        local_data_directory: parent.clone(), home_override: Some(fixture.clone()),
        project_roots: vec![], synchronized_roots: vec![],
    }).unwrap();
    let database = Database::open(&location, None).unwrap();
    let budget = metadata_worker_budget();
    let stamp = PolicyStamp { version: 1, id: "session-export-progress-v1".into() };
    database.initialize_resource_policy(&ReservationPolicy {
        stamp: stamp.clone(), aggregate_ceiling: budget, per_slot_ceiling: budget,
        max_slots: 1, max_lease_ms: 300_000,
    }).unwrap();
    let coordinator = AdmissionCoordinator::new(1).unwrap();
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
        detail: DetailRequest { root: selector, native_id: "export-native-id".into(), cursor: None, limit: 1, overview_only: false },
        format, mode: Mode::Full, with_attachments: false, thread: None, home: None,
    };
    let prepared = PreparedSessionList::resolve_export(&database, &location, request).unwrap();
    let file_bytes = prepared.file_bytes();
    let at = now();
    let revision = database.resource_reservations().unwrap().unwrap().revision;
    let workload = coordinator.prepare(&database, &stamp, revision, PrepareRequest {
        budget, process_commit_bytes: 768 * 1024 * 1024, now_ms: at, lease_expires_at_ms: at + 15_000,
    }).unwrap();
    let out = fixture.join("out.md");
    let staged = RefCell::new(Some(output::stage(&out, output::Overwrite::Refuse).unwrap()));
    let written = Cell::new(0u64);
    let events = RefCell::new(Vec::new());
    let began = Instant::now();
    let mut body = |bytes: &[u8]| {
        written.set(written.get() + bytes.len() as u64);
        staged.borrow_mut().as_mut().unwrap().write(bytes)
    };
    let mut progress = |value: Progress| events.borrow_mut().push((began.elapsed(), value));
    let cancel = || cancel_after.is_some_and(|limit| written.get() >= limit);
    let read = prepared.run_contained_framed(&database,
        Path::new(env!("CARGO_BIN_EXE_shinatama-session-helper")), &location, workload,
        Framed { body: &mut body, progress: &mut progress, cancel: &cancel });
    let elapsed = began.elapsed();
    let issue = read.issue;
    if issue.is_none() {
        let payload = read.export().unwrap().payload.as_ref().expect("a payload");
        commit_export(staged.borrow_mut().take().unwrap(), payload).unwrap();
    }
    // The staged file is dropped, committed or not.
    drop(staged.borrow_mut().take());
    let result = Framed2 {
        issue,
        retained: read.cleanup_retained(),
        progress: events.into_inner(),
        elapsed,
        left_beside_output: fs::read_dir(&fixture).unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with('.')).collect(),
        memory_reserved: database.resource_reservations().unwrap().unwrap().reserved_total().unwrap().memory_bytes,
        out_exists: out.exists(),
        file_bytes,
    };
    drop(read);
    drop(database);
    fs::remove_dir_all(fixture).unwrap();
    result
}

#[test]
#[ignore = "explicit native proof with the contained session helper; synthetic archive only"]
fn contained_export_reports_progress_at_a_bounded_rate() {
    // 20 results of 1 MiB: seconds of work in a debug build, so progress frames are due, and
    // well inside the 10 s wall of a file this size.
    let run = framed_export(&big_session(20, 1024 * 1024), Format::Md, None);
    assert!(run.issue.is_none(), "{:?}", run.issue);
    assert!(run.out_exists && run.left_beside_output.is_empty(), "{} {:?}", run.out_exists, run.left_beside_output);
    assert!(run.file_bytes > 20 * 1024 * 1024, "the file size is known as context: {}", run.file_bytes);
    assert!(!run.progress.is_empty(), "no progress frame in {:?}", run.elapsed);
    let allowed = (run.elapsed.as_millis() / 250) as usize + 1;
    assert!(run.progress.len() <= allowed, "{} frames in {:?}", run.progress.len(), run.elapsed);
    // Counts only grow, never exceed the session (41 entries: the prompt and 20 call/result pairs).
    assert!(run.progress.windows(2).all(|pair| pair[0].1.entries <= pair[1].1.entries && pair[0].1.bytes <= pair[1].1.bytes));
    assert!(run.progress.iter().all(|(_, p)| p.entries <= 41 && p.hits == 0));
    assert_eq!(run.memory_reserved, 0);
}

#[test]
#[ignore = "explicit native proof with the contained session helper; synthetic archive only"]
fn cancelling_an_export_midway_stops_the_worker_and_leaves_no_file() {
    let run = framed_export(&big_session(20, 1024 * 1024), Format::Md, Some(3 * 1024 * 1024));
    assert_eq!(run.issue, Some(SessionReadIssue::Cancelled));
    assert!(!run.retained, "the worker's cleanup is confirmed");
    assert!(!run.out_exists, "nothing was written to the destination");
    assert!(run.left_beside_output.is_empty(), "the staged file was removed: {:?}", run.left_beside_output);
    assert_eq!(run.memory_reserved, 0, "the reservation was released");
    // It stopped long before an uncancelled export of the same session finishes.
    assert!(run.elapsed < Duration::from_secs(8), "{:?}", run.elapsed);
}

/// What a search through the real helper left behind.
struct Searched {
    issue: Option<SessionReadIssue>,
    retained: bool,
    progress: Vec<(Duration, Progress)>,
    total_hits: Option<usize>,
    memory_reserved: u64,
}

/// One search of `query` through the real helper; `cancel_on_progress` stops it at its first progress
/// frame.
fn framed_search(source: &str, query: &str, cancel_on_progress: bool) -> Searched {
    let _only = ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
    let fixture = parent.join(format!("shinatama-search-progress-{}", uuid::Uuid::new_v4()));
    let location = StorageLocation::resolve(LocationRequest {
        local_data_directory: parent.clone(), home_override: Some(fixture.clone()),
        project_roots: vec![], synchronized_roots: vec![],
    }).unwrap();
    let database = Database::open(&location, None).unwrap();
    let budget = metadata_worker_budget();
    let stamp = PolicyStamp { version: 1, id: "session-search-progress-v1".into() };
    database.initialize_resource_policy(&ReservationPolicy {
        stamp: stamp.clone(), aggregate_ceiling: budget, per_slot_ceiling: budget,
        max_slots: 1, max_lease_ms: 300_000,
    }).unwrap();
    let coordinator = AdmissionCoordinator::new(1).unwrap();
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
    let workload = |database: &Database| {
        let at = now();
        let revision = database.resource_reservations().unwrap().unwrap().revision;
        coordinator.prepare(database, &stamp, revision, PrepareRequest {
            budget, process_commit_bytes: 768 * 1024 * 1024, now_ms: at, lease_expires_at_ms: at + 15_000,
        }).unwrap()
    };
    let helper = Path::new(env!("CARGO_BIN_EXE_shinatama-session-helper"));
    // The visible page first: its fingerprint pins the search.
    let detail = DetailRequest { root: selector, native_id: "export-native-id".into(), cursor: None, limit: 50, overview_only: false };
    let prepared = PreparedSessionList::resolve_detail(&database, &location, detail.clone()).unwrap();
    let read = prepared.run_contained(&database, helper, &location, workload(&database));
    assert!(read.issue.is_none(), "{:?}", read.issue);
    let fingerprint = read.detail().unwrap().page.as_ref().unwrap().fingerprint.clone();
    drop(read);
    let mut pinned = detail;
    pinned.cursor = Some(DetailCursor { offset: 0, fingerprint });
    let request = FindRequest { detail: pinned, query: query.into(), cursor: None };
    let prepared = PreparedSessionList::resolve_find(&database, &location, request).unwrap();
    let began = Instant::now();
    let events = RefCell::new(Vec::new());
    let seen = Cell::new(false);
    let mut body = |_: &[u8]| -> std::io::Result<()> { panic!("a search has no body") };
    let mut progress = |value: Progress| {
        seen.set(true);
        events.borrow_mut().push((began.elapsed(), value));
    };
    let cancel = || cancel_on_progress && seen.get();
    let read = prepared.run_contained_framed(&database, helper, &location, workload(&database),
        Framed { body: &mut body, progress: &mut progress, cancel: &cancel });
    let result = Searched {
        issue: read.issue,
        retained: read.cleanup_retained(),
        progress: events.into_inner(),
        total_hits: read.find().and_then(|reply| reply.page.as_ref()).map(|page| page.total_hits),
        memory_reserved: database.resource_reservations().unwrap().unwrap().reserved_total().unwrap().memory_bytes,
    };
    drop(read);
    drop(database);
    fs::remove_dir_all(fixture).unwrap();
    result
}

#[test]
#[ignore = "explicit native proof with the contained session helper; synthetic archive only"]
fn contained_search_reports_progress_with_hits_so_far_and_cancels() {
    // The needle is in the prompt and every result, so hits grow as the entries are read.
    let source = big_session(20, 1024 * 1024).replace("\"content\":\"", "\"content\":\"NEEDLE ");
    let done = framed_search(&source, "needle", false);
    assert!(done.issue.is_none(), "{:?}", done.issue);
    assert_eq!(done.total_hits, Some(21), "the prompt and the 20 results");
    assert!(!done.progress.is_empty(), "no progress frame");
    assert!(done.progress.windows(2).all(|pair| pair[0].1.entries <= pair[1].1.entries && pair[0].1.hits <= pair[1].1.hits));
    assert!(done.progress.iter().all(|(_, p)| p.entries <= 41 && p.hits <= 21 && p.bytes == 0));
    assert_eq!(done.memory_reserved, 0);
    let stopped = framed_search(&source, "needle", true);
    assert_eq!(stopped.issue, Some(SessionReadIssue::Cancelled));
    assert!(!stopped.retained && stopped.total_hits.is_none());
    assert_eq!(stopped.memory_reserved, 0);
}
