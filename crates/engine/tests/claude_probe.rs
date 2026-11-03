#![cfg(windows)]
use shinatama_adapter::{
    AgentId, CapabilitySupport, ObservedVersion, VersionUnknown,
    catalog::{CatalogSnapshot, ProgramCandidate, ProgramKind},
    pe_version::PeVersionMetadata,
};
use shinatama_engine::{
    claude_probe::{
        Authentication, PreparedClaudeProbe, ProbeIssue, discover_claude, parse_version,
        probe_budget, select_claude,
    },
    resources::admission::AdmissionCoordinator,
};
use shinatama_state::{
    database::Database,
    location::{LocationRequest, StorageLocation},
    resource_reservations::{PolicyStamp, ReservationPolicy},
};
use std::{fs, path::PathBuf};

struct Fixture {
    root: PathBuf,
    preserve: bool,
}
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("shinatama-claude-probe-{}", uuid::Uuid::new_v4()));
        Self {
            root,
            preserve: false,
        }
    }
    fn location(&self) -> StorageLocation {
        StorageLocation::resolve(LocationRequest {
            local_data_directory: std::env::temp_dir(),
            home_override: Some(self.root.clone()),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        })
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.preserve {
            return;
        }
        let temporary = fs::canonicalize(std::env::temp_dir()).unwrap();
        assert_eq!(self.root.parent(), Some(temporary.as_path()));
        assert!(
            self.root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("shinatama-claude-probe-")
        );
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn candidate(path: PathBuf, kind: ProgramKind) -> ProgramCandidate {
    ProgramCandidate {
        agent: AgentId::new("claude_code").unwrap(),
        path,
        kind,
        version: ObservedVersion::Unknown {
            reason: VersionUnknown::NeedsContainedProbe,
        },
        file_version_metadata: PeVersionMetadata::NotInspected,
    }
}

#[test]
fn cli_version_is_exact_bounded_and_rejects_diagnostics_or_pe_versions() {
    for (bytes, expected) in [
        (b"2.1.307 (Claude Code)\r\n".as_slice(), "2.1.307"),
        (b"2.1.307".as_slice(), "2.1.307"),
        (
            b"2.1.307-rc.1+build.02 (Claude Code)".as_slice(),
            "2.1.307-rc.1+build.02",
        ),
    ] {
        assert_eq!(parse_version(bytes).unwrap().to_string(), expected);
    }
    for rejected in [
        b"2.1.307.0".as_slice(),
        b"02.1.307".as_slice(),
        b"2.1.307+".as_slice(),
        b"2.1.307-".as_slice(),
        b"2.1.307\nuser@example.test".as_slice(),
        b"token=redacted-example".as_slice(),
        b"2.1.307\0".as_slice(),
        b"\xff".as_slice(),
        &[b'2'; 129],
    ] {
        assert!(parse_version(rejected).is_none());
    }
}

#[test]
fn wrappers_and_ambiguous_installations_do_not_become_native_probes() {
    let mut catalog = CatalogSnapshot::default();
    assert!(matches!(select_claude(&catalog), Err(ProbeIssue::NotFound)));
    catalog.candidates.push(candidate(
        PathBuf::from(r"C:\synthetic-not-read\claude.cmd"),
        ProgramKind::CmdWrapper,
    ));
    assert!(matches!(
        select_claude(&catalog),
        Err(ProbeIssue::WrapperUnresolved)
    ));
    catalog.candidates[0].kind = ProgramKind::DirectExe;
    catalog.candidates.push(candidate(
        PathBuf::from(r"D:\synthetic-not-read\claude.exe"),
        ProgramKind::DirectExe,
    ));
    assert!(matches!(
        select_claude(&catalog),
        Err(ProbeIssue::Ambiguous)
    ));
}

#[test]
fn prepared_probe_pins_the_exact_binary_and_ancestor_without_running_it() {
    let fixture = Fixture::new();
    let location = fixture.location();
    location.prepare_private_directory().unwrap();
    let program_dir = fixture.root.join("synthetic-program");
    fs::create_dir(&program_dir).unwrap();
    let program = program_dir.join("claude.exe");
    fs::copy(std::env::current_exe().unwrap(), &program).unwrap();
    let prepared = PreparedClaudeProbe::prepare(
        &candidate(program.clone(), ProgramKind::DirectExe),
        &location,
    )
    .unwrap();
    assert_eq!(prepared.executable_sha256().len(), 64);
    assert!(fs::OpenOptions::new().write(true).open(&program).is_err());
    assert!(fs::rename(&program, program_dir.join("replaced.exe")).is_err());
    assert!(fs::rename(&program_dir, fixture.root.join("swapped-program")).is_err());
    assert!(!format!("{prepared:?}").contains("synthetic-program"));
    drop(prepared);
    fs::rename(&program, program_dir.join("replaced.exe")).unwrap();
    fs::rename(&program_dir, fixture.root.join("swapped-program")).unwrap();
}

/// Explicitly scheduled by the coordinator; never a normal test-suite side effect.
/// No auth/login/model/install operation is invoked. NotFound is reported as such.
#[test]
#[ignore = "requires an explicit contained installed-Claude version probe grant"]
fn installed_claude_version_only_under_a_private_frozen_budget() {
    let candidate = match discover_claude() {
        Ok(candidate) => candidate,
        Err(
            issue @ (ProbeIssue::NotFound | ProbeIssue::WrapperUnresolved | ProbeIssue::Ambiguous),
        ) => {
            println!("claude_probe_refusal={issue:?}; native_capabilities_qualified=false");
            return;
        }
        Err(issue) => panic!("bounded discovery failed: {issue:?}"),
    };
    let mut fixture = Fixture::new();
    let location = fixture.location();
    let database = Database::open(&location, None).unwrap();
    let stamp = PolicyStamp {
        version: 1,
        id: "claude-version-probe-v1".into(),
    };
    let initial = database
        .initialize_resource_policy(&ReservationPolicy {
            stamp: stamp.clone(),
            aggregate_ceiling: probe_budget(),
            per_slot_ceiling: probe_budget(),
            max_slots: 1,
            max_lease_ms: 30_000,
        })
        .unwrap();
    let admission = AdmissionCoordinator::new(1).unwrap();
    let prepared = PreparedClaudeProbe::prepare(&candidate, &location).unwrap();
    let mut outcome = prepared
        .run(&database, &admission, &stamp, initial.revision)
        .unwrap();
    if outcome.cleanup_retained() {
        outcome.retry_cleanup(&database);
    }
    fixture.preserve = outcome.cleanup_retained();
    let summary = outcome.summary();
    assert_eq!(summary.authentication, Authentication::Unknown);
    assert!(!summary.authentication_probe_performed);
    assert!(
        summary
            .capabilities
            .values()
            .all(|support| matches!(support, CapabilitySupport::Unsupported { .. }))
    );
    // Summary contains exact version/hash/status only, never raw CLI output.
    println!(
        "claude_probe_summary={}",
        serde_json::to_string(summary).unwrap()
    );
    assert!(
        summary.version.is_some(),
        "version probe refused: {:?}",
        summary.issue
    );
}
