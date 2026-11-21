//! Native bootstrap for the dedicated contained terminal helper, not the shell.
use anyhow::{Context, Result, bail};
use shinatama_engine::resources::{admission::PreparedWorkload, sampling::ProcessKey};
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    mem::size_of,
    os::windows::{
        ffi::OsStrExt,
        io::{BorrowedHandle, FromRawHandle},
    },
    path::Path,
    ptr::{null, null_mut},
    sync::Arc,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    System::{
        Memory::{GetProcessHeap, HeapAlloc, HeapFree},
        Pipes::CreatePipe,
        Threading::{
            BELOW_NORMAL_PRIORITY_CLASS, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
            CreateProcessW, DETACHED_PROCESS, DeleteProcThreadAttributeList,
            EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, InitializeProcThreadAttributeList,
            LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION,
            ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess,
            UpdateProcThreadAttribute, WaitForSingleObject,
        },
    },
};

pub(crate) struct Handle(pub(crate) HANDLE);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub(crate) struct Attributes {
    pub(crate) pointer: LPPROC_THREAD_ATTRIBUTE_LIST,
    heap: HANDLE,
}
impl Attributes {
    pub(crate) fn new(count: u32) -> Result<Self> {
        let mut size = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), count, 0, &mut size);
        }
        if !(1..=16384).contains(&size) {
            bail!("Invalid native startup allocation");
        }
        let heap = unsafe { GetProcessHeap() };
        let pointer = unsafe { HeapAlloc(heap, 0, size) };
        if pointer.is_null() {
            bail!("Cannot allocate native startup attributes");
        }
        if unsafe { InitializeProcThreadAttributeList(pointer, count, 0, &mut size) } == 0 {
            let error = std::io::Error::last_os_error();
            unsafe {
                HeapFree(heap, 0, pointer);
            }
            return Err(error).context("Initialize native startup attributes");
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
pub(crate) fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}
pub(crate) fn command_line(executable: &Path, args: &[OsString]) -> Result<Vec<u16>> {
    if !executable.is_absolute() || args.len() > crate::protocol::MAX_LAUNCH_ARGUMENTS {
        bail!("Invalid native executable arguments");
    }
    let mut result = Vec::new();
    for argument in
        std::iter::once(executable.as_os_str()).chain(args.iter().map(OsString::as_os_str))
    {
        if !result.is_empty() {
            result.push(b' ' as u16);
        }
        result.push(b'"' as u16);
        let mut slashes = 0;
        for unit in argument.encode_wide() {
            if unit == 0 {
                bail!("NUL in native process argument");
            }
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
            if result.len() > 32766 {
                bail!("Native command line is too long");
            }
        }
        result.extend(std::iter::repeat_n(b'\\' as u16, 2 * slashes));
        result.push(b'"' as u16);
        if result.len() > 32766 {
            bail!("Native command line is too long");
        }
    }
    result.push(0);
    Ok(result)
}
pub(crate) fn pipe() -> Result<(Handle, Handle)> {
    let (mut read, mut write) = (null_mut(), null_mut());
    if unsafe { CreatePipe(&mut read, &mut write, null(), 4096) } == 0 {
        return Err(std::io::Error::last_os_error()).context("Create terminal pipe");
    }
    Ok((Handle(read), Handle(write)))
}
fn environment(values: &[(OsString, OsString)]) -> Result<Vec<u16>> {
    let mut values = values.to_vec();
    values.sort_by_key(|(name, _)| name.to_string_lossy().to_uppercase());
    let mut result = Vec::new();
    for (name, value) in values {
        let name: Vec<_> = name.encode_wide().take(65536).collect();
        let value: Vec<_> = value.encode_wide().take(65536).collect();
        if name.is_empty()
            || name.contains(&0)
            || name.contains(&(b'=' as u16))
            || value.contains(&0)
        {
            bail!("Invalid terminal environment");
        }
        if result.len() + name.len() + value.len() + 2 > 65535 {
            bail!("Terminal environment is too large");
        }
        result.extend(name);
        result.push(b'=' as u16);
        result.extend(value);
        result.push(0);
    }
    if result.is_empty() {
        result.push(0);
    }
    result.push(0);
    Ok(result)
}
pub struct HelperProcess {
    handle: Handle,
    key: ProcessKey,
}
impl HelperProcess {
    pub fn key(&self) -> ProcessKey {
        self.key
    }
    pub fn try_exit_code(&self) -> Result<Option<u32>> {
        match unsafe { WaitForSingleObject(self.handle.0, 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                if unsafe { GetExitCodeProcess(self.handle.0, &mut code) } == 0 {
                    return Err(std::io::Error::last_os_error()).context("Read helper exit");
                }
                Ok(Some(code))
            }
            _ => Err(std::io::Error::last_os_error()).context("Query helper process"),
        }
    }
    /// Bounded worker-only wait. Never call while holding the terminal registry.
    pub fn wait(&self, timeout: Duration) -> Result<bool> {
        if timeout > Duration::from_secs(3) {
            bail!("Invalid helper wait");
        }
        match unsafe { WaitForSingleObject(self.handle.0, timeout.as_millis() as u32) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(std::io::Error::last_os_error()).context("Wait for helper"),
        }
    }
}
impl Drop for HelperProcess {
    fn drop(&mut self) {
        if matches!(self.try_exit_code(), Ok(None)) {
            unsafe {
                TerminateProcess(self.handle.0, 2);
            }
        }
    }
}
pub struct SuspendedHelper {
    process: Arc<HelperProcess>,
    thread: Handle,
    input: File,
    output: File,
}
pub struct RunningHelper {
    pub process: Arc<HelperProcess>,
    pub input: File,
    pub output: File,
}
impl SuspendedHelper {
    pub fn process_handle(&self) -> BorrowedHandle<'_> {
        unsafe { BorrowedHandle::borrow_raw(self.process.handle.0) }
    }
    pub fn process_key(&self) -> ProcessKey {
        self.process.key
    }
    pub fn resume(self) -> Result<RunningHelper> {
        if unsafe { ResumeThread(self.thread.0) } == u32::MAX {
            return Err(std::io::Error::last_os_error()).context("Resume contained helper");
        }
        let Self {
            process,
            thread,
            input,
            output,
        } = self;
        drop(thread);
        Ok(RunningHelper {
            process,
            input,
            output,
        })
    }
}
pub fn spawn_suspended(
    executable: &Path,
    cwd: &Path,
    variables: &[(OsString, OsString)],
    workload: &PreparedWorkload,
    now_ms: i64,
) -> Result<SuspendedHelper> {
    // This binding precedes its attribute list and all pointed-to handle storage
    // outlives DeleteProcThreadAttributeList on every return path.
    let binding = std::pin::pin!(
        workload
            .startup_binding(now_ms)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?
    );
    let (child_input, input) = pipe()?;
    let (output, child_output) = pipe()?;
    let nul = File::options().read(true).write(true).open("NUL")?;
    use std::os::windows::io::AsRawHandle;
    let inherited = [child_input.0, child_output.0, nul.as_raw_handle()];
    for handle in inherited {
        if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) } == 0 {
            return Err(std::io::Error::last_os_error())
                .context("Configure helper handle inheritance");
        }
    }
    let attributes = Attributes::new(2)?;
    if unsafe {
        UpdateProcThreadAttribute(
            attributes.pointer,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            inherited.as_ptr().cast(),
            size_of::<[HANDLE; 3]>(),
            null_mut(),
            null(),
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error()).context("Set helper HANDLE_LIST");
    }
    unsafe { binding.as_ref().apply_to(attributes.pointer) }
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = child_input.0;
    startup.StartupInfo.hStdOutput = child_output.0;
    startup.StartupInfo.hStdError = nul.as_raw_handle();
    startup.lpAttributeList = attributes.pointer;
    let mut command = command_line(executable, &[OsString::from("--ipc")])?;
    let mut info = PROCESS_INFORMATION::default();
    let environment = environment(variables)?;
    if unsafe {
        CreateProcessW(
            wide(executable).as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            1,
            // The control window comes first: a busy agent waits for the CPU, the
            // window never waits for an agent. The shell and every process it starts
            // inherit the class, unless one asks for another.
            EXTENDED_STARTUPINFO_PRESENT
                | CREATE_SUSPENDED
                | CREATE_UNICODE_ENVIRONMENT
                | DETACHED_PROCESS
                | BELOW_NORMAL_PRIORITY_CLASS,
            environment.as_ptr().cast(),
            wide(cwd).as_ptr(),
            &startup.StartupInfo,
            &mut info,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error()).context("Create contained terminal helper");
    }
    let handle = Handle(info.hProcess);
    let thread = Handle(info.hThread);
    // A suspended child is still killed if verification/activation fails.
    let mut process = HelperProcess {
        handle,
        key: ProcessKey {
            pid: info.dwProcessId,
            created_at_100ns: 1,
        },
    };
    process.key = binding
        .verify_membership(unsafe { BorrowedHandle::borrow_raw(process.handle.0) })
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    drop(child_input);
    drop(child_output);
    use std::mem::ManuallyDrop;
    let input = ManuallyDrop::new(input);
    let output = ManuallyDrop::new(output);
    Ok(SuspendedHelper {
        process: Arc::new(process),
        thread,
        input: unsafe { File::from_raw_handle(input.0) },
        output: unsafe { File::from_raw_handle(output.0) },
    })
}
