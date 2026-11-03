//! Private proxy setup and one retained owner. UI commands supply no program,
//! environment, port, configuration payload or resource budget.
#![cfg(windows)]
use crate::{storage::Services, terminal_runtime::Runtime};
use serde::Serialize;
use shinatama_engine::{
    sidecar_config::prepare_private_sidecar,
    sidecar_runtime::{
        NodePin, RuntimeSummary, prepare_runtime, verify_bundled_node, verify_prepared_bundle,
    },
    sidecar_supervisor::{Command, Phase, Reason, SidecarSupervisor, Snapshot, SupervisorConfig},
};
use shinatama_state::{database::DatabaseAccess, location::StorageLocation};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    net::TcpListener,
    num::NonZeroU16,
    os::windows::fs::OpenOptionsExt,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub phase: &'static str,
    pub healthy: bool,
    pub message: Option<&'static str>,
    pub observation: Option<Snapshot>,
    pub prepared_identity: Option<RuntimeSummary>,
}
pub struct Controller {
    services: Arc<Services>,
    runtime: Arc<Runtime>,
    preparing: AtomicBool,
    stop_requested: AtomicBool,
    supervisor: OnceLock<SidecarSupervisor>,
    identity: OnceLock<RuntimeSummary>,
    connection: OnceLock<(NonZeroU16, StorageLocation)>,
    failure: Mutex<Option<&'static str>>,
}
impl Controller {
    pub fn new(services: Arc<Services>, runtime: Arc<Runtime>) -> Self {
        Self {
            services,
            runtime,
            preparing: AtomicBool::new(false),
            stop_requested: AtomicBool::new(false),
            supervisor: OnceLock::new(),
            identity: OnceLock::new(),
            connection: OnceLock::new(),
            failure: Mutex::new(None),
        }
    }
    pub fn status(&self) -> Status {
        let observation = self.supervisor.get().and_then(SidecarSupervisor::status);
        let fresh = observation.as_ref().is_some_and(|snapshot| {
            snapshot
                .observed_at_unix_ms
                .zip(now_ms())
                .is_some_and(|(at, now)| now.checked_sub(at).is_some_and(|age| age <= 3000))
        });
        let phase = if self.preparing.load(Ordering::Acquire) {
            "preparing"
        } else if let Some(snapshot) = &observation {
            if !fresh
                && !matches!(
                    snapshot.phase,
                    Phase::Stopped | Phase::Failed | Phase::Retired
                )
            {
                "unavailable"
            } else {
                match snapshot.phase {
                    Phase::Stopped => "stopped",
                    Phase::Starting => "starting",
                    Phase::Ready => "ready",
                    Phase::Busy => "busy",
                    Phase::Degraded => "degraded",
                    Phase::Recovering => "recovering",
                    Phase::Failed => "failed",
                    Phase::Retired => "retired",
                }
            }
        } else if self
            .failure
            .try_lock()
            .ok()
            .is_some_and(|value| value.is_some())
        {
            "failed"
        } else {
            "stopped"
        };
        let message = self.failure.try_lock().ok().and_then(|value| *value).or_else(|| observation.as_ref().and_then(|snapshot| match snapshot.reason {
            Reason::AdmissionDeferred => Some("Resources or a free process slot are unavailable. Wait or close unused terminals, then retry Start. Existing sessions keep their limits."),
            Reason::StartupRefused => Some("The local service could not confirm startup. Existing files were preserved; retry explicitly."),
            Reason::RepeatedExit => Some("The local service exited twice. Automatic recovery has stopped; saved sessions were preserved."),
            Reason::CleanupUncertain => Some("Cleanup is not confirmed. Its reservation remains held; retry cleanup before another start."),
            Reason::ResourceLimit => Some("The local service exceeded its own resource budget and was stopped. Saved sessions were preserved."),
            _ => None,
        }));
        Status {
            phase,
            healthy: fresh
                && observation
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.healthy),
            message,
            observation,
            prepared_identity: self.identity.get().cloned(),
        }
    }
    pub fn start(self: &Arc<Self>) -> Result<Status, &'static str> {
        if let Some(supervisor) = self.supervisor.get() {
            self.stop_requested.store(false, Ordering::Release);
            let command = if supervisor
                .status()
                .is_some_and(|snapshot| snapshot.phase == Phase::Failed)
            {
                Command::Retry
            } else {
                Command::Start
            };
            supervisor
                .request(command)
                .map_err(|_| "The proxy owner is busy; retry explicitly.")?;
            return Ok(self.status());
        }
        if self
            .preparing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(self.status());
        }
        self.stop_requested.store(false, Ordering::Release);
        let owner = Arc::clone(self);
        tauri::async_runtime::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| owner.prepare()));
            let failure = match result {
                Ok(Ok(())) => None,
                Ok(Err(message)) => Some(message),
                Err(_) => Some("Proxy preparation failed. Saved data was preserved."),
            };
            if let Ok(mut value) = owner.failure.lock() {
                *value = failure;
            }
            owner.preparing.store(false, Ordering::Release);
        });
        Ok(self.status())
    }
    pub fn stop(&self) -> Result<Status, &'static str> {
        self.stop_requested.store(true, Ordering::Release);
        if let Some(supervisor) = self.supervisor.get() {
            supervisor
                .request(Command::Stop)
                .map_err(|_| "Proxy stop could not be delivered.")?;
        }
        Ok(self.status())
    }
    pub fn retry(&self) -> Result<Status, &'static str> {
        self.stop_requested.store(false, Ordering::Release);
        self.supervisor
            .get()
            .ok_or("Prepare the proxy before retrying recovery.")?
            .request(Command::Retry)
            .map_err(|_| "The proxy owner is busy; no replacement was created.")?;
        Ok(self.status())
    }
    pub fn is_active(&self) -> bool {
        self.preparing.load(Ordering::Acquire)
            || self.supervisor.get().is_some_and(|owner| {
                owner.status().is_none_or(|snapshot| {
                    !matches!(snapshot.phase, Phase::Stopped | Phase::Failed)
                })
            })
    }
    /// Trusted launch workers only: no frontend port or certificate selector.
    /// The engine repeats the live owner check before issuing a launch permit.
    pub fn qualified_owner(
        &self,
    ) -> Result<(&SidecarSupervisor, NonZeroU16, PathBuf), &'static str> {
        let status = self.status();
        if status.phase != "ready" || !status.healthy {
            return Err("Start the session index and wait until it is ready.");
        }
        let (port, root) = self
            .connection
            .get()
            .ok_or("The session index connection is unavailable.")?;
        let ca = root
            .resolve_related_directory(&root.directory().join("data/ca"))
            .map_err(|_| "The session index certificate directory is unavailable.")?;
        ca.verify_private_directory()
            .map_err(|_| "The session index certificate directory could not be verified.")?;
        let path = ca.directory().join("combined-ca.pem");
        // This is the public CA bundle created by our contained proxy, never a
        // system-store installation or a read of the private CA key.
        ca.secure_private_file(&path)
            .map_err(|_| "The session index public certificate bundle is unavailable.")?;
        Ok((
            self.supervisor
                .get()
                .ok_or("The session index owner is unavailable.")?,
            *port,
            path,
        ))
    }
    /// Called only on the existing bounded close worker. A timeout preserves the
    /// retained cleanup owner and reservation; it never starts another process.
    pub fn stop_and_wait(&self, timeout: Duration) {
        let _ = self.stop();
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if !self.preparing.load(Ordering::Acquire)
                && self.supervisor.get().is_none_or(|owner| {
                    owner.status().is_some_and(|snapshot| {
                        matches!(
                            snapshot.phase,
                            Phase::Stopped | Phase::Failed | Phase::Retired
                        )
                    })
                })
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    fn prepare(&self) -> Result<(), &'static str> {
        let state = self
            .services
            .load()
            .map_err(|_| "Private storage is unavailable.")?;
        if !matches!(state.store.access(), DatabaseAccess::ReadWrite) {
            return Err("Update Shinatama before preparing the proxy in read-only storage.");
        }
        let lock = owner_lock(&state.location)?;
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .map_err(|_| "No local proxy address is available.")?;
        let port = NonZeroU16::new(
            listener
                .local_addr()
                .map_err(|_| "The local proxy address is unavailable.")?
                .port(),
        )
        .ok_or("The local proxy address is invalid.")?;
        let private = prepare_private_sidecar(&state.location, port).map_err(
            |_| "Private proxy configuration could not be verified; existing files were preserved.",
        )?;
        let resources = resources_root()?;
        let bundle = verify_prepared_bundle(&resources.join("source-build/prepared")).map_err(|_| "The proxy package is missing or its fingerprint differs. Prepare the pinned package before opening an agent.")?;
        let pin: serde_json::Value =
            serde_json::from_str(include_str!("../../../../sidecar/runtime.lock.json"))
                .map_err(|_| "The bundled runtime inventory is invalid.")?;
        let node = verify_bundled_node(
            &resources.join("runtime"),
            NodePin {
                relative_path: PathBuf::from(
                    pin.get("relativePath")
                        .and_then(|value| value.as_str())
                        .ok_or("The runtime inventory is incomplete.")?,
                ),
                sha256: pin
                    .get("sha256")
                    .and_then(|value| value.as_str())
                    .ok_or("The runtime fingerprint is unavailable.")?
                    .to_owned(),
                version: pin
                    .get("fileVersion")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned),
                modules_abi: pin
                    .get("modulesAbi")
                    .and_then(|value| value.as_u64())
                    .and_then(|value| u32::try_from(value).ok()),
            },
        )
        .map_err(|_| "The bundled runtime is missing or its fingerprint differs.")?;
        let mut os = BTreeMap::new();
        for name in ["SystemRoot", "WINDIR"] {
            if let Some(value) = std::env::var_os(name) {
                os.insert(name.into(), value);
            }
        }
        let recipe = prepare_runtime(bundle, node, private.root(), &os, port)
            .map_err(|_| "The private proxy launch environment could not be verified.")?;
        let identity = recipe.summary();
        let provider = self
            .runtime
            .sidecar_provider(lock, state.location.clone())
            .map_err(|_| "Proxy resource admission is unavailable.")?;
        self.runtime.wait_for_admission().map_err(|_| "Resources are not yet ready for the local service. Wait and retry Start; no process was created.")?;
        // A competing bind is handled by exact child/health identity checks and
        // finite startup failure, never by attaching to or stopping that listener.
        drop(listener);
        if self.stop_requested.load(Ordering::Acquire) {
            return Err("Proxy preparation was cancelled; saved files were preserved.");
        }
        let supervisor = SidecarSupervisor::start(
            recipe,
            SupervisorConfig {
                port,
                poll_period: Duration::from_millis(100),
                health_period: Duration::from_millis(500),
                health_timeout: Duration::from_millis(200),
                startup_timeout: Duration::from_secs(20),
                graceful_stop_timeout: Duration::from_secs(2),
                cleanup_timeout: Duration::from_secs(3),
                reservation_lease: Duration::from_secs(300),
                output_bytes_per_stream: 64 * 1024 * 1024,
            },
            provider,
        )
        .map_err(
            |_| "A proxy owner is already retained or could not start; no duplicate was created.",
        )?;
        self.supervisor
            .set(supervisor)
            .map_err(|_| "A proxy owner is already retained.")?;
        let _ = self.connection.set((port, private.root().clone()));
        self.supervisor
            .get()
            .ok_or("The proxy owner is unavailable.")?
            .request(if self.stop_requested.load(Ordering::Acquire) {
                Command::Stop
            } else {
                Command::Start
            })
            .map_err(|_| "Proxy start could not be delivered.")?;
        let _ = self.identity.set(identity);
        Ok(())
    }
}
fn now_ms() -> Option<i64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| i64::try_from(value.as_millis()).ok())
}
fn owner_lock(location: &StorageLocation) -> Result<File, &'static str> {
    let path = location.directory().join("sidecar.owner.lock");
    let options = |new| {
        let mut value = OpenOptions::new();
        value
            .read(true)
            .write(true)
            .create_new(new)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
        value
    };
    let file = match options(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => options(false)
            .open(&path)
            .map_err(|_| "The proxy owner lock is unavailable.")?,
        Err(_) => return Err("The proxy owner lock could not be created."),
    };
    location
        .secure_private_file(&path)
        .map_err(|_| "The proxy owner lock failed private-storage verification.")?;
    file.try_lock()
        .map_err(|_| "Another Shinatama window owns this proxy. Its process was preserved.")?;
    Ok(file)
}
fn resources_root() -> Result<PathBuf, &'static str> {
    let executable = std::env::current_exe()
        .map_err(|_| "The application resource directory is unavailable.")?;
    let packaged = executable
        .parent()
        .ok_or("The application resource directory is unavailable.")?
        .join("resources/sidecar");
    if packaged
        .try_exists()
        .map_err(|_| "The application resources are unavailable.")?
    {
        return Ok(packaged);
    }
    #[cfg(debug_assertions)]
    {
        return PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .map(|root| root.join("sidecar"))
            .ok_or("The development resource directory is unavailable.");
    }
    #[cfg(not(debug_assertions))]
    Err("The proxy resources are missing from this application package.")
}
