#![cfg(windows)]
//! Bounded native faults; no provider, terminal, renderer or host exhaustion.
use shinatama_engine::resources::{
    job::{CleanupOutcome, JobBudget, SlotState, WorkloadJob, WorkloadSlot},
    sampling::{ProcessKey, WindowsSampler},
};
use shinatama_state::location::{LocationRequest, StorageLocation};
use std::{
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    mem::size_of,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, BorrowedHandle},
    },
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation,
        WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    Storage::FileSystem::ReadFile,
    System::{
        Memory::{
            GetProcessHeap, HeapAlloc, HeapFree, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE,
            PAGE_READWRITE, VirtualAlloc, VirtualFree,
        },
        Pipes::{CreatePipe, PeekNamedPipe},
        Threading::{
            CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DETACHED_PROCESS,
            DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
            InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION, ResumeThread,
            STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess, WaitForSingleObject,
        },
    },
};

const MODE: &str = "SHINATAMA_FAULT_MODE";
const ROOT: &str = "SHINATAMA_FAULT_ROOT";
const CASE: &str = "SHINATAMA_FAULT_CASE";
const WAIT: Duration = Duration::from_secs(3);
const PING_LIMIT: Duration = Duration::from_millis(750);
const MIB: u64 = 1024 * 1024;
const READ_LIMIT: usize = 32 * 1024;

