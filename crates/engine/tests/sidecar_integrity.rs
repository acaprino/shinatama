#![cfg(windows)]
//! SC9: control messages pass through the pinned sidecar byte-identical. A synthetic
//! local origin (fixtures/origin.mjs) stands in for the model provider: the sidecar's
//! only upstream is that loopback server, so no paid call can happen. The test client
//! sends an Anthropic Messages request through the running sidecar and reads back, from
//! the origin's own recording, what the sidecar actually forwarded.
//!
//! The harness below (Running, Origin, Msg, last_user_text_at_origin, Origin::bodies) is
//! meant to be reused by the session-isolation, drain and soak proofs.
use serde_json::{Value, json};
use shinatama_engine::{
    resources::{
        admission::{AdmissionCoordinator, PrepareRequest, PreparedWorkload},
        sampling::{Measurement, WindowsSampler},
    },
    sidecar_config::{PreparedSidecarConfig, prepare_private_sidecar},
    sidecar_runtime::{NodePin, prepare_runtime, verify_bundled_node, verify_prepared_bundle},
    sidecar_supervisor::{
        AdmissionUnavailable, Command, Phase, SidecarSupervisor, Snapshot, SupervisorConfig,
        WorkloadProvider,
    },
};
use shinatama_hop::channel::bootstrap;
use shinatama_state::{
    database::Database,
    location::{LocationRequest, StorageLocation},
    resource_reservations::{Budget, PolicyStamp, ReservationPolicy},
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    num::NonZeroU16,
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

type ProofResult<T> = Result<T, String>;
const MIB: u64 = 1024 * 1024;
/// Marker that opens every derived note the sidecar may add after a human message.
const NOTE_MARKER: &str = "<musashi_terminal_prompt_rewrite";
const REPLY: &str = "synthetic origin reply";

fn safe<T, E>(value: Result<T, E>, code: &'static str) -> ProofResult<T> {
    value.map_err(|_| code.into())
}
fn check(value: bool, code: &'static str) -> ProofResult<()> {
    if value { Ok(()) } else { Err(code.into()) }
}
fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .into()
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
                "shinatama-sidecar-integrity-{}",
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
        let parent = std::env::temp_dir();
        if self.released
            && self.root.parent() == Some(parent.as_path())
            && self
                .root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("shinatama-sidecar-integrity-")
        {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

struct Provider {
    db: Arc<Database>,
    coordinator: Arc<AdmissionCoordinator>,
    stamp: PolicyStamp,
    calls: Arc<AtomicU32>,
    finished: mpsc::SyncSender<()>,
}
impl WorkloadProvider for Provider {
    fn database(&self) -> &Database {
        &self.db
    }
    fn prepare(&mut self, at: i64, until: i64) -> Result<PreparedWorkload, AdmissionUnavailable> {
        // A proof never needs a restart: one start, and a finite ceiling if it misbehaves.
        if self.calls.fetch_add(1, Ordering::AcqRel) >= 2 {
            return Err(AdmissionUnavailable);
        }
        let sampled = WindowsSampler::new()
            .sample(&[])
            .map_err(|_| AdmissionUnavailable)?;
        let Measurement::Available(system) = sampled.system else {
            return Err(AdmissionUnavailable);
        };
        if system.physical_available_bytes < budget().memory_bytes + 512 * MIB
            || system
                .commit_limit_bytes
                .saturating_sub(system.commit_total_bytes)
                < budget().memory_bytes + 1024 * MIB
        {
            return Err(AdmissionUnavailable);
        }
        let revision = self
            .db
            .resource_reservations()
            .map_err(|_| AdmissionUnavailable)?
            .ok_or(AdmissionUnavailable)?
            .revision;
        self.coordinator
            .prepare(
                &self.db,
                &self.stamp,
                revision,
                PrepareRequest {
                    budget: budget(),
                    process_commit_bytes: 768 * MIB,
                    now_ms: at,
                    lease_expires_at_ms: until,
                },
            )
            .map_err(|_| AdmissionUnavailable)
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        let _ = self.finished.try_send(());
    }
}

/// One HTTP response, body already de-chunked.
struct Response {
    status: u16,
    body: Vec<u8>,
}
fn dechunk(body: &[u8]) -> ProofResult<Vec<u8>> {
    let mut decoded = Vec::new();
    let mut cursor = 0;
    loop {
        let end = body[cursor..]
            .windows(2)
            .position(|part| part == b"\r\n")
            .ok_or("invalid chunk")?
            + cursor;
        let size = std::str::from_utf8(&body[cursor..end]).map_err(|_| "invalid chunk length")?;
        let size = usize::from_str_radix(size.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| "invalid chunk length")?;
        cursor = end + 2;
        if size == 0 {
            return Ok(decoded);
        }
        check(cursor + size + 2 <= body.len(), "truncated chunk")?;
        decoded.extend_from_slice(&body[cursor..cursor + size]);
        cursor += size + 2;
    }
}
/// Sends the request and returns the open connection; the caller decides how to read it.
fn send_request(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, String)],
    body: &[u8],
) -> ProofResult<TcpStream> {
    let mut stream = safe(
        TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), Duration::from_secs(2)),
        "connect to sidecar",
    )?;
    safe(
        stream.set_write_timeout(Some(Duration::from_secs(30))),
        "bound request write",
    )?;
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    safe(stream.write_all(head.as_bytes()), "write request head")?;
    safe(stream.write_all(body), "write request body")?;
    Ok(stream)
}
fn read_response(mut stream: TcpStream, timeout: Duration) -> ProofResult<Response> {
    safe(
        stream.set_read_timeout(Some(timeout)),
        "bound response read",
    )?;
    let mut bytes = Vec::new();
    safe(stream.read_to_end(&mut bytes), "read response")?;
    let split = bytes
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .ok_or("invalid response framing")?;
    let head = String::from_utf8_lossy(&bytes[..split]).to_ascii_lowercase();
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or("invalid status line")?;
    let body = &bytes[split + 4..];
    let body = if head.contains("transfer-encoding: chunked") {
        dechunk(body)?
    } else {
        body.to_vec()
    };
    Ok(Response { status, body })
}
fn request(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, String)],
    body: &[u8],
) -> ProofResult<Response> {
    read_response(
        send_request(port, method, path, headers, body)?,
        Duration::from_secs(60),
    )
}

