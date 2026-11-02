#![cfg(windows)]
//! Real, bounded kernel containment; no agent/provider command or pressure test.
use shinatama_engine::resources::job::{
    CleanupOutcome, JobBudget, JobError, ReconcileOutcome, SlotState, WorkloadJob, WorkloadSlot,
    reconcile_dead_owner,
};
use shinatama_engine::resources::sampling::{Measurement, ProcessKey, Unavailable, WindowsSampler};
use shinatama_state::location::{LocationRequest, StorageLocation};
use std::{
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::Read,
    mem::size_of,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, BorrowedHandle},
    },
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, FILETIME, GetLastError, HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation,
        WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    System::{
        JobObjects::{
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation,
            OpenJobObjectW, QueryInformationJobObject,
        },
        Memory::{GetProcessHeap, HeapAlloc, HeapFree},
        Threading::{
            CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DETACHED_PROCESS,
            DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetProcessId,
            GetProcessTimes, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
            OpenProcess, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, ResumeThread,
            STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute,
            WaitForSingleObject,
        },
    },
};

const MODE: &str = "SHINATAMA_JOB_TEST_MODE";
const ROOT: &str = "SHINATAMA_JOB_TEST_ROOT";
const INNER_KEY: &str = "SHINATAMA_JOB_TEST_INNER_KEY";
const WAIT: Duration = Duration::from_secs(3);
type TestResult<T> = Result<T, String>;

fn key() -> [u8; 16] {
    let mut key = [0; 16];
    getrandom::fill(&mut key).unwrap();
    assert_ne!(key, [0; 16]);
    key
}
fn hex(key: [u8; 16]) -> String {
    key.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn budget() -> JobBudget {
    JobBudget::new(128 * 1024 * 1024, 64 * 1024 * 1024, 4, 2500).unwrap()
}
fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}
fn os_error(operation: &str) -> String {
    format!("{operation}: {}", std::io::Error::last_os_error())
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn exact_process_handle(expected: ProcessKey) -> Handle {
    let handle = Handle(unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            expected.pid,
        )
    });
    assert!(
        !handle.0.is_null(),
        "open known worker: {}",
        os_error("OpenProcess")
    );
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    assert_ne!(
        unsafe { GetProcessTimes(handle.0, &mut creation, &mut exit, &mut kernel, &mut user) },
        0
    );
    assert_eq!(unsafe { GetProcessId(handle.0) }, expected.pid);
    assert_eq!(
        (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime),
        expected.created_at_100ns,
    );
    handle
}

fn job_diagnostic(key: [u8; 16]) -> String {
    // QUERY only, immediately closed. This observation is diagnostic and cannot
    // establish termination/coverage or substitute for the strict process wait.
    let name = wide(format!("Local\\Shinatama.Workload.{}", hex(key)));
    let raw = unsafe { OpenJobObjectW(0x0004, 0, name.as_ptr()) }; // JOB_OBJECT_QUERY
    if raw.is_null() {
        return format!("OpenJobObjectW error={}", unsafe { GetLastError() });
    }
    let handle = Handle(raw);
    let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
    if unsafe {
        QueryInformationJobObject(
            handle.0,
            JobObjectBasicAccountingInformation,
            (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
            size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
            null_mut(),
        )
    } == 0
    {
        return format!("query error={}", unsafe { GetLastError() });
    }
    format!(
        "query active={} total={} terminated={}",
        accounting.ActiveProcesses, accounting.TotalProcesses, accounting.TotalTerminatedProcesses,
    )
}
struct Attributes {
    pointer: LPPROC_THREAD_ATTRIBUTE_LIST,
    heap: HANDLE,
}
impl Attributes {
    fn new() -> TestResult<Self> {
        let mut size = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut size);
        }
        if size == 0 || size > 16384 {
            return Err("invalid attribute allocation".into());
        }
        let heap = unsafe { GetProcessHeap() };
        let pointer = unsafe { HeapAlloc(heap, 0, size) };
        if pointer.is_null() {
            return Err("attribute allocation failed".into());
        }
        if unsafe { InitializeProcThreadAttributeList(pointer, 2, 0, &mut size) } == 0 {
            unsafe {
                HeapFree(heap, 0, pointer);
            }
            return Err(os_error("initialize attributes"));
        }
        Ok(Self { pointer, heap })
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.pointer);
            HeapFree(self.heap, 0, self.pointer);
        }
    }
}

