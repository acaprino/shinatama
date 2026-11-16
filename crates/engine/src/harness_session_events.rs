//! Shared private command-hook transport for supported native harnesses.
//! Routes and payload dialects are fixed by the trusted harness driver.
//! Accepted means queued, never durably committed or authorized to acquire a root.
use crate::launch::identity::{LaunchIdentityRecord, verify_record};
use serde::{Deserialize, Serialize};
use shinatama_state::records::{CanonicalIdKind, validate_id};
use std::{
    io::{self, Read, Write},
    net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread::{self, Thread},
    time::{Duration, Instant},
};

pub const MAX_HEADER_BYTES: usize = 4096;
pub const MAX_BODY_BYTES: usize = 16 * 1024;
pub const MAX_CWD_BYTES: usize = 4096;
pub const EVENT_QUEUE_CAPACITY: usize = 32;
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
const RESPONSE_TIMEOUT: Duration = Duration::from_millis(250);
/// A browser tool call (ADR-0056) can take seconds and answer with a screenshot:
/// it runs off the accept loop, at most this many at once across receivers.
const MAX_BROWSER_CALLS: usize = 4;
const BROWSER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
static BROWSER_CALLS: AtomicUsize = AtomicUsize::new(0);
const IO_POLL: Duration = Duration::from_millis(10);
const MAX_HEADERS: usize = 32;
const MAX_RECEIVERS: usize = 16;
static RECEIVERS: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HarnessKind {
    ClaudeCode,
    Codex,
}
impl HarnessKind {
    pub const fn id(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude_code",
            Self::Codex => "codex",
        }
    }
    pub const fn hook_command(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-session",
            Self::Codex => "codex-session",
        }
    }
    pub const fn route_prefix(self) -> &'static str {
        match self {
            Self::ClaudeCode => "/v1/claude/session/",
            Self::Codex => "/v1/codex/session/",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartError {
    InvalidBinding,
    InvalidProject,
    Capacity,
    Socket,
    Worker,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    Stopped,
    WorkerUnavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum SessionEventKind {
    SessionStart,
    SessionEnd,
}

/// Codex rust-v0.167.1 hooks/src/events/session_start.rs. This describes
/// the event, never authority to replace a process owner or infer fork lineage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStartSource {
    Startup,
    Resume,
    Clear,
    Compact,
    Fork,
}

/// Only authenticated, bounded metadata leaves the transport. It is still an
/// observation from the child, not filesystem evidence or a cleanup receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionEvent {
    pub instance_id: String,
    pub thread_id: String,
    pub session_id: String,
    pub kind: SessionEventKind,
    pub cwd: String,
    /// Required on Codex SessionStart, absent for Claude and SessionEnd.
    pub source: Option<SessionStartSource>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReceiverStatus {
    pub stopped: bool,
    pub failed: bool,
    /// Sticky: at least one authenticated event could not be retained. The
    /// controller must treat its observed session binding as incomplete.
    pub queue_overflowed: bool,
    pub rejected_requests: u64,
}
struct Shared {
    stop: Arc<AtomicBool>,
    stopped: AtomicBool,
    failed: AtomicBool,
    queue_overflowed: AtomicBool,
    rejected: AtomicU64,
}
struct Lease(Arc<Shared>);
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.stopped.store(true, Ordering::Release);
        RECEIVERS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// One numeric-loopback listener/worker per known instance, with a fixed global
/// receiver ceiling. It retains only the existing hash record, never the bearer
/// token. Pass the URL privately to the child and reference its existing
/// SHINATAMA_INSTANCE_TOKEN environment variable; do not persist the token.
pub struct SessionEventReceiver {
    harness: HarnessKind,
    address: SocketAddr,
    route: String,
    events: Receiver<SessionEvent>,
    shared: Arc<Shared>,
    worker: Thread,
    caller_instance: String,
    broker: Option<Arc<crate::session_control::Broker>>,
}
impl SessionEventReceiver {
    /// Kept for the Claude compatibility alias; new drivers use start_for.
    pub fn start(record: LaunchIdentityRecord, expected_cwd: &Path) -> Result<Self, StartError> {
        Self::start_for(HarnessKind::ClaudeCode, record, expected_cwd)
    }
    pub fn start_for(
        harness: HarnessKind,
        record: LaunchIdentityRecord,
        expected_cwd: &Path,
    ) -> Result<Self, StartError> {
        Self::start_with_broker(
            harness,
            record,
            expected_cwd,
            crate::session_control::installed(),
        )
    }
    fn start_with_broker(
        harness: HarnessKind,
        record: LaunchIdentityRecord,
        expected_cwd: &Path,
        broker: Option<Arc<crate::session_control::Broker>>,
    ) -> Result<Self, StartError> {
        if validate_id(&record.instance_id, CanonicalIdKind::Instance).is_err()
            || validate_id(&record.thread_id, CanonicalIdKind::Thread).is_err()
            || record.started_at_utc_ms < 0
            || record.ended_at_utc_ms.is_some()
            || record.token_sha256 == [0; 32]
        {
            return Err(StartError::InvalidBinding);
        }
        let cwd = expected_cwd.to_str().ok_or(StartError::InvalidProject)?;
        let project = project_key(cwd).ok_or(StartError::InvalidProject)?;
        let expected_cwd = expected_cwd.to_path_buf();
        RECEIVERS
            .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_RECEIVERS).then_some(count + 1)
            })
            .map_err(|_| StartError::Capacity)?;
        let shared = Arc::new(Shared {
            stop: Arc::new(AtomicBool::new(false)),
            stopped: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            queue_overflowed: AtomicBool::new(false),
            rejected: AtomicU64::new(0),
        });
        let lease = Lease(Arc::clone(&shared));
        let listener =
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|_| StartError::Socket)?;
        listener
            .set_nonblocking(true)
            .map_err(|_| StartError::Socket)?;
        let address = listener.local_addr().map_err(|_| StartError::Socket)?;
        let route = format!("{}{}", harness.route_prefix(), record.instance_id);
        let worker_route = route.clone();
        let worker_shared = Arc::clone(&shared);
        let worker_broker = broker.clone();
        let caller_instance = record.instance_id.clone();
        if let Some(broker) = &broker {
            broker.attach_caller(&caller_instance, &shared.stop);
        }
        let (sender, events) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
        let handle = thread::Builder::new()
            .name(format!("{}-session-events", harness.id()))
            .spawn(move || {
                let _lease = lease;
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(
                        listener,
                        address,
                        &worker_route,
                        &record,
                        &project,
                        sender,
                        &worker_shared,
                        harness,
                        &expected_cwd,
                        worker_broker.as_deref(),
                    );
                }))
                .is_err()
                {
                    worker_shared.failed.store(true, Ordering::Release);
                }
            })
            .map_err(|_| StartError::Worker)?;
        Ok(Self {
            harness,
            address,
            route,
            events,
            shared,
            worker: handle.thread().clone(),
            caller_instance,
            broker,
        })
    }
    pub fn harness(&self) -> HarnessKind {
        self.harness
    }
    pub fn endpoint(&self) -> String {
        format!("http://{}{}", self.address, self.route)
    }
    pub fn port(&self) -> u16 {
        self.address.port()
    }
    pub fn try_next(&self) -> Result<Option<SessionEvent>, ReadError> {
        match self.events.try_recv() {
            Ok(event) => Ok(Some(event)),
            // Stop is complete only after the sender is gone and every queued
            // event has been read. An in-flight request can still enqueue while
            // the nonblocking worker observes the stop signal.
            Err(TryRecvError::Disconnected) if self.shared.stop.load(Ordering::Acquire) => {
                Err(ReadError::Stopped)
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(ReadError::WorkerUnavailable),
        }
    }
    pub fn status(&self) -> ReceiverStatus {
        ReceiverStatus {
            stopped: self.shared.stopped.load(Ordering::Acquire),
            failed: self.shared.failed.load(Ordering::Acquire),
            queue_overflowed: self.shared.queue_overflowed.load(Ordering::Acquire),
            rejected_requests: self.shared.rejected.load(Ordering::Acquire),
        }
    }
    /// Nonblocking even during a partial request. Every socket operation is
    /// nonblocking and checks stop between bounded polling intervals. The worker
    /// retains its global slot until it has actually closed its listener/socket.
    pub fn stop(&self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(broker) = &self.broker {
            broker.revoke(&self.caller_instance);
        }
        self.worker.unpark();
    }
}
impl Drop for SessionEventReceiver {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run(
    listener: TcpListener,
    address: SocketAddr,
    route: &str,
    record: &LaunchIdentityRecord,
    project: &str,
    sender: SyncSender<SessionEvent>,
    shared: &Arc<Shared>,
    harness: HarnessKind,
    expected_cwd: &Path,
    broker: Option<&crate::session_control::Broker>,
) {
    while !shared.stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, peer)) => {
                if peer.ip() != Ipv4Addr::LOCALHOST || stream.set_nonblocking(true).is_err() {
                    let _ = stream.shutdown(Shutdown::Both);
                    continue;
                }
                let Some(response) = handle(
                    &mut stream,
                    address,
                    route,
                    record,
                    project,
                    &sender,
                    shared,
                    harness,
                    expected_cwd,
                    broker,
                ) else {
                    // A browser call answers from its own thread.
                    continue;
                };
                if !(200..300).contains(&response.status) {
                    let _ =
                        shared
                            .rejected
                            .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                                Some(count.saturating_add(1))
                            });
                }
                respond(&mut stream, response, shared, RESPONSE_TIMEOUT);
                let _ = stream.shutdown(Shutdown::Both);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => {
                shared.failed.store(true, Ordering::Release);
                break;
            }
        }
        // Pace both idle polling and floods; no per-connection threads or queues.
        if !shared.stop.load(Ordering::Acquire) {
            thread::park_timeout(IO_POLL);
        }
    }
}
fn readable(
    stream: &mut TcpStream,
    output: &mut [u8],
    deadline: Instant,
    shared: &Shared,
) -> Result<usize, u16> {
    loop {
        if shared.stop.load(Ordering::Acquire) {
            return Err(503);
        }
        if Instant::now() >= deadline {
            return Err(408);
        }
        match stream.read(output) {
            Ok(0) => return Err(400),
            Ok(read) => return Ok(read),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::park_timeout(IO_POLL)
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return Err(400),
        }
    }
}
struct Response {
    status: u16,
    body: Vec<u8>,
}
impl From<u16> for Response {
    fn from(status: u16) -> Self {
        Self {
            status,
            body: Vec::new(),
        }
    }
}
fn handle(
    stream: &mut TcpStream,
    address: SocketAddr,
    route: &str,
    record: &LaunchIdentityRecord,
    project: &str,
    sender: &SyncSender<SessionEvent>,
    shared: &Arc<Shared>,
    harness: HarnessKind,
    expected_cwd: &Path,
    broker: Option<&crate::session_control::Broker>,
) -> Option<Response> {
    let (mcp, body) = match read_request(stream, address, route, record, shared) {
        Ok(request) => request,
        Err(status) => return Some(status.into()),
    };
    if shared.stop.load(Ordering::Acquire) {
        return Some(503.into());
    }
    if mcp {
        let Some(broker) = broker else {
            return Some(503.into());
        };
        if crate::session_control::runs_detached(&body) {
            return detach_browser_call(stream, record, harness, expected_cwd, body, shared);
        }
        return Some(rpc_response(broker, record, harness, expected_cwd, &body));
    }
    Some(match parse_event(&body, record, project, harness) {
        Ok(event) if !shared.stop.load(Ordering::Acquire) => match sender.try_send(event) {
            Ok(()) => 204.into(),
            Err(TrySendError::Full(_)) => {
                shared.queue_overflowed.store(true, Ordering::Release);
                503.into()
            }
            Err(TrySendError::Disconnected(_)) => 503.into(),
        },
        Ok(_) => 503.into(),
        Err(status) => status.into(),
    })
}
fn rpc_response(
    broker: &crate::session_control::Broker,
    record: &LaunchIdentityRecord,
    harness: HarnessKind,
    project: &Path,
    body: &[u8],
) -> Response {
    let caller = crate::session_control::Caller {
        instance: &record.instance_id,
        session: &record.thread_id,
        harness: harness.id(),
    };
    match crate::session_control::handle_rpc(broker, &caller, project, body) {
        Some(reply) => match serde_json::to_vec(&reply) {
            Ok(body) => Response { status: 200, body },
            Err(_) => 503.into(),
        },
        None => 202.into(),
    }
}

