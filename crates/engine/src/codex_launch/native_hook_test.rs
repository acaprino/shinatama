//! Explicit native bridge proof, not a Codex launch or hook-trust qualification.
//! The real Windows PowerShell trampoline receives only a synthetic hook body.
//! No account, model endpoint, Codex process or personal profile is accessed.
use super::*;
use crate::{
    harness_session_events::{SessionEventKind, SessionStartSource},
    resources::{
        admission::{AdmissionCoordinator, CleanupStatus, PrepareRequest, QueueCredit},
        process::{ContainedProcess, NativeCommand, SpawnedProcess, spawn_contained},
        sampling::{Measurement, WindowsSampler},
    },
};
use shinatama_state::{
    location::LocationRequest,
    resource_reservations::{PolicyStamp, ReservationPolicy},
};
use std::{ptr::null_mut, sync::Arc, thread};
use windows_sys::Win32::{Foundation::ERROR_BROKEN_PIPE, System::Pipes::PeekNamedPipe};

const CAP: usize = 4096;
type ProofResult<T> = std::result::Result<T, &'static str>;
fn checked<T, E>(result: std::result::Result<T, E>, code: &'static str) -> ProofResult<T> {
    result.map_err(|_| code)
}
fn require(value: bool, code: &'static str) -> ProofResult<()> {
    if value { Ok(()) } else { Err(code) }
}
fn clock_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
fn drain(pipe: &mut File, bytes: &mut Vec<u8>) -> ProofResult<()> {
    for _ in 0..4 {
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
                Err("peek-contained-hook-output")
            };
        }
        if available == 0 {
            return Ok(());
        }
        let mut buffer = [0; 1024];
        let count = (available as usize).min(buffer.len());
        require(bytes.len() + count <= CAP, "bounded-hook-output")?;
        let read = checked(
            pipe.read(&mut buffer[..count]),
            "read-contained-hook-output",
        )?;
        if read == 0 {
            return Ok(());
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    Ok(())
}
struct Owner {
    db: Arc<Database>,
    process: ContainedProcess,
    stdin: Option<File>,
    stdout: Option<File>,
    stderr: Option<File>,
    credit: Option<QueueCredit>,
    released: bool,
}
impl Owner {
    fn from_spawn(db: Arc<Database>, child: SpawnedProcess) -> Self {
        Self {
            db,
            process: child.process,
            stdin: Some(child.stdin),
            stdout: Some(child.stdout),
            stderr: Some(child.stderr),
            credit: None,
            released: false,
        }
    }
    fn cleanup(&mut self) -> ProofResult<()> {
        if self.released {
            return Ok(());
        }
        self.stdin.take();
        self.stdout.take();
        self.stderr.take();
        self.credit.take();
        let status = checked(
            self.process
                .finish_cleanup(&self.db, Duration::from_secs(2)),
            "hook-native-cleanup",
        )?;
        require(
            matches!(status, CleanupStatus::Released { .. }),
            "hook-owner-remains-retained",
        )?;
        self.released = true;
        Ok(())
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

fn proof() -> ProofResult<()> {
    let budget = Budget {
        memory_bytes: 512 * MIB,
        processes: 4,
        cpu_basis_points: 1000,
        queue_bytes: (2 * CAP) as u64,
        queue_items: 2,
    };
    let sample = checked(WindowsSampler::new().sample(&[]), "hook-headroom-sample")?;
    let Measurement::Available(system) = sample.system else {
        return Err("hook-headroom-unavailable");
    };
    require(
        system.physical_available_bytes >= budget.memory_bytes + 512 * MIB
            && system
                .commit_limit_bytes
                .saturating_sub(system.commit_total_bytes)
                >= budget.memory_bytes + 1024 * MIB,
        "hook-headroom-refused-before-spawn",
    )?;

    let deadline = Instant::now() + Duration::from_secs(10);
    let shell = checked(pin_hook_shell(deadline), "pin-real-windows-powershell")?;
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("workspace-root")?;
    let helper = checked(
        crate::harness_hooks::pin_session_hook(
            &workspace.join("target/debug/shinatama-hook.exe"),
            deadline,
        ),
        "pin-built-hook-helper",
    )?;
    let root = std::env::temp_dir().join(format!(
        "shinatama-codex-hook-command-{}",
        uuid::Uuid::new_v4()
    ));
    let base = checked(
        StorageLocation::resolve(LocationRequest {
            local_data_directory: root.clone(),
            home_override: Some(root.join("private")),
            project_roots: vec![],
            synchronized_roots: vec![],
        }),
        "new-private-hook-location",
    )?;
    let db = Arc::new(checked(
        Database::open(&base, None),
        "new-private-hook-database",
    )?);
    let mut locations = Vec::new();
    for name in ["project", "home", "roaming", "local", "tmp"] {
        let location = checked(
            base.resolve_related_directory(&base.directory().join(name)),
            "private-hook-child",
        )?;
        checked(
            location.prepare_private_directory(),
            "prepare-private-hook-child",
        )?;
        locations.push(location);
    }
    let project = base.directory().join("project");
    let identity = checked(
        LaunchIdentity::generate("t_abcdefghijkl"),
        "synthetic-hook-identity",
    )?;
    let receiver = checked(
        SessionEventReceiver::start_for(HarnessKind::Codex, identity.record(), &project),
        "private-hook-receiver",
    )?;
    // Use the exact production command builder. Its outer executable path cannot
    // contain whitespace; the remainder is fixed switches plus one base64 token.
    let rendered = checked(
        session_hook_command(&helper.path, &shell.path, receiver.port()),
        "production-hook-command",
    )?;
    let parts: Vec<_> = rendered.split_ascii_whitespace().collect();
    require(
        parts.len() == 5 && parts[1..4] == ["-NoProfile", "-NonInteractive", "-EncodedCommand"],
        "fixed-hook-trampoline",
    )?;
    let windows = shell.path.ancestors().nth(4).ok_or("windows-directory")?;
    let mut environment: BTreeMap<OsString, OsString> = BTreeMap::new();
    for name in ["SystemRoot", "WINDIR"] {
        environment.insert(name.into(), windows.as_os_str().to_owned());
    }
    for (name, child) in [
        ("HOME", "home"),
        ("USERPROFILE", "home"),
        ("APPDATA", "roaming"),
        ("LOCALAPPDATA", "local"),
        ("TEMP", "tmp"),
        ("TMP", "tmp"),
    ] {
        environment.insert(name.into(), base.directory().join(child).into());
    }
    environment.insert("PATH".into(), windows.join("System32").into());
    environment.insert("SHINATAMA_INSTANCE".into(), identity.instance_id().into());
    environment.insert(
        "SHINATAMA_INSTANCE_TOKEN".into(),
        identity.child_secret_override().value().to_owned(),
    );
    let command = checked(
        NativeCommand::new(
            shell.path.clone(),
            parts[1..]
                .iter()
                .map(|part| OsString::from(*part))
                .collect(),
            project.clone(),
            environment,
        ),
        "contained-hook-command",
    )?;
    let stamp = PolicyStamp {
        version: 1,
        id: "synthetic-codex-hook-command".into(),
    };
    let ledger = checked(
        db.initialize_resource_policy(&ReservationPolicy {
            stamp: stamp.clone(),
            aggregate_ceiling: budget,
            per_slot_ceiling: budget,
            max_slots: 1,
            max_lease_ms: 15000,
        }),
        "freeze-hook-job-budget",
    )?;
    let coordinator = checked(AdmissionCoordinator::new(1), "one-hook-owner")?;
    let at = clock_ms();
    let workload = checked(
        coordinator.prepare(
            &db,
            &stamp,
            ledger.revision,
            PrepareRequest {
                budget,
                process_commit_bytes: 384 * MIB,
                now_ms: at,
                lease_expires_at_ms: at + 15000,
            },
        ),
        "admit-contained-hook",
    )?;
    let child = match spawn_contained(command, workload, &db, at, at + 15000) {
        Ok(child) => child,
        Err(mut failure) => {
            let status = checked(
                failure.workload.finish_cleanup(&db, Duration::from_secs(2)),
                "failed-hook-spawn-cleanup",
            )?;
            require(
                matches!(status, CleanupStatus::Released { .. }),
                "failed-hook-spawn-retained",
            )?;
            return Err("contained-hook-spawn");
        }
    };
    let mut owner = Owner::from_spawn(Arc::clone(&db), child);
    let outcome = (|| {
        owner.credit = Some(checked(
            owner.process.queue_credits().acquire((2 * CAP) as u64, 2),
            "bounded-hook-output-credit",
        )?);
        let native = uuid::Uuid::new_v4().to_string();
        let payload = serde_json::to_vec(&serde_json::json!({"session_id":native, "cwd":project,
            "hook_event_name":"SessionStart", "source":"startup", "transcript_path":"synthetic-ignored-extension"})).map_err(|_| "synthetic-hook-payload")?;
        require(payload.len() <= 1024, "bounded-synthetic-hook-payload")?;
        checked(
            owner
                .stdin
                .as_mut()
                .ok_or("hook-stdin")?
                .write_all(&payload),
            "write-synthetic-hook-stdin",
        )?;
        owner.stdin.take();
        let end = Instant::now() + Duration::from_secs(8);
        let mut output = Vec::new();
        let mut errors = Vec::new();
        loop {
            drain(owner.stdout.as_mut().ok_or("hook-stdout")?, &mut output)?;
            drain(owner.stderr.as_mut().ok_or("hook-stderr")?, &mut errors)?;
            if let Some(code) = checked(owner.process.try_wait(), "owned-hook-exit")? {
                drain(owner.stdout.as_mut().ok_or("hook-stdout")?, &mut output)?;
                drain(owner.stderr.as_mut().ok_or("hook-stderr")?, &mut errors)?;
                if code != 0 || !output.is_empty() || !errors.is_empty() {
                    let text = String::from_utf8_lossy(&errors);
                    let status = receiver.status();
                    // Only fixed classes and counters, never the text, token,
                    // command line, native path or request body.
                    eprintln!(
                        "{}",
                        serde_json::json!({
                            "diagnostic":"codex-hook-command-exit", "exitCode":code,
                            "stdoutBytes":output.len(), "stderrBytes":errors.len(),
                            "clixml":text.contains("CLIXML"),
                            "progressRecord":text.contains("S=\"progress\"") || text.contains("ProgressRecord"),
                            "commandNotFound":text.contains("CommandNotFoundException"),
                            "nativeCommandError":text.contains("NativeCommandError"),
                            "parserError":text.contains("ParserError"),
                            "securityError":text.contains("PSSecurityException"),
                            "eventQueued":receiver.try_next().ok().flatten().is_some(),
                            "rejectedRequests":status.rejected_requests,
                            "receiverFailed":status.failed,"queueOverflowed":status.queue_overflowed
                        })
                    );
                }
                require(
                    code == 0 && output.is_empty() && errors.is_empty(),
                    "hook-command-clean-exit",
                )?;
                break;
            }
            require(Instant::now() < end, "hook-command-deadline")?;
            thread::sleep(Duration::from_millis(10));
        }
        let event = checked(receiver.try_next(), "authenticated-hook-observation")?
            .ok_or("hook-event-missing")?;
        require(
            event.instance_id == identity.instance_id()
                && event.thread_id == "t_abcdefghijkl"
                && event.session_id == native
                && event.kind == SessionEventKind::SessionStart
                && event.source == Some(SessionStartSource::Startup)
                && Path::new(&event.cwd) == project,
            "exact-synthetic-hook-event",
        )?;
        require(
            checked(receiver.try_next(), "one-hook-event")?.is_none(),
            "duplicate-hook-event",
        )?;
        let status = receiver.status();
        require(
            !status.failed && !status.queue_overflowed && status.rejected_requests == 0,
            "complete-hook-transport",
        )
    })();
    let cleanup = owner.cleanup();
    receiver.stop();
    cleanup?;
    require(
        checked(db.resource_reservations(), "released-hook-ledger")?
            .ok_or("hook-ledger")?
            .slots
            .is_empty(),
        "hook-ticket-released",
    )?;
    outcome?;
    // The fresh fixture is intentionally retained on disk for local inspection;
    // it contains bounded synthetic metadata, never the in-memory bearer token.
    drop(locations);
    Ok(())
}

#[test]
#[ignore = "explicit native Windows PowerShell bridge proof; build shinatama-hook first and schedule with headroom"]
fn encoded_command_forwards_synthetic_codex_stdin_and_releases_its_owned_job() {
    if let Err(code) = proof() {
        panic!("Native hook-command proof failed at {code}; no Codex launch was qualified.");
    }
}
