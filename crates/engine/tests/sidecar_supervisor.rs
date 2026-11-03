#![cfg(windows)]
//! Bounded local qualification of the real owner worker and pinned CLI. No agent,
//! model, user configuration, OS trust change, global PID kill or pressure test.
use serde_json::{Value, json};
use shinatama_engine::{
    resources::{
        admission::{AdmissionCoordinator, CleanupStatus, PrepareRequest, PreparedWorkload},
        job::OwnedJobKey,
        process::{NativeCommand, spawn_contained},
        sampling::{Measurement, WindowsSampler},
    },
    sidecar_config::prepare_private_sidecar,
    sidecar_runtime::{NodePin, prepare_runtime, verify_bundled_node, verify_prepared_bundle},
    sidecar_supervisor::{
        AdmissionUnavailable, Command, Phase, Reason, SidecarSupervisor, Snapshot,
        SupervisorConfig, WorkloadProvider,
    },
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
    mem::size_of,
    net::{TcpListener, TcpStream},
    num::NonZeroU16,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::{Path, PathBuf},
    ptr::null_mut,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::{
        ERROR_BROKEN_PIPE, ERROR_NO_MORE_FILES, FILETIME, GetLastError, HANDLE,
        INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
        },
        JobObjects::{IsProcessInJob, OpenJobObjectW},
        Pipes::PeekNamedPipe,
        Threading::{
            GetProcessIdOfThread, GetProcessTimes, GetThreadTimes, OpenProcess, OpenThread,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
            ResumeThread, SuspendThread, THREAD_QUERY_LIMITED_INFORMATION, THREAD_SUSPEND_RESUME,
            TerminateProcess, WaitForSingleObject,
        },
    },
};
type ProofResult<T> = Result<T, String>;
const MIB: u64 = 1024 * 1024;
const DEPS: &str = "D:/Projects/horness-launcher/sidecar/source-build/node_modules";
fn safe<T, E>(value: Result<T, E>, code: &'static str) -> ProofResult<T> {
    value.map_err(|_| code.into())
}
fn check(value: bool, code: &'static str) -> ProofResult<()> {
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
        queue_bytes: 256 * 1024,
        queue_items: 4,
    }
}
struct Fixture {
    root: PathBuf,
    released: bool,
}
impl Fixture {
    fn new() -> Self {
        Self {
            root: std::env::temp_dir().join(format!(
                "shinatama-sidecar-supervisor-{}",
                uuid::Uuid::new_v4()
            )),
            released: false,
        }
    }
    fn location(&self) -> StorageLocation {
        StorageLocation::resolve(LocationRequest {
            local_data_directory: self.root.clone(),
            home_override: Some(self.root.join("private")),
            project_roots: vec![workspace()],
            synchronized_roots: vec![],
        })
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let parent = std::env::temp_dir();
        if self.released
            && self.root.parent() == Some(parent.as_path())
            && self
                .root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("shinatama-sidecar-supervisor-")
        {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
struct Provider {
    db: Arc<Database>,
    coordinator: Arc<AdmissionCoordinator>,
    stamp: PolicyStamp,
    calls: Arc<AtomicU32>,
    jobs: Arc<Mutex<Vec<OwnedJobKey>>>,
    finished: mpsc::SyncSender<()>,
}
impl WorkloadProvider for Provider {
    fn database(&self) -> &Database {
        &self.db
    }
    fn prepare(&mut self, at: i64, until: i64) -> Result<PreparedWorkload, AdmissionUnavailable> {
        let count = self.calls.fetch_add(1, Ordering::AcqRel);
        // A broken restart loop fails visibly at3, with a finite fixture ceiling4.
        if count >= 4 {
            return Err(AdmissionUnavailable);
        }
        let sampled = WindowsSampler::new()
            .sample(&[])
            .map_err(|_| AdmissionUnavailable)?;
        let Measurement::Available(system) = sampled.system else {
            return Err(AdmissionUnavailable);
        };
        if system.physical_available_bytes < budget().memory_bytes + 512 * MIB
            || system
                .commit_limit_bytes
                .saturating_sub(system.commit_total_bytes)
                < budget().memory_bytes + 1024 * MIB
        {
            return Err(AdmissionUnavailable);
        }
        let revision = self
            .db
            .resource_reservations()
            .map_err(|_| AdmissionUnavailable)?
            .ok_or(AdmissionUnavailable)?
            .revision;
        let prepared = self
            .coordinator
            .prepare(
                &self.db,
                &self.stamp,
                revision,
                PrepareRequest {
                    budget: budget(),
                    process_commit_bytes: 768 * MIB,
                    now_ms: at,
                    lease_expires_at_ms: until,
                },
            )
            .map_err(|_| AdmissionUnavailable)?;
        self.jobs
            .lock()
            .map_err(|_| AdmissionUnavailable)?
            .push(prepared.owned_key());
        Ok(prepared)
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        let _ = self.finished.try_send(());
    }
}
fn drain(pipe: &mut File, total: &mut usize) -> ProofResult<()> {
    let mut buffer = [0; 4096];
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
                Err("peek synthetic preparation output".into())
            };
        }
        if available == 0 {
            return Ok(());
        }
        let count = (available as usize).min(buffer.len());
        *total += safe(
            pipe.read(&mut buffer[..count]),
            "read synthetic preparation output",
        )?;
        check(*total <= 8192, "synthetic preparation output exceeded8KiB")?;
    }
    Ok(())
}
fn setup_ca(
    recipe: &shinatama_engine::sidecar_runtime::SidecarRuntimeRecipe,
    private: &Path,
    db: &Database,
    coordinator: &AdmissionCoordinator,
    stamp: &PolicyStamp,
    deadline: Instant,
) -> ProofResult<()> {
    let at = now();
    let revision = safe(db.resource_reservations(), "read setup ledger")?
        .unwrap()
        .revision;
    let workload = safe(
        coordinator.prepare(
            db,
            stamp,
            revision,
            PrepareRequest {
                budget: budget(),
                process_commit_bytes: 768 * MIB,
                now_ms: at,
                lease_expires_at_ms: at + 30_000,
            },
        ),
        "prepare synthetic CA job",
    )?;
    let command = safe(
        NativeCommand::new(
            recipe.executable().into(),
            vec![
                workspace()
                    .join("crates/engine/tests/fixtures/sidecar-runtime.mjs")
                    .into_os_string(),
                "probe".into(),
                workspace()
                    .join("sidecar/source-build/prepared")
                    .into_os_string(),
                private.as_os_str().into(),
                DEPS.into(),
            ],
            recipe.working_directory().into(),
            recipe
                .child_environment()
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .collect(),
        ),
        "prepare synthetic CA command",
    )?;
    let mut child = match spawn_contained(command, workload, db, at, at + 30_000) {
        Ok(child) => child,
        Err(mut failure) => {
            let _ = failure.workload.finish_cleanup(db, Duration::from_secs(2));
            return Err("synthetic CA startup failed".into());
        }
    };
    let credit = safe(
        child.process.queue_credits().acquire(8192, 2),
        "reserve synthetic CA pipe credits",
    )?;
    let result = (|| -> ProofResult<()> {
        let mut output = 0;
        let mut errors = 0;
        loop {
            check(Instant::now() < deadline, "CA setup deadline")?;
            drain(&mut child.stdout, &mut output)?;
            drain(&mut child.stderr, &mut errors)?;
            let sample = safe(
                child.process.sample(&WindowsSampler::new()),
                "sample CA setup job",
            )?;
            check(sample.active_processes <= 8, "CA setup raw count exceeded8")?;
            if let Some(code) = safe(child.process.try_wait(), "wait CA setup")? {
                check(code == 0, "CA setup exited unsuccessfully")?;
                return Ok(());
            }
            thread::sleep(Duration::from_millis(5));
        }
    })();
    drop(credit);
    drop((child.stdin, child.stdout, child.stderr));
    let released = matches!(
        child.process.finish_cleanup(db, Duration::from_secs(2)),
        Ok(CleanupStatus::Released { .. })
    );
    result?;
    check(released, "CA setup strict cleanup incomplete")
}
fn http(port: NonZeroU16, path: &str) -> ProofResult<Value> {
    let mut stream = safe(
        TcpStream::connect_timeout(
            &([127, 0, 0, 1], port.get()).into(),
            Duration::from_millis(300),
        ),
        "connect private CLI health",
    )?;
    safe(
        stream.set_read_timeout(Some(Duration::from_millis(500))),
        "bound health read",
    )?;
    safe(
        stream.set_write_timeout(Some(Duration::from_millis(300))),
        "bound health write",
    )?;
    safe(
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            port.get()
        ),
        "request private health",
    )?;
    let mut bytes = Vec::new();
    safe(
        stream.take(16385).read_to_end(&mut bytes),
        "read private health",
    )?;
    check(bytes.len() <= 16384, "private health exceeded cap")?;
    let split = bytes
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .ok_or("invalid health framing")?;
    let header = safe(
        std::str::from_utf8(&bytes[..split]),
        "invalid health header",
    )?;
    check(
        header.starts_with("HTTP/1.1 200"),
        "private health not ready",
    )?;
    let body = &bytes[split + 4..];
    let mut decoded = Vec::new();
    if header
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        let mut cursor = 0;
        for _ in 0..32 {
            let end = body[cursor..]
                .windows(2)
                .position(|part| part == b"\r\n")
                .ok_or("invalid chunk")?
                + cursor;
            let count = usize::from_str_radix(
                safe(
                    std::str::from_utf8(&body[cursor..end]),
                    "invalid chunk length",
                )?,
                16,
            )
            .map_err(|_| "invalid chunk length")?;
            cursor = end + 2;
            if count == 0 {
                break;
            }
            check(cursor + count + 2 <= body.len(), "truncated chunk")?;
            decoded.extend_from_slice(&body[cursor..cursor + count]);
            cursor += count + 2;
        }
    } else {
        decoded.extend_from_slice(body);
    }
    safe(serde_json::from_slice(&decoded), "invalid health JSON")
}
fn wait_status(
    supervisor: &SidecarSupervisor,
    deadline: Instant,
    expected: Phase,
    condition: impl Fn(&Snapshot) -> bool,
) -> ProofResult<Snapshot> {
    loop {
        check(Instant::now() < deadline, "supervisor observation deadline")?;
        if let Some(snapshot) = supervisor.status() {
            if snapshot.phase == expected && condition(&snapshot) {
                return Ok(snapshot);
            }
            check(
                snapshot.phase != Phase::Retired,
                "supervisor retained uncertain resources",
            )?;
            if snapshot.phase == Phase::Failed && expected != Phase::Stopped {
                return Err(format!(
                    "supervisor failed before expected state: {:?}",
                    snapshot.reason
                ));
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
}
fn handle(raw: HANDLE) -> ProofResult<OwnedHandle> {
    check(
        !raw.is_null() && raw != INVALID_HANDLE_VALUE,
        "open own native handle",
    )?;
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}
fn filetime(value: FILETIME) -> u64 {
    (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
}
struct ExactChild {
    process: OwnedHandle,
    pid: u32,
    created: u64,
}
impl ExactChild {
    fn open(pid: u32, owned: OwnedJobKey) -> ProofResult<Self> {
        let process = handle(unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE | PROCESS_TERMINATE,
                0,
                pid,
            )
        })?;
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        check(
            unsafe {
                GetProcessTimes(
                    process.as_raw_handle(),
                    &mut creation,
                    &mut exit,
                    &mut kernel,
                    &mut user,
                )
            } != 0,
            "read own process identity",
        )?;
        let hex: String = owned
            .job_key
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let name: Vec<u16> = format!("Local\\Shinatama.Workload.{hex}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let job = handle(unsafe { OpenJobObjectW(0x0004, 0, name.as_ptr()) })?;
        let mut inside = 0;
        check(
            unsafe { IsProcessInJob(process.as_raw_handle(), job.as_raw_handle(), &mut inside) }
                != 0
                && inside != 0,
            "health PID is not in own exact job",
        )?;
        check(
            unsafe { WaitForSingleObject(process.as_raw_handle(), 0) } == WAIT_TIMEOUT,
            "own child already exited",
        )?;
        Ok(Self {
            process,
            pid,
            created: filetime(creation),
        })
    }
    fn pause_main(&self) -> ProofResult<PausedThread> {
        let snapshot = handle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) })?;
        let mut entry = THREADENTRY32 {
            dwSize: size_of::<THREADENTRY32>() as u32,
            cntUsage: 0,
            th32ThreadID: 0,
            th32OwnerProcessID: 0,
            tpBasePri: 0,
            tpDeltaPri: 0,
            dwFlags: 0,
        };
        check(
            unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) } != 0,
            "enumerate own threads",
        )?;
        let mut earliest: Option<(u64, OwnedHandle)> = None;
        let mut complete = false;
        let mut own_count = 0;
        for _ in 0..8192 {
            if entry.th32OwnerProcessID == self.pid {
                own_count += 1;
                check(own_count <= 256, "own thread census exceeded cap")?;
                let thread = handle(unsafe {
                    OpenThread(
                        THREAD_QUERY_LIMITED_INFORMATION | THREAD_SUSPEND_RESUME,
                        0,
                        entry.th32ThreadID,
                    )
                })?;
                check(
                    unsafe { GetProcessIdOfThread(thread.as_raw_handle()) } == self.pid,
                    "thread does not belong to own child",
                )?;
                let mut creation = FILETIME::default();
                let mut exit = FILETIME::default();
                let mut kernel = FILETIME::default();
                let mut user = FILETIME::default();
                check(
                    unsafe {
                        GetThreadTimes(
                            thread.as_raw_handle(),
                            &mut creation,
                            &mut exit,
                            &mut kernel,
                            &mut user,
                        )
                    } != 0,
                    "read own thread identity",
                )?;
                let at = filetime(creation);
                check(at >= self.created, "thread predates exact child")?;
                if earliest.as_ref().is_none_or(|(old, _)| at < *old) {
                    earliest = Some((at, thread));
                }
            }
            if unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) } == 0 {
                complete = unsafe { GetLastError() } == ERROR_NO_MORE_FILES;
                break;
            }
        }
        check(complete, "thread census truncated")?;
        check(
            unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } == WAIT_TIMEOUT,
            "own child exited before pause",
        )?;
        let (_, thread) = earliest.ok_or("own main thread unavailable")?;
        let previous = unsafe { SuspendThread(thread.as_raw_handle()) };
        check(previous != u32::MAX, "suspend exact private main thread")?;
        let paused = PausedThread {
            thread,
            active: true,
        };
        check(previous == 0, "private main thread already suspended")?;
        Ok(paused)
    }
    fn crash(&self) -> ProofResult<()> {
        check(
            unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } == WAIT_TIMEOUT,
            "exact child already ended",
        )?;
        check(
            unsafe { TerminateProcess(self.process.as_raw_handle(), 19) } != 0,
            "terminate exact private child handle",
        )?;
        check(
            unsafe { WaitForSingleObject(self.process.as_raw_handle(), 2000) } == WAIT_OBJECT_0,
            "wait exact private child exit",
        )
    }
}
struct PausedThread {
    thread: OwnedHandle,
    active: bool,
}
impl PausedThread {
    fn resume(mut self) -> ProofResult<()> {
        let count = unsafe { ResumeThread(self.thread.as_raw_handle()) };
        if count != u32::MAX {
            self.active = false;
        }
        check(count == 1, "resume exact private main thread")
    }
}
impl Drop for PausedThread {
    fn drop(&mut self) {
        if self.active {
            unsafe {
                ResumeThread(self.thread.as_raw_handle());
            }
        }
    }
}