/// Runs one browser or thread tool call on its own thread so a slow page or a
/// busy store never stalls this session's hook events. Returns `None` when that thread owns the reply.
fn detach_browser_call(
    stream: &mut TcpStream,
    record: &LaunchIdentityRecord,
    harness: HarnessKind,
    expected_cwd: &Path,
    body: Vec<u8>,
    shared: &Arc<Shared>,
) -> Option<Response> {
    if BROWSER_CALLS
        .try_update(Ordering::AcqRel, Ordering::Acquire, |count| (count < MAX_BROWSER_CALLS).then_some(count + 1))
        .is_err()
    {
        return Some(503.into());
    }
    let release = || {
        BROWSER_CALLS.fetch_sub(1, Ordering::AcqRel);
    };
    let (Ok(mut own), Some(broker)) = (stream.try_clone(), crate::session_control::installed()) else {
        release();
        return Some(503.into());
    };
    let (caller, project, shared) = (record.clone(), expected_cwd.to_path_buf(), Arc::clone(shared));
    let spawned = thread::Builder::new().name("session-detached-call".into()).spawn(move || {
        let response = rpc_response(&broker, &caller, harness, &project, &body);
        respond(&mut own, response, &shared, BROWSER_RESPONSE_TIMEOUT);
        let _ = own.shutdown(Shutdown::Both);
        BROWSER_CALLS.fetch_sub(1, Ordering::AcqRel);
    });
    if spawned.is_err() {
        release();
        return Some(503.into());
    }
    None
}
fn read_request(
    stream: &mut TcpStream,
    address: SocketAddr,
    route: &str,
    record: &LaunchIdentityRecord,
    shared: &Shared,
) -> Result<(bool, Vec<u8>), u16> {
    let deadline = Instant::now() + REQUEST_TIMEOUT;
    let mut header = Vec::with_capacity(MAX_HEADER_BYTES);
    let mut chunk = [0; 1024];
    let end = loop {
        if let Some(at) = header.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            break at + 4;
        }
        let room = MAX_HEADER_BYTES - header.len();
        if room == 0 {
            return Err(431);
        }
        let read = readable(stream, &mut chunk[..room.min(1024)], deadline, shared)?;
        header.extend_from_slice(&chunk[..read]);
    };
    let (length, mcp) = parse_header(&header[..end], address, route, record)?;
    let prefetched = &header[end..];
    if prefetched.len() > length {
        return Err(400);
    }
    let mut body = Vec::with_capacity(length);
    body.extend_from_slice(prefetched);
    header.fill(0); // The bearer header is not kept while waiting for the body.
    while body.len() < length {
        let read = readable(
            stream,
            &mut chunk[..(length - body.len()).min(1024)],
            deadline,
            shared,
        )?;
        body.extend_from_slice(&chunk[..read]);
    }
    Ok((mcp, body))
}
fn parse_event(
    body: &[u8],
    record: &LaunchIdentityRecord,
    project: &str,
    harness: HarnessKind,
) -> Result<SessionEvent, u16> {
    let payload = hook_payload(harness, body).map_err(|_| 400u16)?;
    if project_key(&payload.cwd).as_deref() != Some(project) {
        return Err(400);
    }
    Ok(SessionEvent {
        instance_id: record.instance_id.clone(),
        thread_id: record.thread_id.clone(),
        session_id: payload.session_id,
        kind: payload.hook_event_name,
        cwd: payload.cwd,
        source: payload.source,
    })
}
// Ignore native event extensions without retaining them. Prompts, transcripts,
// model output, transcript paths and permission details never enter the queue.
#[derive(Deserialize, Serialize)]
struct HookPayload {
    session_id: String,
    hook_event_name: SessionEventKind,
    cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<SessionStartSource>,
}