fn budget(cpu: u16) -> JobBudget {
    JobBudget::new(128 * MIB, 64 * MIB, 1, cpu).unwrap()
}
fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}
fn key() -> [u8; 16] {
    *uuid::Uuid::new_v4().as_bytes()
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct Attributes {
    pointer: LPPROC_THREAD_ATTRIBUTE_LIST,
    heap: HANDLE,
}
impl Attributes {
    fn new() -> Self {
        let mut size = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut size);
        }
        assert!((1..=16384).contains(&size));
        let heap = unsafe { GetProcessHeap() };
        let pointer = unsafe { HeapAlloc(heap, 0, size) };
        assert!(!pointer.is_null());
        if unsafe { InitializeProcThreadAttributeList(pointer, 2, 0, &mut size) } == 0 {
            unsafe {
                HeapFree(heap, 0, pointer);
            }
            panic!("initialize synthetic attributes");
        }
        Self { pointer, heap }
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
    safe_cleanup: Arc<AtomicBool>,
}
impl Child {
    fn resume(&self) {
        assert_ne!(unsafe { ResumeThread(self.thread.0) }, u32::MAX);
    }
    fn alive(&self) -> bool {
        unsafe { WaitForSingleObject(self.process.0, 0) == WAIT_TIMEOUT }
    }
    fn wait(&self) {
        assert_eq!(
            unsafe { WaitForSingleObject(self.process.0, 3000) },
            WAIT_OBJECT_0
        );
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        if self.alive() {
            unsafe {
                TerminateProcess(self.process.0, 77);
            }
        }
        if unsafe { WaitForSingleObject(self.process.0, 3000) } != WAIT_OBJECT_0 {
            self.safe_cleanup.store(false, Ordering::Release);
        }
    }
}
fn arguments(executable: &Path) -> Vec<u16> {
    let mut result = Vec::new();
    for argument in [
        executable.as_os_str(),
        OsStr::new("--exact"),
        OsStr::new("fault_child"),
        OsStr::new("--test-threads=1"),
        OsStr::new("--nocapture"),
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
            result.extend(std::iter::repeat_n(
                b'\\' as u16,
                if unit == b'"' as u16 {
                    2 * slashes + 1
                } else {
                    slashes
                },
            ));
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
fn environment(root: &Path, mode: &str, case: &str) -> Vec<u16> {
    let mut entries = vec![
        (
            OsString::from("SystemRoot"),
            std::env::var_os("SystemRoot").unwrap(),
        ),
        (OsString::from(ROOT), root.as_os_str().to_owned()),
        (OsString::from(MODE), OsString::from(mode)),
        (OsString::from(CASE), OsString::from(case)),
    ];
    entries.sort_by(|a, b| a.0.cmp(&b.0));
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
fn spawn(
    job: &WorkloadJob,
    fixture: &Fixture,
    mode: &str,
    case: &str,
    output: Option<&Handle>,
) -> Result<Child, u32> {
    // Same pinned JOB_LIST/HANDLE_LIST ordering as job_containment.rs; the
    // attribute list is destroyed before its borrowed pinned binding storage.
    let binding = std::pin::pin!(job.startup_binding().unwrap());
    let nul = OpenOptions::new()
        .read(true)
        .write(true)
        .open("NUL")
        .unwrap();
    let stdout = output.map_or(nul.as_raw_handle(), |handle| handle.0);
    let inherited = [nul.as_raw_handle(), stdout];
    let count = if output.is_some() { 2 } else { 1 };
    for handle in &inherited[..count] {
        assert_ne!(
            unsafe { SetHandleInformation(*handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) },
            0
        );
    }
    let attributes = Attributes::new();
    assert_ne!(
        unsafe {
            windows_sys::Win32::System::Threading::UpdateProcThreadAttribute(
                attributes.pointer,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                inherited.as_ptr().cast(),
                count * size_of::<HANDLE>(),
                null_mut(),
                null(),
            )
        },
        0
    );
    unsafe { binding.as_ref().apply_to(attributes.pointer) }.unwrap();
    let executable = std::env::current_exe().unwrap();
    let mut command = arguments(&executable);
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = nul.as_raw_handle();
    startup.StartupInfo.hStdOutput = stdout;
    startup.StartupInfo.hStdError = nul.as_raw_handle();
    startup.lpAttributeList = attributes.pointer;
    let mut info = PROCESS_INFORMATION::default();
    if unsafe {
        CreateProcessW(
            wide(executable).as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            1,
            CREATE_SUSPENDED
                | DETACHED_PROCESS
                | CREATE_UNICODE_ENVIRONMENT
                | EXTENDED_STARTUPINFO_PRESENT,
            environment(&fixture.root, mode, case).as_ptr().cast(),
            wide(&fixture.root).as_ptr(),
            &startup.StartupInfo,
            &mut info,
        )
    } == 0
    {
        return Err(unsafe { GetLastError() });
    }
    let mut child = Child {
        process: Handle(info.hProcess),
        thread: Handle(info.hThread),
        key: ProcessKey {
            pid: info.dwProcessId,
            created_at_100ns: 1,
        },
        safe_cleanup: fixture.safe_cleanup.clone(),
    };
    child.key = binding
        .verify_membership(unsafe { BorrowedHandle::borrow_raw(child.process.0) })
        .unwrap();
    Ok(child)
}
fn publish(root: &Path, name: &str, text: &str) {
    assert!(text.len() <= 256);
    let partial = root.join(format!("{name}.part"));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&partial)
        .unwrap();
    file.write_all(text.as_bytes()).unwrap();
    drop(file);
    fs::rename(partial, root.join(name)).unwrap();
}
fn receive(root: &Path, name: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    loop {
        match File::open(root.join(name)) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(257).read_to_end(&mut bytes).unwrap();
                assert!(bytes.len() <= 256);
                return String::from_utf8(bytes).unwrap();
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(error) => panic!("bounded metadata {name}: {error}"),
        }
    }
}
fn ping(fixture: &Fixture, sequence: &mut usize, child: &Child) {
    let token = uuid::Uuid::new_v4().to_string();
    let started = Instant::now();
    publish(&fixture.root, &format!("ping-{sequence}"), &token);
    assert_eq!(
        receive(&fixture.root, &format!("pong-{sequence}"), PING_LIMIT),
        token
    );
    assert!(
        started.elapsed() <= PING_LIMIT,
        "fresh sibling ping exceeded 750 ms"
    );
    assert!(child.alive());
    *sequence += 1;
}
struct Fixture {
    root: PathBuf,
    base: PathBuf,
    safe_cleanup: Arc<AtomicBool>,
}
impl Fixture {
    fn new() -> Self {
        let base = fs::canonicalize(std::env::temp_dir()).unwrap();
        let root = base.join(format!(
            "shinatama-resource-faults-{}",
            uuid::Uuid::new_v4()
        ));
        StorageLocation::resolve(LocationRequest {
            local_data_directory: base.clone(),
            home_override: Some(root.clone()),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        })
        .unwrap()
        .prepare_private_directory()
        .unwrap();
        fs::write(root.join("saved-work.txt"), b"synthetic saved work\n").unwrap();
        Self {
            root,
            base,
            safe_cleanup: Arc::new(AtomicBool::new(true)),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !self.safe_cleanup.load(Ordering::Acquire) {
            eprintln!("Unconfirmed helper shutdown: fixture retained");
            return;
        }
        assert!(self.root.is_absolute());
        assert_eq!(self.root.parent(), Some(self.base.as_path()));
        assert!(
            self.root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("shinatama-resource-faults-")
        );
        if let Err(error) = fs::remove_dir_all(&self.root) {
            eprintln!("Synthetic fault fixture cleanup: {error}");
        }
    }
}
fn finish(mut job: WorkloadJob, child: &Child, slot: &WorkloadSlot, allow_census_gap: bool) {
    let expected = job.owned_key();
    let outcome = job.terminate_and_wait(WAIT).unwrap();
    child.wait();
    match outcome {
        CleanupOutcome::EmptyConfirmed => {
            let proof = job
                .close_after_empty()
                .ok()
                .expect("native closed-empty proof");
            assert_eq!(proof.owned_key(), expected);
            assert_eq!(slot.state(), SlotState::Free);
        }
        CleanupOutcome::Retired { error } => {
            assert!(
                allow_census_gap,
                "unexpected unconfirmed shutdown: {error:?}"
            );
            job = job
                .close_after_empty()
                .err()
                .expect("uncertain job must not return proof");
            drop(job);
            assert_eq!(slot.state(), SlotState::Retired);
        }
    }
}
struct Pipe {
    read: Handle,
    write: Handle,
}
impl Pipe {
    fn new() -> Self {
        let (mut read, mut write) = (null_mut(), null_mut());
        assert_ne!(
            unsafe { CreatePipe(&mut read, &mut write, null(), 4096) },
            0
        );
        Self {
            read: Handle(read),
            write: Handle(write),
        }
    }
    fn available(&self) -> u32 {
        let mut bytes = 0;
        assert_ne!(
            unsafe {
                PeekNamedPipe(
                    self.read.0,
                    null_mut(),
                    0,
                    null_mut(),
                    &mut bytes,
                    null_mut(),
                )
            },
            0
        );
        bytes
    }
    fn read_bounded(&self) -> usize {
        let mut buffer = [0_u8; 4096];
        let mut total = 0;
        let deadline = Instant::now() + WAIT;
        while total < READ_LIMIT {
            let available = self
                .available()
                .min(buffer.len() as u32)
                .min((READ_LIMIT - total) as u32);
            if available == 0 {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            let mut count = 0;
            assert_ne!(
                unsafe {
                    ReadFile(
                        self.read.0,
                        buffer.as_mut_ptr(),
                        available,
                        &mut count,
                        null_mut(),
                    )
                },
                0
            );
            assert_ne!(count, 0);
            total += count as usize;
        }
        assert_eq!(total, READ_LIMIT);
        total
    }
}

#[test]
fn fault_child() {
    let Some(mode) = std::env::var_os(MODE) else {
        return;
    };
    let root = PathBuf::from(std::env::var_os(ROOT).unwrap());
    let case = std::env::var(CASE).unwrap();
    let mode = mode.to_str().unwrap();
    if mode == "memory" {
        let mut allocations = Vec::with_capacity(24);
        let mut refusal = 0;
        for _ in 0..24 {
            let pointer = unsafe {
                VirtualAlloc(
                    null(),
                    (4 * MIB) as usize,
                    MEM_COMMIT | MEM_RESERVE,
                    PAGE_READWRITE,
                )
            };
            if pointer.is_null() {
                refusal = unsafe { GetLastError() };
                break;
            }
            allocations.push(pointer);
        }
        assert_ne!(
            refusal, 0,
            "fixture did not hit the frozen process commit ceiling"
        );
        assert!(!allocations.is_empty());
        publish(
            &root,
            &format!("{case}-ready"),
            &format!("{} {refusal}", allocations.len() * 4),
        );
        std::thread::sleep(Duration::from_secs(20));
        for pointer in allocations {
            unsafe {
                VirtualFree(pointer, 0, MEM_RELEASE);
            }
        }
        return;
    }
    publish(&root, &format!("{case}-ready"), "ready");
    let deadline = Instant::now() + Duration::from_secs(20);
    match mode {
        "cpu" => {
            let mut counter = 0_u64;
            while Instant::now() < deadline {
                counter = std::hint::black_box(counter.wrapping_add(1));
            }
        }
        "flood" => {
            let mut output = std::io::stdout().lock();
            for _ in 0..256 {
                output.write_all(&[b'X'; 4096]).unwrap();
            }
            publish(&root, &format!("{case}-finished"), "all 1 MiB written");
        }
        "healthy" => {
            let mut sequence = 0;
            while Instant::now() < deadline {
                let name = format!("ping-{sequence}");
                if root.join(&name).is_file() {
                    let token = receive(&root, &name, WAIT);
                    publish(&root, &format!("pong-{sequence}"), &token);
                    sequence += 1;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        "count" => std::thread::sleep(Duration::from_secs(20)),
        _ => panic!("unexpected synthetic mode"),
    }
}

#[test]
fn resource_faults_are_confined_and_a_sibling_keeps_answering() {
    let fixture = Fixture::new();
    let sampler = WindowsSampler::new();
    let mut sequence = 0;
    let healthy_slot = WorkloadSlot::new();
    let healthy_job = WorkloadJob::create(&healthy_slot, key(), budget(1000)).unwrap();
    let healthy = spawn(&healthy_job, &fixture, "healthy", "healthy", None).unwrap();
    healthy.resume();
    receive(&fixture.root, "healthy-ready", WAIT);
    for round in 0..2 {
        for mode in ["memory", "cpu", "count", "flood"] {
            ping(&fixture, &mut sequence, &healthy);
            let slot = WorkloadSlot::new();
            let requested = budget(if mode == "cpu" { 100 } else { 1000 });
            let job = WorkloadJob::create(&slot, key(), requested).unwrap();
            let case = format!("{mode}-{round}");
            let pipe = (mode == "flood").then(Pipe::new);
            let child = spawn(
                &job,
                &fixture,
                mode,
                &case,
                pipe.as_ref().map(|pipe| &pipe.write),
            )
            .unwrap();
            assert_eq!(WindowsSampler::identify(child.key.pid).unwrap(), child.key);
            child.resume();
            let ready = receive(&fixture.root, &format!("{case}-ready"), WAIT);
            match mode {
                "memory" => {
                    let committed_mib: usize =
                        ready.split_whitespace().next().unwrap().parse().unwrap();
                    assert!((1..16).contains(&(committed_mib / 4)));
                    let observed = job.query(&sampler).unwrap();
                    assert!(observed.membership_complete);
                    assert!(
                        observed.observed_private_commit_bytes <= requested.process_commit_bytes()
                    );
                    assert!(observed.peak_job_commit_bytes <= requested.job_commit_bytes());
                }
                "cpu" => {
                    let before = job.query(&sampler).unwrap().cpu_time_100ns;
                    let deadline = Instant::now() + WAIT;
                    while job.query(&sampler).unwrap().cpu_time_100ns <= before {
                        assert!(Instant::now() < deadline, "CPU helper did not execute");
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    assert!(child.alive());
                }
                "count" => {
                    let failure = spawn(&job, &fixture, "count", "forbidden", None);
                    match failure {
                        Err(error) => assert_ne!(error, 0),
                        Ok(_) => panic!("native process cap admitted a second child"),
                    }
                    assert!(!fixture.root.join("forbidden-ready").exists());
                    assert_eq!(job.query(&sampler).unwrap().active_processes, 1);
                }
                "flood" => {
                    assert_eq!(pipe.as_ref().unwrap().read_bounded(), READ_LIMIT);
                    assert!(
                        !fixture.root.join(format!("{case}-finished")).exists(),
                        "producer did not backpressure"
                    );
                    assert!(child.alive());
                }
                _ => unreachable!(),
            }
            ping(&fixture, &mut sequence, &healthy);
            finish(job, &child, &slot, mode == "count");
            ping(&fixture, &mut sequence, &healthy);
            assert_eq!(
                fs::read(fixture.root.join("saved-work.txt")).unwrap(),
                b"synthetic saved work\n"
            );
            println!(
                "PASS {case}: responsible job ended; fresh sibling ping <=750ms; saved fixture intact"
            );
        }
    }
    finish(healthy_job, &healthy, &healthy_slot, false);
}
