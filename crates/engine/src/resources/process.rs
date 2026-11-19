//! Trusted native-worker launcher, not a public Start/Resume authorization.
//! Requires an already persisted C09R reservation. No shell, PATH search, inherited
//! environment, reader thread, output collection, process-name lookup or PID kill.
#![cfg(windows)]
use super::admission::{AdmissionError, CleanupStatus, PreparedWorkload, QueueCredits};
use super::job::{JobBudget, JobSnapshot, OwnedJobKey};
use super::sampling::{ProcessKey, WindowsSampler};
use shinatama_state::database::Database;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs::File;
use std::mem::size_of;
use std::os::windows::{
    ffi::OsStrExt,
    io::{AsHandle, AsRawHandle, FromRawHandle, OwnedHandle},
};
use std::path::{Component, Path, PathBuf};
use std::ptr::{null, null_mut};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows_sys::Win32::{
    Foundation::{
        GetLastError, HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation, WAIT_OBJECT_0,
        WAIT_TIMEOUT,
    },
    Security::SECURITY_ATTRIBUTES,
    System::{
        Memory::{GetProcessHeap, HeapAlloc, HeapFree},
        Pipes::CreatePipe,
        Threading::{
            BELOW_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS, CREATE_NO_WINDOW, CREATE_SUSPENDED,
            CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess,
            InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION, ResumeThread,
            STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute,
            WaitForSingleObject,
        },
    },
};

