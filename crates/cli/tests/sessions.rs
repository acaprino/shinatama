#![cfg(windows)]
//! C26.3 native proof: the real `shinatama` binary lists and exports one synthetic session
//! through the contained helper, with stable JSON and exit codes (R36-R37).
use shinatama_state::{
    database::Database,
    location::{LocationRequest, StorageLocation},
    records::{PreparedStoreRoot, RootAccess, RootKind, RootOrigin, RootScope, StoreRootRecord},
    resource_reservations::{Budget, PolicyStamp, ReservationPolicy},
};
use std::{fs, path::PathBuf, process::Command};

const SOURCE: &str = concat!(
    r#"{"type":"user","uuid":"u1","parentUuid":null,"sessionId":"cli-export-id","cwd":"C:/fixture/project","timestamp":"2026-11-09T10:00:00Z","message":{"role":"user","content":"use sk-ant-api03-AbCdEf0123456789AbCdEf0123456789xyz"}}"#,
    "\n",
    r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","sessionId":"cli-export-id","message":{"id":"m1","role":"assistant","model":"m","stop_reason":"end_turn","content":[{"type":"thinking","thinking":"hidden-thought","signature":"s"},{"type":"text","text":"Okay."}]}}"#,
    "\n"
);

fn cli(home: &PathBuf, args: &[&str]) -> (i32, String) {
    let cwd = home.parent().unwrap().join("cwd");
    let output = Command::new(env!("CARGO_BIN_EXE_shinatama"))
        .args(args)
        .env("SHINATAMA_HOME", home)
        .current_dir(&cwd)
        .output()
        .unwrap();
    (output.status.code().unwrap_or(-1), String::from_utf8_lossy(&output.stdout).into_owned())
}

#[test]
#[ignore = "native proof: needs shinatama-session-helper.exe built next to the CLI"]
fn cli_lists_and_exports_through_the_contained_worker() {
    let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
    let base = parent.join(format!("shinatama-cli-proof-{}", uuid_like()));
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(base.join("cwd")).unwrap();
    // First run: no database yet, so the CLI asks for the desktop and creates nothing.
    let (code, out) = cli(&home, &["sessions", "--json"]);
    assert_eq!(code, 3, "{out}");
    assert!(out.contains("\"needs_user\""));
    // SAFETY: single-threaded test setup before any other thread reads the environment.
    unsafe { std::env::set_var("SHINATAMA_HOME", &home) };
    let location = StorageLocation::resolve(LocationRequest::from_environment(vec![base.join("cwd")]).unwrap()).unwrap();
    location.prepare_private_directory().unwrap();
    let database = Database::open(&location, None).unwrap();
    let budget = Budget { memory_bytes: 1024 * 1024 * 1024, processes: 8, cpu_basis_points: 1000, queue_bytes: 1024 * 1024, queue_items: 4 };
    database
        .initialize_resource_policy(&ReservationPolicy {
            stamp: PolicyStamp { version: 1, id: "cli-proof-v1".into() },
            aggregate_ceiling: budget,
            per_slot_ceiling: budget,
            max_slots: 2,
            max_lease_ms: 15_000,
        })
        .unwrap();
    let archive = home.join("archive");
    let file = archive.join("projects/fixture/cli-export-id.jsonl");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, SOURCE).unwrap();
    let root = PreparedStoreRoot::resolve(StoreRootRecord {
        id: "cli-root".into(),
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
    drop(database);

    let (code, out) = cli(&home, &["sessions", "--agent", "claude", "--json"]);
    assert_eq!(code, 0, "{out}");
    let listed: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(listed["schema"], "shinatama.cli.sessions/1");
    assert_eq!(listed["sessions"][0]["session"], "claude_code:cli-export-id");

    let target = home.join("out.stx.jsonl");
    let target_text = target.to_string_lossy().into_owned();
    let (code, out) = cli(&home, &["export", "claude:cli-export-id", "-o", &target_text, "--mode", "portable", "--json"]);
    assert_eq!(code, 0, "{out}");
    let exported = fs::read_to_string(&target).unwrap();
    assert!(exported.starts_with("{\"format\":\"shinatama.stx\",\"version\":1"));
    assert!(!exported.contains("sk-ant-api03") && !exported.contains("hidden-thought"));
    let receipt: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(receipt["report"]["redactions"]["anthropic_key"].as_u64().unwrap() >= 1, "body and derived title are both redacted");

    // Existing file without --replace: refused, unchanged.
    let (code, out) = cli(&home, &["export", "claude:cli-export-id", "-o", &target_text, "--json"]);
    assert_eq!(code, 2, "{out}");
    assert_eq!(fs::read_to_string(&target).unwrap(), exported);
    // Markdown to stdout.
    let (code, out) = cli(&home, &["export", "claude:cli-export-id", "--format", "md"]);
    assert_eq!(code, 0);
    assert!(out.contains("## User") && out.contains("[REDACTED:anthropic_key]"));
    // Native is not offered, resume/hop are desktop-only.
    assert_eq!(cli(&home, &["export", "claude:cli-export-id", "--format", "native"]).0, 2);
    assert_eq!(cli(&home, &["resume", "x"]).0, 2);
    assert_eq!(cli(&home, &["export", "claude:missing-id"]).0, 2);
    assert_eq!(fs::read_to_string(&file).unwrap(), SOURCE, "the native source must not change");
    fs::remove_dir_all(&base).unwrap();
}

fn uuid_like() -> String {
    format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())
}
