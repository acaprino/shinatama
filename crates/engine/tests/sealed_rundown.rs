#![cfg(windows)]
//! Local proof of the sealed, continuously held job path. Three real descendants
//! finish before the first census; no agents, shell, provider or inherited home.
use shinatama_engine::{
    resources::{
        admission::{AdmissionCoordinator, CleanupStatus, PrepareRequest},
        process::{NativeCommand, SpawnedProcess, spawn_contained},
        sampling::{Measurement, WindowsSampler},
    },
    sidecar_runtime::{NodePin, verify_bundled_node},
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
    io::Read,
    os::windows::io::AsRawHandle,
    path::{Path, PathBuf},
    ptr::null_mut,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{Foundation::ERROR_BROKEN_PIPE, System::Pipes::PeekNamedPipe};

const MIB: u64 = 1024 * 1024;
const CAP: usize = 8192;
const NODE_SHA: &str = "58e74bf02fc5bbacc41dcb8bef089961cd5bddd37830b87784e4fc624d145d1f";
const READY: &[u8] = b"sealed-rundown-ready:3\n";
const SCRIPT: &str = r#"
const {spawnSync} = require('node:child_process');
for (let index = 0; index < 3; index++) {
  const child = spawnSync(process.execPath, ['-e', 'process.exit(0)'], {
    env: process.env, stdio: 'ignore', windowsHide: true, timeout: 1800,
  });
  if (child.error || child.signal || child.status !== 0) process.exit(31);
}
process.stdin.resume();
setInterval(() => {}, 1000);
process.stdout.write('sealed-rundown-ready:3\n');
"#;
type ProofResult<T> = Result<T, &'static str>;
fn safe<T, E>(result: Result<T, E>, code: &'static str) -> ProofResult<T> {
    result.map_err(|_| code)
}
fn require(value: bool, code: &'static str) -> ProofResult<()> {
    if value { Ok(()) } else { Err(code) }
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
        cpu_basis_points: 1000,
        queue_bytes: CAP as u64,
        queue_items: 2,
    }
}
struct Fixture {
    root: PathBuf,
    released: bool,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let parent = std::env::temp_dir();
        // Retain the private ledger and files whenever native cleanup is uncertain.
        if self.released
            && self.root.parent() == Some(parent.as_path())
            && self.root.file_name().is_some_and(|name| {
                name.to_string_lossy()
                    .starts_with("shinatama-sealed-rundown-")
            })
        {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
fn drain(pipe: &mut File, bytes: &mut Vec<u8>, total: &mut usize) -> ProofResult<()> {
    let mut buffer = [0; 1024];
    for _ in 0..8 {
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
                Err("peek private fixture output")
            };
        }
        if available == 0 {
            return Ok(());
        }
        let count = (available as usize).min(buffer.len());
        require(
            total.checked_add(count).is_some_and(|next| next <= CAP),
            "fixture output exceeds 8 KiB",
        )?;
        let count = safe(
            pipe.read(&mut buffer[..count]),
            "read private fixture output",
        )?;
        require(count > 0, "private fixture output ended")?;
        *total += count;
        bytes.extend_from_slice(&buffer[..count]);
    }
    Ok(())
}

