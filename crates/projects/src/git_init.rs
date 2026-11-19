//! Optional Git initialization, contained before the primary thread can execute.
//!
//! These fixed limits cover this helper only, not C09R's adaptive supervision.
//! Build wiring requires windows-sys/Win32_System_Pipes. No raw child output is retained.
//! Git's empty --template= is intentional: setup.c/copy_templates returns for an
//! empty template path, so neither system templates nor their hooks are copied.
//! References: https://git-scm.com/docs/git-init and
//! https://github.com/git/git/blob/master/setup.c (copy_templates).

use crate::create::{Cancellation, GitInitError, GitInitializer};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

const FREE: u8 = 0;
const ACTIVE: u8 = 1;
const RETIRED: u8 = 2;
static GIT_SLOT: AtomicU8 = AtomicU8::new(FREE);

/// The caller selects a trusted native Git executable; this type never searches
/// PATH, resolves a wrapper, invokes a shell, or launches an agent/model.
pub struct ContainedGit {
    executable: PathBuf,
}

impl ContainedGit {
    pub fn new(executable: PathBuf) -> Result<Self, GitInitError> {
        if !executable.is_absolute()
            || !executable
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("git.exe"))
        {
            return Err(GitInitError::Unavailable(
                "Select a trusted absolute path to native git.exe.".into(),
            ));
        }
        Ok(Self { executable })
    }
}

struct Active<'a> {
    state: &'a AtomicU8,
    unconfirmed_child: Arc<AtomicBool>,
}
impl<'a> Active<'a> {
    fn acquire(state: &'a AtomicU8) -> Result<Self, GitInitError> {
        match state.compare_exchange(FREE, ACTIVE, Ordering::Acquire, Ordering::Relaxed) {
            Ok(_) => Ok(Self {
                state,
                unconfirmed_child: Arc::new(AtomicBool::new(false)),
            }),
            Err(ACTIVE) => Err(GitInitError::Busy),
            Err(_) => Err(GitInitError::Containment(
                "Previous Git shutdown could not be confirmed. Further Git initialization is paused for this app run.".into(),
            )),
        }
    }
}
impl Drop for Active<'_> {
    fn drop(&mut self) {
        // Closing a job requests termination but does not prove kernel rundown
        // finished. Never admit another launch after an unconfirmed cleanup.
        self.state.store(
            if self.unconfirmed_child.load(Ordering::Acquire) {
                RETIRED
            } else {
                FREE
            },
            Ordering::Release,
        );
    }
}

