#![cfg(windows)]
#![allow(dead_code)] // Each separately compiled ignored proof uses a different API subset.
//! Shared owned process support for explicitly scheduled synthetic Claude proofs.
//! No native work occurs unless an ignored proof calls start_sidecar/start_claude.
use serde_json::{Value, json};
use shinatama_engine::{
    claude_probe::{PreparedClaudeProbe, discover_claude},
    resources::{
        admission::{AdmissionCoordinator, CleanupStatus, PrepareRequest, QueueCredit},
        process::{NativeCommand, SpawnedProcess, spawn_contained},
        sampling::{Measurement, WindowsSampler},
    },
    sidecar_config::prepare_private_sidecar,
    sidecar_runtime::{
        NodePin, SidecarRuntimeRecipe, prepare_runtime, verify_bundled_node, verify_prepared_bundle,
    },
};
use shinatama_state::{
    database::Database,
    location::{LocationRequest, StorageLocation},
    resource_reservations::{Budget, PolicyStamp, ReservationPolicy},
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::TcpListener,
    num::NonZeroU16,
    os::windows::io::AsRawHandle,
    path::{Path, PathBuf},
    ptr::null_mut,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{Foundation::ERROR_BROKEN_PIPE, System::Pipes::PeekNamedPipe};

pub type Result<T> = std::result::Result<T, &'static str>;
const MIB: u64 = 1024 * 1024;
const CAP: usize = 64 * 1024;
const DUMMY_KEY: &str = "shinatama-synthetic-no-provider-key";
// Read-only PE metadata and the byte-identical installer version file identify
// this exact installation. The ignored proof still pins the executable at run time.
const CLAUDE_VERSION: &str = "2.1.318";
const CLAUDE_SHA: &str = "b9aa5b08dc2cc46ea77d334dce792d1b8fdbb3a43bb290efdebdc905b1181082";
const NODE_SHA: &str = "58e74bf02fc5bbacc41dcb8bef089961cd5bddd37830b87784e4fc624d145d1f";
pub fn checked<T, E>(value: std::result::Result<T, E>, code: &'static str) -> Result<T> {
    value.map_err(|_| code)
}
pub fn require(value: bool, code: &'static str) -> Result<()> {
    if value { Ok(()) } else { Err(code) }
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
pub fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .into()
}
#[derive(Clone, Copy)]
enum Role {
    Fixture,
    Sidecar,
    Claude,
}
impl Role {
    fn label(self) -> &'static str {
        match self {
            Self::Fixture => "synthetic-endpoints",
            Self::Sidecar => "canonical-sidecar",
            Self::Claude => "claude",
        }
    }
    fn budget(self) -> Budget {
        Budget {
            memory_bytes: if matches!(self, Self::Fixture) {
                256 * MIB
            } else {
                1024 * MIB
            },
            processes: if matches!(self, Self::Fixture) { 4 } else { 8 },
            cpu_basis_points: if matches!(self, Self::Sidecar) {
                2000
            } else {
                1000
            },
            queue_bytes: 256 * 1024,
            queue_items: 4,
        }
    }
    fn process_commit(self) -> u64 {
        if matches!(self, Self::Fixture) {
            256 * MIB
        } else {
            768 * MIB
        }
    }
}
fn aggregate_budget() -> Budget {
    Budget {
        memory_bytes: 2304 * MIB,
        processes: 20,
        cpu_basis_points: 4000,
        queue_bytes: 3 * 256 * 1024,
        queue_items: 12,
    }
}
fn drain(pipe: &mut File, bytes: &mut Vec<u8>) -> Result<()> {
    for _ in 0..8 {
        let mut available = 0;
        if unsafe {
            PeekNamedPipe(
                pipe.as_raw_handle(),
                null_mut(),
                0,
                null_mut(),
                &mut available,
                null_mut(),
            )
        } == 0
        {
            return if std::io::Error::last_os_error().raw_os_error()
                == Some(ERROR_BROKEN_PIPE as i32)
            {
                Ok(())
            } else {
                Err("peek-owned-pipe")
            };
        }
        if available == 0 {
            return Ok(());
        }
        let mut buffer = [0; 4096];
        let count = (available as usize).min(buffer.len());
        require(bytes.len() + count <= CAP, "bounded-owner-output")?;
        let read = checked(pipe.read(&mut buffer[..count]), "read-owned-pipe")?;
        if read == 0 {
            return Ok(());
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    Ok(())
}
struct Owner {
    child: SpawnedProcess,
    role: Role,
    credit: Option<QueueCredit>,
    output: Vec<u8>,
    errors: Vec<u8>,
    cursor: usize,
    exited: Option<u32>,
    released: bool,
    peak_processes: u32,
}
impl Owner {
    fn poll(&mut self) -> Result<()> {
        if self.released {
            return Ok(());
        }
        drain(&mut self.child.stdout, &mut self.output)?;
        drain(&mut self.child.stderr, &mut self.errors)?;
        let census = checked(
            self.child.process.sample(&WindowsSampler::new()),
            "exact-job-census",
        )?;
        self.peak_processes = self.peak_processes.max(census.active_processes);
        if census.active_processes > self.role.budget().processes {
            // Counters and fixed labels only; never inspect ambient processes.
            eprintln!(
                "{}",
                json!({"diagnostic":"claude-tls-process-cap", "owner":self.role.label(),
                "activeProcesses":census.active_processes, "memberCount":census.members.len(),
                "verifiedMemberCount":census.members.iter().filter(|member| matches!(member.identity, Measurement::Available(_))).count(),
                "membershipComplete":census.membership_complete, "totalProcesses":census.total_processes,
                "totalTerminatedProcesses":census.total_terminated_processes,
                "configuredProcessCap":self.role.budget().processes,
                "appliedProcessCap":self.child.process.applied_budget().map(|budget| budget.max_processes())})
            );
            return Err("frozen-native-owner-process-cap");
        }
        self.exited = checked(self.child.process.try_wait(), "observe-owned-exit")?;
        if self.exited.is_some() {
            // Exit can follow the first empty peek. Drain again before inspecting
            // a Claude result or the fixture's final complete protocol line.
            drain(&mut self.child.stdout, &mut self.output)?;
            drain(&mut self.child.stderr, &mut self.errors)?;
        }
        Ok(())
    }
    fn release(&mut self, db: &Database) -> Result<()> {
        if self.released {
            return Ok(());
        }
        self.output = Vec::new();
        self.errors = Vec::new();
        self.cursor = 0;
        self.credit.take();
        let cleanup = checked(
            self.child
                .process
                .finish_cleanup(db, Duration::from_secs(2)),
            "native-proof-rundown",
        )?;
        require(
            matches!(cleanup, CleanupStatus::Released { .. }),
            "native-proof-owner-retained",
        )?;
        self.released = true;
        Ok(())
    }
}
struct Proof {
    db: std::sync::Arc<Database>,
    coordinator: AdmissionCoordinator,
    stamp: PolicyStamp,
    owners: Vec<Owner>,
    deadline: Instant,
}
impl Proof {
    fn start(&mut self, command: NativeCommand, role: Role) -> Result<usize> {
        require(Instant::now() < self.deadline, "proof-wall-deadline")?;
        let started = now();
        let revision = checked(self.db.resource_reservations(), "current-native-admission")?
            .ok_or("native-policy-missing")?
            .revision;
        let workload = checked(
            self.coordinator.prepare(
                &self.db,
                &self.stamp,
                revision,
                PrepareRequest {
                    budget: role.budget(),
                    process_commit_bytes: role.process_commit(),
                    now_ms: started,
                    lease_expires_at_ms: started + 45_000,
                },
            ),
            "persist-contained-admission",
        )?;
        let child = match spawn_contained(command, workload, &self.db, started, started + 45_000) {
            Ok(child) => child,
            Err(mut failure) => {
                let cleanup = checked(
                    failure
                        .workload
                        .finish_cleanup(&self.db, Duration::from_secs(2)),
                    "failed-spawn-rundown",
                )?;
                require(
                    matches!(cleanup, CleanupStatus::Released { .. }),
                    "failed-spawn-owner-retained",
                )?;
                return Err("native-contained-spawn");
            }
        };
        let index = self.owners.len();
        // Store the owner before the fallible queue reservation. Every failure
        // is followed by cleanup of every previously admitted job.
        self.owners.push(Owner {
            child,
            role,
            credit: None,
            output: Vec::new(),
            errors: Vec::new(),
            cursor: 0,
            exited: None,
            released: false,
            peak_processes: 0,
        });
        let owner = &mut self.owners[index];
        owner.credit = Some(checked(
            owner
                .child
                .process
                .queue_credits()
                .acquire((2 * CAP) as u64, 2),
            "reserve-bounded-owner-output",
        )?);
        Ok(index)
    }
    fn poll(&mut self) -> Result<()> {
        require(Instant::now() < self.deadline, "proof-wall-deadline")?;
        for owner in &mut self.owners {
            owner.poll()?;
        }
        Ok(())
    }
    fn phase(&mut self, expected: &str) -> Result<Value> {
        loop {
            self.poll()?;
            let fixture = &mut self.owners[0];
            if let Some(relative) = fixture.output[fixture.cursor..]
                .iter()
                .position(|byte| *byte == b'\n')
            {
                let end = fixture.cursor + relative;
                require(relative <= 4096, "bounded-owner-protocol")?;
                let value: Value = checked(
                    serde_json::from_slice(&fixture.output[fixture.cursor..end]),
                    "bounded-owner-protocol",
                )?;
                fixture.cursor = end + 1;
                if value["phase"] == "failed" {
                    return Err(match value["stage"].as_str() {
                        Some("canonical-sidecar-startup") => "synthetic-sidecar-startup-refused",
                        Some("no-ca") => "synthetic-no-ca-case-refused",
                        Some("private-ca") => "synthetic-private-ca-case-refused",
                        Some("cleanup") => "synthetic-shutdown-refused",
                        Some("main-record") => "synthetic-main-record-refused",
                    Some("main-steering") => "synthetic-main-steering-refused",
                    Some("main-steering-system-absent") => "synthetic-steering-live-system-absent",
                    Some("main-steering-system-present") => "synthetic-steering-live-system-unsteered",
                    _ => "synthetic-fixture-refused",
                    });
                }
                require(
                    value["phase"].as_str() == Some(expected),
                    "unexpected-owner-protocol-phase",
                )?;
                return Ok(value);
            }
            require(fixture.exited.is_none(), "synthetic-fixture-early-exit")?;
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn send(&mut self, value: Value) -> Result<()> {
        let mut bytes = checked(serde_json::to_vec(&value), "fixed-owner-command")?;
        require(bytes.len() < 1024, "bounded-owner-command")?;
        bytes.push(b'\n');
        // Only four finite acknowledgements, together below the pipe capacity.
        checked(
            self.owners[0].child.stdin.write_all(&bytes),
            "private-owner-command",
        )
    }
    fn wait_exit(&mut self, index: usize, duration: Duration) -> Result<u32> {
        let deadline = Instant::now() + duration;
        loop {
            self.poll()?;
            if let Some(code) = self.owners[index].exited {
                return Ok(code);
            }
            require(Instant::now() < deadline, "native-case-deadline")?;
            require(
                self.owners[0].exited.is_none(),
                "synthetic-fixture-early-exit",
            )?;
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn release_all(&mut self) -> Result<()> {
        let mut result = Ok(());
        for owner in self.owners.iter_mut().rev() {
            if let Err(error) = owner.release(&self.db) {
                result = Err(error);
            }
        }
        result
    }
}
fn port(value: &Value, name: &str) -> Result<u16> {
    value[name]
        .as_u64()
        .and_then(|value| u16::try_from(value).ok())
        .filter(|value| *value != 0)
        .ok_or("invalid-synthetic-loopback-port")
}
fn proxy_environment(environment: &mut BTreeMap<OsString, OsString>, port: u16) {
    // Windows environment names are case insensitive; NativeCommand rejects
    // duplicate uppercase/lowercase aliases instead of forwarding ambiguity.
    for name in ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"] {
        environment.insert(name.into(), format!("http://127.0.0.1:{port}").into());
    }
    environment.insert("NO_PROXY".into(), "".into());
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    Tls,
    NativeResume,
    EffectiveConfig,
}
impl Scenario {
    fn name(self) -> &'static str {
        match self {
            Self::Tls => "tls",
            Self::NativeResume => "native-resume",
            Self::EffectiveConfig => "effective-config",
        }
    }
}
pub struct PrivateCase {
    root: PathBuf,
    pub project: PathBuf,
    pub config: PathBuf,
    pub home: PathBuf,
}
pub struct Fixture {
    proof: Proof,
    base: StorageLocation,
    _claude_pin: PreparedClaudeProbe,
    claude: PathBuf,
    recipe: SidecarRuntimeRecipe,
    os: BTreeMap<OsString, OsString>,
    scenario: Scenario,
    startup: Option<NativeCommand>,
    endpoints: Option<(u16, u16)>,
    sidecar: Option<usize>,
}
impl Fixture {
    pub fn prepare(scenario: Scenario) -> Result<Self> {
        let root = std::env::temp_dir().join(format!(
            "shinatama-claude-{}-{}",
            scenario.name(),
            uuid::Uuid::new_v4()
        ));
        let base = checked(
            StorageLocation::resolve(LocationRequest {
                local_data_directory: root.clone(),
                home_override: Some(root.join("private")),
                project_roots: vec![],
                synchronized_roots: vec![],
            }),
            "private-location",
        )?;
        let db = std::sync::Arc::new(checked(Database::open(&base, None), "private-state")?);
        let candidate = checked(discover_claude(), "unambiguous-native-claude")?;
        let pin = checked(
            PreparedClaudeProbe::prepare(&candidate, &base),
            "pin-native-claude",
        )?;
        require(
            pin.executable_sha256() == CLAUDE_SHA,
            "exact-claude-2.1.318-hash",
        )?;
        let listener = checked(
            TcpListener::bind(("127.0.0.1", 0)),
            "reserve-private-sidecar-port",
        )?;
        let port = NonZeroU16::new(checked(listener.local_addr(), "private-port")?.port()).unwrap();
        drop(listener);
        let config = checked(
            prepare_private_sidecar(&base, port),
            "private-sidecar-config",
        )?;
        let bundle = checked(
            verify_prepared_bundle(&workspace().join("sidecar/source-build/prepared")),
            "pinned-canonical-bundle",
        )?;
        let node = checked(
            verify_bundled_node(
                &workspace().join("sidecar/runtime"),
                NodePin {
                    relative_path: "node.exe".into(),
                    sha256: NODE_SHA.into(),
                    version: Some("24.14.1".into()),
                    modules_abi: Some(137),
                },
            ),
            "pinned-native-node",
        )?;
        let os: BTreeMap<OsString, OsString> = ["SystemRoot", "WINDIR"]
            .into_iter()
            .filter_map(|key| std::env::var_os(key).map(|value| (key.into(), value)))
            .collect();
        let recipe = checked(
            prepare_runtime(bundle, node, config.root(), &os, port, None),
            "private-runtime-recipe",
        )?;
        let fixture_path = config.root().directory().join("claude-proxy-tls.mjs");
        let mut file = checked(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&fixture_path),
            "write-new-fixture",
        )?;
        checked(
            file.write_all(include_bytes!("../fixtures/claude-proxy-tls.mjs")),
            "write-fixed-fixture-bytes",
        )?;
        checked(file.sync_all(), "flush-fixed-fixture")?;
        drop(file);
        checked(
            config.root().secure_private_file(&fixture_path),
            "protect-fixed-fixture",
        )?;
        let startup = checked(
            NativeCommand::new(
                recipe.executable().to_owned(),
                vec![
                    recipe
                        .working_directory()
                        .join("claude-proxy-tls.mjs")
                        .into_os_string(),
                    recipe.working_directory().as_os_str().to_owned(),
                    port.to_string().into(),
                    scenario.name().into(),
                ],
                recipe.working_directory().to_owned(),
                recipe
                    .child_environment()
                    .map(|(key, value)| (key.to_owned(), value.to_owned()))
                    .collect(),
            ),
            "fixed-contained-fixture-command",
        )?;
        let stamp = PolicyStamp {
            version: 1,
            id: "claude-local-tls-separated-owners-v2".into(),
        };
        checked(
            db.initialize_resource_policy(&ReservationPolicy {
                stamp: stamp.clone(),
                aggregate_ceiling: aggregate_budget(),
                per_slot_ceiling: Role::Sidecar.budget(),
                max_slots: 3,
                max_lease_ms: 45_000,
            }),
            "freeze-native-proof-limits",
        )?;
        Ok(Self {
            proof: Proof {
                db,
                coordinator: checked(AdmissionCoordinator::new(3), "three-independent-workloads")?,
                stamp,
                owners: Vec::new(),
                deadline: Instant::now() + Duration::from_secs(40),
            },
            base,
            _claude_pin: pin,
            claude: candidate.path,
            recipe,
            os,
            scenario,
            startup: Some(startup),
            endpoints: None,
            sidecar: None,
        })
    }
    pub fn database(&self) -> &Database {
        &self.proof.db
    }
    pub fn base(&self) -> &StorageLocation {
        &self.base
    }
    pub fn private_root(&self) -> &Path {
        self.recipe.working_directory()
    }
    pub fn start_sidecar(&mut self) -> Result<()> {
        let sampled = checked(WindowsSampler::new().sample(&[]), "fresh-native-headroom")?;
        let Measurement::Available(system) = sampled.system else {
            return Err("native-headroom-unavailable");
        };
        require(
            system.physical_available_bytes >= aggregate_budget().memory_bytes + 512 * MIB
                && system
                    .commit_limit_bytes
                    .saturating_sub(system.commit_total_bytes)
                    >= aggregate_budget().memory_bytes + 1024 * MIB,
            "native-headroom-refused",
        )?;
        let command = self.startup.take().ok_or("fixture-already-started")?;
        self.proof.deadline = Instant::now() + Duration::from_secs(40);
        self.proof.start(command, Role::Fixture)?;
        let ready = self.phase("fixture-ready")?;
        let origin = port(&ready, "originPort")?;
        let front = port(&ready, "frontPort")?;
        let back = port(&ready, "backPort")?;
        require(
            origin != front && front != back && origin != back,
            "distinct-loopback-ports",
        )?;
        let mut environment: BTreeMap<OsString, OsString> = self
            .recipe
            .child_environment()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect();
        proxy_environment(&mut environment, back);
        environment.insert("BILI_MITM_DOMAINS".into(), "localhost".into());
        environment.insert(
            "BILI_UPSTREAM_PROXY".into(),
            format!("http://127.0.0.1:{back}").into(),
        );
        environment.insert("BILI_UPSTREAM_PROXY_MODE".into(), "manual".into());
        for name in ["NODE_EXTRA_CA_CERTS", "SSL_CERT_FILE"] {
            environment.insert(
                name.into(),
                self.private_root()
                    .join("data/ca/root-ca.pem")
                    .into_os_string(),
            );
        }
        let command = checked(
            NativeCommand::new(
                self.recipe.executable().to_owned(),
                self.recipe.arguments().to_vec(),
                self.private_root().to_owned(),
                environment,
            ),
            "fixed-canonical-sidecar-command",
        )?;
        let sidecar = self.proof.start(command, Role::Sidecar)?;
        self.sidecar = Some(sidecar);
        self.endpoints = Some((origin, front));
        self.send(json!({"sidecarPid":self.proof.owners[sidecar].child.process.key().pid}))
    }
    pub fn create_case(&self, label: &str) -> Result<PrivateCase> {
        require(
            !label.is_empty()
                && label.len() <= 32
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'-'),
            "fixed-private-case-name",
        )?;
        let root = self.private_root().join(label);
        checked(fs::create_dir(&root), "fresh-native-case-directory")?;
        for folder in ["home", "config", "roaming", "local", "tmp", "project"] {
            checked(
                fs::create_dir(root.join(folder)),
                "fresh-native-case-subdirectory",
            )?;
        }
        Ok(PrivateCase {
            project: root.join("project"),
            config: root.join("config"),
            home: root.join("home"),
            root,
        })
    }
    pub fn claude_command(
        &self,
        case: &PrivateCase,
        private_ca: bool,
        argv: Vec<OsString>,
        explicit_env: BTreeMap<OsString, OsString>,
    ) -> Result<NativeCommand> {
        require(
            case.root.parent() == Some(self.private_root()),
            "exact-private-case-owner",
        )?;
        let (origin, front) = self.endpoints.ok_or("local-sidecar-not-started")?;
        let mut environment = self.os.clone();
        let system = environment
            .get(&OsString::from("SystemRoot"))
            .ok_or("private-system-root")?
            .clone();
        environment.insert(
            "PATH".into(),
            Path::new(&system).join("System32").into_os_string(),
        );
        for (name, directory) in [
            ("HOME", "home"),
            ("USERPROFILE", "home"),
            ("CLAUDE_CONFIG_DIR", "config"),
            ("APPDATA", "roaming"),
            ("LOCALAPPDATA", "local"),
            ("TEMP", "tmp"),
            ("TMP", "tmp"),
            ("CLAUDE_CODE_TMPDIR", "tmp"),
        ] {
            environment.insert(name.into(), case.root.join(directory).into_os_string());
        }
        for (name, value) in [
            ("ANTHROPIC_API_KEY", DUMMY_KEY),
            ("CLAUDE_CODE_CERT_STORE", "bundled"),
            ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
            ("CLAUDE_CODE_DISABLE_OFFICIAL_MARKETPLACE_AUTOINSTALL", "1"),
            ("CLAUDE_CODE_SKIP_MCP_DEBUG_LOG_FILES", "1"),
            ("CLAUDE_CODE_AUTO_CONNECT_IDE", "false"),
            ("DISABLE_AUTOUPDATER", "1"),
            ("DISABLE_UPDATES", "1"),
            ("DISABLE_TELEMETRY", "1"),
            ("DISABLE_ERROR_REPORTING", "1"),
            ("API_TIMEOUT_MS", "3000"),
            ("CLAUDE_STREAM_FIRST_BYTE_TIMEOUT_MS", "3000"),
            ("CLAUDE_CODE_MAX_RETRIES", "0"),
            ("CLAUDE_CODE_GZIP_REQUEST_BODIES", "0"),
            ("MAX_THINKING_TOKENS", "0"),
        ] {
            environment.insert(name.into(), value.into());
        }
        // The canonical sidecar treats max_tokens<=200 as a side request and
        // forwards it untouched (kernel state, output steering included). The
        // effective-config proof observes the main lane, so only that scenario
        // gets an output cap above the side threshold; the other scenarios
        // keep the tight synthetic bound.
        environment.insert(
            "CLAUDE_CODE_MAX_OUTPUT_TOKENS".into(),
            if self.scenario == Scenario::EffectiveConfig {
                "1024"
            } else {
                "128"
            }
            .into(),
        );
        if matches!(self.scenario, Scenario::Tls | Scenario::EffectiveConfig) {
            environment.insert("CLAUDE_CODE_SAFE_MODE".into(), "1".into());
        }
        environment.insert(
            "ANTHROPIC_BASE_URL".into(),
            format!("https://localhost:{origin}").into(),
        );
        proxy_environment(&mut environment, front);
        if private_ca {
            for name in ["NODE_EXTRA_CA_CERTS", "SSL_CERT_FILE"] {
                environment.insert(
                    name.into(),
                    self.private_root()
                        .join("data/ca/root-ca.pem")
                        .into_os_string(),
                );
            }
        }
        for (name, value) in explicit_env {
            require(
                matches!(
                    name.to_str(),
                    Some(
                        "SHINATAMA_INSTANCE"
                            | "SHINATAMA_INSTANCE_TOKEN"
                            | "CLAUDE_CODE_GIT_BASH_PATH"
                    )
                ),
                "fixed-proof-extra-environment",
            )?;
            require(
                self.scenario == Scenario::NativeResume,
                "tls-proof-has-no-hooks",
            )?;
            environment.insert(name, value);
        }
        // Both scenarios forbid tool use and permission bypass. The caller is
        // trusted test Rust, never IPC, and supplies reviewed fixed argv only.
        require(
            argv.windows(2)
                .any(|pair| pair[0] == "--tools" && pair[1].is_empty())
                && argv
                    .windows(2)
                    .any(|pair| pair[0] == "--mcp-config" && pair[1] == "{\"mcpServers\":{}}")
                && !argv.iter().any(|arg| {
                    arg == "--dangerously-skip-permissions"
                        || arg == "--allow-dangerously-skip-permissions"
                }),
            "tools-disabled-proof-recipe",
        )?;
        checked(
            NativeCommand::new(self.claude.clone(), argv, case.project.clone(), environment),
            "fixed-native-claude-command",
        )
    }
    pub fn start_claude(&mut self, command: NativeCommand) -> Result<usize> {
        require(
            !self
                .proof
                .owners
                .iter()
                .any(|owner| matches!(owner.role, Role::Claude) && !owner.released),
            "previous-claude-owner-not-released",
        )?;
        self.proof.start(command, Role::Claude)
    }
    pub fn phase(&mut self, expected: &str) -> Result<Value> {
        self.proof.phase(expected)
    }
    pub fn send(&mut self, value: Value) -> Result<()> {
        self.proof.send(value)
    }
    pub fn wait_exit(&mut self, index: usize, duration: Duration) -> Result<u32> {
        self.proof.wait_exit(index, duration)
    }
    pub fn output(&self, index: usize) -> Result<(&[u8], &[u8])> {
        let owner = self.proof.owners.get(index).ok_or("unknown-proof-owner")?;
        require(
            owner.exited.is_some() && !owner.released,
            "owned-output-not-ready",
        )?;
        Ok((&owner.output, &owner.errors))
    }
    pub fn release(&mut self, index: usize) -> Result<()> {
        self.proof
            .owners
            .get_mut(index)
            .ok_or("unknown-proof-owner")?
            .release(&self.proof.db)
    }
    pub fn shutdown(&mut self) -> Result<Value> {
        self.phase("sidecar-stop")?;
        let sidecar = self.sidecar.ok_or("sidecar-owner-missing")?;
        let exit = self.wait_exit(sidecar, Duration::from_secs(2))?;
        require(exit == 0, "canonical-sidecar-clean-exit")?;
        self.release(sidecar)?;
        self.send(json!({"sidecarReleased":true,"exitCode":exit,"outputBounded":true}))?;
        let completed = self.phase("complete")?;
        require(
            self.wait_exit(0, Duration::from_secs(2))? == 0,
            "synthetic-fixture-clean-exit",
        )?;
        Ok(completed)
    }
    pub fn release_all(&mut self) -> Result<()> {
        self.proof.release_all()?;
        require(
            checked(
                self.database().resource_reservations(),
                "released-private-ledger",
            )?
            .is_some_and(|value| value.slots.is_empty()),
            "private-ledger-not-empty",
        )
    }
    pub fn evidence(&self, child: Value) -> Result<Value> {
        // tls/native-resume run two Claude owners (negative+positive, start+resume);
        // effective-config runs one main-lane Claude owner beside fixture and sidecar.
        let expected_owners = if self.scenario == Scenario::EffectiveConfig {
            3
        } else {
            4
        };
        require(
            self.proof.owners.len() == expected_owners
                && self.proof.owners.iter().all(|owner| owner.released),
            "expected-released-proof-owners",
        )?;
        require(
            checked(
                self.database().resource_reservations(),
                "released-private-ledger",
            )?
            .is_some_and(|value| value.slots.is_empty()),
            "private-ledger-not-empty",
        )?;
        let owners: Vec<Value> = self.proof.owners.iter().map(|owner| json!({
            "role":owner.role.label(), "jobCommitBytes":owner.role.budget().memory_bytes,
            "processCommitBytes":owner.role.process_commit(), "rawProcessCap":owner.role.budget().processes,
            "cpuBasisPoints":owner.role.budget().cpu_basis_points, "peakObservedProcesses":owner.peak_processes,
            "rootIdentityVerifiedBeforeRun":true, "released":owner.released })).collect();
        Ok(
            json!({"passed":true,"platform":"Windows","scenario":self.scenario.name(),"claudeVersion":CLAUDE_VERSION,
            "claudeSha256":CLAUDE_SHA,"nodeSha256":NODE_SHA,
            "scope":"separate C09 native headless fixture; no C11 Start authority, login or provider qualification", "child":child,
            "separateOwners":true,"maxConcurrentOwners":3,"aggregateCommitBytes":aggregate_budget().memory_bytes,
            "aggregateProcessCap":aggregate_budget().processes,"owners":owners,"ledgerReleased":true,"fixtureRetained":true}),
        )
    }
}
pub fn tls_arguments() -> Vec<OsString> {
    restricted_arguments("Reply with the fixed synthetic marker.")
}
/// Same restricted headless recipe with the fixed synthetic start prompt the
/// effective-config fixture classifies main requests by.
pub fn ec_arguments() -> Vec<OsString> {
    restricted_arguments("SHINATAMA_SYNTHETIC_START_PROMPT_A")
}
fn restricted_arguments(prompt: &str) -> Vec<OsString> {
    let mut argv: Vec<OsString> = [
        "--restricted",
        "--safe-mode",
        "--tools",
        "",
        "--disallowedTools",
        "mcp__*",
        "--mcp-config",
        "{\"mcpServers\":{}}",
        "--strict-mcp-config",
        "--setting-sources",
        "",
        "--settings",
        "{\"disableAllHooks\":true}",
        "--no-chrome",
        "--no-session-persistence",
        "--max-turns",
        "1",
        "--model",
        "claude-sonnet-4-6",
        "--output-format",
        "json",
        "-p",
    ]
    .map(OsString::from)
    .to_vec();
    argv.push(OsString::from(prompt));
    argv
}
pub fn write_evidence(name: &str, value: &Value) -> Result<()> {
    require(
        matches!(
            name,
            "claude-proxy-tls.json" | "claude-native-resume.json" | "claude-effective-config.json"
        ),
        "fixed-proof-evidence-name",
    )?;
    let directory = workspace().join(".prototype-evidence");
    checked(fs::create_dir_all(&directory), "evidence-directory")?;
    checked(
        fs::write(
            directory.join(name),
            serde_json::to_vec_pretty(value).unwrap(),
        ),
        "write-bounded-boolean-proof",
    )
}
