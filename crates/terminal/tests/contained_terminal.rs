#![cfg(windows)]
//! Real contained ConPTY, synthetic commands only. Frozen bounds precede faults.
use anyhow::{Context, Result, anyhow, bail};
use shinatama_engine::resources::{
    admission::{
        AdmissionCoordinator, CleanupStatus, PrepareRequest, PreparedWorkload, QueueStatus,
    },
    job::{JobSnapshot, OwnedJobKey},
    sampling::{Measurement, ProcessKey},
};
use shinatama_state::{
    database::Database,
    location::{LocationRequest, StorageLocation},
    resource_reservations::{Budget, PolicyStamp, ReservationPolicy, ReservationState},
};
use shinatama_terminal::{
    SessionInfo, TerminalDirective, TerminalHost, TerminalResourceReason, WindowsTerminalAdmission,
    WindowsTerminalConfig,
};
use std::{
    fs,
    os::windows::io::{AsRawHandle, BorrowedHandle},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, FILETIME, HANDLE, WAIT_OBJECT_0,
        WAIT_TIMEOUT,
    },
    System::Threading::{
        GetCurrentProcess, GetProcessTimes, TerminateProcess, WaitForSingleObject,
    },
};

const MIB: u64 = 1024 * 1024;
const STARTUP: Duration = Duration::from_secs(20);
const RTT: Duration = Duration::from_millis(1500);
const CAPTURE: usize = 512 * 1024;
const SENTINEL: &[u8] = b"synthetic saved work remains unchanged\n";
fn budget() -> Budget {
    Budget {
        memory_bytes: 512 * MIB,
        processes: 8,
        cpu_basis_points: 1000,
        queue_bytes: 4 * MIB,
        queue_items: 256,
    }
}
fn now_ms() -> Result<i64> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}