struct Child {
    process: Handle,
    thread: Handle,
    key: ProcessKey,
}
impl Child {
    fn resume(&self) -> TestResult<()> {
        if unsafe { ResumeThread(self.thread.0) } == u32::MAX {
            Err(os_error("resume owned primary thread"))
        } else {
            Ok(())
        }
    }
    fn wait(&self) -> TestResult<()> {
        if unsafe { WaitForSingleObject(self.process.0, 3000) } == WAIT_OBJECT_0 {
            Ok(())
        } else {
            Err("owned child did not finish within 3 seconds".into())
        }
    }
    fn terminate(&self) -> TestResult<()> {
        if unsafe { WaitForSingleObject(self.process.0, 0) } == WAIT_TIMEOUT
            && unsafe { TerminateProcess(self.process.0, 73) } == 0
        {
            return Err(os_error("terminate owned test child"));
        }
        self.wait()
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        // Exact handles only: never terminate a PID discovered from a file/name.
        if unsafe { WaitForSingleObject(self.process.0, 0) } == WAIT_TIMEOUT {
            unsafe {
                TerminateProcess(self.process.0, 74);
                WaitForSingleObject(self.process.0, 3000);
            }
        }
    }
}

fn command(executable: &Path) -> Vec<u16> {
    let mut result = Vec::new();
    for argument in [
        executable.as_os_str(),
        OsStr::new("--exact"),
        OsStr::new("synthetic_job_child"),
        OsStr::new("--test-threads=1"),
    ] {
        if !result.is_empty() {
            result.push(b' ' as u16);
        }
        result.push(b'"' as u16);
        let mut slashes = 0;
        for unit in argument.encode_wide() {
            if unit == b'\\' as u16 {
                slashes += 1;
                continue;
            }
            let count = if unit == b'"' as u16 {
                2 * slashes + 1
            } else {
                slashes
            };
            result.extend(std::iter::repeat_n(b'\\' as u16, count));
            result.push(unit);
            slashes = 0;
        }
        result.extend(std::iter::repeat_n(b'\\' as u16, 2 * slashes));
        result.push(b'"' as u16);
    }
    assert!(result.len() < 32767);
    result.push(0);
    result
}

fn environment(root: &Path, mode: &str, inner_key: [u8; 16]) -> Vec<u16> {
    // No inherited PATH, HOME, provider keys, credentials or agent markers.
    let mut entries = vec![
        (
            OsString::from("SystemRoot"),
            std::env::var_os("SystemRoot").unwrap(),
        ),
        (OsString::from(MODE), OsString::from(mode)),
        (OsString::from(ROOT), root.as_os_str().to_owned()),
        (OsString::from(INNER_KEY), OsString::from(hex(inner_key))),
    ];
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    let mut result = Vec::new();
    for (name, value) in entries {
        result.extend(name.encode_wide());
        result.push(b'=' as u16);
        result.extend(value.encode_wide());
        result.push(0);
    }
    result.push(0);
    assert!(result.len() < 8192);
    result
}