/// The pinned sidecar, started through the real owner supervisor exactly as the product
/// does, with the sidecar's only upstream being whatever origin a test points it at.
struct Running {
    supervisor: SidecarSupervisor,
    port: u16,
    node: PathBuf,
    db: Arc<Database>,
    closed: mpsc::Receiver<()>,
    _config: PreparedSidecarConfig,
    fixture: Fixture,
}
impl Running {
    fn start() -> ProofResult<Self> {
        let fixture = Fixture::new();
        // A failed start leaves its evidence on disk: the fixture is not marked released.
        let (supervisor, port, node, db, closed, config) = Self::start_in(&fixture)?;
        Ok(Self {
            supervisor,
            port,
            node,
            db,
            closed,
            _config: config,
            fixture,
        })
    }
    #[allow(clippy::type_complexity)]
    fn start_in(
        fixture: &Fixture,
    ) -> ProofResult<(
        SidecarSupervisor,
        u16,
        PathBuf,
        Arc<Database>,
        mpsc::Receiver<()>,
        PreparedSidecarConfig,
    )> {
        let location = fixture.location();
        let db = Arc::new(safe(
            Database::open(&location, None),
            "open private supervisor ledger",
        )?);
        let reserved = safe(
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0)),
            "reserve private sidecar port",
        )?;
        let port = NonZeroU16::new(reserved.local_addr().unwrap().port()).unwrap();
        drop(reserved);
        let config = safe(
            prepare_private_sidecar(&location, port),
            "prepare private sidecar",
        )?;
        let bundle = safe(
            verify_prepared_bundle(&workspace().join("sidecar/source-build/prepared")),
            "verify source and CLI pin",
        )?;
        let node_root = workspace().join("sidecar/runtime");
        let node = safe(
            verify_bundled_node(
                &node_root,
                NodePin {
                    relative_path: "node.exe".into(),
                    sha256: "58e74bf02fc5bbacc41dcb8bef089961cd5bddd37830b87784e4fc624d145d1f"
                        .into(),
                    version: Some("24.14.1".into()),
                    modules_abi: Some(137),
                },
            ),
            "verify Node pin",
        )?;
        let os: BTreeMap<OsString, OsString> = ["SystemRoot", "WINDIR"]
            .into_iter()
            .filter_map(|key| std::env::var_os(key).map(|value| (key.into(), value)))
            .collect();
        let recipe = safe(
            prepare_runtime(bundle, node, config.root(), &os, port),
            "prepare verified child recipe",
        )?;
        let stamp = PolicyStamp {
            version: 1,
            id: "sidecar-integrity-proof-v1".into(),
        };
        safe(
            db.initialize_resource_policy(&ReservationPolicy {
                stamp: stamp.clone(),
                aggregate_ceiling: budget(),
                per_slot_ceiling: budget(),
                max_slots: 1,
                max_lease_ms: 30_000,
            }),
            "install frozen fixture policy",
        )?;
        let coordinator = Arc::new(safe(
            AdmissionCoordinator::new(1),
            "create fixed fixture slot",
        )?);
        let (finished, closed) = mpsc::sync_channel(1);
        let provider = Provider {
            db: db.clone(),
            coordinator,
            stamp,
            calls: Arc::new(AtomicU32::new(0)),
            finished,
        };
        let supervisor = safe(
            SidecarSupervisor::start(
                recipe,
                SupervisorConfig {
                    port,
                    poll_period: Duration::from_millis(50),
                    health_period: Duration::from_millis(500),
                    health_timeout: Duration::from_secs(2),
                    startup_timeout: Duration::from_secs(20),
                    graceful_stop_timeout: Duration::from_secs(2),
                    cleanup_timeout: Duration::from_secs(5),
                    reservation_lease: Duration::from_secs(30),
                    drain_timeout: Duration::from_secs(30),
                    output_bytes_per_stream: 1024 * 1024,
                },
                provider,
            ),
            "start sole owner worker",
        )?;
        safe(supervisor.request(Command::Start), "request sidecar start")?;
        let node = workspace().join("sidecar/runtime/node.exe");
        let running = Self::wait_status(&supervisor, Duration::from_secs(60), Phase::Ready, |s| {
            s.healthy
        });
        if let Err(error) = running {
            let _ = supervisor.request(Command::Stop);
            return Err(error);
        }
        Ok((supervisor, port.get(), node, db, closed, config))
    }
    fn wait_status(
        supervisor: &SidecarSupervisor,
        within: Duration,
        expected: Phase,
        condition: impl Fn(&Snapshot) -> bool,
    ) -> ProofResult<Snapshot> {
        let deadline = Instant::now() + within;
        loop {
            check(Instant::now() < deadline, "supervisor observation deadline")?;
            if let Some(snapshot) = supervisor.status() {
                if snapshot.phase == expected && condition(&snapshot) {
                    return Ok(snapshot);
                }
                check(
                    snapshot.phase != Phase::Retired,
                    "supervisor retained uncertain resources",
                )?;
                if snapshot.phase == Phase::Failed && expected != Phase::Stopped {
                    return Err(format!(
                        "supervisor failed before expected state: {:?}",
                        snapshot.reason
                    ));
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn health(&self) -> ProofResult<Value> {
        let response = request(self.port, "GET", "/__bili/health", &[], b"")?;
        check(response.status == 200, "sidecar health not ready")?;
        safe(
            serde_json::from_slice(&response.body),
            "invalid health JSON",
        )
    }
    /// Directory under the private fixture root for a test's own artifacts.
    fn scratch(&self, name: &str) -> PathBuf {
        self.fixture.root.join(name)
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.supervisor.request(Command::Stop);
        let stopped = Self::wait_status(
            &self.supervisor,
            Duration::from_secs(30),
            Phase::Stopped,
            |s| s.phase == Phase::Stopped,
        );
        let empty =
            matches!(self.db.resource_reservations(), Ok(Some(ledger)) if ledger.slots.is_empty());
        if stopped.is_ok() && empty && !thread::panicking() {
            self.fixture.released = true;
        }
        let _ = &self.closed;
    }
}

/// The synthetic origin: fixtures/origin.mjs under the pinned Node, on a system-chosen port.
struct Origin {
    child: Child,
    port: u16,
    dir: PathBuf,
}
/// One request as the origin received it.
struct Recorded {
    method: String,
    path: String,
    headers: Value,
    body: Vec<u8>,
}
impl Origin {
    fn start(running: &Running, delay_ms: u64) -> ProofResult<Self> {
        let dir = running.scratch(&format!("origin-{}", uuid::Uuid::new_v4()));
        let mut child = safe(
            std::process::Command::new(&running.node)
                .arg(workspace().join("crates/engine/tests/fixtures/origin.mjs"))
                .arg("--dir")
                .arg(&dir)
                .arg("--delay-ms")
                .arg(delay_ms.to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn(),
            "start synthetic origin",
        )?;
        let stdout = child.stdout.take().ok_or("synthetic origin stdout")?;
        let (sender, line) = mpsc::channel();
        thread::spawn(move || {
            let mut first = String::new();
            let _ = BufReader::new(stdout).read_line(&mut first);
            let _ = sender.send(first);
        });
        let announced = line.recv_timeout(Duration::from_secs(10));
        let port = announced
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|value| value["port"].as_u64())
            .and_then(|port| u16::try_from(port).ok());
        let Some(port) = port else {
            let _ = child.kill();
            return Err("synthetic origin did not announce a port".into());
        };
        Ok(Self { child, port, dir })
    }
    /// Every recorded request, in arrival order.
    fn requests(&self) -> Vec<Recorded> {
        let mut found = Vec::new();
        for n in 1.. {
            let (Ok(meta), Ok(body)) = (
                fs::read(self.dir.join(format!("{n}.json"))),
                fs::read(self.dir.join(format!("{n}.body"))),
            ) else {
                break;
            };
            let meta: Value = serde_json::from_slice(&meta).expect("origin metadata is JSON");
            found.push(Recorded {
                method: meta["method"].as_str().unwrap_or_default().into(),
                path: meta["path"].as_str().unwrap_or_default().into(),
                headers: meta["headers"].clone(),
                body,
            });
        }
        found
    }
    /// The body of every request the origin saw, auxiliary calls included.
    fn bodies(&self) -> Vec<Vec<u8>> {
        self.requests().into_iter().map(|r| r.body).collect()
    }
}
impl Drop for Origin {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One message of an Anthropic Messages history.
#[derive(Clone)]
struct Msg {
    role: &'static str,
    content: Value,
}
impl Msg {
    fn user_text(text: &str) -> Self {
        Self {
            role: "user",
            content: json!([{ "type": "text", "text": text }]),
        }
    }
    fn assistant_text(text: &str) -> Self {
        Self {
            role: "assistant",
            content: json!([{ "type": "text", "text": text }]),
        }
    }
    fn assistant_tool_use(id: &str, name: &str, input: Value) -> Self {
        Self {
            role: "assistant",
            content: json!([{ "type": "tool_use", "id": id, "name": name, "input": input }]),
        }
    }
    fn tool_result(id: &str, text: &str) -> Self {
        Self {
            role: "user",
            content: json!([{ "type": "tool_result", "tool_use_id": id, "content": text }]),
        }
    }
    fn wire(&self) -> Value {
        json!({ "role": self.role, "content": self.content })
    }
}

/// What the sidecar forwarded for one probe request.
struct Forwarded {
    /// The messages of the probe request as the origin received them.
    messages: Vec<Value>,
    /// Every request that reached the origin, the probe included.
    total_requests: usize,
    /// The requests that reached the origin besides the probe, described for a diagnosis.
    others: Vec<String>,
}
fn message_text(message: &Value) -> Vec<String> {
    match &message["content"] {
        Value::String(text) => vec![text.clone()],
        Value::Array(parts) => parts
            .iter()
            .filter(|part| part["type"] == "text")
            .filter_map(|part| part["text"].as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}
fn is_note(message: &Value) -> bool {
    message["role"] == "user"
        && message_text(message)
            .first()
            .is_some_and(|text| text.starts_with(NOTE_MARKER))
}

/// Sends the history followed by `user_text` as one Messages request through the running
/// sidecar to `origin`, and returns what the origin received for it.
fn forward(
    sidecar: &Running,
    origin: &Origin,
    history: &[Msg],
    user_text: &str,
) -> ProofResult<Forwarded> {
    let tag = uuid::Uuid::new_v4().to_string();
    let mut messages: Vec<Value> = history.iter().map(Msg::wire).collect();
    messages.push(Msg::user_text(user_text).wire());
    let body = serde_json::to_vec(&json!({
        "model": "claude-sonnet-4-5",
        "max_tokens": 1024,
        "stream": true,
        "system": [{ "type": "text", "text": "You are Claude Code, Anthropic's official CLI for Claude." }],
        "messages": messages,
    }))
    .unwrap();
    let response = request(
        sidecar.port,
        "POST",
        &format!("/bili/http://127.0.0.1:{}/v1/messages", origin.port),
        &[
            ("content-type", "application/json".into()),
            ("anthropic-version", "2023-06-01".into()),
            ("x-api-key", "sk-ant-synthetic-not-a-credential".into()),
            ("x-app", "cli".into()),
            ("user-agent", "claude-cli/2.0.0 (external, cli)".into()),
            ("x-claude-code-session-id", uuid::Uuid::new_v4().to_string()),
            ("x-sc9-probe", tag.clone()),
        ],
        &body,
    )?;
    let text = String::from_utf8_lossy(&response.body).into_owned();
    check(
        response.status == 200,
        "sidecar did not answer the probe with 200",
    )
    .map_err(|e| {
        format!(
            "{e}: status {} body {}",
            response.status,
            truncate(&text, 400)
        )
    })?;
    check(
        text.contains(REPLY),
        "probe reply did not come from the synthetic origin",
    )?;
    let recorded = origin.requests();
    // The sidecar copies the client's headers onto its own auxiliary requests, so the tag
    // alone does not single out the probe: it is the tagged streaming Messages request.
    let is_probe = |r: &Recorded| {
        r.headers["x-sc9-probe"] == tag.as_str()
            && r.method == "POST"
            && r.path.ends_with("/v1/messages")
            && serde_json::from_slice::<Value>(&r.body).is_ok_and(|b| b["stream"] == true)
    };
    let probe = recorded
        .iter()
        .find(|r| is_probe(r))
        .ok_or("the probe request never reached the synthetic origin")?;
    let sent: Value = safe(
        serde_json::from_slice(&probe.body),
        "probe body at origin is not JSON",
    )?;
    let messages = sent["messages"]
        .as_array()
        .ok_or("probe body at origin has no messages")?
        .clone();
    let others = recorded
        .iter()
        .filter(|r| !is_probe(r))
        .map(|r| format!("{} {} ({} body bytes)", r.method, r.path, r.body.len()))
        .collect();
    Ok(Forwarded {
        messages,
        others,
        total_requests: recorded.len(),
    })
}
fn truncate(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}
/// The human message the origin received: the last user message that is not a derived note.
fn human_message(forwarded: &Forwarded) -> ProofResult<(usize, &Value)> {
    forwarded
        .messages
        .iter()
        .enumerate()
        .rev()
        .find(|(_, message)| message["role"] == "user" && !is_note(message))
        .ok_or_else(|| "no human message at origin".into())
}

/// Sends `user_text` as the last user message of an Anthropic Messages request through
/// the running sidecar and returns the bytes of that message as the origin received them.
/// When the message reached the origin split into several text blocks the blocks are
/// returned joined by a visible separator, so an equality check against `user_text` fails
/// and shows the split.
fn last_user_text_at_origin(
    sidecar: &Running,
    origin: &Origin,
    history: &[Msg],
    user_text: &str,
) -> String {
    let forwarded = forward(sidecar, origin, history, user_text).expect("forward the probe");
    let (_, message) = human_message(&forwarded).expect("human message");
    message_text(message).join("\n<<next block>>\n")
}

fn proof() -> (Running, Origin) {
    let sidecar = Running::start().expect("start the pinned sidecar");
    let origin = Origin::start(&sidecar, 0).expect("start the synthetic origin");
    assert!(
        sidecar.health().expect("health")["pid"].is_u64(),
        "the sidecar must be the pinned CLI"
    );
    (sidecar, origin)
}
fn handoff_text() -> String {
    bootstrap(
        "n-1",
        Path::new("C:\\synthetic\\seed.md"),
        Some("Continua il lavoro."),
    )
}

// SC9 outcome against the pinned sidecar (all four fail, assertions intact; Task 12 owns the fix):
// - the human message reaches the origin as `<acp tokens="N" type="text">mNNNNN</acp>` and a newline
//   before the original text: a reference tag is prepended inside the same text block;
// - the first destination turn makes the sidecar fetch the model catalog (GET /v1/models) and
//   send the control message to a lightweight model for rewriting (a second POST) before the
//   probe itself is forwarded.
#[test]
#[ignore = "SC9 open: see Task 12"]
fn bootstrap_reaches_origin_byte_identical() {
    let (sidecar, origin) = proof();
    let text = handoff_text();
    assert_eq!(
        last_user_text_at_origin(&sidecar, &origin, &[], &text),
        text
    );
}

/// A conversation whose tool results are each larger than the 4000-token compression
/// threshold and together sum to about 150_000 estimated tokens (four bytes per token),
/// just under the 160_000 adaptive-pressure ceiling of the baseline.
fn long_history() -> Vec<Msg> {
    const RESULTS: usize = 30;
    const TOKENS_EACH: usize = 5_000;
    let mut seed = 0x2545_f491_u64;
    let mut word = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        format!("{:x}", seed >> 24)
    };
    let mut history = vec![
        Msg::user_text("Analizza il repository e riassumi cosa trovi file per file."),
        Msg::assistant_text("Parto leggendo i file uno alla volta."),
    ];
    for index in 0..RESULTS {
        let id = format!("toolu_{index:04}");
        let mut result = format!("file {index}\n");
        while result.len() < TOKENS_EACH * 4 {
            result.push_str(&word());
            result.push(' ');
        }
        history.push(Msg::assistant_tool_use(
            &id,
            "Read",
            json!({ "file_path": format!("C:\\synthetic\\src\\file{index}.rs") }),
        ));
        history.push(Msg::tool_result(&id, &result));
    }
    history.push(Msg::assistant_text("Ho letto tutti i file."));
    history
}

#[test]
#[ignore = "SC9 open: see Task 12"]
fn bootstrap_is_untouched_with_history_near_the_limit() {
    let history = long_history();
    let tokens: usize = history
        .iter()
        .map(|m| m.content.to_string().len() / 4)
        .sum();
    assert!(
        (140_000..160_000).contains(&tokens),
        "history must sit near the limit, estimated {tokens}"
    );
    let (sidecar, origin) = proof();
    let text = handoff_text();
    assert_eq!(
        last_user_text_at_origin(&sidecar, &origin, &history, &text),
        text
    );
}

#[test]
#[ignore = "SC9 open: see Task 12"]
fn first_destination_turn_sends_no_auxiliary_request() {
    let (sidecar, origin) = proof();
    let text = handoff_text();
    let forwarded = forward(&sidecar, &origin, &[], &text).expect("forward the first turn");
    assert_eq!(
        forwarded.total_requests, 1,
        "requests besides the probe: {:?}",
        forwarded.others
    );
    // A late auxiliary call would still be on its way: give it the chance to arrive.
    thread::sleep(Duration::from_millis(1500));
    // origin directory holds exactly one body after the turn: no rewrite call, no summary call
    assert_eq!(origin.bodies().len(), 1);
    let only = &origin.requests()[0];
    assert_eq!(only.method, "POST");
    assert!(only.path.ends_with("/v1/messages"), "{}", only.path);
}

const DICTATED: &str = "Vorrei che sistemassi il calcolo dei totali nel report mensile, ma non toccare per nessun motivo la parte sugli arrotondamenti, perché i clienti si aspettano sempre 2 decimali; poi dimmi quante righe hai cambiato e se i test esistenti passano.";

#[test]
#[ignore = "SC9 open: see Task 12"]
fn dictated_prompt_is_kept_and_any_rewrite_note_is_additive() {
    assert_eq!(DICTATED.split_whitespace().count(), 40);
    assert!(DICTATED.contains("non toccare") && DICTATED.contains("2 decimali"));
    let (sidecar, origin) = proof();
    let history = [
        Msg::user_text("Ciao, lavoriamo sul report mensile."),
        Msg::assistant_text("Va bene, dimmi cosa serve."),
    ];
    let forwarded = forward(&sidecar, &origin, &history, DICTATED).expect("forward dictation");
    let (at, human) = human_message(&forwarded).expect("human message");
    // The original text is present byte-identical, as the one text block of its own message.
    assert_eq!(message_text(human), vec![DICTATED.to_owned()]);
    // Everything the sidecar added is a derived note: a separate message directly after the
    // original, never text spliced into it, and it says the original stays authoritative.
    let notes: Vec<&Value> = forwarded.messages.iter().filter(|m| is_note(m)).collect();
    for note in &notes {
        let text = message_text(note).concat();
        assert!(
            text.contains("The original user message remains authoritative"),
            "{text}"
        );
    }
    let after: Vec<&Value> = forwarded.messages[at + 1..].iter().collect();
    assert!(
        after.iter().all(|m| is_note(m)),
        "only derived notes may follow the original"
    );
    assert_eq!(
        after.len(),
        notes.len(),
        "no note may sit anywhere but after the original"
    );
    // The history keeps its messages; the sidecar adds nothing before the original.
    assert_eq!(forwarded.messages[..at].len(), history.len());
}