struct Handle(HANDLE);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct CapturedHelper {
    key: ProcessKey,
    owned: OwnedJobKey,
    handle: Handle,
}
impl CapturedHelper {
    fn verify(&self) -> Result<()> {
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        if unsafe {
            GetProcessTimes(
                self.handle.0,
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error()).context("verify synthetic helper handle");
        }
        let creation = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        if creation != self.key.created_at_100ns {
            bail!("Synthetic helper creation identity changed");
        }
        Ok(())
    }
    fn crash_exact(&self) -> Result<()> {
        self.verify()?;
        if unsafe { WaitForSingleObject(self.handle.0, 0) } != WAIT_TIMEOUT {
            bail!("Synthetic helper was not alive for the injected crash");
        }
        if unsafe { TerminateProcess(self.handle.0, 71) } == 0 {
            return Err(std::io::Error::last_os_error())
                .context("stop exact owned synthetic helper");
        }
        Ok(())
    }
    fn wait(&self) -> Result<()> {
        self.verify()?;
        if unsafe { WaitForSingleObject(self.handle.0, 3000) } != WAIT_OBJECT_0 {
            bail!("Synthetic helper death was not kernel-confirmed");
        }
        Ok(())
    }
}
struct Admission {
    database: Arc<Database>,
    coordinator: AdmissionCoordinator,
    stamp: PolicyStamp,
    helpers: Mutex<Vec<Arc<CapturedHelper>>>,
    evidence: Mutex<Vec<JobEvidence>>,
}
#[derive(Clone)]
struct JobEvidence {
    owned: OwnedJobKey,
    peak_job_commit: u64,
    peak_process_commit: u64,
    peak_processes: u32,
    complete_samples: u32,
    memory_cap_violated: bool,
    process_guard_stopped: bool,
    snapshot: JobSnapshot,
}
impl Admission {
    fn latest_evidence(&self) -> Result<JobEvidence> {
        self.evidence
            .lock()
            .map_err(|_| anyhow!("synthetic telemetry poisoned"))?
            .last()
            .cloned()
            .context("missing actual terminal job sample")
    }
}
impl WindowsTerminalAdmission for Admission {
    fn prepare(&self) -> Result<PreparedWorkload> {
        let revision = self
            .database
            .resource_reservations()?
            .context("missing frozen test policy")?
            .revision;
        let now = now_ms()?;
        self.coordinator
            .prepare(
                &self.database,
                &self.stamp,
                revision,
                PrepareRequest {
                    budget: budget(),
                    process_commit_bytes: 256 * MIB,
                    now_ms: now,
                    lease_expires_at_ms: now + 120_000,
                },
            )
            .map_err(|error| anyhow!("synthetic admission: {error:?}"))
    }
    fn activate(
        &self,
        workload: &mut PreparedWorkload,
        process: BorrowedHandle<'_>,
    ) -> Result<ProcessKey> {
        let now = now_ms()?;
        let key = workload
            .activate_verified(&self.database, process, now, now + 120_000)
            .map_err(|error| anyhow!("synthetic activation: {error:?}"))?;
        let mut duplicate = std::ptr::null_mut();
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                process.as_raw_handle() as HANDLE,
                GetCurrentProcess(),
                &mut duplicate,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error())
                .context("retain exact synthetic helper handle");
        }
        let captured = Arc::new(CapturedHelper {
            key,
            owned: workload.owned_key(),
            handle: Handle(duplicate),
        });
        captured.verify()?;
        let mut helpers = self
            .helpers
            .lock()
            .map_err(|_| anyhow!("synthetic helper registry poisoned"))?;
        if helpers.len() >= 4 {
            bail!("Unexpected helper replacement/activation");
        }
        helpers.push(captured);
        Ok(key)
    }
    fn cleanup(&self, workload: &mut PreparedWorkload, timeout: Duration) -> Result<CleanupStatus> {
        workload
            .finish_cleanup(&self.database, timeout)
            .map_err(|error| anyhow!("synthetic cleanup: {error:?}"))
    }
    fn observe(
        &self,
        owned: OwnedJobKey,
        snapshot: &JobSnapshot,
        _queues: QueueStatus,
    ) -> Result<TerminalDirective> {
        if !self
            .helpers
            .lock()
            .map_err(|_| anyhow!("synthetic helper registry poisoned"))?
            .iter()
            .any(|helper| helper.owned == owned)
        {
            bail!("refuse to attribute an unregistered job");
        }
        let mut evidence = self
            .evidence
            .lock()
            .map_err(|_| anyhow!("synthetic telemetry poisoned"))?;
        let index = match evidence.iter().position(|item| item.owned == owned) {
            Some(index) => index,
            None => {
                if evidence.len() >= 4 {
                    bail!("synthetic telemetry history exceeded its bound");
                }
                evidence.push(JobEvidence {
                    owned,
                    peak_job_commit: 0,
                    peak_process_commit: 0,
                    peak_processes: 0,
                    complete_samples: 0,
                    memory_cap_violated: false,
                    process_guard_stopped: false,
                    snapshot: snapshot.clone(),
                });
                evidence.len() - 1
            }
        };
        let evidence = &mut evidence[index];
        let process_commit = snapshot
            .processes
            .iter()
            .filter_map(|process| match process.counters {
                Measurement::Available(counters) => Some(counters.private_commit_bytes),
                Measurement::Unavailable(_) => None,
            })
            .max()
            .unwrap_or(0);
        evidence.peak_job_commit = evidence.peak_job_commit.max(snapshot.peak_job_commit_bytes);
        evidence.peak_process_commit = evidence.peak_process_commit.max(process_commit);
        evidence.peak_processes = evidence.peak_processes.max(snapshot.active_processes);
        evidence.complete_samples += u32::from(snapshot.membership_complete);
        evidence.memory_cap_violated |=
            snapshot.peak_job_commit_bytes > 512 * MIB || process_commit > 256 * MIB;
        evidence.snapshot = snapshot.clone();
        // The physical count is raw, including conhost. Quota8 stays applied;
        // known overshoot must stop only the already-registered responsible job.
        let stop = snapshot.membership_complete && snapshot.active_processes > budget().processes;
        evidence.process_guard_stopped |= stop;
        Ok(if stop {
            TerminalDirective::StopResponsible
        } else {
            TerminalDirective::Continue
        })
    }
}
struct TempRoot {
    root: PathBuf,
    base: PathBuf,
}
impl TempRoot {
    fn new() -> Result<Self> {
        let base = fs::canonicalize(std::env::temp_dir())?;
        let root = base.join(format!(
            "shinatama-contained-terminal-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&root)?;
        Ok(Self { root, base })
    }
}
impl Drop for TempRoot {
    fn drop(&mut self) {
        if self.root.is_absolute()
            && self.root.parent() == Some(self.base.as_path())
            && self.root.file_name().is_some_and(|name| {
                name.to_string_lossy()
                    .starts_with("shinatama-contained-terminal-")
            })
        {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
#[derive(Default)]
struct Transcript {
    tail: Vec<u8>,
    cursor_tail: Vec<u8>,
    bytes: u64,
    eof: bool,
    exit: Option<u32>,
    errors: usize,
    last_error: Option<String>,
}
impl Transcript {
    fn poll(&mut self, host: &TerminalHost, info: &SessionInfo, healthy: bool) -> Result<()> {
        let batch = host.read(&info.id)?;
        if let Some(error) = &batch.error {
            self.errors += 1;
            self.last_error = Some(error.chars().take(1024).collect());
            if healthy {
                let error = error.chars().take(1024).collect::<String>();
                let tail =
                    String::from_utf8_lossy(&self.tail[self.tail.len().saturating_sub(4096)..]);
                bail!(
                    "Healthy terminal reported an output error: {error}; bounded synthetic tail={tail:?}"
                );
            }
        }
        for bytes in &batch.chunks {
            self.bytes += bytes.len() as u64;
            let mut scan = std::mem::take(&mut self.cursor_tail);
            scan.extend_from_slice(bytes);
            let queries = scan
                .windows(4)
                .filter(|window| *window == b"\x1b[6n")
                .count();
            if queries > 8 {
                bail!("Unexpected synthetic cursor query flood");
            }
            for _ in 0..queries {
                host.write(&info.id, b"\x1b[1;1R".to_vec())?;
            }
            self.cursor_tail = scan[scan.len().saturating_sub(3)..].to_vec();
            self.tail.extend_from_slice(bytes);
        }
        if self.tail.len() > CAPTURE {
            self.tail.drain(..self.tail.len() - CAPTURE);
        }
        self.eof |= batch.eof;
        self.exit = batch.exit_code.or(self.exit);
        if healthy && (self.eof || self.exit.is_some()) {
            bail!("Healthy sibling stopped during a synthetic fault");
        }
        Ok(())
    }
    fn has(&self, marker: &str) -> bool {
        self.tail
            .windows(marker.len())
            .any(|window| window == marker.as_bytes())
    }
    fn count(&self, marker: &str) -> usize {
        self.tail
            .windows(marker.len())
            .filter(|window| *window == marker.as_bytes())
            .count()
    }
    fn codes(&self, marker: &str) -> Vec<u32> {
        String::from_utf8_lossy(&self.tail)
            .split(marker)
            .skip(1)
            .take(16)
            .filter_map(|suffix| suffix.strip_prefix(':'))
            .filter_map(|suffix| {
                suffix
                    .split(|c: char| !c.is_ascii_digit())
                    .next()?
                    .parse()
                    .ok()
            })
            .collect()
    }
}
fn send_marker(host: &TerminalHost, info: &SessionInfo, prefix: &str) -> Result<String> {
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let expected = format!("{prefix}{nonce}");
    // The echoed source never contains the full expected output marker.
    host.write(
        &info.id,
        format!("Write-Output ('{prefix}' + '{nonce}')\r").into_bytes(),
    )?;
    Ok(expected)
}
fn marker(
    host: &TerminalHost,
    info: &SessionInfo,
    trace: &mut Transcript,
    other: Option<(&SessionInfo, &mut Transcript)>,
    expected: &str,
    timeout: Duration,
    healthy: bool,
) -> Result<Duration> {
    let started = Instant::now();
    let mut other = other;
    while started.elapsed() < timeout {
        trace.poll(host, info, healthy)?;
        if let Some((info, trace)) = other.as_mut() {
            trace.poll(host, info, false)?;
        }
        if trace.has(expected) {
            return Ok(started.elapsed());
        }
        thread::sleep(Duration::from_millis(5));
    }
    bail!("Synthetic terminal marker missed its frozen deadline")
}
fn ping(
    host: &TerminalHost,
    healthy: &SessionInfo,
    trace: &mut Transcript,
    other: Option<(&SessionInfo, &mut Transcript)>,
) -> Result<Duration> {
    let started = Instant::now();
    let expected = send_marker(host, healthy, "SHN_HEALTHY_")?;
    let remaining = RTT.saturating_sub(started.elapsed());
    marker(host, healthy, trace, other, &expected, remaining, true)?;
    Ok(started.elapsed())
}
fn helper_path() -> Result<PathBuf> {
    if let Some(path) = option_env!("CARGO_BIN_EXE_shinatama-terminal-helper") {
        return Ok(PathBuf::from(path));
    }
    let executable = std::env::current_exe()?;
    Ok(executable
        .parent()
        .and_then(|parent| parent.parent())
        .context("test binary directory")?
        .join("shinatama-terminal-helper.exe"))
}

fn applied_limits(host: &TerminalHost, info: &SessionInfo) -> Result<()> {
    let status = host
        .resources()
        .into_iter()
        .find(|status| status.id == info.id)
        .context("actual resource status was not exposed for the owned terminal")?;
    let applied = status
        .applied
        .context("native applied limits were not cached")?;
    assert_eq!(applied.job_commit_bytes, 512 * MIB);
    assert_eq!(applied.process_commit_bytes, 256 * MIB);
    assert_eq!(applied.processes, 8);
    assert_eq!(applied.cpu_basis_points, 1000);
    assert!(status.observed.is_some());
    assert!(
        status.sample_age_ms.is_some(),
        "missing samples must remain explicit"
    );
    Ok(())
}

/// Each iteration sends a new command to the healthy sibling while the fault
/// stays admitted. A cached prompt/output marker cannot satisfy a fresh ping.
fn fault_until(
    host: &TerminalHost,
    healthy: &SessionInfo,
    healthy_trace: &mut Transcript,
    fault: &SessionInfo,
    fault_trace: &mut Transcript,
    expected: &str,
    replies: &mut Vec<Duration>,
) -> Result<()> {
    let started = Instant::now();
    while started.elapsed() < STARTUP {
        fault_trace.poll(host, fault, false)?;
        if fault_trace.has(expected) {
            return Ok(());
        }
        if fault_trace.eof || fault_trace.exit.is_some() {
            let tail = String::from_utf8_lossy(
                &fault_trace.tail[fault_trace.tail.len().saturating_sub(4096)..],
            );
            // Who ended it: the exit code, the host's errors and its last resource reason.
            let status = host
                .resources()
                .into_iter()
                .find(|status| status.id == fault.id);
            bail!(
                "Synthetic resource fault ended before printing {expected}; eof={} exit={:?} \
                 errors={} last_error={:?} status={status:?}; tail={tail:?}",
                fault_trace.eof,
                fault_trace.exit,
                fault_trace.errors,
                fault_trace.last_error,
            );
        }
        replies.push(ping(
            host,
            healthy,
            healthy_trace,
            Some((fault, fault_trace)),
        )?);
        thread::sleep(Duration::from_millis(20));
    }
    let tail =
        String::from_utf8_lossy(&fault_trace.tail[fault_trace.tail.len().saturating_sub(4096)..]);
    bail!("Synthetic resource refusal missed its frozen deadline; tail={tail:?}")
}

#[test]
fn real_contained_terminal_faults_keep_a_fresh_sibling_responsive_and_saved_work_intact()
-> Result<()> {
    let temporary = TempRoot::new()?;
    let project = temporary.root.join("project");
    fs::create_dir(&project)?;
    let saved = project.join("saved-fixture.txt");
    fs::write(&saved, SENTINEL)?;
    let location = StorageLocation::resolve(LocationRequest {
        local_data_directory: temporary.root.clone(),
        home_override: Some(temporary.root.join("private-state")),
        project_roots: vec![project.clone()],
        synchronized_roots: Vec::new(),
    })?;
    let database = Arc::new(Database::open(&location, None)?);
    let stamp = PolicyStamp {
        version: 1,
        id: "contained-terminal-frozen-v1".into(),
    };
    let per_slot = budget();
    database.initialize_resource_policy(&ReservationPolicy {
        stamp: stamp.clone(),
        per_slot_ceiling: per_slot,
        aggregate_ceiling: Budget {
            memory_bytes: 4 * per_slot.memory_bytes,
            processes: 4 * per_slot.processes,
            cpu_basis_points: 4000,
            queue_bytes: 4 * per_slot.queue_bytes,
            queue_items: 4 * per_slot.queue_items,
        },
        max_slots: 4,
        max_lease_ms: 120_000,
    })?;
    let admission = Arc::new(Admission {
        database: Arc::clone(&database),
        coordinator: AdmissionCoordinator::new(4)
            .map_err(|error| anyhow!("test pool: {error:?}"))?,
        stamp,
        helpers: Mutex::new(Vec::new()),
        evidence: Mutex::new(Vec::new()),
    });
    let helper = helper_path()?;
    if !helper.is_file() {
        bail!("Build shinatama-terminal-helper before this native test");
    }
    let shell = PathBuf::from(std::env::var_os("SystemRoot").context("Windows system root")?)
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let host = TerminalHost::default();
    host.configure_windows(WindowsTerminalConfig {
        helper_executable: helper,
        shell_executable: shell,
        shell_arguments: [
            "-NoLogo",
            "-NoProfile",
            "-NoExit",
            "-Command",
            "Import-Module PSReadLine; Set-PSReadLineOption -HistorySaveStyle SaveNothing",
        ]
        .into_iter()
        .map(Into::into)
        .collect(),
        shell_label: "Synthetic PowerShell".into(),
        admission: admission.clone(),
        startup_timeout: Duration::from_secs(15),
        cleanup_timeout: Duration::from_secs(3),
    })?;
    // Drop only signals on exceptional test unwinding; every planned stop below
    // uses the real bounded cleanup, with Retired explicitly accepted, never reset.
    let healthy = host.start(&project, 24, 100)?;
    let mut healthy_output = Transcript::default();
    let ready = send_marker(&host, &healthy, "SHN_READY_")?;
    marker(
        &host,
        &healthy,
        &mut healthy_output,
        None,
        &ready,
        STARTUP,
        true,
    )?;
    host.resize(&healthy.id, 32, 120)?;
    let fault = host.start(&project, 24, 100)?;
    let mut fault_output = Transcript::default();
    let ready = send_marker(&host, &fault, "SHN_FAULT_READY_")?;
    marker(
        &host,
        &fault,
        &mut fault_output,
        Some((&healthy, &mut healthy_output)),
        &ready,
        STARTUP,
        false,
    )?;
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let begin = format!("SHN_FLOOD_{nonce}");
    host.write(&fault.id, format!("Write-Output ('SHN_FLOOD_' + '{nonce}'); $s=[Diagnostics.Stopwatch]::StartNew(); while($s.ElapsedMilliseconds -lt 10000){{[Console]::WriteLine('SHN_SYNTHETIC_FLOOD');for($j=0;$j -lt 500;$j++){{$null=[Math]::Sqrt($j)}}}}\r").into_bytes())?;
    marker(
        &host,
        &fault,
        &mut fault_output,
        Some((&healthy, &mut healthy_output)),
        &begin,
        STARTUP,
        false,
    )?;
    let mut roundtrips = Vec::new();
    for _ in 0..3 {
        roundtrips.push(ping(
            &host,
            &healthy,
            &mut healthy_output,
            Some((&fault, &mut fault_output)),
        )?);
    }
    host.write(&fault.id, b"Stop-Process -Id $PID -Force\r".to_vec())?;
    let closing = Instant::now();
    while !fault_output.eof && closing.elapsed() < STARTUP {
        fault_output.poll(&host, &fault, false)?;
        healthy_output.poll(&host, &healthy, true)?;
        thread::sleep(Duration::from_millis(5));
    }
    if !fault_output.eof {
        bail!("Synthetic self-crash did not finish within its deadline");
    }
    assert!(
        fault_output.bytes > 64 * 1024,
        "the fault must actually produce noisy output"
    );
    let _ = host.stop(&fault.id);
    roundtrips.push(ping(&host, &healthy, &mut healthy_output, None)?);

    let crash = host.start(&project, 24, 100)?;
    let mut crash_output = Transcript::default();
    // This shell reads its lines without PSReadLine. After the managed OutOfMemoryException below,
    // Windows PowerShell ended by itself (exit code 5, nothing terminated, no PSReadLine message)
    // when it read its next line, in four of six Verify Windows runs, arrays released or not:
    // the line reader is PSReadLine's, which allocates on every key.
    host.write(&crash.id, b"Remove-Module PSReadLine -Force\r".to_vec())?;
    let armed = send_marker(&host, &crash, "SHN_CRASH_READY_")?;
    marker(
        &host,
        &crash,
        &mut crash_output,
        Some((&healthy, &mut healthy_output)),
        &armed,
        STARTUP,
        false,
    )?;
    // Actual managed memory refusal, bounded by the already-applied 256MiB
    // process/512MiB job ceilings. The emergency array makes catch/report viable
    // without requiring the process to allocate more memory at its limit. The same
    // command then drops its arrays: left at its ceiling, Windows PowerShell ended
    // by itself (exit code 5, nothing terminated) while it read the next line in
    // three of six Verify Windows runs. The peak stays in the sampled evidence.
    let memory_nonce = uuid::Uuid::new_v4().simple().to_string();
    let memory_refused = format!("SHN_MEMORY_REFUSED_{memory_nonce}");
    host.write(&crash.id, format!(
        "$shnRefused='SHN_MEMORY_REFUSED_'+'{memory_nonce}';$shnUnexpected='SHN_MEMORY_UNEXPECTED_'+'{memory_nonce}';$shnHold=[Collections.Generic.List[byte[]]]::new();$shnReserve=[byte[]]::new(8388608);for($shnN=0;$shnN -lt 64;$shnN++){{try{{$shnBlock=[byte[]]::new(8388608);for($shnPage=0;$shnPage -lt $shnBlock.Length;$shnPage+=4096){{$shnBlock[$shnPage]=1}};$shnHold.Add($shnBlock)}}catch{{$shnReserve=$null;[GC]::Collect();$shnError=$_.Exception;while($shnError.InnerException){{$shnError=$shnError.InnerException}};if($shnError -is [OutOfMemoryException]){{Write-Output $shnRefused}}else{{Write-Output $shnUnexpected}};break}}}};$shnHold.Clear();$shnBlock=$null;$shnReserve=$null\r"
    ).into_bytes())?;
    fault_until(
        &host,
        &healthy,
        &mut healthy_output,
        &crash,
        &mut crash_output,
        &memory_refused,
        &mut roundtrips,
    )?;
    for _ in 0..3 {
        roundtrips.push(ping(
            &host,
            &healthy,
            &mut healthy_output,
            Some((&crash, &mut crash_output)),
        )?);
        thread::sleep(Duration::from_millis(100));
    }
    let memory_evidence = admission.latest_evidence()?;
    assert!(memory_evidence.complete_samples > 0);
    assert!(
        memory_evidence.peak_process_commit >= 128 * MIB,
        "managed refusal must actually approach its existing local memory limit"
    );
    assert!(!memory_evidence.memory_cap_violated);
    applied_limits(&host, &healthy)?;
    applied_limits(&host, &crash)?;
    assert_eq!(fs::read(&saved)?, SENTINEL);
    // Release only the synthetic arrays explicitly (the fault command already dropped them; this
    // proves the shell still answers). No quota/baseline is reset, and no collection is forced:
    // dropping the references is the release, collected when the runtime next needs the memory.
    host.write(
        &crash.id,
        b"$shnHold.Clear();$shnBlock=$null;$shnReserve=$null\r".to_vec(),
    )?;
    let memory_cleared = send_marker(&host, &crash, "SHN_MEMORY_CLEARED_")?;
    fault_until(
        &host,
        &healthy,
        &mut healthy_output,
        &crash,
        &mut crash_output,
        &memory_cleared,
        &mut roundtrips,
    )?;

    // Direct native children only: no Start-Process/broker/shell interpretation.
    // Each child is finite even if this test is interrupted. A fresh complete
    // physical overshoot must trigger the registered-job guard immediately;
    // never delay that guard to force all attempts/a particular output marker.
    let process_nonce = uuid::Uuid::new_v4().simple().to_string();
    let process_refused = format!("SHN_PROCESS_REFUSED_{process_nonce}");
    let process_started = format!("SHN_PROCESS_STARTED_{process_nonce}");
    let process_done = format!("SHN_PROCESS_DONE_{process_nonce}");
    host.write(&crash.id, format!(
        "$shnProcRefused='SHN_PROCESS_REFUSED_'+'{process_nonce}';$shnProcStarted='SHN_PROCESS_STARTED_'+'{process_nonce}';$shnProcDone='SHN_PROCESS_DONE_'+'{process_nonce}';$shnChildren=[Collections.Generic.List[Diagnostics.Process]]::new();for($shnN=0;$shnN -lt 16;$shnN++){{try{{$shnStart=[Diagnostics.ProcessStartInfo]::new();$shnStart.FileName=[IO.Path]::Combine($env:SystemRoot,'System32\\ping.exe');$shnStart.Arguments='-n 30 127.0.0.1';$shnStart.UseShellExecute=$false;$shnStart.CreateNoWindow=$true;$shnStart.RedirectStandardOutput=$true;$shnStart.RedirectStandardError=$true;$shnChild=[Diagnostics.Process]::Start($shnStart);$shnChildren.Add($shnChild);Write-Output $shnProcStarted}}catch{{$shnError=$_.Exception;while($shnError.InnerException){{$shnError=$shnError.InnerException}};if(($shnError -is [ComponentModel.Win32Exception]) -and ($shnError.NativeErrorCode -ne 0) -and ($shnChildren.Count -gt 0)){{Write-Output ($shnProcRefused+':'+$shnError.NativeErrorCode)}}}}}};Write-Output $shnProcDone\r"
    ).into_bytes())?;
    let guard_deadline = Instant::now() + STARTUP;
    while !crash_output.eof && Instant::now() < guard_deadline {
        crash_output.poll(&host, &crash, false)?;
        roundtrips.push(ping(
            &host,
            &healthy,
            &mut healthy_output,
            Some((&crash, &mut crash_output)),
        )?);
        thread::sleep(Duration::from_millis(20));
    }
    let process_evidence = admission.latest_evidence()?;
    assert_eq!(process_evidence.owned, memory_evidence.owned);
    assert!(
        process_evidence.peak_processes > 8 && process_evidence.snapshot.membership_complete,
        "physical console-helper overshoot must be visible as a complete raw census"
    );
    assert!(
        process_evidence.process_guard_stopped,
        "the registered responsible job must receive the resource-limit stop"
    );
    assert!(
        crash_output.eof,
        "the attributed stop must finish within the unchanged deadline"
    );
    assert!(!process_evidence.memory_cap_violated);
    let stopped_status = host
        .resources()
        .into_iter()
        .find(|status| status.id == crash.id)
        .context("responsible terminal stop status")?;
    assert_eq!(stopped_status.reason, TerminalResourceReason::ResourceLimit);
    assert!(crash_output.has(&process_started) || process_evidence.peak_processes > 8);
    let native_process_refusals = crash_output.codes(&process_refused);
    let reported_child_starts = crash_output.count(&process_started);
    let script_finished_before_stop = crash_output.has(&process_done);
    applied_limits(&host, &healthy)?;
    applied_limits(&host, &crash)?;
    assert_eq!(fs::read(&saved)?, SENTINEL);
    let _ = host.stop(&crash.id); // Unconfirmed cleanup remains retained.
    roundtrips.push(ping(&host, &healthy, &mut healthy_output, None)?);

    // Fourth launch is explicit. Resource-limit recovery never starts/replays it.
    let helper_crash = host.start(&project, 24, 100)?;
    let mut helper_crash_output = Transcript::default();
    let armed = send_marker(&host, &helper_crash, "SHN_EXACT_HELPER_READY_")?;
    marker(
        &host,
        &helper_crash,
        &mut helper_crash_output,
        Some((&healthy, &mut healthy_output)),
        &armed,
        STARTUP,
        false,
    )?;
    let captured = admission
        .helpers
        .lock()
        .map_err(|_| anyhow!("test helper registry"))?
        .last()
        .cloned()
        .context("exact helper was not captured")?;
    captured.crash_exact()?;
    captured.wait()?;
    roundtrips.push(ping(
        &host,
        &healthy,
        &mut healthy_output,
        Some((&helper_crash, &mut helper_crash_output)),
    )?);
    let _ = host.stop(&helper_crash.id);
    roundtrips.push(ping(&host, &healthy, &mut healthy_output, None)?);
    assert_eq!(
        admission.helpers.lock().unwrap().len(),
        4,
        "no automatic helper replacement or replay"
    );
    assert_eq!(fs::read(&saved)?, SENTINEL);
    assert_eq!(
        healthy_output.errors, 0,
        "the healthy sibling must not report an output error in any phase"
    );
    assert!(
        healthy_output.tail.len() <= CAPTURE
            && fault_output.tail.len() <= CAPTURE
            && crash_output.tail.len() <= CAPTURE
            && helper_crash_output.tail.len() <= CAPTURE
    );
    for elapsed in &roundtrips {
        assert!(
            *elapsed <= RTT,
            "healthy terminal exceeded the frozen 1500ms response bound"
        );
    }
    let healthy_stop = host.stop(&healthy.id);
    host.shutdown();
    for helper in admission.helpers.lock().unwrap().iter() {
        helper.wait()?;
    }
    let ledger = database
        .resource_reservations()?
        .context("frozen test ledger")?;
    assert!(ledger.slots.len() <= 4);
    let retired = ledger
        .slots
        .iter()
        .filter(|slot| slot.state == ReservationState::Retired)
        .count();
    if healthy_stop.is_err() {
        assert!(
            retired > 0,
            "unconfirmed cleanup must retain its reservation"
        );
    }
    database.integrity_check()?;
    assert_eq!(fs::read(&saved)?, SENTINEL);
    println!(
        "contained_terminal PASS: replies={}, max_rtt_ms={}, fault_bytes={}, retired={}, memory_kernel_refusal=true, process_kernel_refusals={:?}, process_guard_stop=true, reported_child_starts={}, script_finished_before_stop={}, peak_process_commit_mib={}, peak_job_commit_mib={}, physical_peak_processes={}, native_process_limit=8, saved_fixture=intact, autoreplay=false",
        roundtrips.len(),
        roundtrips
            .iter()
            .map(|duration| duration.as_millis())
            .max()
            .unwrap_or(0),
        fault_output.bytes,
        retired,
        native_process_refusals,
        reported_child_starts,
        script_finished_before_stop,
        process_evidence.peak_process_commit / MIB,
        process_evidence.peak_job_commit / MIB,
        process_evidence.peak_processes
    );
    Ok(())
}
