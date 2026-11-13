//! Window-owned terminals. Windows requires explicit resource admission and runs
//! ConPTY only inside a contained helper. No UI or agent-store access.
#[cfg(not(windows))]
mod process_tree;
#[cfg(windows)]
pub mod protocol;
#[cfg(windows)]
pub mod windows_helper;
#[cfg(windows)]
mod windows_host;
#[cfg(windows)]
pub mod windows_pty;
#[cfg(windows)]
pub use windows_host::{
    TerminalDirective, TerminalResourceLimits, TerminalResourceObservation, TerminalResourcePhase,
    TerminalResourceQueues, TerminalResourceReason, TerminalResourceStatus, WindowsNativeAdmission,
    WindowsNativeSessionLifecycle, WindowsTerminalAdmission, WindowsTerminalConfig,
};
pub type TerminalResult<T> = anyhow::Result<T>;

#[cfg(not(windows))]
use anyhow::Context;
use anyhow::{Result, bail};
use portable_pty::PtySize;
#[cfg(not(windows))]
use portable_pty::{Child, CommandBuilder, MasterPty, native_pty_system};
use serde::Serialize;
use std::path::Path;
#[cfg(not(windows))]
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
};

const MAX_SESSIONS: usize = 8;
const CHUNK_BYTES: usize = 8192;
const OUTPUT_CHUNKS: usize = 64;
const INPUT_BYTES: usize = 65536;

// Exact names only; configuration/home/provider variables intentionally pass through.
const AGENT_MARKERS: &[&str] = &[
    "CLAUDECODE",
    "AI_AGENT",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_PID",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_BRIDGE_SESSION_ID",
    "CLAUDE_CODE_HOST_WORKTREE",
    "CLAUDE_CODE_HOST_WORKTREE_FENCE",
    "CLAUDE_CODE_SSE_PORT",
    "CODEX_THREAD_ID",
    "CODEX_INTERNAL_ORIGINATOR_OVERRIDE",
    "SHINATAMA_INSTANCE",
    "SHINATAMA_THREAD",
    "SHINATAMA_DB",
    "SHINATAMA_INSTANCE_TOKEN",
];