#[test]
#[ignore = "local native supervisor proof; requires pinned Node, prepared bundle and offline synthetic CA tool"]
fn real_owner_busy_keeps_identity_and_exits_restart_once_on_same_port() {
    let mut fixture = Fixture::new();
    let result = qualify(&fixture);
    let evidence = match &result {
        Ok(value) => value.clone(),
        Err(error) => json!({"passed":false,"failure":error,"nativeAgentStartEnabled":false}),
    };
    let target = workspace().join(".prototype-evidence/sidecar-supervisor.json");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    if result.is_ok() {
        fixture.released = true;
    }
    assert!(result.is_ok(), "{}", result.err().unwrap_or_default());
}
fn qualify(fixture: &Fixture) -> ProofResult<Value> {
    let location = fixture.location();
    let db = Arc::new(safe(
        Database::open(&location, None),
        "open private supervisor ledger",
    )?);
    let reserved = safe(
        TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)),
        "reserve private CLI port",
    )?;
    let port = NonZeroU16::new(reserved.local_addr().unwrap().port()).unwrap();
    drop(reserved);
    let config = safe(
        prepare_private_sidecar(&location, port),
        "prepare private sidecar",
    )?;
    let marker = config.root().directory().join("data/saved-marker.txt");
    safe(
        fs::write(&marker, b"saved-before-supervision"),
        "write private saved marker",
    )?;
    let bundle = safe(
        verify_prepared_bundle(&workspace().join("sidecar/source-build/prepared")),
        "verify source and CLI pin",
    )?;
    let node = safe(
        verify_bundled_node(
            &workspace().join("sidecar/runtime"),
            NodePin {
                relative_path: "node.exe".into(),
                sha256: "58e74bf02fc5bbacc41dcb8bef089961cd5bddd37830b87784e4fc624d145d1f".into(),
                version: Some("24.14.1".into()),
                modules_abi: Some(137),
            },
        ),
        "verify Node pin",
    )?;
    let os: BTreeMap<OsString, OsString> = ["SystemRoot", "WINDIR"]
        .into_iter()
        .filter_map(|key| std::env::var_os(key).map(|value| (key.into(), value)))
        .collect();
    let recipe = safe(
        prepare_runtime(bundle, node, config.root(), &os, port),
        "prepare verified child recipe",
    )?;
    let stamp = PolicyStamp {
        version: 1,
        id: "sidecar-supervisor-proof-v1".into(),
    };
    safe(
        db.initialize_resource_policy(&ReservationPolicy {
            stamp: stamp.clone(),
            aggregate_ceiling: budget(),
            per_slot_ceiling: budget(),
            max_slots: 1,
            max_lease_ms: 30_000,
        }),
        "install frozen fixture policy",
    )?;
    let coordinator = Arc::new(safe(
        AdmissionCoordinator::new(1),
        "create fixed fixture slot",
    )?);
    let deadline = Instant::now() + Duration::from_secs(30);
    setup_ca(
        &recipe,
        config.root().directory(),
        &db,
        &coordinator,
        &stamp,
        deadline,
    )?;
    let calls = Arc::new(AtomicU32::new(0));
    let jobs = Arc::new(Mutex::new(Vec::with_capacity(4)));
    let (finished, closed) = mpsc::sync_channel(1);
    let provider = Provider {
        db: db.clone(),
        coordinator: coordinator.clone(),
        stamp,
        calls: calls.clone(),
        jobs: jobs.clone(),
        finished,
    };
    let supervisor = safe(
        SidecarSupervisor::start(
            recipe,
            SupervisorConfig {
                port,
                poll_period: Duration::from_millis(50),
                health_period: Duration::from_millis(200),
                health_timeout: Duration::from_millis(150),
                startup_timeout: Duration::from_secs(3),
                graceful_stop_timeout: Duration::from_secs(1),
                cleanup_timeout: Duration::from_secs(2),
                reservation_lease: Duration::from_secs(15),
                output_bytes_per_stream: 1024 * 1024,
            },
            provider,
        ),
        "start sole owner worker",
    )?;
    let proof = (|| -> ProofResult<Value> {
        safe(
            supervisor.request(Command::Start),
            "request first proxy start",
        )?;
        wait_status(&supervisor, deadline, Phase::Ready, |value| {
            value.phase == Phase::Ready && value.healthy
        })?;
        check(
            calls.load(Ordering::Acquire) == 1,
            "first start reserved more than once",
        )?;
        let pid = http(port, "/__bili/health")?["pid"]
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .ok_or("missing actual CLI pid")?;
        let first_key = jobs.lock().unwrap()[0];
        let first = ExactChild::open(pid, first_key)?;
        let paused = first.pause_main()?;
        let busy = wait_status(
            &supervisor,
            deadline.min(Instant::now() + Duration::from_secs(2)),
            Phase::Busy,
            |value| value.phase == Phase::Busy && !value.healthy && value.port_held == Some(true),
        )?;
        thread::sleep(Duration::from_millis(200));
        let fresh = supervisor.status().ok_or("busy snapshot unavailable")?;
        check(
            fresh.phase == Phase::Busy && fresh.observed_at_unix_ms > busy.observed_at_unix_ms,
            "busy worker heartbeat did not advance during backoff",
        )?;
        check(
            calls.load(Ordering::Acquire) == 1
                && jobs.lock().unwrap().len() == 1
                && !fresh.restart_used,
            "busy sidecar caused a replacement",
        )?;
        paused.resume()?;
        let recovered = wait_status(&supervisor, deadline, Phase::Ready, |value| {
            value.phase == Phase::Ready && value.healthy
        })?;
        check(
            recovered.health_probe_delay_ms == 200,
            "healthy response did not reset probe backoff",
        )?;
        check(
            http(port, "/__bili/health")?["pid"].as_u64() == Some(pid.into()),
            "busy recovery changed process identity",
        )?;
        first.crash()?;
        wait_status(&supervisor, deadline, Phase::Ready, |value| {
            value.phase == Phase::Ready && value.healthy && value.restart_used
        })?;
        check(
            calls.load(Ordering::Acquire) == 2 && jobs.lock().unwrap().len() == 2,
            "unexpected exit did not restart exactly once",
        )?;
        let second_pid = http(port, "/__bili/health")?["pid"]
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .ok_or("missing replacement CLI pid")?;
        let second_key = jobs.lock().unwrap()[1];
        check(
            first_key.job_key != second_key.job_key,
            "replacement reused native job identity",
        )?;
        let second = ExactChild::open(second_pid, second_key)?;
        check(
            second.created != first.created || second_pid != pid,
            "replacement reused process identity",
        )?;
        second.crash()?;
        let failed = wait_status(&supervisor, deadline, Phase::Failed, |value| {
            value.phase == Phase::Failed && value.reason == Reason::RepeatedExit
        })?;
        thread::sleep(Duration::from_millis(300));
        check(
            calls.load(Ordering::Acquire) == 2,
            "repeated exit started a third process",
        )?;
        check(
            safe(fs::read(&marker), "read saved marker")? == b"saved-before-supervision",
            "saved fixture work changed",
        )?;
        Ok(
            json!({"passed":true,"actualPinnedCli":true,"busyKeptExactOwner":true,"busyProbeDelayMs":busy.health_probe_delay_ms,
            "busyHeartbeatAdvanced":true,"healthyRecoveryResetDelayMs":recovered.health_probe_delay_ms,"automaticRestarts":1,
            "sameAddressAfterRestart":true,"secondExitPhase":failed.phase,"secondExitReason":failed.reason,"prepareCalls":calls.load(Ordering::Acquire),
            "savedMarkerPreserved":true,"agentCommandsReplayed":false,"nativeLimits":budget(),"perProcessCommitBytes":768*MIB,"wallBudgetSeconds":30,
            "limits":["One Windows pinned CLI, finite main-thread pause and two controlled exits; no extended soak or provider traffic."]}),
        )
    })();
    let stop = safe(
        supervisor.request(Command::Stop),
        "request final owner stop",
    );
    let stopped = wait_status(&supervisor, deadline, Phase::Stopped, |value| {
        value.phase == Phase::Stopped
    });
    drop(supervisor);
    let worker_closed = safe(
        closed.recv_timeout(Duration::from_secs(2)),
        "sole owner worker did not close",
    );
    let empty = safe(db.resource_reservations(), "read final supervisor ledger")?
        .unwrap()
        .slots
        .is_empty();
    let mut result = proof?;
    stop?;
    stopped?;
    worker_closed?;
    check(empty, "supervisor retained a reservation")?;
    result["allNativeJobsClosedAndLedgerReleased"] = json!(true);
    result["ownerWorkerClosed"] = json!(true);
    Ok(result)
}