fn spawn_suspended(
    job: &WorkloadJob,
    root: &Path,
    mode: &str,
    inner_key: [u8; 16],
) -> TestResult<Child> {
    // Declaration order matters: the attribute list is destroyed before its
    // pinned JOB_LIST storage/borrow, including every failure path.
    let binding = std::pin::pin!(
        job.startup_binding()
            .map_err(|error| format!("{error:?}"))?
    );
    let null_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open("NUL")
        .map_err(|error| error.to_string())?;
    let inherited = [null_file.as_raw_handle()];
    if unsafe { SetHandleInformation(inherited[0], HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) } == 0
    {
        return Err(os_error("inherit NUL"));
    }
    let attributes = Attributes::new()?;
    if unsafe {
        UpdateProcThreadAttribute(
            attributes.pointer,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            inherited.as_ptr().cast(),
            size_of::<HANDLE>(),
            null_mut(),
            null(),
        )
    } == 0
    {
        return Err(os_error("HANDLE_LIST"));
    }
    unsafe { binding.as_ref().apply_to(attributes.pointer) }
        .map_err(|error| format!("{error:?}"))?;
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut arguments = command(&executable);
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = inherited[0];
    startup.StartupInfo.hStdOutput = inherited[0];
    startup.StartupInfo.hStdError = inherited[0];
    startup.lpAttributeList = attributes.pointer;
    let mut information = PROCESS_INFORMATION::default();
    if unsafe {
        CreateProcessW(
            wide(&executable).as_ptr(),
            arguments.as_mut_ptr(),
            null(),
            null(),
            1,
            // This NUL/file-only helper has no console. CREATE_NO_WINDOW added
            // real conhost.exe members on this host; production budgets must
            // count such helpers rather than ignore them. DETACHED_PROCESS:
            // https://learn.microsoft.com/windows/win32/procthread/process-creation-flags
            CREATE_SUSPENDED
                | DETACHED_PROCESS
                | CREATE_UNICODE_ENVIRONMENT
                | EXTENDED_STARTUPINFO_PRESENT,
            environment(root, mode, inner_key).as_ptr().cast(),
            wide(root).as_ptr(),
            &startup.StartupInfo,
            &mut information,
        )
    } == 0
    {
        return Err(os_error("CreateProcess inside JOB_LIST"));
    }
    let process = Handle(information.hProcess);
    let thread = Handle(information.hThread);
    // A verification failure must still own and clean the suspended process.
    let mut child = Child {
        process,
        thread,
        key: ProcessKey {
            pid: information.dwProcessId,
            created_at_100ns: 1,
        },
    };
    child.key = binding
        .verify_membership(unsafe { BorrowedHandle::borrow_raw(child.process.0) })
        .map_err(|error| format!("membership: {error:?}"))?;
    Ok(child)
}

fn wait_file(root: &Path, name: &str) -> TestResult<Vec<u8>> {
    let path = root.join(name);
    let deadline = Instant::now() + WAIT;
    loop {
        match File::open(&path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(257)
                    .read_to_end(&mut bytes)
                    .map_err(|error| error.to_string())?;
                if bytes.len() > 256 {
                    return Err("synthetic ready metadata exceeded 256 bytes".into());
                }
                return Ok(bytes);
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(error) => return Err(format!("waiting for {name}: {error}")),
        }
    }
}

fn write_ready(root: &Path, name: &str, text: &str) {
    use std::io::Write;
    assert!(text.len() <= 256);
    let partial = root.join(format!("{name}.part"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)
        .unwrap();
    file.write_all(text.as_bytes()).unwrap();
    file.sync_all().unwrap();
    drop(file);
    // Publish only a complete metadata record; seeing an empty/partial ready
    // file would otherwise race the owner's exact identity fields.
    fs::rename(partial, root.join(name)).unwrap();
}

fn finish(
    mut job: WorkloadJob,
    children: &[&Child],
) -> TestResult<shinatama_engine::resources::job::ClosedEmptyJobProof> {
    if job
        .terminate_and_wait(WAIT)
        .map_err(|error| format!("{error:?}"))?
        != CleanupOutcome::EmptyConfirmed
    {
        return Err("owned job cleanup unconfirmed; no release proof".into());
    }
    for child in children {
        child.wait()?;
    }
    job.close_after_empty()
        .map_err(|_| "job close did not produce proof".into())
}

struct Fixture {
    location: Option<StorageLocation>,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("shinatama-job-containment-{}", hex(key())));
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: std::env::temp_dir(),
            home_override: Some(root.clone()),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        })
        .unwrap();
        location.prepare_private_directory().unwrap();
        Self {
            location: Some(location),
            root,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.location.take();
        assert_eq!(self.root.parent(), Some(std::env::temp_dir().as_path()));
        assert!(
            self.root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("shinatama-job-containment-")
        );
        if let Err(error) = fs::remove_dir_all(&self.root) {
            eprintln!("Synthetic job fixture cleanup: {error}");
        }
    }
}

