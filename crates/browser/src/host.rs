//! The window's `shinatama-moli.exe`: started inside a kill-on-close Job
//! Object with a random pipe name and token on stdin, reached only through
//! that pipe. Dropping the host closes stdin and the job, so the process tree
//! ends with its owner, crash included.

use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

use crate::pages::hex;

pub const READY_TIMEOUT: Duration = Duration::from_secs(20);
pub const HOST_EXE: &str = "shinatama-moli.exe";

#[derive(Debug)]
pub enum HostError {
    Entropy,
    Spawn(std::io::Error),
    Job(std::io::Error),
    NotReady(String),
    Connect(std::io::Error),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Entropy => f.write_str("no randomness available for the browser pipe"),
            Self::Spawn(e) => write!(f, "the browser did not start: {e}"),
            Self::Job(e) => write!(f, "the browser could not be contained: {e}"),
            Self::NotReady(why) => write!(f, "the browser did not get ready: {why}"),
            Self::Connect(e) => write!(f, "the browser pipe refused the connection: {e}"),
        }
    }
}

pub struct Host {
    pipe: String,
    token: [u8; 32],
    child: Child,
    // Kept open: closing it asks the host to shut down.
    stdin: Option<ChildStdin>,
    job: job::Job,
}

impl Host {
    /// Starts the host and waits for its ready line. `args` are `moli serve`
    /// options (never host or port).
    pub fn start(exe: &Path, args: &[&str]) -> Result<Self, HostError> {
        let mut name = [0u8; 16];
        let mut token = [0u8; 32];
        getrandom::fill(&mut name).map_err(|_| HostError::Entropy)?;
        getrandom::fill(&mut token).map_err(|_| HostError::Entropy)?;
        let pipe = format!(r"\\.\pipe\shinatama-moli-{}", hex(&name));

        let job = job::Job::new().map_err(HostError::Job)?;
        let mut command = Command::new(exe);
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn().map_err(HostError::Spawn)?;
        // The host reads its launch line before it does anything, so nothing
        // runs outside the job.
        if let Err(error) = job.assign(&child) {
            let _ = child.kill();
            return Err(HostError::Job(error));
        }
        let mut stdin = child.stdin.take();
        let launch = serde_json::json!({ "pipe": pipe, "token": hex(&token), "args": args });
        let sent = stdin
            .as_mut()
            .map(|input| writeln!(input, "{launch}").and_then(|()| input.flush()))
            .unwrap_or_else(|| Err(std::io::ErrorKind::BrokenPipe.into()));
        if let Err(error) = sent {
            let _ = child.kill();
            return Err(HostError::NotReady(error.to_string()));
        }
        let stdout = child.stdout.take();
        let (ready, wait) = mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let result = match stdout {
                Some(out) => BufReader::new(out).read_line(&mut line).map(|_| line),
                None => Err(std::io::ErrorKind::BrokenPipe.into()),
            };
            let _ = ready.send(result);
        });
        match wait.recv_timeout(READY_TIMEOUT) {
            Ok(Ok(line)) if line.trim() == r#"{"ready":true}"# => {}
            Ok(Ok(line)) => {
                let _ = child.kill();
                return Err(HostError::NotReady(if line.is_empty() { "it exited".into() } else { line }));
            }
            Ok(Err(error)) => {
                let _ = child.kill();
                return Err(HostError::NotReady(error.to_string()));
            }
            Err(_) => {
                let _ = child.kill();
                return Err(HostError::NotReady("timed out".into()));
            }
        }
        Ok(Self { pipe, token, child, stdin, job })
    }

    pub fn token(&self) -> &[u8; 32] {
        &self.token
    }

    /// A fresh connection; the caller writes the token first (the CDP client
    /// does).
    pub async fn connect(&self) -> Result<NamedPipeClient, HostError> {
        let mut last = None;
        for _ in 0..50 {
            match ClientOptions::new().open(&self.pipe) {
                Ok(client) => return Ok(client),
                Err(error) => {
                    last = Some(error);
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
        }
        Err(HostError::Connect(last.unwrap_or_else(|| std::io::ErrorKind::TimedOut.into())))
    }

    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        drop(self.stdin.take());
        // Moli's own shutdown budget is 5 s (storage flush); then the job ends it.
        // Blocking: drop a Host off the async runtime (see the desktop's sweeper).
        for _ in 0..120 {
            if !matches!(self.child.try_wait(), Ok(None)) {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        self.job.terminate();
        let _ = self.child.wait();
    }
}

pub mod job {
    use std::{io, os::windows::io::AsRawHandle, process::Child};

    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject, TerminateJobObject,
        },
    };

    /// A Job Object whose processes die when its last handle closes.
    pub struct Job(HANDLE);
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    impl Job {
        pub fn new() -> io::Result<Self> {
            // SAFETY: anonymous job, default security; the handle is owned by Self.
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = Self(handle);
            // SAFETY: zeroed is a valid JOBOBJECT_EXTENDED_LIMIT_INFORMATION.
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            // SAFETY: valid job handle and a correctly sized structure.
            let ok = unsafe {
                SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(job)
        }

        pub fn assign(&self, child: &Child) -> io::Result<()> {
            // SAFETY: both handles are valid for the duration of the call.
            if unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle() as HANDLE) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        pub fn terminate(&self) {
            // SAFETY: valid job handle.
            unsafe { TerminateJobObject(self.0, 1) };
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: owned handle, closed once.
            unsafe { CloseHandle(self.0) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn alive(pid: u32) -> bool {
        let out = Command::new("tasklist").args(["/FI", &format!("PID eq {pid}"), "/NH"]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).contains(&pid.to_string())
    }

    fn wait_dead(pid: u32) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if !alive(pid) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }

    #[test]
    fn closing_the_job_kills_its_process() {
        let job = job::Job::new().unwrap();
        let mut child = Command::new("ping").args(["-n", "60", "127.0.0.1"]).stdout(Stdio::null()).spawn().unwrap();
        job.assign(&child).unwrap();
        let pid = child.id();
        assert!(alive(pid));
        drop(job);
        assert!(wait_dead(pid), "the job's process outlived its last handle");
        let _ = child.wait();
    }

    #[test]
    fn a_host_that_never_says_ready_is_refused_and_killed() {
        // `cmd /c pause` reads stdin and prints a prompt, never the ready line.
        let error = Host::start(Path::new(r"C:\Windows\System32\cmd.exe"), &[]).err().unwrap();
        assert!(matches!(error, HostError::NotReady(_)), "{error}");
    }

    #[test]
    fn pipe_names_and_tokens_are_fresh_hex() {
        assert_eq!(hex(&[0, 255, 16]), "00ff10");
    }
}
