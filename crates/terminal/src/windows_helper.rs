//! Dedicated bounded helper. It must already belong to the admitted workload.
use crate::protocol::{self, HelperEvent, HostCommand, LaunchConfig, MAX_OUTPUT};
use crate::windows_pty::{Attributes, Handle, command_line, pipe, wide};
use anyhow::{Context, Result, bail};
use shinatama_engine::resources::{job::InheritedJobBinding, sampling::ProcessKey};
use std::{
    fs::File,
    io::{Read, Write},
    mem::{ManuallyDrop, size_of},
    os::windows::io::{BorrowedHandle, FromRawHandle},
    ptr::{null, null_mut},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, TrySendError},
    },
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::{
        Console::{COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, ResizePseudoConsole},
        Threading::{
            CREATE_SUSPENDED, CreateProcessW, EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION, ResumeThread,
            STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute,
            WaitForSingleObject,
        },
    },
};

type Sink = Arc<Mutex<std::io::Stdout>>;
type Console = Arc<Mutex<Option<HPCON>>>;
fn emit(sink: &Sink, event: &HelperEvent) -> Result<()> {
    protocol::write_event(
        &mut *sink
            .lock()
            .map_err(|_| anyhow::anyhow!("Helper output unavailable"))?,
        event,
    )?;
    Ok(())
}
fn input_watchdog() -> Result<Receiver<HostCommand>> {
    let (sender, receiver) = mpsc::sync_channel(32);
    std::thread::Builder::new()
        .name("terminal-helper-control".into())
        .spawn(move || {
            let mut input = std::io::stdin().lock();
            loop {
                match protocol::read_command(&mut input) {
                    Ok(Some(command)) => match sender.try_send(command) {
                        Ok(()) => {}
                        Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                            std::process::exit(2)
                        }
                    },
                    // Never wait for the ConPTY thread or output lock on owner loss.
                    // This also closes a temporary QUERY/ASSIGN job handle if native
                    // startup is stuck while the parent dies.
                    Ok(None) | Err(_) => std::process::exit(2),
                }
            }
        })
        .context("Create helper control worker")?;
    Ok(receiver)
}
struct Shell {
    process: Arc<Handle>,
    thread: Handle,
    key: ProcessKey,
}
impl Drop for Shell {
    fn drop(&mut self) {
        if unsafe { WaitForSingleObject(self.process.0, 0) } == WAIT_TIMEOUT {
            unsafe {
                TerminateProcess(self.process.0, 2);
            }
        }
    }
}
fn spawn_shell(config: &LaunchConfig, hpc: HPCON) -> Result<Shell> {
    // The temporary job handle is closed before ShellReady. Child inheritance is
    // FALSE; only the pseudoconsole connection is passed through the attribute.
    let binding = std::pin::pin!(
        InheritedJobBinding::open(config.owned).map_err(|error| anyhow::anyhow!("{error:?}"))?
    );
    let attributes = Attributes::new(2)?;
    unsafe { binding.as_ref().apply_to(attributes.pointer) }
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    if unsafe {
        UpdateProcThreadAttribute(
            attributes.pointer,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            hpc as *const std::ffi::c_void,
            size_of::<HPCON>(),
            null_mut(),
            null(),
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error())
            .context("Bind pseudoconsole before shell execution");
    }
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    // Explicit NULL stdhandles are replaced by the attached console. Leaving
    // STARTF_USESTDHANDLES unset leaked this helper's redirected RPC stdout into
    // PowerShell on the qualified Windows build (Microsoft terminal issue 4585).
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = null_mut();
    startup.StartupInfo.hStdOutput = null_mut();
    startup.StartupInfo.hStdError = null_mut();
    startup.lpAttributeList = attributes.pointer;
    let mut command = command_line(&config.executable, &config.args)?;
    let mut info = PROCESS_INFORMATION::default();
    if unsafe {
        CreateProcessW(
            wide(&config.executable).as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            0,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED,
            null(),
            wide(&config.cwd).as_ptr(),
            &startup.StartupInfo,
            &mut info,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error()).context("Create protected shell");
    }
    let mut shell = Shell {
        process: Arc::new(Handle(info.hProcess)),
        thread: Handle(info.hThread),
        key: ProcessKey {
            pid: info.dwProcessId,
            created_at_100ns: 1,
        },
    };
    shell.key = binding
        .verify_membership(unsafe { BorrowedHandle::borrow_raw(shell.process.0) })
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    Ok(shell)
}
fn open_console(config: &LaunchConfig) -> Result<(Console, File, File)> {
    let (input_read, input_write) = pipe()?;
    let (output_read, output_write) = pipe()?;
    let mut hpc = 0;
    let status = unsafe {
        CreatePseudoConsole(
            COORD {
                X: config.cols as i16,
                Y: config.rows as i16,
            },
            input_read.0,
            output_write.0,
            0,
            &mut hpc,
        )
    };
    if status < 0 || hpc == 0 {
        bail!("Cannot create contained pseudoconsole: HRESULT {status:#x}");
    }
    drop(input_read);
    drop(output_write);
    let input_write = ManuallyDrop::new(input_write);
    let output_read = ManuallyDrop::new(output_read);
    Ok((
        Arc::new(Mutex::new(Some(hpc))),
        unsafe { File::from_raw_handle(input_write.0) },
        unsafe { File::from_raw_handle(output_read.0) },
    ))
}
fn close_console(console: &Console) {
    let hpc = console.lock().ok().and_then(|mut console| console.take());
    if let Some(hpc) = hpc {
        unsafe {
            ClosePseudoConsole(hpc);
        }
    }
}
fn run_inner(sink: &Sink, commands: Receiver<HostCommand>) -> Result<()> {
    let config = match commands.recv_timeout(Duration::from_secs(10))? {
        HostCommand::Configure(config) => config,
        _ => bail!("Configure must be the first helper command"),
    };
    // Close bootstrap QUERY/ASSIGN capability BEFORE CreatePseudoConsole. The
    // independent EOF watchdog remains live throughout every native call.
    {
        let _binding = InheritedJobBinding::open(config.owned)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    }
    let (console, mut writer, mut reader) = open_console(&config)?;
    let (output_done, drained) = mpsc::sync_channel(1);
    let output_sink = Arc::clone(sink);
    std::thread::Builder::new()
        .name("terminal-helper-output".into())
        .spawn(move || {
            let mut bytes = [0_u8; MAX_OUTPUT];
            loop {
                match reader.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(count) => {
                        if emit(&output_sink, &HelperEvent::Output(bytes[..count].to_vec()))
                            .is_err()
                        {
                            std::process::exit(2);
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            let _ = output_done.try_send(());
        })
        .context("Create pseudoconsole output worker")?;
    emit(sink, &HelperEvent::ConsoleReady)?;
    if !matches!(
        commands.recv_timeout(Duration::from_secs(10))?,
        HostCommand::AllowShell
    ) {
        bail!("Shell launch was not authorized");
    }
    let hpc = console
        .lock()
        .map_err(|_| anyhow::anyhow!("Console unavailable"))?
        .ok_or_else(|| anyhow::anyhow!("Console closed"))?;
    let shell = spawn_shell(&config, hpc)?;
    emit(sink, &HelperEvent::ShellReady(shell.key))?;
    if !matches!(
        commands.recv_timeout(Duration::from_secs(10))?,
        HostCommand::ResumeShell
    ) {
        bail!("Shell resume was not authorized");
    }
    if unsafe { ResumeThread(shell.thread.0) } == u32::MAX {
        return Err(std::io::Error::last_os_error()).context("Resume verified shell");
    }
    let process = Arc::clone(&shell.process);
    let watcher_console = Arc::clone(&console);
    let watcher_sink = Arc::clone(sink);
    std::thread::Builder::new()
        .name("terminal-helper-exit".into())
        .spawn(move || {
            if unsafe { WaitForSingleObject(process.0, u32::MAX) } != WAIT_OBJECT_0 {
                std::process::exit(2);
            }
            let mut code = 0;
            if unsafe { GetExitCodeProcess(process.0, &mut code) } == 0 {
                std::process::exit(2);
            }
            if emit(&watcher_sink, &HelperEvent::ShellClosing(code)).is_err() {
                std::process::exit(2);
            }
            // Close can block while output drains. This is confined to the admitted
            // helper; the parent owns the finite deadline and can terminate its job.
            close_console(&watcher_console);
            if drained.recv_timeout(Duration::from_secs(2)).is_err() {
                std::process::exit(2);
            }
            let _ = emit(&watcher_sink, &HelperEvent::Exited(code));
            std::process::exit(0);
        })
        .context("Create shell exit worker")?;
    loop {
        match commands.recv()? {
            HostCommand::Input(bytes) => {
                writer.write_all(&bytes).context("Write shell input")?;
            }
            HostCommand::Resize { rows, cols } => {
                let console = console
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Console unavailable"))?;
                if let Some(hpc) = *console {
                    let status = unsafe {
                        ResizePseudoConsole(
                            hpc,
                            COORD {
                                X: cols as i16,
                                Y: rows as i16,
                            },
                        )
                    };
                    if status < 0 {
                        bail!("Resize pseudoconsole failed: HRESULT {status:#x}");
                    }
                }
            }
            HostCommand::Close => return Ok(()),
            _ => bail!("Unexpected helper command after launch"),
        }
    }
}
/// Dedicated executable entry only. Parent-side APIs must never call this.
pub fn run() -> Result<()> {
    let commands = input_watchdog()?;
    let sink = Arc::new(Mutex::new(std::io::stdout()));
    let result = run_inner(&sink, commands);
    if result.is_err() {
        let _ = emit(
            &sink,
            &HelperEvent::Error(
                "The protected terminal helper could not complete its operation.".into(),
            ),
        );
    }
    result
}