#[test]
#[ignore = "explicit local native proof; requires the prepared, hash-pinned Node executable"]
fn sealed_owned_job_releases_after_uncensused_short_lived_descendants() {
    let started = Instant::now();
    let mut fixture = Fixture {
        root: std::env::temp_dir()
            .join(format!("shinatama-sealed-rundown-{}", uuid::Uuid::new_v4())),
        released: false,
    };
    let result = qualify(&mut fixture, started + Duration::from_secs(10));
    assert!(
        result.is_ok(),
        "{}",
        result.err().unwrap_or("sealed rundown proof failed")
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "sealed rundown exceeded 10 s"
    );
}
fn qualify(fixture: &mut Fixture, deadline: Instant) -> ProofResult<()> {
    let location = safe(
        StorageLocation::resolve(LocationRequest {
            local_data_directory: fixture.root.clone(),
            home_override: Some(fixture.root.join("private")),
            project_roots: vec![workspace()],
            synchronized_roots: vec![],
        }),
        "resolve private fixture",
    )?;
    let db = safe(
        Database::open(&location, None),
        "open private fixture ledger",
    )?;
    let marker = location.directory().join("saved-marker");
    safe(
        fs::write(&marker, b"preserve-saved-content"),
        "create private integrity marker",
    )?;
    let runtime = workspace().join("sidecar/runtime");
    let _node_pin = safe(
        verify_bundled_node(
            &runtime,
            NodePin {
                relative_path: "node.exe".into(),
                sha256: NODE_SHA.into(),
                version: Some("24.14.1".into()),
                modules_abi: Some(137),
            },
        ),
        "verify pinned Node image",
    )?;
    let sampler = WindowsSampler::new();
    let host = safe(sampler.sample(&[]), "sample host headroom")?;
    let Measurement::Available(system) = host.system else {
        return Err("host headroom unavailable");
    };
    require(
        system.physical_available_bytes >= budget().memory_bytes + 512 * MIB
            && system
                .commit_limit_bytes
                .saturating_sub(system.commit_total_bytes)
                >= budget().memory_bytes + 1024 * MIB,
        "host headroom insufficient for fixed native limits",
    )?;
    let stamp = PolicyStamp {
        version: 1,
        id: "sealed-rundown-local-proof-v1".into(),
    };
    let ledger = safe(
        db.initialize_resource_policy(&ReservationPolicy {
            stamp: stamp.clone(),
            aggregate_ceiling: budget(),
            per_slot_ceiling: budget(),
            max_slots: 1,
            max_lease_ms: 15_000,
        }),
        "install frozen private policy",
    )?;
    let coordinator = safe(AdmissionCoordinator::new(1), "create fixed private slot")?;
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
                lease_expires_at_ms: at + 15_000,
            },
        ),
        "reserve fixed private workload",
    )?;
    let mut environment: BTreeMap<OsString, OsString> = ["SystemRoot", "WINDIR"]
        .into_iter()
        .filter_map(|name| std::env::var_os(name).map(|value| (name.into(), value)))
        .collect();
    for name in [
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "TMP",
        "TEMP",
    ] {
        environment.insert(name.into(), location.directory().as_os_str().to_owned());
    }
    let command = safe(
        NativeCommand::new(
            runtime.join("node.exe"),
            vec!["-e".into(), SCRIPT.into()],
            location.directory().to_owned(),
            environment,
        ),
        "prepare private Node command",
    )?;
    let spawned = spawn_contained(command, workload, &db, at, at + 15_000);
    let SpawnedProcess {
        mut process,
        stdin,
        mut stdout,
        mut stderr,
    } = match spawned {
        Ok(value) => value,
        Err(mut failure) => {
            let cleaned = failure.workload.finish_cleanup(&db, Duration::from_secs(2));
            fixture.released = matches!(cleaned, Ok(CleanupStatus::Released { .. }));
            return Err("start sealed private Node fixture");
        }
    };
    let owned = process.owned_job_key();
    let key = process.key();
    let credits = process.queue_credits();
    let credit = safe(
        credits.acquire(CAP as u64, 2),
        "reserve bounded output credits",
    )?;
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let mut total = 0;
    let measurement = (|| -> ProofResult<_> {
        let limits = process
            .applied_budget()
            .ok_or("missing applied native limits")?;
        require(
            limits.job_commit_bytes() == budget().memory_bytes
                && limits.process_commit_bytes() == 768 * MIB
                && limits.max_processes() == 8
                && limits.cpu_hard_cap_basis_points() == 1000,
            "applied limits differ from fixed policy",
        )?;
        // Deliberately no job sample until all three real children have exited.
        loop {
            drain(&mut stdout, &mut output, &mut total)?;
            drain(&mut stderr, &mut errors, &mut total)?;
            require(errors.is_empty(), "private Node fixture reported an error")?;
            require(
                process
                    .try_wait()
                    .map_err(|_| "inspect exact parent handle")?
                    .is_none(),
                "private parent exited before barrier",
            )?;
            if output == READY {
                break;
            }
            require(
                output.len() < READY.len(),
                "unexpected private barrier output",
            )?;
            require(
                Instant::now() + Duration::from_secs(2) < deadline,
                "private child barrier exceeded deadline",
            )?;
            std::thread::sleep(Duration::from_millis(5));
        }
        let sample = safe(
            process.sample(&sampler),
            "sample exact sealed job after child exits",
        )?;
        require(
            sample.membership_complete
                && sample.active_processes > 0
                && sample.active_processes <= 8,
            "sealed current membership incomplete or over fixed limit",
        )?;
        require(
            sample.total_processes >= 4 && sample.total_processes > sample.active_processes,
            "real short-lived descendants not present in lifetime accounting",
        )?;
        require(
            process.key() == key && process.owned_job_key() == owned,
            "owned identity changed",
        )?;
        Ok((sample.total_processes, sample.active_processes))
    })();
    // Return all buffer allocations, pipes and credits before persisted release.
    drop((stdin, stdout, stderr, output, errors, credit));
    require(
        credits.status().current_bytes == 0 && credits.status().current_items == 0,
        "output credits remain live",
    )?;
    let remaining = deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_secs(2));
    let cleanup = safe(
        process.finish_cleanup(&db, remaining),
        "finish exact held sealed job cleanup",
    )?;
    require(
        matches!(cleanup, CleanupStatus::Released { .. }),
        "sealed native cleanup remains reserved",
    )?;
    require(
        safe(process.try_wait(), "verify exact known parent signaled")?.is_some(),
        "known parent handle not signaled after release",
    )?;
    let ledger = safe(db.resource_reservations(), "read final private ledger")?
        .ok_or("final private ledger missing")?;
    require(ledger.slots.is_empty(), "private reservation not released")?;
    fixture.released = true;
    let (total, active) = measurement?;
    require(
        safe(fs::read(marker), "read private integrity marker")? == b"preserve-saved-content",
        "saved private content changed",
    )?;
    println!(
        "{{\"sealedRundown\":true,\"shortLivedChildren\":3,\"totalProcesses\":{total},\"activeAtBarrier\":{active},\"knownHandleSignaled\":true,\"released\":true,\"outputBytes\":{}}}",
        READY.len()
    );
    Ok(())
}