impl GitInitializer for ContainedGit {
    fn initialize(
        &self,
        directory: &Path,
        cancellation: &dyn Cancellation,
    ) -> Result<(), GitInitError> {
        if cancellation.is_cancelled() {
            return Err(GitInitError::Cancelled);
        }
        let active = Active::acquire(&GIT_SLOT)?;
        #[cfg(windows)]
        {
            native::initialize(
                &self.executable,
                directory,
                cancellation,
                active.unconfirmed_child.clone(),
            )
        }
        #[cfg(not(windows))]
        {
            let _ = (&self.executable, directory, &active);
            Err(GitInitError::Unavailable(
                "Contained Git initialization is supported only on Windows.".into(),
            ))
        }
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::{
        ffi::OsString,
        fs::{self, File, OpenOptions},
        io::Read,
        mem,
        os::windows::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
            io::{AsRawHandle, FromRawHandle},
        },
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, ERROR_BROKEN_PIPE, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE,
            SetHandleInformation, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        Security::SECURITY_ATTRIBUTES,
        Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            FILE_SHARE_WRITE, GetDriveTypeW,
        },
        System::{
            JobObjects::{
                CreateJobObjectW, IsProcessInJob, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
                JOB_OBJECT_LIMIT_JOB_MEMORY, JOB_OBJECT_LIMIT_JOB_TIME,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
                JOB_OBJECT_TERMINATE_AT_END_OF_JOB, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
                JOBOBJECT_END_OF_JOB_TIME_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JobObjectBasicAccountingInformation, JobObjectEndOfJobTimeInformation,
                JobObjectExtendedLimitInformation, QueryInformationJobObject,
                SetInformationJobObject, TerminateJobObject,
            },
            Memory::{GetProcessHeap, HeapAlloc, HeapFree},
            Pipes::{CreatePipe, PeekNamedPipe},
            Threading::{
                CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
                DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess,
                InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST,
                PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW,
                TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject,
            },
        },
    };

    const MAX_OUTPUT_BYTES: usize = 32 * 1024;
    const MAX_WIDE_UNITS: usize = 16 * 1024;
    const WALL_TIME: Duration = Duration::from_secs(5);
    const POLL_MILLIS: u32 = 20;

    const ARGUMENTS: &[&str] = &[
        "--no-pager",
        "--no-optional-locks",
        "-c",
        "core.hooksPath=NUL",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "init.defaultBranch=main",
        "init",
        "--initial-branch=main",
        "--template=",
        "--shared=false",
        "--quiet",
        ".",
    ];

    pub(super) fn initialize(
        executable: &Path,
        directory: &Path,
        cancellation: &dyn Cancellation,
        unconfirmed_child: Arc<AtomicBool>,
    ) -> Result<(), GitInitError> {
        // Hold no-delete-sharing guards through spawn and completion. Filesystem
        // admission itself is not a hard-preemptible OS operation.
        let _program_guards = guard_local_path(executable, false)?;
        let _directory_guards = guard_local_path(directory, true)?;
        if fs::read_dir(directory)
            .map_err(|_| failure("Cannot inspect the new project directory."))?
            .next()
            .is_some()
        {
            return Err(failure(
                "Git initialization requires an empty staging directory.",
            ));
        }
        if cancellation.is_cancelled() {
            return Err(GitInitError::Cancelled);
        }
        let arguments: Vec<_> = ARGUMENTS.iter().map(OsString::from).collect();
        let environment = environment(directory)?;
        let mut child = launch(
            executable,
            &arguments,
            directory,
            &environment,
            LaunchControl::new(unconfirmed_child),
        )?;
        let outcome = supervise(&mut child, cancellation, WALL_TIME);
        // A successful root exit is insufficient: no descendant may retain cwd
        // handles when the creator attempts its atomic staging rename.
        child.stop()?;
        outcome?;
        if cancellation.is_cancelled() {
            return Err(GitInitError::Cancelled);
        }
        let head_path = directory.join(".git").join("HEAD");
        let head_guards = guard_local_path(&head_path, false)?;
        let head = head_guards
            .last()
            .ok_or_else(|| failure("Git did not create its branch reference."))?;
        let mut value = Vec::new();
        head.take(64)
            .read_to_end(&mut value)
            .map_err(|_| failure("Cannot verify the initial Git branch."))?;
        if value != b"ref: refs/heads/main\n" {
            return Err(failure("Git did not create the requested main branch."));
        }
        Ok(())
    }

    fn failure(message: &str) -> GitInitError {
        GitInitError::Failed(message.into())
    }
    fn containment(message: &str) -> GitInitError {
        GitInitError::Containment(message.into())
    }

    fn verify_owned(job: HANDLE, process: HANDLE) -> bool {
        let mut member = 0;
        // SAFETY: callers supply this launch's private job and suspended process.
        unsafe { IsProcessInJob(process, job, &mut member) != 0 && member != 0 }
    }

    fn resume_owned(thread: HANDLE) -> bool {
        // SAFETY: callers supply the primary thread after successful containment.
        unsafe { ResumeThread(thread) != u32::MAX }
    }

    struct LaunchControl {
        unconfirmed_child: Arc<AtomicBool>,
        verify_job: fn(HANDLE, HANDLE) -> bool,
        resume: fn(HANDLE) -> bool,
        #[cfg(test)]
        invalid_job_attribute: bool,
    }
    impl LaunchControl {
        fn new(unconfirmed_child: Arc<AtomicBool>) -> Self {
            Self {
                unconfirmed_child,
                verify_job: verify_owned,
                resume: resume_owned,
                #[cfg(test)]
                invalid_job_attribute: false,
            }
        }
    }

    struct Handle(HANDLE);
    impl Handle {
        fn new(handle: HANDLE) -> Result<Self, GitInitError> {
            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                Err(containment("Cannot allocate a private process handle."))
            } else {
                Ok(Self(handle))
            }
        }
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: this wrapper has sole ownership of a valid kernel handle.
            unsafe { CloseHandle(self.0) };
        }
    }

    fn guard_local_path(path: &Path, directory: bool) -> Result<Vec<File>, GitInitError> {
        let wide: Vec<_> = path.as_os_str().encode_wide().collect();
        let lexical = path.to_string_lossy();
        // Canonical staging paths may carry the ordinary extended-length prefix.
        let raw = lexical.strip_prefix(r"\\?\").unwrap_or(&lexical);
        let bytes = raw.as_bytes();
        if wide.contains(&0)
            || wide.len() > 4096
            || bytes.len() < 3
            || !bytes[0].is_ascii_alphabetic()
            || bytes[1] != b':'
            || !matches!(bytes[2], b'\\' | b'/')
            || bytes.iter().skip(2).any(|byte| *byte == b':')
            || raw.split(['\\', '/']).any(|part| part == "..")
        {
            return Err(GitInitError::Unavailable(
                "Git requires local absolute paths.".into(),
            ));
        }
        let root = [u16::from(bytes[0]), u16::from(b':'), u16::from(b'\\'), 0];
        // SAFETY: root is a NUL-terminated drive root; DRIVE_FIXED is 3.
        if unsafe { GetDriveTypeW(root.as_ptr()) } != 3 {
            return Err(GitInitError::Unavailable(
                "Git requires a local fixed drive.".into(),
            ));
        }
        let ancestors: Vec<_> = path.ancestors().take(65).collect();
        if ancestors.len() > 64 {
            return Err(GitInitError::Unavailable(
                "The Git path is too deep.".into(),
            ));
        }
        let mut guards = Vec::with_capacity(ancestors.len());
        for ancestor in ancestors.into_iter().rev() {
            let leaf = ancestor == path;
            let mut options = OpenOptions::new();
            options.read(true).share_mode(if leaf && !directory {
                FILE_SHARE_READ
            } else {
                FILE_SHARE_READ | FILE_SHARE_WRITE
            });
            options.custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
            let handle = options.open(ancestor).map_err(|_| {
                GitInitError::Unavailable("Cannot lock the local Git paths.".into())
            })?;
            let metadata = handle.metadata().map_err(|_| {
                GitInitError::Unavailable("Cannot verify the local Git paths.".into())
            })?;
            if metadata.file_attributes() & 0x400 != 0
                || if leaf && !directory {
                    !metadata.is_file()
                } else {
                    !metadata.is_dir()
                }
            {
                return Err(GitInitError::Unavailable(
                    "Git paths cannot traverse reparse points.".into(),
                ));
            }
            guards.push(handle);
        }
        Ok(guards)
    }

    fn environment(directory: &Path) -> Result<Vec<u16>, GitInitError> {
        let system_root = std::env::var_os("SystemRoot").ok_or_else(|| {
            GitInitError::Unavailable("Windows system directory is unavailable.".into())
        })?;
        let home = directory.as_os_str();
        let mut values: Vec<(OsString, OsString)> = vec![
            ("SystemRoot".into(), system_root),
            ("HOME".into(), home.into()),
            ("USERPROFILE".into(), home.into()),
            ("XDG_CONFIG_HOME".into(), home.into()),
            ("TEMP".into(), home.into()),
            ("TMP".into(), home.into()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_CONFIG_SYSTEM".into(), "NUL".into()),
            ("GIT_CONFIG_GLOBAL".into(), "NUL".into()),
            ("GIT_CONFIG_COUNT".into(), "0".into()),
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ("GCM_INTERACTIVE".into(), "Never".into()),
            ("GIT_ASKPASS".into(), "".into()),
            ("SSH_ASKPASS".into(), "".into()),
            ("GIT_ALLOW_PROTOCOL".into(), "".into()),
            ("GIT_OPTIONAL_LOCKS".into(), "0".into()),
            ("GIT_ATTR_NOSYSTEM".into(), "1".into()),
            ("GIT_CEILING_DIRECTORIES".into(), home.into()),
            ("GIT_DISCOVERY_ACROSS_FILESYSTEM".into(), "0".into()),
        ];
        encode_environment(&mut values)
    }

    fn encode_environment(values: &mut [(OsString, OsString)]) -> Result<Vec<u16>, GitInitError> {
        values.sort_by_key(|(key, _)| key.to_string_lossy().to_ascii_uppercase());
        let mut result = Vec::new();
        for (key, value) in values {
            let key: Vec<_> = key.encode_wide().collect();
            let value: Vec<_> = value.encode_wide().collect();
            if key.is_empty()
                || key.contains(&0)
                || key.contains(&u16::from(b'='))
                || value.contains(&0)
            {
                return Err(containment("Invalid isolated Git environment."));
            }
            if result
                .len()
                .saturating_add(key.len())
                .saturating_add(value.len())
                .saturating_add(3)
                > MAX_WIDE_UNITS
            {
                return Err(containment("Isolated Git environment exceeds its limit."));
            }
            result.extend(key);
            result.push(u16::from(b'='));
            result.extend(value);
            result.push(0);
        }
        result.push(0);
        if result.len() == 1 {
            result.push(0);
        }
        Ok(result)
    }

    fn command_line(executable: &Path, arguments: &[OsString]) -> Result<Vec<u16>, GitInitError> {
        let mut result = Vec::new();
        for argument in
            std::iter::once(executable.as_os_str()).chain(arguments.iter().map(OsString::as_os_str))
        {
            if !result.is_empty() {
                result.push(u16::from(b' '));
            }
            result.push(u16::from(b'"'));
            let mut slashes = 0;
            for unit in argument.encode_wide() {
                if unit == 0 {
                    return Err(containment("Git arguments contain a NUL."));
                }
                if unit == u16::from(b'\\') {
                    slashes += 1;
                    continue;
                }
                let count = if unit == u16::from(b'"') {
                    2 * slashes + 1
                } else {
                    slashes
                };
                result.extend(std::iter::repeat_n(u16::from(b'\\'), count));
                result.push(unit);
                slashes = 0;
            }
            result.extend(std::iter::repeat_n(u16::from(b'\\'), 2 * slashes));
            result.push(u16::from(b'"'));
            if result.len() >= MAX_WIDE_UNITS {
                return Err(containment("Git arguments exceed their limit."));
            }
        }
        result.push(0);
        Ok(result)
    }

    struct Attributes {
        pointer: LPPROC_THREAD_ATTRIBUTE_LIST,
        heap: HANDLE,
    }
    impl Attributes {
        fn handles_and_job(handles: &[HANDLE], jobs: &[HANDLE]) -> Result<Self, GitInitError> {
            let mut size = 0;
            // SAFETY: the first call only queries the allocation size.
            unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 2, 0, &mut size) };
            if size == 0 || size > 16 * 1024 {
                return Err(containment("Invalid process attribute size."));
            }
            // SAFETY: the process heap supplies the alignment Windows requires.
            let heap = unsafe { GetProcessHeap() };
            let pointer: LPPROC_THREAD_ATTRIBUTE_LIST = unsafe { HeapAlloc(heap, 0, size) };
            if pointer.is_null() {
                return Err(containment("Cannot allocate process attributes."));
            }
            if unsafe { InitializeProcThreadAttributeList(pointer, 2, 0, &mut size) } == 0 {
                unsafe { HeapFree(heap, 0, pointer.cast()) };
                return Err(containment("Cannot initialize process attributes."));
            }
            let attributes = Self { pointer, heap };
            // SAFETY: handle storage remains live through CreateProcessW.
            if unsafe {
                UpdateProcThreadAttribute(
                    pointer,
                    0,
                    PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                    handles.as_ptr().cast(),
                    mem::size_of_val(handles),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            } == 0
            {
                return Err(containment("Cannot isolate inherited handles."));
            }
            // Assignment is part of process creation, closing the host-crash
            // window between CreateProcessW and a later AssignProcessToJobObject.
            // Failure is terminal; there is no launch without this attribute.
            if unsafe {
                UpdateProcThreadAttribute(
                    pointer,
                    0,
                    PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                    jobs.as_ptr().cast(),
                    mem::size_of_val(jobs),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            } == 0
            {
                return Err(containment(
                    "Cannot bind the private job at process creation.",
                ));
            }
            Ok(attributes)
        }
    }
    impl Drop for Attributes {
        fn drop(&mut self) {
            // SAFETY: both resources were initialized by this wrapper.
            unsafe {
                DeleteProcThreadAttributeList(self.pointer);
                HeapFree(self.heap, 0, self.pointer.cast());
            }
        }
    }

    fn job() -> Result<Handle, GitInitError> {
        // SAFETY: a private unnamed job has no inheritable security attributes.
        let job = Handle::new(unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) })?;
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_JOB_MEMORY
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | JOB_OBJECT_LIMIT_JOB_TIME;
        limits.BasicLimitInformation.ActiveProcessLimit = 2;
        limits.BasicLimitInformation.PerJobUserTimeLimit = 5 * 10_000_000;
        limits.ProcessMemoryLimit = 128 * 1024 * 1024;
        limits.JobMemoryLimit = 256 * 1024 * 1024;
        let end = JOBOBJECT_END_OF_JOB_TIME_INFORMATION {
            EndOfJobTimeAction: JOB_OBJECT_TERMINATE_AT_END_OF_JOB,
        };
        // SAFETY: both records have the exact type and size expected by Windows.
        if unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                mem::size_of_val(&limits) as u32,
            )
        } == 0
            || unsafe {
                SetInformationJobObject(
                    job.0,
                    JobObjectEndOfJobTimeInformation,
                    (&end as *const JOBOBJECT_END_OF_JOB_TIME_INFORMATION).cast(),
                    mem::size_of_val(&end) as u32,
                )
            } == 0
        {
            return Err(containment("Cannot enforce the Git process budget."));
        }
        Ok(job)
    }

    struct Child {
        process: Handle,
        thread: Handle,
        job: Handle,
        output: File,
        stopped: bool,
        cleanup_attempted: bool,
        unconfirmed_child: Arc<AtomicBool>,
    }
    impl Child {
        fn stop(&mut self) -> Result<(), GitInitError> {
            self.stop_with(Self::confirm_stopped)
        }

        fn stop_with(
            &mut self,
            confirm: impl FnOnce(&Self, Instant) -> Result<(), GitInitError>,
        ) -> Result<(), GitInitError> {
            if self.stopped {
                return Ok(());
            }
            if self.cleanup_attempted {
                return Err(containment("Owned Git shutdown remains unconfirmed."));
            }
            self.cleanup_attempted = true;
            let started = Instant::now();
            // SAFETY: only this launch's process and job can be terminated.
            unsafe {
                TerminateProcess(self.process.0, 1);
                if TerminateJobObject(self.job.0, 1) == 0 {
                    return Err(containment("Cannot stop the owned Git process job."));
                }
            }
            confirm(self, started)?;
            self.stopped = true;
            self.unconfirmed_child.store(false, Ordering::Release);
            Ok(())
        }

        fn confirm_stopped(&self, started: Instant) -> Result<(), GitInitError> {
            // Accounting can reach zero while process rundown is still closing
            // cwd/file handles. Require the root process itself to be signaled.
            match unsafe { WaitForSingleObject(self.process.0, 250) } {
                WAIT_OBJECT_0 => {}
                WAIT_TIMEOUT => return Err(containment("The owned Git process has not stopped.")),
                _ => return Err(containment("Cannot confirm the owned Git process stopped.")),
            }
            loop {
                let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION =
                    unsafe { mem::zeroed() };
                // SAFETY: the record matches the requested job accounting class.
                if unsafe {
                    QueryInformationJobObject(
                        self.job.0,
                        JobObjectBasicAccountingInformation,
                        (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                        mem::size_of_val(&accounting) as u32,
                        std::ptr::null_mut(),
                    )
                } == 0
                {
                    return Err(containment("Cannot verify the owned Git job stopped."));
                }
                if accounting.ActiveProcesses == 0 {
                    return Ok(());
                }
                if started.elapsed() >= Duration::from_millis(250) {
                    return Err(containment("The owned Git job has not finished stopping."));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
    impl Drop for Child {
        fn drop(&mut self) {
            // At most one finite cleanup attempt, including failure paths before
            // Resume. Handles still close on an unconfirmed result; the slot retires.
            if !self.cleanup_attempted {
                let _ = self.stop();
            }
        }
    }

    fn launch(
        executable: &Path,
        arguments: &[OsString],
        directory: &Path,
        environment: &[u16],
        control: LaunchControl,
    ) -> Result<Child, GitInitError> {
        let job = job()?;
        let security = SECURITY_ATTRIBUTES {
            nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: 1,
        };
        let mut read = std::ptr::null_mut();
        let mut write = std::ptr::null_mut();
        // SAFETY: output handles and the security record are valid.
        if unsafe { CreatePipe(&mut read, &mut write, &security, 4096) } == 0 {
            return Err(containment("Cannot allocate the bounded Git output pipe."));
        }
        let read = Handle::new(read)?;
        let write = Handle::new(write)?;
        if unsafe { SetHandleInformation(read.0, HANDLE_FLAG_INHERIT, 0) } == 0 {
            return Err(containment("Cannot isolate the output reader."));
        }
        let input = OpenOptions::new()
            .read(true)
            .write(true)
            .open("NUL")
            .map_err(|_| containment("Cannot close Git standard input."))?;
        let input_handle = input.as_raw_handle() as HANDLE;
        if unsafe { SetHandleInformation(input_handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) }
            == 0
        {
            return Err(containment("Cannot isolate Git standard input."));
        }
        let handles = [input_handle, write.0];
        let job_handle = job.0;
        #[cfg(test)]
        let job_handle = if control.invalid_job_attribute {
            std::ptr::null_mut()
        } else {
            job_handle
        };
        let jobs = [job_handle];
        let attributes = Attributes::handles_and_job(&handles, &jobs)?;
        let mut startup: STARTUPINFOEXW = unsafe { mem::zeroed() };
        startup.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = input_handle;
        startup.StartupInfo.hStdOutput = write.0;
        startup.StartupInfo.hStdError = write.0;
        startup.lpAttributeList = attributes.pointer;
        let mut command = command_line(executable, arguments)?;
        let executable: Vec<_> = executable
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let cwd: Vec<_> = directory.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut information: PROCESS_INFORMATION = unsafe { mem::zeroed() };
        // SAFETY: all NUL-terminated buffers remain live, the command line is
        // writable, environment is explicit, and only the two listed handles inherit.
        if unsafe {
            CreateProcessW(
                executable.as_ptr(),
                command.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                CREATE_SUSPENDED
                    | CREATE_NO_WINDOW
                    | CREATE_UNICODE_ENVIRONMENT
                    | EXTENDED_STARTUPINFO_PRESENT,
                environment.as_ptr().cast(),
                cwd.as_ptr(),
                &startup.StartupInfo,
                &mut information,
            )
        } == 0
        {
            return Err(containment(
                "Windows could not create Git inside its private process job.",
            ));
        }
        control.unconfirmed_child.store(true, Ordering::Release);
        // The child was born in the private job and is still suspended. From
        // here every error closes that job and attempts confirmed cleanup.
        let read = mem::ManuallyDrop::new(read);
        let output = unsafe { File::from_raw_handle(read.0) };
        let child = Child {
            process: Handle(information.hProcess),
            thread: Handle(information.hThread),
            job,
            output,
            stopped: false,
            cleanup_attempted: false,
            unconfirmed_child: control.unconfirmed_child,
        };
        if !(control.verify_job)(child.job.0, child.process.0) {
            return Err(containment(
                "Cannot verify the Git process belongs to its private job.",
            ));
        }
        // SAFETY: the primary thread belongs to the now-contained suspended child.
        if !(control.resume)(child.thread.0) {
            return Err(containment("The contained Git process could not resume."));
        }
        Ok(child)
    }

    fn drain(child: &mut Child, total: &mut usize) -> Result<(), GitInitError> {
        let mut buffer = [0_u8; 4096];
        loop {
            let mut available = 0;
            // SAFETY: this is the only reader; peeking does not wait for child EOF.
            if unsafe {
                PeekNamedPipe(
                    child.output.as_raw_handle() as HANDLE,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut available,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                if std::io::Error::last_os_error().raw_os_error() == Some(ERROR_BROKEN_PIPE as i32)
                {
                    return Ok(());
                }
                return Err(failure("Cannot inspect Git output."));
            }
            if available == 0 {
                return Ok(());
            }
            let length = usize::min(available as usize, buffer.len());
            let count = child
                .output
                .read(&mut buffer[..length])
                .map_err(|_| failure("Cannot drain Git output."))?;
            if count == 0 {
                return Ok(());
            }
            *total = total.saturating_add(count);
            if *total > MAX_OUTPUT_BYTES {
                return Err(GitInitError::OutputLimit);
            }
        }
    }

    fn supervise(
        child: &mut Child,
        cancellation: &dyn Cancellation,
        timeout: Duration,
    ) -> Result<(), GitInitError> {
        let started = Instant::now();
        let mut output = 0;
        loop {
            if cancellation.is_cancelled() {
                return Err(GitInitError::Cancelled);
            }
            if started.elapsed() >= timeout {
                return Err(GitInitError::TimedOut);
            }
            drain(child, &mut output)?;
            // SAFETY: this handle belongs to the process being supervised.
            match unsafe { WaitForSingleObject(child.process.0, POLL_MILLIS) } {
                WAIT_OBJECT_0 => {
                    drain(child, &mut output)?;
                    let mut code = 0;
                    if unsafe { GetExitCodeProcess(child.process.0, &mut code) } == 0 {
                        return Err(failure("Cannot read the Git exit status."));
                    }
                    return if code == 0 {
                        Ok(())
                    } else {
                        Err(failure(&format!(
                            "Git initialization exited with code {code}."
                        )))
                    };
                }
                WAIT_TIMEOUT => {}
                WAIT_FAILED => return Err(containment("Cannot supervise the owned Git process.")),
                _ => return Err(containment("Unexpected Git process wait result.")),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{io::Write, sync::Mutex};

        static SERIAL: Mutex<()> = Mutex::new(());
        const MODE: &str = "SHINATAMA_GIT_SYNTHETIC_CHILD";
        struct NeverCancel;
        impl Cancellation for NeverCancel {
            fn is_cancelled(&self) -> bool {
                false
            }
        }

        struct Fixture(PathBuf);
        impl Fixture {
            fn new() -> Self {
                let path = std::env::temp_dir()
                    .join(format!("shinatama-git-helper-{}", uuid::Uuid::new_v4()));
                fs::create_dir(&path).unwrap();
                fs::write(path.join("unrelated-saved-work"), b"keep").unwrap();
                Self(path)
            }
            fn preserved(&self) {
                assert_eq!(
                    fs::read(self.0.join("unrelated-saved-work")).unwrap(),
                    b"keep"
                );
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                assert_eq!(self.0.parent(), Some(std::env::temp_dir().as_path()));
                assert!(
                    self.0
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("shinatama-git-helper-")
                );
                fs::remove_dir_all(&self.0).unwrap();
            }
        }

        fn fixture_environment(mode: &str) -> Vec<u16> {
            encode_environment(&mut [
                ("SystemRoot".into(), std::env::var_os("SystemRoot").unwrap()),
                (MODE.into(), mode.into()),
            ])
            .unwrap()
        }

        fn fixture_arguments() -> Vec<OsString> {
            let module = module_path!().split_once("::").unwrap().1;
            vec![
                "--exact".into(),
                format!("{module}::synthetic_child").into(),
                "--nocapture".into(),
                "--test-threads=1".into(),
            ]
        }

        fn launch_fixture(fixture: &Fixture, mode: &str) -> Child {
            launch(
                &std::env::current_exe().unwrap(),
                &fixture_arguments(),
                &fixture.0,
                &fixture_environment(mode),
                LaunchControl::new(Arc::new(AtomicBool::new(false))),
            )
            .unwrap()
        }

        fn wait_started(fixture: &Fixture) {
            let deadline = Instant::now() + Duration::from_secs(2);
            while !fixture.0.join("child-started").exists() {
                assert!(Instant::now() < deadline, "synthetic child did not start");
                std::thread::sleep(Duration::from_millis(5));
            }
        }

        // Re-executes only this same test binary with an explicit child-only env.
        // A normal test run returns without starting another process.
        #[test]
        fn synthetic_child() {
            let Ok(mode) = std::env::var(MODE) else {
                return;
            };
            fs::write("child-started", b"synthetic").unwrap();
            match mode.as_str() {
                "ok" => {}
                "hang" => loop {
                    std::thread::sleep(Duration::from_secs(1));
                },
                "output" => {
                    let block = [b'x'; 4096];
                    for _ in 0..512 {
                        std::io::stdout().write_all(&block).unwrap();
                    }
                }
                _ => panic!("unknown synthetic child mode"),
            }
        }

        #[test]
        fn contained_child_completes_and_staging_is_preserved() {
            let _serial = SERIAL.lock().unwrap();
            let fixture = Fixture::new();
            let mut child = launch_fixture(&fixture, "ok");
            assert!(supervise(&mut child, &NeverCancel, Duration::from_secs(3)).is_ok());
            child.stop().unwrap();
            assert!(child.stopped);
            fixture.preserved();
        }

        #[test]
        fn timed_out_child_is_stopped_without_removing_staging_work() {
            let _serial = SERIAL.lock().unwrap();
            let fixture = Fixture::new();
            let mut child = launch_fixture(&fixture, "hang");
            wait_started(&fixture);
            assert!(matches!(
                supervise(&mut child, &NeverCancel, Duration::from_millis(100)),
                Err(GitInitError::TimedOut)
            ));
            child.stop().unwrap();
            fixture.preserved();
        }

        #[test]
        fn cancellation_stops_the_owned_running_child() {
            struct Requested(AtomicBool);
            impl Cancellation for Requested {
                fn is_cancelled(&self) -> bool {
                    self.0.load(Ordering::Acquire)
                }
            }
            let _serial = SERIAL.lock().unwrap();
            let fixture = Fixture::new();
            let mut child = launch_fixture(&fixture, "hang");
            wait_started(&fixture);
            let cancellation = Requested(AtomicBool::new(true));
            assert!(matches!(
                supervise(&mut child, &cancellation, WALL_TIME),
                Err(GitInitError::Cancelled)
            ));
            child.stop().unwrap();
            fixture.preserved();
        }

        #[test]
        fn stopping_one_owned_job_keeps_another_healthy_process_running() {
            let _serial = SERIAL.lock().unwrap();
            let healthy_fixture = Fixture::new();
            let mut healthy = launch_fixture(&healthy_fixture, "hang");
            wait_started(&healthy_fixture);
            let failed_fixture = Fixture::new();
            let mut failed = launch_fixture(&failed_fixture, "hang");
            wait_started(&failed_fixture);
            assert!(matches!(
                supervise(&mut failed, &NeverCancel, Duration::from_millis(100)),
                Err(GitInitError::TimedOut)
            ));
            failed.stop().unwrap();
            // SAFETY: this is the independently owned healthy child's process handle.
            assert_eq!(
                unsafe { WaitForSingleObject(healthy.process.0, 0) },
                WAIT_TIMEOUT
            );
            healthy.stop().unwrap();
            healthy_fixture.preserved();
            failed_fixture.preserved();
        }

        #[test]
        fn excessive_output_is_drained_with_a_fixed_limit_and_stops_the_child() {
            let _serial = SERIAL.lock().unwrap();
            let fixture = Fixture::new();
            let mut child = launch_fixture(&fixture, "output");
            wait_started(&fixture);
            assert!(matches!(
                supervise(&mut child, &NeverCancel, Duration::from_secs(3)),
                Err(GitInitError::OutputLimit)
            ));
            child.stop().unwrap();
            fixture.preserved();
        }

        #[test]
        fn refused_membership_verification_or_resume_never_executes_the_child() {
            let _serial = SERIAL.lock().unwrap();
            for reject_membership in [true, false] {
                let fixture = Fixture::new();
                let unconfirmed = Arc::new(AtomicBool::new(false));
                let mut control = LaunchControl::new(unconfirmed.clone());
                if reject_membership {
                    control.verify_job = |_, _| false;
                }
                control.resume = |_| false;
                let result = launch(
                    &std::env::current_exe().unwrap(),
                    &fixture_arguments(),
                    &fixture.0,
                    &fixture_environment("ok"),
                    control,
                );
                assert!(matches!(result, Err(GitInitError::Containment(_))));
                assert!(
                    !unconfirmed.load(Ordering::Acquire),
                    "failure cleanup was confirmed"
                );
                assert!(!fixture.0.join("child-started").exists());
                fixture.preserved();
            }
        }

        #[test]
        fn invalid_creation_job_attribute_has_no_uncontained_fallback() {
            let _serial = SERIAL.lock().unwrap();
            let fixture = Fixture::new();
            let state = AtomicU8::new(FREE);
            let active = Active::acquire(&state).unwrap();
            let mut control = LaunchControl::new(active.unconfirmed_child.clone());
            control.invalid_job_attribute = true;
            let result = launch(
                &std::env::current_exe().unwrap(),
                &fixture_arguments(),
                &fixture.0,
                &fixture_environment("ok"),
                control,
            );
            assert!(matches!(result, Err(GitInitError::Containment(_))));
            assert!(!fixture.0.join("child-started").exists());
            drop(active);
            assert_eq!(state.load(Ordering::Acquire), FREE, "no child was created");
            fixture.preserved();
        }

        #[test]
        fn unconfirmed_native_cleanup_retires_slot_without_a_second_drop_attempt() {
            let _serial = SERIAL.lock().unwrap();
            let fixture = Fixture::new();
            let state = AtomicU8::new(FREE);
            let active = Active::acquire(&state).unwrap();
            let mut child = launch(
                &std::env::current_exe().unwrap(),
                &fixture_arguments(),
                &fixture.0,
                &fixture_environment("hang"),
                LaunchControl::new(active.unconfirmed_child.clone()),
            )
            .unwrap();
            wait_started(&fixture);
            let confirmation_calls = AtomicU8::new(0);
            let outcome = child.stop_with(|_, _| {
                confirmation_calls.fetch_add(1, Ordering::Relaxed);
                Err(containment("Synthetic shutdown confirmation deadline."))
            });
            assert!(matches!(outcome, Err(GitInitError::Containment(_))));
            assert!(child.cleanup_attempted);
            assert!(!child.stopped);
            // Stop requested termination before the deliberately failed proof.
            // The test independently waits only to release its own fixture safely;
            // that observation must not clear the production retirement latch.
            assert_eq!(
                unsafe { WaitForSingleObject(child.process.0, 1000) },
                WAIT_OBJECT_0
            );
            drop(child);
            assert_eq!(confirmation_calls.load(Ordering::Relaxed), 1);
            drop(active);
            assert_eq!(state.load(Ordering::Acquire), RETIRED);
            for _ in 0..4 {
                assert!(matches!(
                    Active::acquire(&state),
                    Err(GitInitError::Containment(_))
                ));
            }
            fixture.preserved();
        }

        #[test]
        fn production_plan_is_explicit_and_does_not_inherit_host_configuration() {
            assert!(ARGUMENTS.contains(&"--template="));
            assert!(ARGUMENTS.contains(&"--initial-branch=main"));
            let fixture = Fixture::new();
            let encoded = environment(&fixture.0).unwrap();
            let entries: Vec<_> = encoded
                .split(|unit| *unit == 0)
                .filter(|entry| !entry.is_empty())
                .map(|entry| String::from_utf16(entry).unwrap())
                .collect();
            assert!(entries.iter().any(|entry| entry == "GIT_CONFIG_GLOBAL=NUL"));
            assert!(entries.iter().any(|entry| entry == "GIT_CONFIG_SYSTEM=NUL"));
            assert!(entries.iter().any(|entry| entry == "GIT_TERMINAL_PROMPT=0"));
            assert!(!entries.iter().any(|entry| entry.starts_with("PATH=")
                || entry.starts_with("GIT_CONFIG_PARAMETERS=")
                || entry.starts_with("GIT_DIR=")));
            assert!(encoded.ends_with(&[0, 0]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Cancelled;
    impl Cancellation for Cancelled {
        fn is_cancelled(&self) -> bool {
            true
        }
    }

    #[test]
    fn wrappers_and_relative_program_paths_are_refused() {
        for path in ["git.exe", "git.cmd", "git.ps1"] {
            assert!(ContainedGit::new(PathBuf::from(path)).is_err());
        }
        assert!(ContainedGit::new(std::env::temp_dir().join("git.cmd")).is_err());
    }

    #[test]
    fn cancelled_request_never_inspects_or_starts_a_missing_program() {
        let initializer = ContainedGit::new(std::env::temp_dir().join("git.exe")).unwrap();
        assert!(matches!(
            initializer.initialize(Path::new("missing-staging"), &Cancelled),
            Err(GitInitError::Cancelled)
        ));
    }

    #[test]
    fn unconfirmed_cleanup_retires_the_slot_and_refuses_repeated_new_attempts() {
        let state = AtomicU8::new(FREE);
        let active = Active::acquire(&state).unwrap();
        // A deterministic cleanup backend reports a spawned child but cannot
        // confirm process+job completion. No kernel hang or subprocess is needed.
        active.unconfirmed_child.store(true, Ordering::Release);
        drop(active);
        assert_eq!(state.load(Ordering::Acquire), RETIRED);
        for _ in 0..4 {
            assert!(matches!(
                Active::acquire(&state),
                Err(GitInitError::Containment(_))
            ));
        }
        assert_eq!(state.load(Ordering::Acquire), RETIRED);
    }

    #[test]
    fn confirmed_cleanup_releases_a_slot_while_active_work_refuses_overlap() {
        let state = AtomicU8::new(FREE);
        let active = Active::acquire(&state).unwrap();
        active.unconfirmed_child.store(true, Ordering::Release);
        assert!(matches!(Active::acquire(&state), Err(GitInitError::Busy)));
        active.unconfirmed_child.store(false, Ordering::Release);
        drop(active);
        assert!(Active::acquire(&state).is_ok());
    }
}