#[derive(Debug)]
pub enum ProcessError {
    InvalidCommand,
    Allocation,
    Os { operation: &'static str, code: u32 },
    UnexpectedSuspendCount,
    ClockUnavailable,
    Admission(AdmissionError),
}
impl From<AdmissionError> for ProcessError {
    fn from(value: AdmissionError) -> Self {
        Self::Admission(value)
    }
}
fn os(operation: &'static str) -> ProcessError {
    ProcessError::Os {
        operation,
        code: unsafe { GetLastError() },
    }
}

/// No serde: only a trusted, separately qualified recipe may provide these values.
pub struct NativeCommand {
    executable: PathBuf,
    cwd: PathBuf,
    command_line: Vec<u16>,
    environment: Vec<u16>,
    priority_class: u32,
}
impl std::fmt::Debug for NativeCommand {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("NativeCommand([redacted])")
    }
}
impl NativeCommand {
    pub fn new(
        executable: PathBuf,
        arguments: Vec<OsString>,
        cwd: PathBuf,
        environment: BTreeMap<OsString, OsString>,
    ) -> Result<Self, ProcessError> {
        if !absolute(&executable)
            || !absolute(&cwd)
            || !executable
                .extension()
                .is_some_and(|value| value.eq_ignore_ascii_case("exe"))
            || arguments.len() > 128
            || environment.len() > 1024
        {
            return Err(ProcessError::InvalidCommand);
        }
        let command_line = command_line(&executable, &arguments)?;
        let environment = environment_block(environment)?;
        Ok(Self {
            executable,
            cwd,
            command_line,
            environment,
            priority_class: BELOW_NORMAL_PRIORITY_CLASS,
        })
    }
    /// The shared model proxy serves every interactive session. Other contained
    /// workers keep their background priority and all job budgets still apply.
    pub(crate) fn high_priority(mut self) -> Self {
        self.priority_class = HIGH_PRIORITY_CLASS;
        self
    }
}
fn absolute(path: &Path) -> bool {
    path.is_absolute()
        && path.as_os_str().len() <= 8192
        && !path.as_os_str().as_encoded_bytes().contains(&0)
        && !path
            .components()
            .any(|value| matches!(value, Component::ParentDir | Component::CurDir))
}
fn command_line(executable: &Path, args: &[OsString]) -> Result<Vec<u16>, ProcessError> {
    let mut result = Vec::new();
    for argument in
        std::iter::once(executable.as_os_str()).chain(args.iter().map(OsString::as_os_str))
    {
        if argument.len() > 8192 {
            return Err(ProcessError::InvalidCommand);
        }
        let units: Vec<_> = argument.encode_wide().collect();
        if units.len() > 8192 || units.contains(&0) {
            return Err(ProcessError::InvalidCommand);
        }
        if !result.is_empty() {
            result.push(b' ' as u16);
        }
        result.push(b'"' as u16);
        let mut slashes = 0;
        for unit in units {
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
        if result.len() >= 32766 {
            return Err(ProcessError::InvalidCommand);
        }
    }
    result.push(0);
    Ok(result)
}
fn environment_block(entries: BTreeMap<OsString, OsString>) -> Result<Vec<u16>, ProcessError> {
    let mut names = BTreeSet::new();
    let mut sorted = Vec::new();
    for (name, value) in entries {
        let name_text = name.to_str().ok_or(ProcessError::InvalidCommand)?;
        let pseudo_drive = name_text.len() == 3
            && name_text.starts_with('=')
            && name_text.as_bytes()[1].is_ascii_alphabetic()
            && name_text.ends_with(':');
        if name_text.is_empty()
            || name_text.len() > 256
            || !name_text.is_ascii()
            || name_text.as_bytes().contains(&0)
            || (name_text.contains('=') && !pseudo_drive)
            || value.as_encoded_bytes().contains(&0)
            || value.len() > 32766
            || !names.insert(name_text.to_ascii_uppercase())
        {
            return Err(ProcessError::InvalidCommand);
        }
        sorted.push((name_text.to_ascii_uppercase(), name, value));
    }
    sorted.sort_by(|left, right| left.0.cmp(&right.0));
    let mut block = Vec::new();
    for (_, name, value) in sorted {
        block.extend(name.encode_wide());
        block.push(b'=' as u16);
        block.extend(value.encode_wide());
        block.push(0);
        if block.len() >= 32766 {
            return Err(ProcessError::InvalidCommand);
        }
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

struct Pipe {
    read: OwnedHandle,
    write: OwnedHandle,
}
fn pipe() -> Result<Pipe, ProcessError> {
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    let (mut read, mut write) = (null_mut(), null_mut());
    if unsafe { CreatePipe(&mut read, &mut write, &attributes, 16 * 1024) } == 0 {
        return Err(os("create bounded pipe"));
    }
    Ok(Pipe {
        read: unsafe { OwnedHandle::from_raw_handle(read) },
        write: unsafe { OwnedHandle::from_raw_handle(write) },
    })
}
fn not_inherited(handle: &OwnedHandle) -> Result<(), ProcessError> {
    if unsafe { SetHandleInformation(handle.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(os("restrict parent pipe inheritance"));
    }
    Ok(())
}
struct Attributes {
    pointer: LPPROC_THREAD_ATTRIBUTE_LIST,
    heap: HANDLE,
}
impl Attributes {
    fn new() -> Result<Self, ProcessError> {
        let mut bytes = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut bytes);
        }
        if bytes == 0 || bytes > 16384 {
            return Err(ProcessError::Allocation);
        }
        let heap = unsafe { GetProcessHeap() };
        let pointer = unsafe { HeapAlloc(heap, 0, bytes) };
        if pointer.is_null() {
            return Err(ProcessError::Allocation);
        }
        if unsafe { InitializeProcThreadAttributeList(pointer, 2, 0, &mut bytes) } == 0 {
            let error = os("initialize startup attributes");
            unsafe {
                HeapFree(heap, 0, pointer);
            }
            return Err(error);
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
struct SuspendedChild {
    process: Option<OwnedHandle>,
    thread: Option<OwnedHandle>,
}
impl Drop for SuspendedChild {
    fn drop(&mut self) {
        if let Some(process) = &self.process {
            unsafe {
                TerminateProcess(process.as_raw_handle(), 75);
            }
        }
    }
}
pub struct SpawnedProcess {
    pub process: ContainedProcess,
    pub stdin: File,
    pub stdout: File,
    pub stderr: File,
}
pub struct SpawnFailure {
    pub error: ProcessError,
    /// The original ticket/job remains available for explicit cleanup. Dropping
    /// this owner conservatively retires it; an error never silently frees it.
    pub workload: PreparedWorkload,
}
impl std::fmt::Debug for SpawnFailure {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("SpawnFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
struct RunningChild {
    process: OwnedHandle,
    key: ProcessKey,
    stdin: File,
    stdout: File,
    stderr: File,
}
pub struct ContainedProcess {
    process: OwnedHandle,
    workload: PreparedWorkload,
    key: ProcessKey,
}
impl ContainedProcess {
    pub fn key(&self) -> ProcessKey {
        self.key
    }
    pub fn owned_job_key(&self) -> OwnedJobKey {
        self.workload.owned_key()
    }
    pub fn queue_credits(&self) -> QueueCredits {
        self.workload.queue_credits()
    }
    /// Limits read back from this exact owned job; absent after native close.
    pub fn applied_budget(&self) -> Option<JobBudget> {
        self.workload.applied_budget()
    }
    pub fn sample(&self, sampler: &WindowsSampler) -> Result<JobSnapshot, ProcessError> {
        Ok(self.workload.sample(sampler)?)
    }
    pub fn renew(
        &mut self,
        db: &Database,
        now_ms: i64,
        lease_until_ms: i64,
    ) -> Result<(), ProcessError> {
        Ok(self.workload.renew(db, now_ms, lease_until_ms)?)
    }
    pub fn try_wait(&self) -> Result<Option<u32>, ProcessError> {
        match unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                if unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) } == 0 {
                    return Err(os("read owned exit code"));
                }
                Ok(Some(code))
            }
            _ => Err(os("inspect owned process")),
        }
    }
    /// Owner worker only. Close/drop bounded reader queues before expecting
    /// release; the reservation retains capacity while any queue credits live.
    pub fn finish_cleanup(
        &mut self,
        db: &Database,
        timeout: Duration,
    ) -> Result<CleanupStatus, ProcessError> {
        Ok(self.workload.finish_cleanup(db, timeout)?)
    }
    pub fn retry_cleanup(
        &mut self,
        db: &Database,
        timeout: Duration,
    ) -> Result<CleanupStatus, ProcessError> {
        Ok(self.workload.retry_cleanup(db, timeout)?)
    }
}
impl Drop for ContainedProcess {
    fn drop(&mut self) {
        // Exact owned handle only; the retained WorkloadJob then closes its own
        // tree. No wait, SQL, global PID operation or capacity release occurs.
        if unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } == WAIT_TIMEOUT {
            unsafe {
                TerminateProcess(self.process.as_raw_handle(), 76);
            }
        }
    }
}

/// Blocking owner-worker operation. Existing product capability gates remain
/// closed; constructing NativeCommand/reservation does not qualify a recipe.
/// Failure is terminal: never retry CreateProcess without JOB_LIST or assignment
/// after execution. The child is verified/activated before its FIRST instruction.
#[allow(
    clippy::result_large_err,
    reason = "Keep the failed workload owned without allocating during resource pressure."
)]
pub fn spawn_contained(
    mut command: NativeCommand,
    mut workload: PreparedWorkload,
    database: &Database,
    now_ms: i64,
    lease_expires_at_ms: i64,
) -> Result<SpawnedProcess, SpawnFailure> {
    let child = match spawn_child(
        &mut command,
        &mut workload,
        database,
        now_ms,
        lease_expires_at_ms,
    ) {
        Ok(child) => child,
        Err(error) => return Err(SpawnFailure { error, workload }),
    };
    Ok(SpawnedProcess {
        process: ContainedProcess {
            process: child.process,
            workload,
            key: child.key,
        },
        stdin: child.stdin,
        stdout: child.stdout,
        stderr: child.stderr,
    })
}
fn spawn_child(
    command: &mut NativeCommand,
    workload: &mut PreparedWorkload,
    database: &Database,
    now_ms: i64,
    lease_expires_at_ms: i64,
) -> Result<RunningChild, ProcessError> {
    let input = pipe()?;
    let output = pipe()?;
    let error = pipe()?;
    not_inherited(&input.write)?;
    not_inherited(&output.read)?;
    not_inherited(&error.read)?;
    let mut child = {
        // The inner scope destroys the attribute list BEFORE the pinned binding
        // and its referenced handle storage, including all early-return paths.
        let binding = std::pin::pin!(workload.startup_binding(now_ms)?);
        let inherited = [
            input.read.as_raw_handle(),
            output.write.as_raw_handle(),
            error.write.as_raw_handle(),
        ];
        let attributes = Attributes::new()?;
        if unsafe {
            UpdateProcThreadAttribute(
                attributes.pointer,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                inherited.as_ptr().cast(),
                size_of::<HANDLE>() * inherited.len(),
                null_mut(),
                null(),
            )
        } == 0
        {
            return Err(os("apply exact stdio handle list"));
        }
        unsafe { binding.as_ref().apply_to(attributes.pointer) }.map_err(AdmissionError::from)?;
        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = inherited[0];
        startup.StartupInfo.hStdOutput = inherited[1];
        startup.StartupInfo.hStdError = inherited[2];
        startup.lpAttributeList = attributes.pointer;
        let executable: Vec<u16> = command
            .executable
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let cwd: Vec<u16> = command
            .cwd
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let mut info = PROCESS_INFORMATION::default();
        if unsafe {
            CreateProcessW(
                executable.as_ptr(),
                command.command_line.as_mut_ptr(),
                null(),
                null(),
                1,
                // Priority is selected by the trusted owner, never by agent input.
                CREATE_SUSPENDED
                    | CREATE_UNICODE_ENVIRONMENT
                    | CREATE_NO_WINDOW
                    | EXTENDED_STARTUPINFO_PRESENT
                    | command.priority_class,
                command.environment.as_ptr().cast(),
                cwd.as_ptr(),
                &startup.StartupInfo,
                &mut info,
            )
        } == 0
        {
            return Err(os("create contained suspended process"));
        }
        SuspendedChild {
            process: Some(unsafe { OwnedHandle::from_raw_handle(info.hProcess) }),
            thread: Some(unsafe { OwnedHandle::from_raw_handle(info.hThread) }),
        }
    };
    let process = child.process.as_ref().expect("created process handle");
    let actual_now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| i64::try_from(value.as_millis()).ok())
        .filter(|value| *value >= now_ms)
        .ok_or(ProcessError::ClockUnavailable)?;
    let key = workload.activate_verified(
        database,
        process.as_handle(),
        actual_now,
        lease_expires_at_ms,
    )?;
    // The attribute-list scope is already gone and HANDLE_LIST contains only
    // stdio. This executor exposes no later root assignment or inherited job
    // handle. Seal before the child's first instruction, not after it runs.
    workload.seal_assignments()?;
    let count = unsafe {
        ResumeThread(
            child
                .thread
                .as_ref()
                .expect("created thread handle")
                .as_raw_handle(),
        )
    };
    if count == u32::MAX {
        return Err(os("resume contained primary thread"));
    }
    if count != 1 {
        return Err(ProcessError::UnexpectedSuspendCount);
    }
    let process = child.process.take().expect("created process handle");
    drop(child); // Close the primary thread handle; its process guard was consumed.
    drop(input.read);
    drop(output.write);
    drop(error.write);
    Ok(RunningChild {
        process,
        key,
        stdin: File::from(input.write),
        stdout: File::from(output.read),
        stderr: File::from(error.read),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoting_is_crt_literal_and_never_shell_interpreted() {
        let result = command_line(
            Path::new("C:\\owned\\node.exe"),
            &[
                "a b".into(),
                "quoted\"value".into(),
                "ends\\".into(),
                "$()&".into(),
            ],
        )
        .unwrap();
        assert_eq!(
            String::from_utf16(&result[..result.len() - 1]).unwrap(),
            "\"C:\\owned\\node.exe\" \"a b\" \"quoted\\\"value\" \"ends\\\\\" \"$()&\""
        );
    }
    #[test]
    fn environment_is_explicit_sorted_bounded_and_rejects_duplicates_and_nul() {
        let environment: BTreeMap<OsString, OsString> = [("z", "synthetic"), ("A", "value")]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        let block = environment_block(environment).unwrap();
        assert_eq!(
            String::from_utf16(&block).unwrap(),
            "A=value\0z=synthetic\0\0"
        );
        let duplicates = [
            (OsString::from("Path"), OsString::from("a")),
            (OsString::from("PATH"), OsString::from("b")),
        ]
        .into_iter()
        .collect();
        assert!(environment_block(duplicates).is_err());
        let invalid = [(OsString::from("KEY"), OsString::from("x\0y"))]
            .into_iter()
            .collect();
        assert!(environment_block(invalid).is_err());
    }
}
