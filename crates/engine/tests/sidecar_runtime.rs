#![cfg(windows)]
//! Explicit local qualification. All native children enter the persisted limited
//! job before their first instruction. No provider, installer or user state.
use serde_json::{Value, json};
use shinatama_engine::{
    resources::{
        admission::{AdmissionCoordinator, CleanupStatus, PrepareRequest, QueueCredit},
        process::{NativeCommand, SpawnedProcess, spawn_contained},
        sampling::WindowsSampler,
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
    fs::{self, File},
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    num::NonZeroU16,
    os::windows::io::AsRawHandle,
    path::{Path, PathBuf},
    ptr::null_mut,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{Foundation::ERROR_BROKEN_PIPE, System::Pipes::PeekNamedPipe};

type ProofResult<T> = Result<T, String>;
const MIB: u64 = 1024 * 1024;
const CAP: usize = 64 * 1024;
const LIMIT: Duration = Duration::from_millis(750);
const DEPENDENCIES: &str = "D:/Projects/horness-launcher/sidecar/source-build/node_modules";
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
fn safe<T, E>(result: Result<T, E>, operation: &'static str) -> ProofResult<T> {
    result.map_err(|_| operation.to_owned())
}
fn require(condition: bool, message: &'static str) -> ProofResult<()> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}
fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}
fn budget() -> Budget {
    Budget {
        memory_bytes: 1024 * MIB,
        processes: 8,
        cpu_basis_points: 2000,
        queue_bytes: 256 * 1024,
        queue_items: 4,
    }
}
struct Fixture {
    root: PathBuf,
    released: bool,
}
impl Fixture {
    fn new() -> Self {
        Self {
            root: std::env::temp_dir().join(format!(
                "shinatama-sidecar-runtime-{}",
                uuid::Uuid::new_v4()
            )),
            released: false,
        }
    }
    fn location(&self) -> StorageLocation {
        StorageLocation::resolve(LocationRequest {
            local_data_directory: self.root.clone(),
            home_override: Some(self.root.join("private")),
            project_roots: vec![workspace()],
            synchronized_roots: vec![],
        })
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        // A failed native cleanup preserves this synthetic fixture for inspection.
        let parent = std::env::temp_dir();
        if self.released
            && self.root.parent() == Some(parent.as_path())
            && self
                .root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("shinatama-sidecar-runtime-")
        {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
struct Child {
    spawned: SpawnedProcess,
    output: Vec<u8>,
    errors: Vec<u8>,
    credit: Option<QueueCredit>,
    deadline: Instant,
    cursor: usize,
}
impl Child {
    fn start(
        command: NativeCommand,
        coordinator: &AdmissionCoordinator,
        db: &Database,
        stamp: &PolicyStamp,
        deadline: Instant,
    ) -> ProofResult<Self> {
        let revision = safe(db.resource_reservations(), "read reservation ledger")?
            .unwrap()
            .revision;
        let started = now();
        let workload = safe(
            coordinator.prepare(
                db,
                stamp,
                revision,
                PrepareRequest {
                    budget: budget(),
                    process_commit_bytes: 768 * MIB,
                    now_ms: started,
                    lease_expires_at_ms: started + 30_000,
                },
            ),
            "prepare contained workload",
        )?;
        let spawned = match spawn_contained(command, workload, db, started, started + 30_000) {
            Ok(value) => value,
            Err(mut failure) => {
                let _ = failure.workload.finish_cleanup(db, Duration::from_secs(2));
                return Err(format!("contained native startup: {:?}", failure.error));
            }
        };
        let credit = safe(
            spawned.process.queue_credits().acquire((2 * CAP) as u64, 2),
            "reserve bounded output credits",
        )?;
        Ok(Self {
            spawned,
            output: Vec::with_capacity(CAP),
            errors: Vec::with_capacity(CAP),
            credit: Some(credit),
            deadline,
            cursor: 0,
        })
    }
    fn poll(&mut self) -> ProofResult<()> {
        require(
            Instant::now() < self.deadline,
            "native proof exceeded 30-second wall budget",
        )?;
        drain(&mut self.spawned.stdout, &mut self.output)?;
        drain(&mut self.spawned.stderr, &mut self.errors)?;
        let census = safe(
            self.spawned.process.sample(&WindowsSampler::new()),
            "sample exact job members",
        )?;
        require(
            census.active_processes <= 8,
            "raw native process count exceeded frozen cap8",
        )?;
        Ok(())
    }
    fn phase(&mut self, phase: &str) -> ProofResult<Value> {
        loop {
            self.poll()?;
            while let Some(end) = self.output[self.cursor..]
                .iter()
                .position(|byte| *byte == b'\n')
            {
                let end = self.cursor + end;
                let line = &self.output[self.cursor..end];
                self.cursor = end + 1;
                if let Ok(value) = serde_json::from_slice::<Value>(line) {
                    if value["phase"] == "failed" {
                        return Err(format!(
                            "contained fixture refused at {}",
                            value["stage"].as_str().unwrap_or("unknown")
                        ));
                    }
                    if value["phase"] == phase {
                        return Ok(value);
                    }
                }
            }
            require(
                safe(self.spawned.process.try_wait(), "read owned process exit")?.is_none(),
                "fixture exited before expected phase",
            )?;
            std::thread::sleep(Duration::from_millis(3));
        }
    }
    fn instruction(&mut self, command: &'static [u8]) -> ProofResult<()> {
        safe(
            self.spawned.stdin.write_all(command),
            "send fixed private fixture instruction",
        )
    }
    fn wait_exit(&mut self) -> ProofResult<()> {
        loop {
            self.poll()?;
            if let Some(code) = safe(self.spawned.process.try_wait(), "read owned exit")? {
                self.poll()?;
                return require(code == 0, "contained process exited unsuccessfully");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn diagnostics(&self, exit_code: Option<u32>, private: &Path) -> Value {
        fn sanitized(bytes: &[u8], private: &Path) -> String {
            let mut text = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).into_owned();
            for (path, label) in [
                (private.to_owned(), "<private-fixture>"),
                (workspace(), "<workspace>"),
                (PathBuf::from(DEPENDENCIES), "<offline-toolchain>"),
            ] {
                let path = path.to_string_lossy();
                text = text
                    .replace(path.as_ref(), label)
                    .replace(&path.replace('\\', "/"), label)
                    .replace(&path.replace('/', "\\"), label);
            }
            text.chars()
                .filter(|ch| !ch.is_control() || *ch == '\n' || *ch == '\t')
                .collect()
        }
        json!({"exitCode":exit_code,"stdout":sanitized(&self.output,private),"stderr":sanitized(&self.errors,private),"captureCapBytes":8192,"syntheticPrivateFixtureOnly":true})
    }
    fn cleanup(mut self, db: &Database) -> ProofResult<()> {
        // Allocations and corresponding credits return before persisted release.
        self.output = Vec::new();
        self.errors = Vec::new();
        self.credit.take();
        let result = safe(
            self.spawned
                .process
                .finish_cleanup(db, Duration::from_secs(2)),
            "finish exact job cleanup",
        )?;
        require(
            matches!(result, CleanupStatus::Released { .. }),
            "exact process-death/census cleanup remains retired",
        )
    }
}
fn drain(pipe: &mut File, bytes: &mut Vec<u8>) -> ProofResult<()> {
    // No blocking reader or output thread. Read only bytes PeekNamedPipe observes.
    for _ in 0..16 {
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
                Err("peek owned bounded pipe".to_owned())
            };
        }
        if available == 0 {
            return Ok(());
        }
        let mut buffer = [0; 4096];
        let count = (available as usize).min(buffer.len());
        require(
            bytes.len() + count <= CAP,
            "contained output exceeded 64-KiB cap",
        )?;
        let read = safe(pipe.read(&mut buffer[..count]), "drain owned bounded pipe")?;
        if read == 0 {
            return Ok(());
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    Ok(())
}
fn fixture_command(
    recipe: &SidecarRuntimeRecipe,
    mode: &str,
    private: &Path,
) -> ProofResult<NativeCommand> {
    let mut arguments = vec![
        workspace()
            .join("crates/engine/tests/fixtures/sidecar-runtime.mjs")
            .into_os_string(),
        mode.into(),
        workspace()
            .join("sidecar/source-build/prepared")
            .into_os_string(),
        private.as_os_str().to_owned(),
        DEPENDENCIES.into(),
    ];
    if mode == "tls" {
        arguments.push(recipe.arguments()[5].clone());
    }
    safe(
        NativeCommand::new(
            recipe.executable().to_owned(),
            arguments,
            recipe.working_directory().to_owned(),
            recipe
                .child_environment()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect(),
        ),
        "validate fixture command",
    )
}
fn http(port: u16, method: &str, path: &str) -> ProofResult<(u16, Vec<u8>)> {
    let address: SocketAddr = ([127, 0, 0, 1], port).into();
    let mut stream = safe(
        TcpStream::connect_timeout(&address, Duration::from_millis(500)),
        "connect owned loopback server",
    )?;
    safe(
        stream.set_read_timeout(Some(Duration::from_secs(2))),
        "bound loopback read",
    )?;
    safe(
        stream.set_write_timeout(Some(Duration::from_millis(500))),
        "bound loopback write",
    )?;
    safe(
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
        ),
        "send loopback request",
    )?;
    let mut bytes = Vec::new();
    safe(
        (&mut stream).take(32 * 1024 + 1).read_to_end(&mut bytes),
        "read bounded loopback response",
    )?;
    require(bytes.len() <= 32 * 1024, "loopback response exceeded cap")?;
    let split = bytes
        .windows(4)
        .position(|value| value == b"\r\n\r\n")
        .ok_or("invalid loopback HTTP framing")?;
    let headers = safe(
        std::str::from_utf8(&bytes[..split]),
        "invalid loopback HTTP headers",
    )?;
    let status = headers
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .ok_or("invalid loopback HTTP status")?;
    let body = &bytes[split + 4..];
    if !headers
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        return Ok((status, body.to_vec()));
    }
    let mut decoded = Vec::new();
    let mut offset = 0;
    loop {
        let end = body[offset..]
            .windows(2)
            .position(|value| value == b"\r\n")
            .ok_or("invalid chunk framing")?
            + offset;
        let count = usize::from_str_radix(
            safe(
                std::str::from_utf8(&body[offset..end]),
                "invalid chunk header",
            )?,
            16,
        )
        .map_err(|_| "invalid chunk length")?;
        offset = end + 2;
        if count == 0 {
            break;
        }
        require(offset + count + 2 <= body.len(), "truncated HTTP chunk")?;
        decoded.extend_from_slice(&body[offset..offset + count]);
        offset += count + 2;
    }
    Ok((status, decoded))
}
fn measured_health(port: u16) -> ProofResult<Value> {
    let epoch = now();
    let started = Instant::now();
    let (status, body) = http(port, "GET", "/__bili/health")?;
    require(
        status == 200 && body == b"healthy",
        "source fixture health failed",
    )?;
    require(
        started.elapsed() < LIMIT,
        "local health exceeded 750 ms during CCR save",
    )?;
    Ok(json!({"milliseconds": started.elapsed().as_secs_f64()*1000.0,"startedAtMs":epoch}))
}
fn measured_connect(port: u16, echo_port: u16) -> ProofResult<Value> {
    let epoch = now();
    let started = Instant::now();
    let mut stream = safe(
        TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(500)),
        "connect real local CONNECT",
    )?;
    safe(
        stream.set_read_timeout(Some(Duration::from_secs(2))),
        "bound CONNECT read",
    )?;
    safe(
        stream.set_write_timeout(Some(Duration::from_millis(500))),
        "bound CONNECT write",
    )?;
    safe(
        write!(
            stream,
            "CONNECT 127.0.0.1:{echo_port} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"
        ),
        "send real CONNECT",
    )?;
    let mut response = Vec::new();
    let mut byte = [0];
    while !response.ends_with(b"\r\n\r\n") && response.len() < 1024 {
        safe(stream.read_exact(&mut byte), "read real CONNECT response")?;
        response.push(byte[0]);
    }
    require(
        response.starts_with(b"HTTP/1.1 200"),
        "real CONNECT refused",
    )?;
    safe(stream.write_all(b"latency-ping"), "write local tunnel echo")?;
    let mut echoed = [0; 12];
    stream.read_exact(&mut echoed).map_err(|error| {
        format!(
            "read local tunnel echo: {:?}/os={:?}",
            error.kind(),
            error.raw_os_error()
        )
    })?;
    require(&echoed == b"latency-ping", "real tunnel echo mismatch")?;
    require(
        started.elapsed() < LIMIT,
        "local CONNECT+echo exceeded 750 ms during CCR save",
    )?;
    Ok(json!({"milliseconds": started.elapsed().as_secs_f64()*1000.0,"startedAtMs":epoch}))
}
fn echo(listener: TcpListener, deadline: Instant) -> ProofResult<()> {
    safe(listener.set_nonblocking(true), "bound local echo listener")?;
    let mut count = 0;
    while count < 3 && Instant::now() < deadline {
        match listener.accept() {
            Ok((mut socket, _)) => {
                // Windows accept inherits the listener's nonblocking mode.
                // The accepted stream uses explicit finite blocking timeouts.
                safe(
                    socket.set_nonblocking(false),
                    "set owned echo stream blocking",
                )?;
                safe(
                    socket.set_read_timeout(Some(Duration::from_secs(2))),
                    "bound echo reader",
                )?;
                safe(
                    socket.set_write_timeout(Some(Duration::from_millis(500))),
                    "bound echo writer",
                )?;
                let mut bytes = [0; 12];
                socket.read_exact(&mut bytes).map_err(|error| {
                    format!(
                        "read synthetic echo: {:?}/os={:?}",
                        error.kind(),
                        error.raw_os_error()
                    )
                })?;
                require(
                    &bytes == b"latency-ping",
                    "unexpected synthetic echo payload",
                )?;
                safe(socket.write_all(&bytes), "return synthetic echo")?;
                count += 1;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(2))
            }
            Err(_) => return Err("accept owned echo".into()),
        }
    }
    require(
        count == 3,
        "not all three CONNECT probes reached loopback echo",
    )
}

#[test]
#[ignore = "local Windows qualification; requires reviewed prepared bundle, pinned Node and offline esbuild"]
fn pinned_node_and_real_bundle_plus_canonical_storage_under_containment() {
    let mut fixture = Fixture::new();
    // Preserve the already passed storage-only observation as its own evidence.
    let prior = workspace().join(".prototype-evidence/sidecar-runtime.json");
    let preserved = workspace().join(".prototype-evidence/sidecar-runtime-storage.json");
    if let Ok(file) = File::open(&prior) {
        let mut bytes = Vec::new();
        if file.take(64 * 1024 + 1).read_to_end(&mut bytes).is_ok()
            && bytes.len() <= 64 * 1024
            && serde_json::from_slice::<Value>(&bytes)
                .is_ok_and(|value| value["passed"] == true && value.get("childTlsTrust").is_none())
            && let Ok(mut destination) = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(preserved)
            {
                destination.write_all(&bytes).unwrap();
            }
    }
    let before = ["NODE_EXTRA_CA_CERTS", "NODE_TLS_REJECT_UNAUTHORIZED"].map(std::env::var_os);
    let mut result = qualify(&fixture);
    let parent_unchanged =
        before == ["NODE_EXTRA_CA_CERTS", "NODE_TLS_REJECT_UNAUTHORIZED"].map(std::env::var_os);
    if !parent_unchanged {
        result = Err("parent TLS environment changed".into());
    }
    let mut evidence = match &result {
        Ok(value) => value.clone(),
        Err(code) => {
            json!({"passed":false,"failure":code,"nativeStartEnabled":false,"SC16":"open"})
        }
    };
    evidence["parentTlsEnvironmentUnchanged"] = json!(parent_unchanged);
    let target = workspace().join(".prototype-evidence/sidecar-runtime.json");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    if result.is_ok() {
        fixture.released = true;
    }
    assert!(result.is_ok(), "{}", result.err().unwrap_or_default());
}
fn qualify(fixture: &Fixture) -> ProofResult<Value> {
    let location = fixture.location();
    let db = safe(
        Database::open(&location, None),
        "open private fixture SQLite ledger",
    )?;
    let listener = safe(
        TcpListener::bind(("127.0.0.1", 0)),
        "reserve loopback CLI port",
    )?;
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let config = safe(
        prepare_private_sidecar(&location, NonZeroU16::new(port).unwrap()),
        "prepare canonical private configuration",
    )?;
    let bundle = safe(
        verify_prepared_bundle(&workspace().join("sidecar/source-build/prepared")),
        "verify every prepared source/artifact hash",
    )?;
    let node = safe(
        verify_bundled_node(
            &workspace().join("sidecar/runtime"),
            NodePin {
                relative_path: "node.exe".into(),
                sha256: "58e74bf02fc5bbacc41dcb8bef089961cd5bddd37830b87784e4fc624d145d1f".into(),
                version: Some("24.14.1".into()),
                modules_abi: None,
            },
        ),
        "verify pinned native Node identity",
    )?;
    let mut os: BTreeMap<OsString, OsString> = ["SystemRoot", "WINDIR"]
        .into_iter()
        .filter_map(|key| std::env::var_os(key).map(|value| (key.into(), value)))
        .collect();
    // Synthetic hostile inherited settings must never enter the private recipe.
    // No real certificate path is read and the parent environment is untouched.
    os.insert("NODE_TLS_REJECT_UNAUTHORIZED".into(), "0".into());
    os.insert(
        "NODE_EXTRA_CA_CERTS".into(),
        r"C:\synthetic-forbidden-ca.pem".into(),
    );
    let recipe = safe(
        prepare_runtime(
            bundle,
            node,
            config.root(),
            &os,
            NonZeroU16::new(port).unwrap(),
            None,
        ),
        "prepare private child-only environment",
    )?;
    require(
        recipe.child_environment().all(|(key, _)| {
            !key.eq_ignore_ascii_case("NODE_TLS_REJECT_UNAUTHORIZED")
                && !key.eq_ignore_ascii_case("NODE_EXTRA_CA_CERTS")
        }),
        "private recipe inherited hostile TLS settings",
    )?;
    let stamp = PolicyStamp {
        version: 1,
        id: "sidecar-local-proof-v1".into(),
    };
    safe(
        db.initialize_resource_policy(&ReservationPolicy {
            stamp: stamp.clone(),
            aggregate_ceiling: budget(),
            per_slot_ceiling: budget(),
            max_slots: 1,
            max_lease_ms: 30_000,
        }),
        "freeze explicit local native limits",
    )?;
    let coordinator = safe(AdmissionCoordinator::new(1), "create fixed workload slot")?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut probe = Child::start(
        fixture_command(&recipe, "probe", config.root().directory())?,
        &coordinator,
        &db,
        &stamp,
        deadline,
    )?;
    let probe_result: ProofResult<Value> = (|| {
        let value = probe.phase("probe")?;
        probe.wait_exit()?;
        Ok(value)
    })();
    let cleanup = probe.cleanup(&db);
    let probe = probe_result?;
    cleanup?;
    require(
        probe["nodeVersion"] == "24.14.1" && probe["builtinSqlite"] == true,
        "pinned Node runtime disagrees with declaration",
    )?;
    let cli_command = safe(
        NativeCommand::new(
            recipe.executable().to_owned(),
            recipe.arguments().to_vec(),
            recipe.working_directory().to_owned(),
            recipe
                .child_environment()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect(),
        ),
        "validate canonical CLI recipe",
    )?;
    let mut cli = Child::start(cli_command, &coordinator, &db, &stamp, deadline)?;
    let cli_result: ProofResult<Value> = (|| {
        let health = loop {
            cli.poll()?;
            if let Ok((200, body)) = http(port, "GET", "/__bili/health")
                && let Ok(value) = serde_json::from_slice::<Value>(&body)
                    && value["ok"] == true {
                        break value;
                    }
            if let Some(code) = safe(cli.spawned.process.try_wait(), "observe actual CLI startup")?
            {
                drain(&mut cli.spawned.stdout, &mut cli.output)?;
                drain(&mut cli.spawned.stderr, &mut cli.errors)?;
                return Err(format!(
                    "canonical CLI exited before health: {}",
                    cli.diagnostics(Some(code), config.root().directory())
                ));
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        require(
            health["pid"].as_u64() == Some(cli.spawned.process.key().pid.into()),
            "loopback health belongs to a different process",
        )?;
        require(
            health["version"] == "0.1.207",
            "actual bundled CLI reports unexpected package version",
        )?;
        require(
            http(port, "GET", "/__bili/replicas")?.0 == 200,
            "actual CLI unified SQLite storage unavailable",
        )?;
        require(
            http(port, "POST", "/__bili/shutdown")?.0 == 202,
            "actual CLI graceful shutdown refused",
        )?;
        cli.wait_exit()?;
        Ok(
            json!({"health":true,"version":health["version"],"exactOwnedPidVerified":true,"gracefulShutdown":true}),
        )
    })();
    let cleanup = cli.cleanup(&db);
    let cli_result = cli_result?;
    cleanup?;
    let mut inspector = Child::start(
        fixture_command(&recipe, "inspect", config.root().directory())?,
        &coordinator,
        &db,
        &stamp,
        deadline,
    )?;
    let inspect_result: ProofResult<Value> = (|| {
        let value = inspector.phase("inspected")?;
        inspector.wait_exit()?;
        Ok(value)
    })();
    let cleanup = inspector.cleanup(&db);
    let inspection = inspect_result?;
    cleanup?;
    let mut source = Child::start(
        fixture_command(&recipe, "latency", config.root().directory())?,
        &coordinator,
        &db,
        &stamp,
        deadline,
    )?;
    let source_result: ProofResult<Value> = (|| {
        let build = source.phase("built")?;
        let census = safe(
            source.spawned.process.sample(&WindowsSampler::new()),
            "verify retained esbuild service census",
        )?;
        if !(census.membership_complete
            && (2..=8).contains(&census.active_processes)
            && census.total_processes == census.active_processes)
        {
            return Err(format!(
                "native esbuild service census: active={}, total={}, complete={}, observed_members={}",
                census.active_processes,
                census.total_processes,
                census.membership_complete,
                census.members.len()
            ));
        }
        source.instruction(b"census-complete\n")?;
        let ready = source.phase("ready")?;
        require(
            ready["unrelatedMessages"] == 200000,
            "unrelated-history scale is not 200000",
        )?;
        let server_port = ready["port"]
            .as_u64()
            .and_then(|value| u16::try_from(value).ok())
            .ok_or("invalid private fixture port")?;
        let listener = safe(
            TcpListener::bind(("127.0.0.1", 0)),
            "bind private echo target",
        )?;
        let echo_port = listener.local_addr().unwrap().port();
        let mut measurements = Vec::new();
        std::thread::scope(|scope| -> ProofResult<()> {
            let echo_worker = scope.spawn(move || echo(listener, deadline));
            let attempts: ProofResult<()> = (|| {
                for iteration in 0..3 {
                    source.instruction(b"save\n")?;
                    let saving = source.phase("saving")?;
                    require(
                        saving["iteration"] == iteration,
                        "unexpected persistence iteration",
                    )?;
                    let health = scope.spawn(move || measured_health(server_port));
                    let tunnel = scope.spawn(move || measured_connect(server_port, echo_port));
                    let health = safe(health.join(), "health probe panicked")??;
                    let tunnel = safe(tunnel.join(), "CONNECT probe panicked")??;
                    let saved = source.phase("saved")?;
                    require(
                        saved["iteration"] == iteration,
                        "unexpected saved iteration",
                    )?;
                    let save_start = saved["startedAtMs"]
                        .as_i64()
                        .ok_or("missing real save start")?;
                    let save_end = saved["endedAtMs"].as_i64().ok_or("missing real save end")?;
                    for probe in [&health, &tunnel] {
                        let probe_start = probe["startedAtMs"]
                            .as_i64()
                            .ok_or("missing real probe start")?;
                        require(
                            probe_start >= save_start && probe_start <= save_end,
                            "local probe did not overlap the actual synchronous save",
                        )?;
                    }
                    measurements.push(json!({"iteration":iteration,"saveMs":saved["saveMs"],"health":health,"connectEcho":tunnel}));
                }
                Ok(())
            })();
            // Listener is nonblocking and bounded even on an earlier probe error.
            let echoed = safe(echo_worker.join(), "echo worker panicked")?;
            attempts?;
            echoed
        })?;
        source.instruction(b"close\n")?;
        let complete = source.phase("complete")?;
        source.wait_exit()?;
        Ok(
            json!({"sourceBuild":build,"sourceStorage":ready,"checks":complete,"localMeasurements":measurements,
                "nativeCensusBeforeServiceStop":{"active":census.active_processes,"total":census.total_processes,"complete":census.membership_complete},
            "esbuildSameRestrictedJob":true,"esbuildCensusVerifiedBeforeStop":true}),
        )
    })();
    let source_result = source_result.map_err(|error| {
        format!(
            "{error}; synthetic source diagnostic: {}",
            source.diagnostics(
                source.spawned.process.try_wait().ok().flatten(),
                config.root().directory()
            )
        )
    });
    let cleanup = source.cleanup(&db);
    let source_result = source_result?;
    cleanup?;
    let mut tls = Child::start(
        fixture_command(&recipe, "tls", config.root().directory())?,
        &coordinator,
        &db,
        &stamp,
        deadline,
    )?;
    let tls_result: ProofResult<Value> = (|| {
        tls.phase("tls-cli-created")?;
        let cli_census = safe(
            tls.spawned.process.sample(&WindowsSampler::new()),
            "census inherited actual CLI before TLS",
        )?;
        require(
            cli_census.membership_complete && (2..=8).contains(&cli_census.active_processes),
            "actual CLI TLS job census incomplete",
        )?;
        tls.instruction(b"census-cli\n")?;
        let ready = tls.phase("tls-cli-ready")?;
        let mut cases = Vec::new();
        let mut censuses = Vec::new();
        for index in 0..4 {
            let created = tls.phase("tls-client-created")?;
            require(created["index"] == index, "unexpected TLS case index")?;
            let census = safe(
                tls.spawned.process.sample(&WindowsSampler::new()),
                "census paused TLS client before network",
            )?;
            require(
                census.membership_complete && (3..=8).contains(&census.active_processes),
                "TLS client escaped exact job census",
            )?;
            censuses.push(json!({"index":index,"active":census.active_processes,"total":census.total_processes,"complete":census.membership_complete}));
            tls.instruction(b"census-client\n")?;
            let result = tls.phase("tls-client-finished")?;
            require(result["index"] == index, "unexpected finished TLS index")?;
            if index % 2 == 0 {
                require(
                    result["certificateRejected"] == true && result["authorized"] == false,
                    "no-CA client did not specifically reject certificate",
                )?;
            } else {
                require(
                    result["authorized"] == true && result["responseMarkerVerified"] == true,
                    "private-CA client did not validate real HTTPS",
                )?;
            }
            cases.push(result);
        }
        let complete = tls.phase("tls-complete")?;
        tls.wait_exit()?;
        Ok(
            json!({"passed":true,"transport":"actual-pinned-cli-inherited-limited-job","cases":cases,"checks":complete,
            "runtimeConfiguration":ready["runtimeConfiguration"],"clientCensusBeforeTls":censuses,"hostileInheritedTlsSettingsDropped":true,
            "nodeChildCertificateTrustVerified":true,"otherAgentRuntimesQualified":false}),
        )
    })();
    let tls_result = tls_result.map_err(|error| {
        format!(
            "{error}; synthetic TLS diagnostic: {}",
            tls.diagnostics(
                tls.spawned.process.try_wait().ok().flatten(),
                config.root().directory()
            )
        )
    });
    let cleanup = tls.cleanup(&db);
    let tls_result = tls_result?;
    cleanup?;
    require(
        safe(db.resource_reservations(), "read released fixture ledger")?
            .unwrap()
            .slots
            .is_empty(),
        "fixture left persisted reserved resources",
    )?;
    Ok(
        json!({"passed":true,"platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,
        "logicalProcessors":std::thread::available_parallelism().map(|n|n.get()).ok(),"node":probe,
        "pinnedIdentity":recipe.summary(),"actualBundle":cli_result,"actualBundleStorage":inspection,
        "canonicalSourceStorage":source_result,"childTlsTrust":tls_result,"nativeLimits":budget(),"perProcessCommitBytes":768*MIB,
        "wallBudgetSeconds":30,"allNativeJobsClosedAndLedgerReleased":true,"nativeStartEnabled":false,"SC16":"open",
        "limits":["Source storage fixture is separately built; it is not a changed or rebuilt production CLI.",
            "Only Windows Node child TLS trust qualified; other runtimes, real providers, five-instance hour-long soak and other operating systems remain open.",
            "Running CLI exposes CCR/Absorb through its manifest; the rest of effective compression configuration remains unavailable."]}),
    )
}