#[test]
fn synthetic_job_child() {
    let Some(mode) = std::env::var_os(MODE) else {
        return;
    };
    let root = PathBuf::from(std::env::var_os(ROOT).unwrap());
    let mode = mode.to_str().unwrap();
    if mode == "owner" {
        let encoded = std::env::var(INNER_KEY).unwrap();
        assert_eq!(encoded.len(), 32);
        let mut inner_key = [0u8; 16];
        for (index, byte) in inner_key.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&encoded[index * 2..index * 2 + 2], 16).unwrap();
        }
        let slot = WorkloadSlot::new();
        let inner = WorkloadJob::create(&slot, inner_key, budget()).unwrap();
        {
            // The enclosing job never observes this short-lived grandchild.
            // Its lifetime census must retain the missing identity as a gap.
            let transient = spawn_suspended(&inner, &root, "transient", [1; 16]).unwrap();
            transient.resume().unwrap();
            transient.wait().unwrap();
        }
        let worker = spawn_suspended(&inner, &root, "inner", [1; 16]).unwrap();
        worker.resume().unwrap();
        wait_file(&root, "inner-ready").unwrap();
        write_ready(
            &root,
            "owner-ready",
            &format!(
                "{} {} {} {} {}",
                inner.owner().pid,
                inner.owner().created_at_100ns,
                inner.owner_windows_session_id(),
                worker.key.pid,
                worker.key.created_at_100ns
            ),
        );
        // Parent kills this exact owned process handle. Neither job nor child
        // handle is inherited by the worker; bypassing Drop simulates owner loss.
        std::thread::sleep(Duration::from_secs(15));
        drop(worker);
        drop(inner);
        panic!("controlled owner was not stopped by its parent");
    }
    assert!(matches!(
        mode,
        "healthy" | "refused" | "inner" | "transient"
    ));
    write_ready(&root, &format!("{mode}-ready"), "synthetic ready");
    if mode == "transient" {
        return;
    }
    let end = Instant::now() + Duration::from_secs(15);
    while Instant::now() < end {
        for (ping, pong) in [("ping", "pong"), ("ping-after-crash", "pong-after-crash")] {
            if mode == "healthy" && root.join(ping).exists() && !root.join(pong).exists() {
                write_ready(&root, pong, "healthy replied after sibling cleanup");
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn native_job_is_bound_before_resume_and_owner_crash_reconciles_without_harming_a_sibling() {
    let fixture = Fixture::new();
    let sampler = WindowsSampler::new();
    let healthy_slot = WorkloadSlot::new();
    let healthy_key = key();
    let healthy = WorkloadJob::create(&healthy_slot, healthy_key, budget()).unwrap();
    let healthy_child = spawn_suspended(&healthy, &fixture.root, "healthy", [1; 16]).unwrap();
    assert!(
        !fixture.root.join("healthy-ready").exists(),
        "a suspended child executed before validation"
    );
    assert_eq!(
        WindowsSampler::identify(healthy_child.key.pid).unwrap(),
        healthy_child.key
    );
    let before = healthy.query(&sampler).unwrap();
    assert_eq!(before.active_processes, 1);
    assert!(before.membership_complete);
    assert_eq!(
        before.members[0].identity,
        Measurement::Available(healthy_child.key)
    );
    healthy_child.resume().unwrap();
    wait_file(&fixture.root, "healthy-ready").unwrap();

    let refused_slot = WorkloadSlot::new();
    let refused = WorkloadJob::create(&refused_slot, key(), budget()).unwrap();
    let refused_child = spawn_suspended(&refused, &fixture.root, "refused", [1; 16]).unwrap();
    {
        let wrong_binding = healthy.startup_binding().unwrap();
        assert!(matches!(
            wrong_binding
                .verify_membership(unsafe { BorrowedHandle::borrow_raw(refused_child.process.0) }),
            Err(JobError::IncompleteMembership)
        ));
    }
    assert!(!fixture.root.join("refused-ready").exists());
    // Active/non-empty cannot fabricate a closed-empty proof.
    let mut refused = match refused.close_after_empty() {
        Err(job) => job,
        Ok(_) => panic!("unconfirmed job returned release proof"),
    };
    let outcome = refused.terminate_and_wait(Duration::ZERO).unwrap();
    if matches!(outcome, CleanupOutcome::Retired { .. }) {
        assert_eq!(refused_slot.state(), SlotState::Retired);
        assert!(matches!(
            refused.startup_binding(),
            Err(JobError::AdmissionClosed)
        ));
        refused = match refused.close_after_empty() {
            Err(job) => job,
            Ok(_) => panic!("retired job returned release proof"),
        };
    }
    finish(refused, &[&refused_child]).unwrap();
    assert_eq!(refused_slot.state(), SlotState::Free);
    write_ready(&fixture.root, "ping", "respond");
    wait_file(&fixture.root, "pong").unwrap();
    assert_eq!(
        unsafe { WaitForSingleObject(healthy_child.process.0, 0) },
        WAIT_TIMEOUT
    );

    // The outer job guarantees cleanup of both owner and worker on every error.
    let outer_slot = WorkloadSlot::new();
    let mut outer = WorkloadJob::create(&outer_slot, key(), budget()).unwrap();
    let inner_key = key();
    let owner = spawn_suspended(&outer, &fixture.root, "owner", inner_key).unwrap();
    owner.resume().unwrap();
    let bytes = wait_file(&fixture.root, "owner-ready").unwrap();
    let fields: Vec<u64> = std::str::from_utf8(&bytes)
        .unwrap()
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    assert_eq!(fields.len(), 5);
    let expected = ProcessKey {
        pid: u32::try_from(fields[0]).unwrap(),
        created_at_100ns: fields[1],
    };
    assert_eq!(expected, owner.key);
    let session = u32::try_from(fields[2]).unwrap();
    let worker = ProcessKey {
        pid: u32::try_from(fields[3]).unwrap(),
        created_at_100ns: fields[4],
    };
    let worker_handle = exact_process_handle(worker);
    assert!(fixture.root.join("transient-ready").is_file());
    // Records owner/current worker handles. The already-finished transient
    // descendant remains deliberately absent from the wrapper's lifetime census.
    let current = outer.query(&sampler).unwrap();
    assert_eq!(
        current.active_processes, 2,
        "total={} members={:?}",
        current.total_processes, current.members
    );
    assert!(matches!(
        reconcile_dead_owner(inner_key, expected, session),
        Err(JobError::OwnerAlive)
    ));
    assert!(matches!(
        reconcile_dead_owner(inner_key, expected, session.wrapping_add(1)),
        Err(JobError::OwnerSessionMismatch)
    ));
    owner.terminate().unwrap();
    if let ReconcileOutcome::ClosedEmpty(proof) =
        reconcile_dead_owner(inner_key, expected, session).unwrap()
    {
        assert_eq!(proof.job_key(), inner_key);
        assert_eq!(proof.owner(), expected);
        assert_eq!(proof.owner_windows_session_id(), session);
        let worker_wait = unsafe { WaitForSingleObject(worker_handle.0, 0) };
        eprintln!(
            "unexpected closed-empty proof: {}; worker wait={worker_wait}",
            job_diagnostic(inner_key)
        );
        // Preserve the original strict race detector on any future regression.
        match WindowsSampler::identify(worker.pid) {
            Ok(current) => assert_ne!(current, worker, "proof preceded inner child termination"),
            Err(Unavailable::Missing | Unavailable::Terminated) => {}
            Err(other) => panic!("inner child shutdown could not be verified: {other:?}"),
        }
        assert_eq!(
            worker_wait, WAIT_OBJECT_0,
            "proof preceded known worker signaled state"
        );
        panic!("crash reconciliation fabricated complete descendant coverage");
    }
    // Independent OS behavior check: this one known worker really terminates.
    // Its signaled handle is not promoted into a proof for unknown descendants.
    assert_eq!(
        unsafe { WaitForSingleObject(worker_handle.0, 3000) },
        WAIT_OBJECT_0,
        "kill-on-close did not finish the controlled inner worker within 3 seconds"
    );
    assert!(matches!(
        reconcile_dead_owner(inner_key, expected, session).unwrap(),
        ReconcileOutcome::Retired
    ));
    assert_eq!(outer.query(&sampler).unwrap().active_processes, 0);
    assert!(matches!(
        outer.terminate_and_wait(WAIT).unwrap(),
        CleanupOutcome::Retired { .. }
    ));
    outer = match outer.close_after_empty() {
        Err(job) => job,
        Ok(_) => panic!("the unobserved short-lived descendant was omitted from coverage"),
    };
    drop(outer);
    assert_eq!(outer_slot.state(), SlotState::Retired);
    write_ready(
        &fixture.root,
        "ping-after-crash",
        "respond after owner crash",
    );
    wait_file(&fixture.root, "pong-after-crash").unwrap();
    assert_eq!(
        unsafe { WaitForSingleObject(healthy_child.process.0, 0) },
        WAIT_TIMEOUT
    );
    let proof = finish(healthy, &[&healthy_child]).unwrap();
    assert_eq!(proof.job_key(), healthy_key);
    assert_eq!(healthy_slot.state(), SlotState::Free);
}
