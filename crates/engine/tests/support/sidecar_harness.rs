#![allow(dead_code)] // Each separately compiled proof uses a different API subset.
//! Shared harness of the pinned-sidecar proofs: the sidecar started through the real owner
//! supervisor, the synthetic origin, Anthropic Messages helpers and `forward`.
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
use shinatama_hop::channel::{CONTROL_HEADER, ControlKind, bootstrap, control_header};
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

/// Error code of a request whose TCP connection to the sidecar was not accepted.
pub const CONNECT_FAILED: &str = "connect to sidecar";
pub type ProofResult<T> = Result<T, String>;
pub const MIB: u64 = 1024 * 1024;
/// Marker that opens every derived note the sidecar may add after a human message.
pub const NOTE_MARKER: &str = "<musashi_terminal_prompt_rewrite";
pub const REPLY: &str = "synthetic origin reply";

pub fn safe<T, E>(value: Result<T, E>, code: &'static str) -> ProofResult<T> {
    value.map_err(|_| code.into())
}
pub fn check(value: bool, code: &'static str) -> ProofResult<()> {
    if value { Ok(()) } else { Err(code.into()) }
}
pub fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .into()
}
pub fn budget() -> Budget {
    Budget {
        memory_bytes: 1024 * MIB,
        processes: 8,
        cpu_basis_points: 2000,
        queue_bytes: 256 * 1024,
        queue_items: 4,
    }
}