#[derive(Deserialize)]
struct ClaudePayload {
    session_id: String,
    hook_event_name: SessionEventKind,
    cwd: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidHookPayload;

// Official Codex rust-v0.167.1 sources verified before adding this dialect:
// https://github.com/openai/codex/blob/rust-v0.167.1/codex-rs/hooks/src/schema.rs#L466
// https://github.com/openai/codex/blob/rust-v0.167.1/codex-rs/hooks/src/events/session_end.rs#L56
// Unknown native extensions are discarded. Serde rejects duplicate binding
// fields; source is required for Codex starts and is never inferred from argv.
fn hook_payload(harness: HarnessKind, input: &[u8]) -> Result<HookPayload, InvalidHookPayload> {
    if input.is_empty() || input.len() > MAX_BODY_BYTES {
        return Err(InvalidHookPayload);
    }
    let payload = match harness {
        HarnessKind::ClaudeCode => {
            let native: ClaudePayload =
                serde_json::from_slice(input).map_err(|_| InvalidHookPayload)?;
            HookPayload {
                session_id: native.session_id,
                hook_event_name: native.hook_event_name,
                cwd: native.cwd,
                source: None,
            }
        }
        HarnessKind::Codex => {
            let native: HookPayload =
                serde_json::from_slice(input).map_err(|_| InvalidHookPayload)?;
            if (native.hook_event_name == SessionEventKind::SessionStart) != native.source.is_some()
            {
                return Err(InvalidHookPayload);
            }
            native
        }
    };
    if !canonical_uuid(&payload.session_id) || project_key(&payload.cwd).is_none() {
        return Err(InvalidHookPayload);
    }
    Ok(payload)
}
/// Shared command-bridge normalization. Only validated session metadata leaves
/// stdin; no transcript path, prompt, model output or permission context survives.
pub fn normalize_hook_payload(
    harness: HarnessKind,
    input: &[u8],
) -> Result<Vec<u8>, InvalidHookPayload> {
    serde_json::to_vec(&hook_payload(harness, input)?).map_err(|_| InvalidHookPayload)
}

fn parse_header(
    bytes: &[u8],
    address: SocketAddr,
    route: &str,
    record: &LaunchIdentityRecord,
) -> Result<(usize, bool), u16> {
    let text = std::str::from_utf8(bytes).map_err(|_| 400u16)?;
    let mut lines = text.split("\r\n");
    let start = lines.next();
    let (mcp, get) = if start == Some(format!("POST {route} HTTP/1.1").as_str()) {
        (false, false)
    } else if start == Some(format!("POST {route}/mcp HTTP/1.1").as_str()) {
        (true, false)
    } else if start == Some(format!("GET {route}/mcp HTTP/1.1").as_str()) {
        (true, true)
    } else {
        return Err(400);
    };
    let mut host = None;
    let mut authorization = None;
    let mut length = None;
    let mut content_type = None;
    for (index, line) in lines.enumerate() {
        if line.is_empty() {
            break;
        }
        if index >= MAX_HEADERS {
            return Err(431);
        }
        let (name, value) = line.split_once(':').ok_or(400u16)?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || value
                .bytes()
                .any(|byte| byte < 32 && byte != b'\t' || byte == 127)
        {
            return Err(400);
        }
        let value = value.trim_matches([' ', '\t']);
        let target = match name.to_ascii_lowercase().as_str() {
            "host" => Some(&mut host),
            "authorization" => Some(&mut authorization),
            "content-length" => Some(&mut length),
            "content-type" => Some(&mut content_type),
            "transfer-encoding" | "origin" | "expect" | "upgrade" => return Err(400),
            _ => None,
        };
        if let Some(target) = target {
            if target.replace(value).is_some() {
                return Err(400);
            }
        }
    }
    if host != Some(address.to_string().as_str()) {
        return Err(400);
    }
    let token = authorization
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(401u16)?;
    verify_record(&record.instance_id, token, Some(record)).map_err(|_| 401u16)?;
    // Stateless Streamable HTTP uses POST only. Authenticate GET (and reject
    // browser Origin) before saying that the optional SSE stream is unavailable.
    if get {
        return Err(if length.is_none() || length == Some("0") {
            405
        } else {
            400
        });
    }
    if !content_type.is_some_and(|value| {
        value
            .split(';')
            .next()
            .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"))
    }) {
        return Err(415);
    }
    let length = length.ok_or(400u16)?;
    if length.is_empty() || length.len() > 10 || !length.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(400);
    }
    let length = length.parse::<usize>().map_err(|_| 413u16)?;
    if length == 0 {
        return Err(400);
    }
    if length > MAX_BODY_BYTES {
        return Err(413);
    }
    Ok((length, mcp))
}
fn respond(stream: &mut TcpStream, response: Response, shared: &Shared, timeout: Duration) {
    let status = response.status;
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        401 => "Unauthorized",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Bad Request",
    };
    let allow = if status == 405 { "Allow: POST\r\n" } else { "" };
    let mut wire = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nContent-Type: application/json\r\n{allow}Connection: close\r\nCache-Control: no-store\r\n\r\n", response.body.len()
    ).into_bytes();
    wire.extend_from_slice(&response.body);
    let deadline = Instant::now() + timeout;
    let mut written = 0;
    while written < wire.len() && Instant::now() < deadline && !shared.stop.load(Ordering::Acquire)
    {
        match stream.write(&wire[written..]) {
            Ok(0) => break,
            Ok(count) => written += count,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::park_timeout(IO_POLL)
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
}
fn canonical_uuid(value: &str) -> bool {
    value.len() == 36
        && uuid::Uuid::parse_str(value).ok().is_some_and(|id| {
            id.hyphenated().to_string() == value
                && id.get_variant() == uuid::Variant::RFC4122
                && id.get_version().is_some()
        })
}
fn project_key(value: &str) -> Option<String> {
    if value.is_empty() || value.len() > MAX_CWD_BYTES || value.chars().any(char::is_control) {
        return None;
    }
    #[cfg(windows)]
    {
        let mut value = value.replace('/', "\\");
        if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
            value = format!(r"\\{unc}");
        } else if let Some(ordinary) = value.strip_prefix(r"\\?\") {
            value = ordinary.to_owned();
        }
        if value.starts_with(r"\\.\") || value.starts_with(r"\\?\") {
            return None;
        }
        let rest = if let Some(unc) = value.strip_prefix(r"\\") {
            if unc.split('\\').filter(|part| !part.is_empty()).count() < 2 {
                return None;
            }
            unc
        } else {
            let bytes = value.as_bytes();
            if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || &bytes[1..3] != b":\\" {
                return None;
            }
            &value[3..]
        };
        if rest.split('\\').any(|part| {
            part == "."
                || part == ".."
                || part.ends_with(['.', ' '])
                || part.chars().any(|ch| "<>:\"|?*".contains(ch))
        }) {
            return None;
        }
        let mut key = value.trim_end_matches('\\').to_ascii_lowercase();
        if key.len() == 2 {
            key.push('\\');
        }
        Some(key)
    }
    #[cfg(not(windows))]
    {
        if !value.starts_with('/') || value.split('/').any(|part| part == "." || part == "..") {
            return None;
        }
        Some(
            if value == "/" {
                "/"
            } else {
                value.trim_end_matches('/')
            }
            .to_owned(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::identity::LaunchIdentity;

    #[test]
    fn mcp_headers_require_identity_and_reject_origin_without_a_socket() {
        let identity = LaunchIdentity::generate("t_bbbbbbbbbbbb").unwrap();
        let secret = identity.child_secret_override();
        let token = secret.value().to_str().unwrap();
        let record = identity.record();
        let address: SocketAddr = "127.0.0.1:43210".parse().unwrap();
        let route = format!("/v1/claude/session/{}", record.instance_id);
        let header = |method: &str, credential: &str, extra: &str| format!(
            "{method} {route}/mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {credential}\r\n{extra}\r\n"
        );
        assert_eq!(parse_header(header("POST", token, "Content-Type: application/json\r\nContent-Length: 2\r\n").as_bytes(), address, &route, &record), Ok((2, true)));
        assert_eq!(parse_header(header("GET", token, "").as_bytes(), address, &route, &record), Err(405));
        assert_eq!(parse_header(header("GET", "incorrect", "").as_bytes(), address, &route, &record), Err(401));
        assert_eq!(parse_header(header("GET", token, "Origin: http://untrusted.test\r\n").as_bytes(), address, &route, &record), Err(400));
    }

    fn post_mcp(
        receiver: &SessionEventReceiver,
        token: &str,
        suffix: &str,
        payload: serde_json::Value,
    ) -> (u16, Vec<u8>) {
        let body = serde_json::to_vec(&payload).unwrap();
        let mut stream =
            TcpStream::connect_timeout(&receiver.address, Duration::from_secs(1)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let request = format!(
            "POST {}{suffix} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            receiver.route,
            receiver.address,
            body.len()
        );
        stream.write_all(request.as_bytes()).unwrap();
        stream.write_all(&body).unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        let end = response
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .unwrap()
            + 4;
        let header = std::str::from_utf8(&response[..end]).unwrap();
        let status = header.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, response[end..].to_vec())
    }

    fn get_mcp(receiver: &SessionEventReceiver, token: &str, extra: &str) -> String {
        let mut stream =
            TcpStream::connect_timeout(&receiver.address, Duration::from_secs(1)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let request = format!(
            "GET {}/mcp HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {token}\r\n{extra}\r\n",
            receiver.route, receiver.address
        );
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn mcp_route_reuses_authentication_and_queues_once_in_the_original_project() {
        use crate::session_control::{Broker, LaunchReceipt};
        let broker = Arc::new(Broker::new());
        let identity = LaunchIdentity::generate("t_bbbbbbbbbbbb").unwrap();
        let token = identity
            .child_secret_override()
            .value()
            .to_str()
            .unwrap()
            .to_owned();
        let project = std::env::temp_dir().join("Shinatama-Case-Preserved");
        let receiver = SessionEventReceiver::start_with_broker(
            HarnessKind::ClaudeCode,
            identity.record(),
            &project,
            Some(broker.clone()),
        )
        .unwrap();
        let get = get_mcp(&receiver, &token, "");
        assert!(get.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"));
        assert!(get.contains("\r\nAllow: POST\r\n"));
        assert!(get_mcp(&receiver, &"0".repeat(64), "").starts_with("HTTP/1.1 401 "));
        assert!(
            get_mcp(&receiver, &token, "Origin: https://example.test\r\n")
                .starts_with("HTTP/1.1 400 ")
        );
        let init = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"});
        assert_eq!(
            post_mcp(&receiver, &"0".repeat(64), "/mcp", init.clone()).0,
            401
        );
        assert_eq!(
            post_mcp(&receiver, &token, "/mcp/other", init.clone()).0,
            400
        );
        let (status, body) = post_mcp(&receiver, &token, "/mcp", init);
        assert_eq!(status, 200);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap()["result"]["protocolVersion"],
            crate::session_control::PROTOCOL_VERSION
        );
        let (status, body) = post_mcp(
            &receiver,
            &token,
            "/mcp",
            serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        );
        assert_eq!(status, 202);
        assert!(body.is_empty());
        let request_id = uuid::Uuid::new_v4().to_string();
        let open = serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"shinatama_session_open","arguments":{"request_id":request_id,"harness":"codex","instruction":"Review the authorized diff."}}});
        let first = post_mcp(&receiver, &token, "/mcp", open.clone());
        assert_eq!(first.0, 200);
        assert_eq!(first, post_mcp(&receiver, &token, "/mcp", open));
        let launch = broker.next().unwrap();
        assert_eq!(launch.project, project);
        assert_eq!(launch.harness, "codex");
        assert_eq!(launch.caller_instance, identity.instance_id());
        assert!(broker.next().is_none());
        assert!(
            receiver.try_next().unwrap().is_none(),
            "MCP payloads do not enter the hook event queue"
        );
        receiver.stop();
        assert!(!broker.is_pending(&launch.caller_instance, &launch.request_id));
        // Cancellation during a host launch does not discard its real receipt.
        broker.complete(
            &launch.caller_instance,
            &launch.request_id,
            Ok(LaunchReceipt {
                session_id: "pty".into(),
                thread_id: "thread".into(),
                harness: launch.harness,
            }),
        );
    }

    fn post(
        receiver: &SessionEventReceiver,
        token: &str,
        body: &[u8],
        declared: usize,
        extra: &str,
    ) -> u16 {
        let mut stream =
            TcpStream::connect_timeout(&receiver.address, Duration::from_secs(1)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let header = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {declared}\r\n{extra}\r\n",
            receiver.route, receiver.address
        );
        let mut request = header.into_bytes();
        request.extend_from_slice(body);
        stream.write_all(&request).unwrap();
        let mut response = Vec::new();
        let mut byte = [0];
        while !response.ends_with(b"\r\n") && response.len() < 128 {
            stream.read_exact(&mut byte).unwrap();
            response.push(byte[0]);
        }
        std::str::from_utf8(&response)
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap()
    }
    #[test]
    fn loopback_authentication_bounds_and_stop_preserve_only_session_metadata() {
        for harness in [HarnessKind::ClaudeCode, HarnessKind::Codex] {
            let identity = LaunchIdentity::generate("t_aaaaaaaaaaaa").unwrap();
            let token = identity
                .child_secret_override()
                .value()
                .to_str()
                .unwrap()
                .to_owned();
            let project = std::env::temp_dir().join("shinatama-hook-fixture");
            let receiver =
                SessionEventReceiver::start_for(harness, identity.record(), &project).unwrap();
            let native_id = "12cc3a11-45db-4f1e-89c8-40d5c6be1dd3";
            let payload = serde_json::json!({"session_id":native_id,"hook_event_name":"SessionStart","cwd":project,
            "transcript_path":"NEVER_READ","prompt":"NEVER_QUEUED","source":"startup"});
            let body = serde_json::to_vec(&payload).unwrap();
            assert_eq!(post(&receiver, &token, &body, body.len(), ""), 204);
            let event = receiver.try_next().unwrap().unwrap();
            assert_eq!(event.instance_id, identity.instance_id());
            assert_eq!(event.thread_id, "t_aaaaaaaaaaaa");
            assert_eq!(event.session_id, native_id);
            assert_eq!(event.kind, SessionEventKind::SessionStart);
            assert_eq!(
                event.source,
                (harness == HarnessKind::Codex).then_some(SessionStartSource::Startup)
            );
            assert!(!format!("{event:?}").contains("NEVER_"));
            let end = serde_json::to_vec(&serde_json::json!({"session_id":native_id,"hook_event_name":"SessionEnd","cwd":project})).unwrap();
            assert_eq!(post(&receiver, &token, &end, end.len(), ""), 204);
            assert_eq!(
                receiver.try_next().unwrap().unwrap().kind,
                SessionEventKind::SessionEnd
            );
            assert_eq!(post(&receiver, &"0".repeat(64), &body, body.len(), ""), 401);
            assert_eq!(post(&receiver, &token, b"{", 1, ""), 400);
            assert_eq!(post(&receiver, &token, b"", MAX_BODY_BYTES + 1, ""), 413);
            assert_eq!(
                post(
                    &receiver,
                    &token,
                    &body,
                    body.len(),
                    "Transfer-Encoding: chunked\r\n"
                ),
                400
            );
            let other = serde_json::to_vec(&serde_json::json!({"session_id":native_id,"hook_event_name":"SessionStart","cwd":project.join("elsewhere"),"source":"startup"})).unwrap();
            assert_eq!(post(&receiver, &token, &other, other.len(), ""), 400);
            let duplicate = format!(
                r#"{{"session_id":"{native_id}","session_id":"{native_id}","hook_event_name":"SessionStart","cwd":{},"source":"startup"}}"#,
                serde_json::to_string(&project).unwrap()
            );
            assert_eq!(
                post(&receiver, &token, duplicate.as_bytes(), duplicate.len(), ""),
                400
            );
            assert_eq!(receiver.try_next().unwrap(), None);
            for _ in 0..EVENT_QUEUE_CAPACITY {
                assert_eq!(post(&receiver, &token, &body, body.len(), ""), 204);
            }
            assert_eq!(post(&receiver, &token, &body, body.len(), ""), 503);
            assert!(receiver.status().queue_overflowed);
            let mut partial =
                TcpStream::connect_timeout(&receiver.address, Duration::from_secs(1)).unwrap();
            partial.write_all(b"POST ").unwrap();
            receiver.stop();
            let deadline = Instant::now() + Duration::from_secs(1);
            while !receiver.status().stopped && Instant::now() < deadline {
                thread::sleep(IO_POLL);
            }
            assert!(receiver.status().stopped);
            assert!(!receiver.status().failed);
            for _ in 0..EVENT_QUEUE_CAPACITY {
                assert_eq!(receiver.try_next().unwrap().unwrap().session_id, native_id);
            }
            assert_eq!(receiver.try_next(), Err(ReadError::Stopped));
        }
    }
}
