//! Private F02 transport groundwork; no hook installation or normal-profile change.
//! SessionStart requires a command bridge (Claude does not support HTTP hooks for
//! that event). SessionEnd may use the same bridge or a qualified HTTP hook.
//! Accepted means queued, never durably committed or authorized to acquire a root.
use crate::launch::identity::{LaunchIdentityRecord, verify_record};
use serde::Deserialize;
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
const IO_POLL: Duration = Duration::from_millis(10);
const MAX_HEADERS: usize = 32;
const MAX_RECEIVERS: usize = 16;
static RECEIVERS: AtomicUsize = AtomicUsize::new(0);

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum SessionEventKind {
    SessionStart,
    SessionEnd,
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
    stop: AtomicBool,
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
pub struct ClaudeSessionEventReceiver {
    address: SocketAddr,
    route: String,
    events: Receiver<SessionEvent>,
    shared: Arc<Shared>,
    worker: Thread,
}
impl ClaudeSessionEventReceiver {
    pub fn start(record: LaunchIdentityRecord, expected_cwd: &Path) -> Result<Self, StartError> {
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
        RECEIVERS
            .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_RECEIVERS).then_some(count + 1)
            })
            .map_err(|_| StartError::Capacity)?;
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
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
        let route = format!("/v1/claude/session/{}", record.instance_id);
        let worker_route = route.clone();
        let worker_shared = Arc::clone(&shared);
        let (sender, events) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
        let handle = thread::Builder::new()
            .name("claude-session-events".into())
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
                    );
                }))
                .is_err()
                {
                    worker_shared.failed.store(true, Ordering::Release);
                }
            })
            .map_err(|_| StartError::Worker)?;
        Ok(Self {
            address,
            route,
            events,
            shared,
            worker: handle.thread().clone(),
        })
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
            Err(TryRecvError::Disconnected)
                if self.shared.stop.load(Ordering::Acquire) => Err(ReadError::Stopped),
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
        self.worker.unpark();
    }
}
impl Drop for ClaudeSessionEventReceiver {
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
    shared: &Shared,
) {
    while !shared.stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, peer)) => {
                if peer.ip() != Ipv4Addr::LOCALHOST || stream.set_nonblocking(true).is_err() {
                    let _ = stream.shutdown(Shutdown::Both);
                    continue;
                }
                let status = handle(
                    &mut stream,
                    address,
                    route,
                    record,
                    project,
                    &sender,
                    shared,
                );
                if status != 204 {
                    let _ = shared.rejected.try_update(
                        Ordering::AcqRel,
                        Ordering::Acquire,
                        |count| Some(count.saturating_add(1)),
                    );
                }
                respond(&mut stream, status, shared);
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
fn handle(
    stream: &mut TcpStream,
    address: SocketAddr,
    route: &str,
    record: &LaunchIdentityRecord,
    project: &str,
    sender: &SyncSender<SessionEvent>,
    shared: &Shared,
) -> u16 {
    match read_event(stream, address, route, record, project, shared) {
        Ok(event) if !shared.stop.load(Ordering::Acquire) => match sender.try_send(event) {
            Ok(()) => 204,
            Err(TrySendError::Full(_)) => {
                shared.queue_overflowed.store(true, Ordering::Release);
                503
            }
            Err(TrySendError::Disconnected(_)) => 503,
        },
        Ok(_) => 503,
        Err(status) => status,
    }
}
fn read_event(
    stream: &mut TcpStream,
    address: SocketAddr,
    route: &str,
    record: &LaunchIdentityRecord,
    project: &str,
    shared: &Shared,
) -> Result<SessionEvent, u16> {
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
    let length = parse_header(&header[..end], address, route, record)?;
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
    let payload: HookPayload = serde_json::from_slice(&body).map_err(|_| 400u16)?;
    if !canonical_uuid(&payload.session_id) || project_key(&payload.cwd).as_deref() != Some(project)
    {
        return Err(400);
    }
    Ok(SessionEvent {
        instance_id: record.instance_id.clone(),
        thread_id: record.thread_id.clone(),
        session_id: payload.session_id,
        kind: payload.hook_event_name,
        cwd: payload.cwd,
    })
}
// Ignore native event extensions without retaining them. Prompts, transcripts,
// model output, transcript paths and permission details never enter the queue.
#[derive(Deserialize)]
struct HookPayload {
    session_id: String,
    hook_event_name: SessionEventKind,
    cwd: String,
}

fn parse_header(
    bytes: &[u8],
    address: SocketAddr,
    route: &str,
    record: &LaunchIdentityRecord,
) -> Result<usize, u16> {
    let text = std::str::from_utf8(bytes).map_err(|_| 400u16)?;
    let mut lines = text.split("\r\n");
    if lines.next() != Some(format!("POST {route} HTTP/1.1").as_str()) {
        return Err(400);
    }
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
    Ok(length)
}
fn respond(stream: &mut TcpStream, status: u16, shared: &Shared) {
    let reason = match status {
        204 => "No Content",
        401 => "Unauthorized",
        408 => "Request Timeout",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Bad Request",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: 0\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n"
    );
    let deadline = Instant::now() + RESPONSE_TIMEOUT;
    let mut written = 0;
    while written < response.len()
        && Instant::now() < deadline
        && !shared.stop.load(Ordering::Acquire)
    {
        match stream.write(&response.as_bytes()[written..]) {
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

    fn post(
        receiver: &ClaudeSessionEventReceiver,
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
        let identity = LaunchIdentity::generate("t_aaaaaaaaaaaa").unwrap();
        let token = identity
            .child_secret_override()
            .value()
            .to_str()
            .unwrap()
            .to_owned();
        let project = std::env::temp_dir().join("shinatama-hook-fixture");
        let receiver = ClaudeSessionEventReceiver::start(identity.record(), &project).unwrap();
        let native_id = "12cc3a11-45db-4f1e-89c8-40d5c6be1dd3";
        let payload = serde_json::json!({"session_id":native_id,"hook_event_name":"SessionStart","cwd":project,
            "transcript_path":"NEVER_READ","prompt":"NEVER_QUEUED"});
        let body = serde_json::to_vec(&payload).unwrap();
        assert_eq!(post(&receiver, &token, &body, body.len(), ""), 204);
        let event = receiver.try_next().unwrap().unwrap();
        assert_eq!(event.instance_id, identity.instance_id());
        assert_eq!(event.thread_id, "t_aaaaaaaaaaaa");
        assert_eq!(event.session_id, native_id);
        assert_eq!(event.kind, SessionEventKind::SessionStart);
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
        let other = serde_json::to_vec(&serde_json::json!({"session_id":native_id,"hook_event_name":"SessionStart","cwd":project.join("elsewhere")})).unwrap();
        assert_eq!(post(&receiver, &token, &other, other.len(), ""), 400);
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
