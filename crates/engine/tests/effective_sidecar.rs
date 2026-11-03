#![cfg(windows)]
//! Selected canonical source-runtime controls, not a running-CLI attestation.
//! The build service and fixture execute only in one persisted limited job.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use shinatama_engine::{
    resources::{
        admission::{AdmissionCoordinator, CleanupStatus, PrepareRequest},
        process::{ContainedProcess, NativeCommand, spawn_contained},
        sampling::{Measurement, WindowsSampler},
    },
    sidecar_config::prepare_private_sidecar,
    sidecar_runtime::{NodePin, prepare_runtime, verify_bundled_node, verify_prepared_bundle},
};
use shinatama_state::{
    database::Database,
    location::{LocationRequest, StorageLocation},
    resource_reservations::{Budget, PolicyStamp, ReservationPolicy},
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File},
    io::{Read, Write},
    num::NonZeroU16,
    os::windows::io::AsRawHandle,
    path::{Path, PathBuf},
    ptr::null_mut,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{Foundation::ERROR_BROKEN_PIPE, System::Pipes::PeekNamedPipe};

type ProofResult<T> = Result<T, String>;
const MIB: u64 = 1024 * 1024;
const OUTPUT_CAP: usize = 64 * 1024;
const DEPS: &str = "D:/Projects/horness-launcher/sidecar/source-build/node_modules";
const NODE_SHA: &str = "58e74bf02fc5bbacc41dcb8bef089961cd5bddd37830b87784e4fc624d145d1f";
const ESBUILD_SHA: &str = "44ce6728d54c891b1c5a6d7dbfb1a0f13419884cca0b090f1fbcf0dcd8bee0e9";
fn safe<T, E>(result: Result<T, E>, operation: &'static str) -> ProofResult<T> {
    result.map_err(|_| operation.into())
}
fn require(value: bool, code: &'static str) -> ProofResult<()> {
    if value { Ok(()) } else { Err(code.into()) }
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .into()
}
fn budget() -> Budget {
    Budget {
        memory_bytes: 1024 * MIB,
        processes: 8,
        cpu_basis_points: 2000,
        queue_bytes: OUTPUT_CAP as u64,
        queue_items: 2,
    }
}
fn fingerprint(path: &Path, maximum: u64) -> ProofResult<String> {
    let mut file = safe(File::open(path), "open static or synthetic proof input")?;
    let length = safe(file.metadata(), "read proof input metadata")?.len();
    require(
        length <= maximum,
        "proof input exceeds its fixed byte bound",
    )?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 8192];
    let mut read = 0_u64;
    loop {
        let count = safe(file.read(&mut buffer), "hash proof input")?;
        if count == 0 {
            break;
        }
        read += count as u64;
        require(read <= maximum, "proof input grew beyond its byte bound")?;
        digest.update(&buffer[..count]);
    }
    require(read == length, "proof input changed during hashing")?;
    Ok(format!("{:x}", digest.finalize()))
}
struct Fixture {
    root: PathBuf,
    released: bool,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let parent = std::env::temp_dir();
        if self.released
            && self.root.parent() == Some(parent.as_path())
            && self.root.file_name().is_some_and(|name| {
                name.to_string_lossy()
                    .starts_with("shinatama-effective-sidecar-")
            })
        {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
struct Capture {
    output: Vec<u8>,
    errors: Vec<u8>,
    cursor: usize,
    deadline: Instant,
    awaiting: &'static str,
    last_event: Option<&'static str>,
}
struct Child {
    process: ContainedProcess,
    stdin: Option<File>,
    stdout: File,
    stderr: File,
}
impl Capture {
    fn poll(&mut self, child: &mut Child) -> ProofResult<()> {
        require(
            Instant::now() < self.deadline,
            "source-runtime proof exceeded its wall deadline",
        )?;
        let mut total = self.output.len() + self.errors.len();
        drain(&mut child.stdout, &mut self.output, &mut total)?;
        drain(&mut child.stderr, &mut self.errors, &mut total)?;
        let census = safe(
            child.process.sample(&WindowsSampler::new()),
            "inspect exact source-runtime job",
        )?;
        require(
            census.active_processes <= 8,
            "raw process count exceeds fixed native cap",
        )
    }
    fn phase(&mut self, child: &mut Child, expected: &'static str) -> ProofResult<Value> {
        self.awaiting = expected;
        loop {
            self.poll(child)?;
            while let Some(end) = self.output[self.cursor..]
                .iter()
                .position(|byte| *byte == b'\n')
            {
                let end = self.cursor + end;
                let value: Value = safe(
                    serde_json::from_slice(&self.output[self.cursor..end]),
                    "invalid bounded fixture event",
                )?;
                self.cursor = end + 1;
                if value["phase"] == "failed" {
                    let stage = match value["stage"].as_str() {
                        Some("canonical-source-build") => "canonical-source-build",
                        Some("canonical-source-runtime") => "canonical-source-runtime",
                        _ => "unknown",
                    };
                    return Err(format!("source fixture refused at {stage}"));
                }
                require(
                    value["phase"] == expected,
                    "unexpected source fixture phase",
                )?;
                self.last_event = Some(expected);
                return Ok(value);
            }
            require(
                safe(child.process.try_wait(), "inspect exact source parent exit")?.is_none(),
                "source fixture exited before expected event",
            )?;
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
fn drain(pipe: &mut File, destination: &mut Vec<u8>, total: &mut usize) -> ProofResult<()> {
    let mut buffer = [0; 4096];
    for _ in 0..16 {
        let mut available = 0;
        if unsafe {
            PeekNamedPipe(
                pipe.as_raw_handle(),
                null_mut(),
                0,
                null_mut(),
                &mut available,
                null_mut(),
            )
        } == 0
        {
            return if std::io::Error::last_os_error().raw_os_error()
                == Some(ERROR_BROKEN_PIPE as i32)
            {
                Ok(())
            } else {
                Err("peek bounded synthetic output".into())
            };
        }
        if available == 0 {
            return Ok(());
        }
        let count = (available as usize).min(buffer.len());
        require(
            total
                .checked_add(count)
                .is_some_and(|next| next <= OUTPUT_CAP),
            "source fixture output exceeds 64 KiB",
        )?;
        require(
            destination.len() + count <= destination.capacity(),
            "source fixture exceeded its fixed pipe buffer",
        )?;
        let count = safe(
            pipe.read(&mut buffer[..count]),
            "read bounded synthetic output",
        )?;
        require(count > 0, "source fixture pipe closed unexpectedly")?;
        *total += count;
        destination.extend_from_slice(&buffer[..count]);
    }
    Ok(())
}
fn validate_scope(result: &Value) -> ProofResult<()> {
    require(
        result["scope"] == "canonical-source-runtime-not-cli"
            && result["runningCliEffectiveConfigVerified"] == false
            && result["providerTrafficVerified"] == false
            && result["credentialsRead"] == false
            && result["adaptivePressure"]["targetRatioControlQualified"] == false
            && result["adaptivePressure"]["growthWindowControlQualified"] == false
            && result["compressionOff"]["runningCliTrafficVerified"] == false,
        "source fixture overstated its runtime scope",
    )?;
    for pointer in [
        "/tiers/resolved",
        "/tiers/realAdaptiveTurnUsed",
        "/adaptivePressure/resolved",
        "/adaptivePressure/enabledPathObserved",
        "/adaptivePressure/offPathObserved",
        "/adaptivePressure/realWindowUnchanged",
        "/promptRewrite/autoAdmissionObserved",
        "/promptRewrite/offNoDispatchObserved",
        "/outputSteering/verbosityMutationObserved",
        "/outputSteering/effortRoutingOffPreservesHigh",
        "/outputSteering/effortRoutingOnControlLowersEffort",
        "/outputSteering/disabledBodyUnchanged",
        "/compressionOff/canonicalPassthroughResolverObserved",
        "/selectedPinnedSourceFilesUnchanged",
        "/originalCliEntryHashUnchanged",
    ] {
        require(
            result.pointer(pointer) == Some(&Value::Bool(true)),
            "canonical source control was not observed",
        )?;
    }
    require(
        result["promptRewrite"]["paidCalls"] == 0
            && result["promptRewrite"]["syntheticCompletions"] == 1,
        "source rewrite qualification did not stay synthetic",
    )
}

#[test]
#[ignore = "explicit local source-runtime proof; requires pinned Node, prepared source and installed offline esbuild"]
fn selected_canonical_controls_execute_without_claiming_running_cli_configuration() {
    let started = Instant::now();
    let mut fixture = Fixture {
        root: std::env::temp_dir().join(format!(
            "shinatama-effective-sidecar-{}",
            uuid::Uuid::new_v4()
        )),
        released: false,
    };
    let result = qualify(&mut fixture, started + Duration::from_secs(30));
    let evidence = match &result {
        Ok(value) => value.clone(),
        Err(error) => {
            json!({"testPassed":false,"failure":error,"runningCliEffectiveConfigVerified":false})
        }
    };
    let target = workspace().join(".prototype-evidence/effective-sidecar.json");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    assert!(result.is_ok(), "{}", result.err().unwrap_or_default());
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "source-runtime proof exceeded 30 seconds"
    );
}
fn qualify(fixture: &mut Fixture, deadline: Instant) -> ProofResult<Value> {
    let location = safe(
        StorageLocation::resolve(LocationRequest {
            local_data_directory: fixture.root.clone(),
            home_override: Some(fixture.root.join("private")),
            project_roots: vec![workspace()],
            synchronized_roots: vec![],
        }),
        "resolve private proof storage",
    )?;
    let db = safe(
        Database::open(&location, None),
        "open isolated proof ledger",
    )?;
    // No server is started by this fixture; its canonical configuration still
    // receives a valid explicit port and all source probes use injected functions.
    let port = NonZeroU16::new(19179).unwrap();
    let private = safe(
        prepare_private_sidecar(&location, port),
        "prepare private canonical baseline",
    )?;
    let config_sha = fingerprint(private.configuration_file(), 16 * 1024)?;
    let bundle = safe(
        verify_prepared_bundle(&workspace().join("sidecar/source-build/prepared")),
        "verify pinned source and original CLI bytes",
    )?;
    let node = safe(
        verify_bundled_node(
            &workspace().join("sidecar/runtime"),
            NodePin {
                relative_path: "node.exe".into(),
                sha256: NODE_SHA.into(),
                version: Some("24.14.1".into()),
                modules_abi: Some(137),
            },
        ),
        "verify pinned Node image",
    )?;
    let os: BTreeMap<OsString, OsString> = ["SystemRoot", "WINDIR"]
        .into_iter()
        .filter_map(|key| std::env::var_os(key).map(|value| (key.into(), value)))
        .collect();
    let recipe = safe(
        prepare_runtime(bundle, node, private.root(), &os, port),
        "prepare exact private child environment",
    )?;
    let identity = recipe.summary();
    let script = workspace().join("crates/engine/tests/fixtures/effective-sidecar.mjs");
    let fixture_sha = fingerprint(&script, 64 * 1024)?;
    let esbuild_sha = fingerprint(
        &Path::new(DEPS).join("@esbuild/win32-x64/esbuild.exe"),
        16 * MIB,
    )?;
    require(
        esbuild_sha == ESBUILD_SHA,
        "offline build-tool fingerprint changed",
    )?;
    let sampler = WindowsSampler::new();
    let host = safe(sampler.sample(&[]), "inspect host headroom")?;
    let Measurement::Available(system) = host.system else {
        return Err("host headroom unavailable".into());
    };
    require(
        system.physical_available_bytes >= budget().memory_bytes + 512 * MIB
            && system
                .commit_limit_bytes
                .saturating_sub(system.commit_total_bytes)
                >= budget().memory_bytes + 1024 * MIB,
        "fixed source-runtime headroom unavailable",
    )?;
    let stamp = PolicyStamp {
        version: 1,
        id: "effective-sidecar-source-proof-v1".into(),
    };
    let ledger = safe(
        db.initialize_resource_policy(&ReservationPolicy {
            stamp: stamp.clone(),
            aggregate_ceiling: budget(),
            per_slot_ceiling: budget(),
            max_slots: 1,
            max_lease_ms: 30_000,
        }),
        "freeze private source-runtime policy",
    )?;
    let coordinator = safe(
        AdmissionCoordinator::new(1),
        "create fixed source-runtime slot",
    )?;
    let at = now();
    let workload = safe(
        coordinator.prepare(
            &db,
            &stamp,
            ledger.revision,
            PrepareRequest {
                budget: budget(),
                process_commit_bytes: 768 * MIB,
                now_ms: at,
                lease_expires_at_ms: at + 30_000,
            },
        ),
        "reserve source-runtime capacity",
    )?;
    let command = safe(
        NativeCommand::new(
            recipe.executable().to_owned(),
            vec![
                script.into_os_string(),
                workspace()
                    .join("sidecar/source-build/prepared")
                    .into_os_string(),
                recipe.working_directory().as_os_str().to_owned(),
                DEPS.into(),
            ],
            recipe.working_directory().to_owned(),
            recipe
                .child_environment()
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .collect(),
        ),
        "validate source-only native command",
    )?;
    let spawned = match spawn_contained(command, workload, &db, at, at + 30_000) {
        Ok(value) => value,
        Err(mut failure) => {
            fixture.released = matches!(
                failure.workload.finish_cleanup(&db, Duration::from_secs(2)),
                Ok(CleanupStatus::Released { .. })
            );
            return Err("contained source-runtime startup refused".into());
        }
    };
    let mut child = Child {
        process: spawned.process,
        stdin: Some(spawned.stdin),
        stdout: spawned.stdout,
        stderr: spawned.stderr,
    };
    let credits = child.process.queue_credits();
    let credit = safe(
        credits.acquire(OUTPUT_CAP as u64, 2),
        "reserve bounded proof buffers",
    )?;
    let mut capture = Capture {
        output: Vec::with_capacity(48 * 1024),
        errors: Vec::with_capacity(16 * 1024),
        cursor: 0,
        deadline: deadline - Duration::from_secs(2),
        awaiting: "built",
        last_event: None,
    };
    let outcome: ProofResult<Value> = (|| {
        let build = capture.phase(&mut child, "built")?;
        require(
            build["sourceTreeSha256"] == identity.source.source_tree_sha256
                && build["artifactTreeSha256"] == identity.source.artifact_tree_sha256
                && build["esbuildVersion"] == "0.27.7",
            "source fixture build provenance differs from the verified preparation",
        )?;
        let artifact_sha = fingerprint(
            &private
                .root()
                .directory()
                .join("temp/effective-config-runtime.mjs"),
            32 * MIB,
        )?;
        require(
            build["artifactSha256"] == artifact_sha,
            "source artifact fingerprint mismatch",
        )?;
        let census = safe(
            child.process.sample(&sampler),
            "census retained native build service",
        )?;
        require(
            census.membership_complete
                && (2..=8).contains(&census.active_processes)
                && census.total_processes == census.active_processes,
            "build service lifetime census incomplete",
        )?;
        let mut input = child
            .stdin
            .take()
            .ok_or("fixture handshake input already closed")?;
        safe(
            input.write_all(b"census-complete\n"),
            "release verified build-service handshake",
        )?;
        safe(input.flush(), "flush final fixed fixture instruction")?;
        // This finite protocol has no further commands. Send real EOF now,
        // rather than retaining a Windows stdin pipe while awaiting child exit.
        drop(input);
        let result = capture.phase(&mut child, "effective-source-complete")?;
        validate_scope(&result)?;
        capture.awaiting = "exact-parent-exit";
        loop {
            capture.poll(&mut child)?;
            if let Some(code) = safe(child.process.try_wait(), "wait exact source fixture")? {
                require(code == 0, "source fixture exited unsuccessfully")?;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        require(
            fingerprint(private.configuration_file(), 16 * 1024)? == config_sha,
            "private baseline was changed",
        )?;
        Ok(
            json!({"testPassed":true,"scope":"canonical-source-runtime-not-cli","build":build,"controls":result,
            "nodeSha256":NODE_SHA,"fixtureSha256":fixture_sha,"esbuildBinarySha256":esbuild_sha,
            "nativeCensusBeforeBuildServiceStop":{"active":census.active_processes,"total":census.total_processes,"complete":true},
            "frozenLimits":{"jobCommitBytes":budget().memory_bytes,"processCommitBytes":768*MIB,"processes":8,"cpuBasisPoints":2000,"outputBytes":OUTPUT_CAP,"wallSeconds":30},
            "runningCliEffectiveConfigVerified":false,"providerTrafficVerified":false,
            "unqualifiedControls":["adaptivePressure.targetRatio is configured but not consumed by the observed path","adaptivePressure.growthWindow is configured but not consumed by the observed path","whole running CLI effective configuration"]}),
        )
    })();
    let outcome = outcome.map_err(|error| {
        format!(
            "{error}; awaiting={}, lastEvent={:?}, stdoutBytes={}, stderrBytes={}, exactExit={:?}",
            capture.awaiting,
            capture.last_event,
            capture.output.len(),
            capture.errors.len(),
            child.process.try_wait().ok().flatten()
        )
    });
    let output_bytes = capture.output.len() + capture.errors.len();
    let Child {
        mut process,
        stdin,
        stdout,
        stderr,
    } = child;
    drop((stdin, stdout, stderr, capture, credit));
    require(
        credits.status().current_bytes == 0 && credits.status().current_items == 0,
        "proof buffer credits remain live",
    )?;
    let remaining = deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_secs(2));
    let cleaned = safe(
        process.finish_cleanup(&db, remaining),
        "finish source-runtime native cleanup",
    )?;
    require(
        matches!(cleaned, CleanupStatus::Released { .. }),
        "source-runtime cleanup remains reserved",
    )?;
    require(
        safe(process.try_wait(), "confirm exact source parent signaled")?.is_some(),
        "source parent handle not signaled",
    )?;
    require(
        safe(db.resource_reservations(), "inspect final private ledger")?
            .is_some_and(|ledger| ledger.slots.is_empty()),
        "source-runtime reservation remains live",
    )?;
    fixture.released = true;
    let mut outcome = outcome?;
    outcome["cleanupReleased"] = json!(true);
    outcome["capturedBytes"] = json!(output_bytes);
    Ok(outcome)
}
