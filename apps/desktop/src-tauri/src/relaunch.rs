//! Restart from inside Shinatama: the exit the Model proxy panel offers when the proxy's
//! supervisor stops answering. A new process is started with `--after-exit=<this pid>`; before it
//! takes any lock it waits for this process to end, and if this one is still running after the
//! wait (the user kept the window open) it ends without starting: never two Shinatama at once.
use std::time::Duration;

const ARG: &str = "--after-exit=";
/// The successor started by the last Restart, while it may still be waiting.
#[cfg(windows)]
static WAITING: std::sync::Mutex<Option<std::process::Child>> = std::sync::Mutex::new(None);

/// The process a relaunched Shinatama waits for, read from its arguments.
pub(crate) fn after_exit_pid(args: impl IntoIterator<Item = String>) -> Option<u32> {
    args.into_iter()
        .find_map(|arg| {
            arg.strip_prefix(ARG)
                .and_then(|pid| pid.parse::<u32>().ok())
        })
        .filter(|pid| *pid > 0)
}

/// True once process `pid` has ended (or never existed); false when it is still running after
/// `timeout`, or when it cannot be watched.
#[cfg(windows)]
pub(crate) fn wait_for_exit(pid: u32, timeout: Duration) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, GetLastError, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        // No such process any more; any other refusal means it may still run.
        return unsafe { GetLastError() } == ERROR_INVALID_PARAMETER;
    }
    let milliseconds = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX - 1);
    let waited = unsafe { WaitForSingleObject(handle, milliseconds) };
    unsafe { CloseHandle(handle) };
    waited == WAIT_OBJECT_0
}
#[cfg(not(windows))]
pub(crate) fn wait_for_exit(_pid: u32, _timeout: Duration) -> bool {
    true
}

/// Starts the Shinatama that takes over once this one has ended.
#[cfg(windows)]
pub(crate) fn spawn_after_exit() -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    /// Outside any job this process belongs to, so it survives this process's end.
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let program = std::env::current_exe()?;
    let argument = format!("{ARG}{}", std::process::id());
    let spawn = |flags: u32| {
        std::process::Command::new(&program)
            .arg(&argument)
            .creation_flags(flags)
            .spawn()
    };
    let mut waiting = WAITING
        .lock()
        .map_err(|_| std::io::Error::other("the relaunch state is unavailable"))?;
    // One successor at a time: a second Restart while the first one still waits starts nothing.
    if let Some(successor) = waiting.as_mut()
        && matches!(successor.try_wait(), Ok(None))
    {
        return Ok(());
    }
    *waiting = Some(spawn(CREATE_BREAKAWAY_FROM_JOB).or_else(|_| spawn(0))?);
    Ok(())
}

/// The panel's Restart Shinatama: starts the next Shinatama, then the window closes as usual. A
/// refusal here comes before anything was closed.
#[cfg(windows)]
#[tauri::command(async)]
pub(crate) fn app_relaunch(window: tauri::WebviewWindow) -> Result<(), String> {
    crate::local(&window)?;
    spawn_after_exit()
        .map_err(|_| "Shinatama could not start its new window; nothing was closed.".to_owned())
}
#[cfg(not(windows))]
#[tauri::command(async)]
pub(crate) fn app_relaunch(window: tauri::WebviewWindow) -> Result<(), String> {
    crate::local(&window)?;
    Err("Restart is available on Windows only.".to_owned())
}