pub struct Fixture {
    pub root: PathBuf,
    pub released: bool,
}
impl Fixture {
    pub fn new() -> Self {
        Self {
            root: std::env::temp_dir().join(format!(
                "shinatama-sidecar-integrity-{}",
                uuid::Uuid::new_v4()
            )),
            released: false,
        }
    }
    pub fn location(&self) -> StorageLocation {
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

pub struct Provider {
    pub db: Arc<Database>,
    pub coordinator: Arc<AdmissionCoordinator>,
    pub stamp: PolicyStamp,
    pub calls: Arc<AtomicU32>,
    pub finished: mpsc::SyncSender<()>,
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
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}
pub fn dechunk(body: &[u8]) -> ProofResult<Vec<u8>> {
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
pub fn send_request(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, String)],
    body: &[u8],
) -> ProofResult<TcpStream> {
    let mut stream = safe(
        TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), Duration::from_secs(2)),
        CONNECT_FAILED,
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
pub fn read_response(mut stream: TcpStream, timeout: Duration) -> ProofResult<Response> {
    safe(
        stream.set_read_timeout(Some(timeout)),
        "bound response read",
    )?;
    let mut bytes = Vec::new();
    safe(stream.read_to_end(&mut bytes), "read response")?;
    parse_response(&bytes)
}
/// Splits a complete HTTP/1.1 response into status and (de-chunked) body.
pub fn parse_response(bytes: &[u8]) -> ProofResult<Response> {
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
pub fn request(
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
pub struct Running {
    pub supervisor: SidecarSupervisor,
    pub port: u16,
    pub node: PathBuf,
    pub db: Arc<Database>,
    pub closed: mpsc::Receiver<()>,
    pub _config: PreparedSidecarConfig,
    pub fixture: Fixture,
}
impl Running {
    pub fn start() -> ProofResult<Self> {
        Self::start_with_output_limit(1024 * 1024)
    }
    /// `start` with the per-stream output cap the owner enforces on the sidecar's
    /// stdout and stderr (the product's own is 64 MiB).
    pub fn start_with_output_limit(output_bytes_per_stream: u64) -> ProofResult<Self> {
        let fixture = Fixture::new();
        // A failed start leaves its evidence on disk: the fixture is not marked released.
        let (supervisor, port, node, db, closed, config) =
            Self::start_in(&fixture, output_bytes_per_stream)?;
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
    pub fn start_in(
        fixture: &Fixture,
        output_bytes_per_stream: u64,
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
            prepare_runtime(bundle, node, config.root(), &os, port, None),
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
                    output_bytes_per_stream,
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
    pub fn wait_status(
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
    pub fn health(&self) -> ProofResult<Value> {
        let response = request(self.port, "GET", "/__bili/health", &[], b"")?;
        check(response.status == 200, "sidecar health not ready")?;
        safe(
            serde_json::from_slice(&response.body),
            "invalid health JSON",
        )
    }
    /// Directory under the private fixture root for a test's own artifacts.
    pub fn scratch(&self, name: &str) -> PathBuf {
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
pub struct Origin {
    pub child: Child,
    pub port: u16,
    pub dir: PathBuf,
}
/// One request as the origin received it.
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub headers: Value,
    pub body: Vec<u8>,
}
pub fn is_model_call(r: &Recorded) -> bool {
    r.method == "POST"
        && r.path
            .split('?')
            .next()
            .is_some_and(|path| path.ends_with("/v1/messages"))
}
impl Origin {
    pub fn start(running: &Running, delay_ms: u64) -> ProofResult<Self> {
        Self::start_recording(running, delay_ms, true)
    }
    /// An origin that keeps nothing on disk: its `requests()` stay empty. For a long run
    /// whose clients check the replies themselves.
    pub fn start_unrecorded(running: &Running, delay_ms: u64) -> ProofResult<Self> {
        Self::start_recording(running, delay_ms, false)
    }
    fn start_recording(running: &Running, delay_ms: u64, record: bool) -> ProofResult<Self> {
        let dir = running.scratch(&format!("origin-{}", uuid::Uuid::new_v4()));
        let mut command = std::process::Command::new(&running.node);
        command
            .arg(workspace().join("crates/engine/tests/fixtures/origin.mjs"))
            .arg("--dir")
            .arg(&dir)
            .arg("--delay-ms")
            .arg(delay_ms.to_string());
        if !record {
            command.arg("--no-record");
        }
        let mut child = safe(
            command
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
    /// Every recorded request, in arrival order: model calls, catalog fetches, anything.
    /// For diagnosis only; a proof about model traffic counts `requests` / `bodies`.
    pub fn all_requests(&self) -> Vec<Recorded> {
        let mut found = Vec::new();
        for n in 1.. {
            let (Ok(meta), Ok(body)) = (
                fs::read(self.dir.join(format!("{n}.json"))),
                fs::read(self.dir.join(format!("{n}.body"))),
            ) else {
                break;
            };
            // The origin writes the metadata file in one call; a concurrent reader can still
            // catch it half written. That request is not recorded yet, so stop here.
            let Ok(meta) = serde_json::from_slice::<Value>(&meta) else {
                break;
            };
            found.push(Recorded {
                method: meta["method"].as_str().unwrap_or_default().into(),
                path: meta["path"].as_str().unwrap_or_default().into(),
                headers: meta["headers"].clone(),
                body,
            });
        }
        found
    }
    /// The model calls the origin received (`POST .../v1/messages`), in arrival order. A
    /// catalog fetch is not a model call: it carries no prompt and costs nothing.
    pub fn requests(&self) -> Vec<Recorded> {
        self.all_requests()
            .into_iter()
            .filter(is_model_call)
            .collect()
    }
    /// The body of every model call the origin received, auxiliary calls (a rewrite, a
    /// summary) included.
    pub fn bodies(&self) -> Vec<Vec<u8>> {
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
pub struct Msg {
    pub role: &'static str,
    pub content: Value,
}
impl Msg {
    pub fn user_text(text: &str) -> Self {
        Self {
            role: "user",
            content: json!([{ "type": "text", "text": text }]),
        }
    }
    pub fn assistant_text(text: &str) -> Self {
        Self {
            role: "assistant",
            content: json!([{ "type": "text", "text": text }]),
        }
    }
    pub fn assistant_tool_use(id: &str, name: &str, input: Value) -> Self {
        Self {
            role: "assistant",
            content: json!([{ "type": "tool_use", "id": id, "name": name, "input": input }]),
        }
    }
    pub fn tool_result(id: &str, text: &str) -> Self {
        Self {
            role: "user",
            content: json!([{ "type": "tool_result", "tool_use_id": id, "content": text }]),
        }
    }
    pub fn wire(&self) -> Value {
        json!({ "role": self.role, "content": self.content })
    }
}

/// What the sidecar forwarded for one probe request.
pub struct Forwarded {
    /// The messages of the probe request as the origin received them.
    pub messages: Vec<Value>,
    /// Every model call that reached the origin, the probe included.
    pub model_calls: usize,
    /// Everything that reached the origin besides the probe (extra model calls and catalog
    /// fetches alike), described for a diagnosis.
    pub others: Vec<String>,
}
pub fn message_text(message: &Value) -> Vec<String> {
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
pub fn is_note_text(text: &str) -> bool {
    text.starts_with(NOTE_MARKER)
}
/// A user message that holds nothing but derived rewrite notes.
pub fn is_note(message: &Value) -> bool {
    let texts = message_text(message);
    message["role"] == "user" && !texts.is_empty() && texts.iter().all(|t| is_note_text(t))
}

/// A fresh agent session id, as the destination agent would send in its session header.
pub fn new_session() -> String {
    uuid::Uuid::new_v4().to_string()
}
/// One probe request ready to send: where to, which headers, the body, and the per-call
/// tag that singles the probe out among everything the origin records.
pub struct Probe {
    pub path: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
    pub tag: String,
}
pub fn probe(
    origin: &Origin,
    session: &str,
    history: &[Msg],
    user_text: &str,
    tools: Option<&Value>,
) -> Probe {
    let mut messages: Vec<Value> = history.iter().map(Msg::wire).collect();
    messages.push(Msg::user_text(user_text).wire());
    probe_messages(origin, session, messages, tools)
}
pub fn probe_messages(
    origin: &Origin,
    session: &str,
    messages: Vec<Value>,
    tools: Option<&Value>,
) -> Probe {
    probe_messages_with(origin, session, messages, tools, None)
}
/// `probe_messages` that may carry a control header (`x-shinatama-control`), built by the same
/// code Shinatama uses (`shinatama_hop::channel::control_header`).
pub fn probe_messages_with(
    origin: &Origin,
    session: &str,
    messages: Vec<Value>,
    tools: Option<&Value>,
    control: Option<(&'static str, String)>,
) -> Probe {
    let tag = uuid::Uuid::new_v4().to_string();
    let mut body = json!({
        "model": "claude-sonnet-4-5",
        "max_tokens": 1024,
        "stream": true,
        "system": [{ "type": "text", "text": "You are Claude Code, Anthropic's official CLI for Claude." }],
        "messages": messages,
    });
    if let Some(tools) = tools {
        body["tools"] = tools.clone();
    }
    let body = serde_json::to_vec(&body).unwrap();
    let mut headers = vec![
        ("content-type", "application/json".into()),
        ("anthropic-version", "2023-06-01".into()),
        ("x-api-key", "sk-ant-synthetic-not-a-credential".into()),
        ("x-app", "cli".into()),
        ("user-agent", "claude-cli/2.0.0 (external, cli)".into()),
        ("x-claude-code-session-id", session.to_owned()),
        ("x-sc9-probe", tag.clone()),
    ];
    headers.extend(control);
    Probe {
        path: format!("/bili/http://127.0.0.1:{}/v1/messages", origin.port),
        headers,
        body,
        tag,
    }
}
/// The sidecar copies the client's headers onto its own auxiliary requests, so the tag
/// alone does not single out the probe: it is the tagged streaming Messages request.
pub fn is_probe(r: &Recorded, tag: &str) -> bool {
    r.headers["x-sc9-probe"] == tag
        && r.method == "POST"
        && r.path.ends_with("/v1/messages")
        && serde_json::from_slice::<Value>(&r.body).is_ok_and(|b| b["stream"] == true)
}
/// Sends the history followed by `user_text` as one Messages request, on `session`, through
/// the sidecar listening on `sidecar_port` to `origin`, and returns what the origin
/// received for it. It holds no `Running`, so concurrent sessions can call it from threads
/// and a session can reconnect by passing the same id again.
pub fn forward(
    sidecar_port: u16,
    origin: &Origin,
    session: &str,
    history: &[Msg],
    user_text: &str,
) -> ProofResult<Forwarded> {
    forward_with(sidecar_port, origin, session, history, user_text, None)
}
/// `forward` with the client's `tools` array, as the destination agent sends it.
pub fn forward_with(
    sidecar_port: u16,
    origin: &Origin,
    session: &str,
    history: &[Msg],
    user_text: &str,
    tools: Option<&Value>,
) -> ProofResult<Forwarded> {
    let mut messages: Vec<Value> = history.iter().map(Msg::wire).collect();
    messages.push(Msg::user_text(user_text).wire());
    forward_messages(sidecar_port, origin, session, messages, tools)
}
/// `forward` for a request whose messages the caller built, so the newest message can be
/// something other than text (a tool result answering a tool use, as an agent sends it).
pub fn forward_messages(
    sidecar_port: u16,
    origin: &Origin,
    session: &str,
    messages: Vec<Value>,
    tools: Option<&Value>,
) -> ProofResult<Forwarded> {
    forward_messages_with(sidecar_port, origin, session, messages, tools, None)
}
/// `forward` for a message Shinatama declares as a control message: the request carries the
/// `x-shinatama-control` header with the hash of `user_text`.
pub fn forward_control(
    sidecar_port: u16,
    origin: &Origin,
    session: &str,
    history: &[Msg],
    user_text: &str,
    kind: ControlKind,
) -> ProofResult<Forwarded> {
    forward_with_header(sidecar_port, origin, session, history, user_text, control_header(kind, user_text))
}
/// `forward` with any control header, including one whose hash does not belong to `user_text`.
pub fn forward_with_header(
    sidecar_port: u16,
    origin: &Origin,
    session: &str,
    history: &[Msg],
    user_text: &str,
    header: (&'static str, String),
) -> ProofResult<Forwarded> {
    let mut messages: Vec<Value> = history.iter().map(Msg::wire).collect();
    messages.push(Msg::user_text(user_text).wire());
    forward_messages_with(sidecar_port, origin, session, messages, None, Some(header))
}
pub fn forward_messages_with(
    sidecar_port: u16,
    origin: &Origin,
    session: &str,
    messages: Vec<Value>,
    tools: Option<&Value>,
    control: Option<(&'static str, String)>,
) -> ProofResult<Forwarded> {
    let probe = probe_messages_with(origin, session, messages, tools, control);
    let tag = probe.tag.as_str();
    let response = request(
        sidecar_port,
        "POST",
        &probe.path,
        &probe.headers,
        &probe.body,
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
    let recorded = origin.all_requests();
    // Shinatama's marker is internal to the sidecar: no request, marked or not, carries it onward.
    check(
        recorded
            .iter()
            .all(|r| r.headers.get(CONTROL_HEADER).is_none()),
        "the control marker reached the synthetic origin",
    )?;
    let sent_probe = recorded
        .iter()
        .find(|r| is_probe(r, tag))
        .ok_or("the probe request never reached the synthetic origin")?;
    let sent: Value = safe(
        serde_json::from_slice(&sent_probe.body),
        "probe body at origin is not JSON",
    )?;
    let messages = sent["messages"]
        .as_array()
        .ok_or("probe body at origin has no messages")?
        .clone();
    let others = recorded
        .iter()
        .filter(|r| !is_probe(r, tag))
        .map(|r| format!("{} {} ({} body bytes)", r.method, r.path, r.body.len()))
        .collect();
    Ok(Forwarded {
        messages,
        others,
        model_calls: recorded.iter().filter(|r| is_model_call(r)).count(),
    })
}
pub fn truncate(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}
/// The human message the origin received: the last user message that is not a derived note.
pub fn human_message(forwarded: &Forwarded) -> ProofResult<(usize, &Value)> {
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
pub fn last_user_text_at_origin(
    sidecar: &Running,
    origin: &Origin,
    history: &[Msg],
    user_text: &str,
) -> String {
    last_user_text_at_origin_as(sidecar, origin, history, user_text, ControlKind::Bootstrap)
}
/// `last_user_text_at_origin` for a message sent with the control header of `kind`.
pub fn last_user_text_at_origin_as(
    sidecar: &Running,
    origin: &Origin,
    history: &[Msg],
    user_text: &str,
    kind: ControlKind,
) -> String {
    let forwarded = forward_control(sidecar.port, origin, &new_session(), history, user_text, kind)
        .expect("forward the probe");
    let (_, message) = human_message(&forwarded).expect("human message");
    message_text(message).join("\n<<next block>>\n")
}

pub fn proof() -> (Running, Origin) {
    let sidecar = Running::start().expect("start the pinned sidecar");
    let origin = Origin::start(&sidecar, 0).expect("start the synthetic origin");
    assert!(
        sidecar.health().expect("health")["pid"].is_u64(),
        "the sidecar must be the pinned CLI"
    );
    (sidecar, origin)
}
pub fn handoff_text() -> String {
    bootstrap(
        "n-1",
        Path::new("C:\\synthetic\\seed.md"),
        Some("Continua il lavoro."),
    )
}
/// The one tool of the synthetic session, in the Messages `tools` shape. A client that
/// declares tools is what arms the sidecar's content store (CCR) and its retrieve tool.
pub fn read_tool() -> Value {
    json!([{
        "name": "Read",
        "description": "Reads a file from the local filesystem.",
        "input_schema": {
            "type": "object",
            "properties": { "file_path": { "type": "string" } },
            "required": ["file_path"],
        },
    }])
}