#[derive(Clone, Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub cwd: String,
    pub shell: String,
}
#[derive(Serialize)]
pub struct OutputBatch {
    pub chunks: Vec<Vec<u8>>,
    pub eof: bool,
    pub exit_code: Option<u32>,
    pub error: Option<String>,
    #[cfg(windows)]
    #[serde(skip)]
    _queue_credits: Vec<shinatama_engine::resources::admission::QueueCredit>,
}
#[cfg(not(windows))]
enum Output {
    Bytes(Vec<u8>),
    Error(String),
}
#[cfg(not(windows))]
struct Session {
    info: SessionInfo,
    // Drop the receiver before closing ConPTY: a reader blocked on a full queue
    // must be released before ClosePseudoConsole waits for output to drain.
    output: Mutex<Receiver<Output>>,
    input: SyncSender<Vec<u8>>,
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    tree: process_tree::ProcessTree,
}
#[cfg(not(windows))]
impl Drop for Session {
    fn drop(&mut self) {
        self.tree.terminate();
        if let Ok(child) = self.child.get_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
#[cfg(not(windows))]
#[derive(Default)]
pub struct TerminalHost {
    sessions: Mutex<BTreeMap<String, Arc<Session>>>,
}
#[cfg(windows)]
#[derive(Default)]
pub struct TerminalHost {
    windows: windows_host::WindowsTerminalHost,
}
#[cfg(windows)]
impl TerminalHost {
    pub fn configure_native_admission(
        &self,
        admission: std::sync::Arc<dyn WindowsNativeAdmission>,
    ) -> Result<()> {
        self.windows.configure_native_admission(admission)
    }
    /// Prepared plans and sealed permits are Rust-only and consumed per launch.
    pub fn start_prepared(
        &self,
        launch: shinatama_engine::launch::PreparedLaunch,
        permit: shinatama_engine::native_launch::NativeTuiPermit,
        rows: u16,
        cols: u16,
    ) -> Result<SessionInfo> {
        self.windows.start_prepared(launch, permit, rows, cols)
    }
    /// Optional trusted lifecycle binding. Initial callbacks run on the blocking
    /// owner worker; a failed finalization can be retried by the blocking Close
    /// caller, after native cleanup has already been confirmed.
    pub fn start_prepared_with_lifecycle(
        &self,
        launch: shinatama_engine::launch::PreparedLaunch,
        permit: shinatama_engine::native_launch::NativeTuiPermit,
        rows: u16,
        cols: u16,
        lifecycle: Option<std::sync::Arc<dyn WindowsNativeSessionLifecycle>>,
    ) -> Result<SessionInfo> {
        self.windows
            .start_prepared_with_lifecycle(launch, permit, rows, cols, lifecycle)
    }
    pub fn configure_windows(&self, config: WindowsTerminalConfig) -> Result<()> {
        self.windows.configure(config)
    }
    pub fn start(&self, cwd: &Path, rows: u16, cols: u16) -> Result<SessionInfo> {
        self.windows.start(cwd, rows, cols)
    }
    pub fn list(&self) -> Vec<SessionInfo> {
        self.windows.list()
    }
    pub fn info(&self, id: &str) -> Result<SessionInfo> {
        self.windows.info(id)
    }
    pub fn resources(&self) -> Vec<TerminalResourceStatus> {
        self.windows.resources()
    }
    pub fn fully_released(&self) -> Option<bool> {
        self.windows.fully_released()
    }
    pub fn live_native_sessions(&self) -> Option<usize> {
        self.windows.live_native_sessions()
    }
    pub fn write(&self, id: &str, data: Vec<u8>) -> Result<()> {
        self.windows.write(id, data)
    }
    pub fn resize(&self, id: &str, rows: u16, cols: u16) -> Result<()> {
        self.windows.resize(id, rows, cols)
    }
    pub fn read(&self, id: &str) -> Result<OutputBatch> {
        self.windows.read(id)
    }
    pub fn stop(&self, id: &str) -> Result<()> {
        self.windows.stop(id)
    }
    pub fn shutdown(&self) {
        self.windows.shutdown();
    }
}

pub fn validate_size(rows: u16, cols: u16) -> Result<PtySize> {
    if !(2..=500).contains(&rows) || !(2..=1000).contains(&cols) {
        bail!("Terminal size must be 2–500 rows and 2–1000 columns");
    }
    Ok(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })
}
#[cfg(not(windows))]
fn shell_command() -> (CommandBuilder, String) {
    #[cfg(windows)]
    {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        let shell = Path::new(&root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut command = CommandBuilder::new(shell);
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-NoExit",
            "-Command",
            "Import-Module PSReadLine; Set-PSReadLineOption -HistorySaveStyle SaveNothing",
        ]);
        (command, "PowerShell".into())
    }
    #[cfg(not(windows))]
    {
        let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
        let name = Path::new(&shell)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        (CommandBuilder::new(shell), name)
    }
}
#[cfg(not(windows))]
impl TerminalHost {
    pub fn start(&self, cwd: &Path, rows: u16, cols: u16) -> Result<SessionInfo> {
        let size = validate_size(rows, cols)?;
        let cwd = cwd
            .canonicalize()
            .context("Project folder does not exist")?;
        if !cwd.is_dir() {
            bail!("Choose a project folder");
        }
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("Terminal registry unavailable"))?;
        if sessions.len() >= MAX_SESSIONS {
            bail!("This prototype supports at most 8 terminals; stop one before opening another");
        }
        let pair = native_pty_system()
            .openpty(size)
            .context("Cannot open a pseudo-terminal")?;
        let mut reader = pair.master.try_clone_reader()?;
        let mut writer = pair.master.take_writer()?;
        let (mut command, shell) = shell_command();
        // PowerShell treats a verbatim Windows path as a provider-qualified path.
        // Keep canonical validation while passing an ordinary absolute path to it.
        let cwd_text = cwd.to_string_lossy();
        let launch_cwd = if cfg!(windows) {
            if let Some(unc) = cwd_text.strip_prefix("\\\\?\\UNC\\") {
                format!("\\\\{unc}")
            } else {
                cwd_text
                    .strip_prefix("\\\\?\\")
                    .unwrap_or(&cwd_text)
                    .to_string()
            }
        } else {
            cwd_text.to_string()
        };
        command.cwd(&launch_cwd);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        for marker in AGENT_MARKERS {
            command.env_remove(marker);
        }
        let mut child = pair
            .slave
            .spawn_command(command)
            .context("Cannot start the shell")?;
        drop(pair.slave);
        let tree = match process_tree::ProcessTree::attach(child.process_id()) {
            Ok(tree) => tree,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let (output_tx, output_rx) = mpsc::sync_channel(OUTPUT_CHUNKS);
        let (input_tx, input_rx) = mpsc::sync_channel::<Vec<u8>>(32);
        // Bounded queues backpressure noisy children instead of growing without limit.
        std::thread::spawn(move || {
            let mut buf = [0_u8; CHUNK_BYTES];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if output_tx.send(Output::Bytes(buf[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) => {
                        let _ = output_tx.send(Output::Error(error.to_string()));
                        break;
                    }
                }
            }
        });
        std::thread::spawn(move || {
            for data in input_rx {
                if writer
                    .write_all(&data)
                    .and_then(|_| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        let info = SessionInfo {
            id: uuid::Uuid::new_v4().to_string(),
            cwd: launch_cwd,
            shell,
        };
        sessions.insert(
            info.id.clone(),
            Arc::new(Session {
                info: info.clone(),
                master: Mutex::new(Some(pair.master)),
                input: input_tx,
                output: Mutex::new(output_rx),
                child: Mutex::new(child),
                tree,
            }),
        );
        Ok(info)
    }
    fn session(&self, id: &str) -> Result<Arc<Session>> {
        self.sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("Terminal registry unavailable"))?
            .get(id)
            .cloned()
            .context("Terminal no longer exists")
    }
    pub fn info(&self, id: &str) -> Result<SessionInfo> {
        self.session(id).map(|session| session.info.clone())
    }
    pub fn list(&self) -> Vec<SessionInfo> {
        self.sessions
            .lock()
            .map(|items| items.values().map(|s| s.info.clone()).collect())
            .unwrap_or_default()
    }
    pub fn write(&self, id: &str, data: Vec<u8>) -> Result<()> {
        if data.len() > INPUT_BYTES {
            bail!("Paste is too large (maximum 64 KiB per write)");
        }
        self.session(id)?
            .input
            .try_send(data)
            .context("Terminal input is busy or closed; retry the input")
    }
    pub fn resize(&self, id: &str, rows: u16, cols: u16) -> Result<()> {
        let size = validate_size(rows, cols)?;
        let session = self.session(id)?;
        let master = session
            .master
            .lock()
            .map_err(|_| anyhow::anyhow!("Terminal unavailable"))?;
        if let Some(master) = master.as_ref() {
            master.resize(size)?;
        }
        Ok(())
    }
    pub fn read(&self, id: &str) -> Result<OutputBatch> {
        let session = self.session(id)?;
        let receiver = session
            .output
            .lock()
            .map_err(|_| anyhow::anyhow!("Terminal output unavailable"))?;
        let mut batch = OutputBatch {
            chunks: Vec::new(),
            eof: false,
            exit_code: None,
            error: None,
        };
        // A per-poll budget gives every pane a turn; no lossy UTF-8 decoding.
        for _ in 0..8 {
            match receiver.try_recv() {
                Ok(Output::Bytes(bytes)) => batch.chunks.push(bytes),
                Ok(Output::Error(error)) => batch.error = Some(error),
                Err(TryRecvError::Disconnected) => {
                    batch.eof = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        batch.exit_code = session
            .child
            .lock()
            .map_err(|_| anyhow::anyhow!("Terminal process unavailable"))?
            .try_wait()?
            .map(|status| status.exit_code());
        if batch.exit_code.is_some() {
            // Windows keeps the output pipe open until its console is closed.
            // Closing can block while flushing output, so let polling keep draining.
            if let Some(master) = session
                .master
                .lock()
                .map_err(|_| anyhow::anyhow!("Terminal unavailable"))?
                .take()
            {
                std::thread::spawn(move || drop(master));
            }
        }
        Ok(batch)
    }
    pub fn stop(&self, id: &str) -> Result<()> {
        let session = self
            .sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("Terminal registry unavailable"))?
            .remove(id);
        drop(session); // Only explicit stop/shutdown, never a view operation.
        Ok(())
    }
    pub fn shutdown(&self) {
        let sessions = self
            .sessions
            .lock()
            .map(|mut sessions| std::mem::take(&mut *sessions));
        drop(sessions);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_zero_and_unbounded_sizes() {
        for (rows, cols) in [(0, 80), (24, 0), (501, 80), (24, 1001)] {
            assert!(validate_size(rows, cols).is_err());
        }
        assert!(validate_size(24, 80).is_ok());
    }
    #[test]
    fn missing_folder_never_creates_a_session() {
        let host = TerminalHost::default();
        assert!(
            host.start(Path::new("/nonexistent/shinatama-test-folder"), 24, 80)
                .is_err()
        );
        assert!(host.list().is_empty());
    }
    #[test]
    fn unknown_sessions_are_rejected() {
        let host = TerminalHost::default();
        assert!(host.write("missing", vec![13]).is_err());
        assert!(host.read("missing").is_err());
        assert!(host.resize("missing", 24, 80).is_err());
    }
}
