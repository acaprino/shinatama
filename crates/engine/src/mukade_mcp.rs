//! Connection to the local Mukade chat service.
//! Shinatama owns one service: the Mukade program it ships (built in this workspace from the
//! pinned Mukade revision, `crates/mukade`), with its
//! data in `mukade` under Shinatama's own data folder; it is started on demand and
//! stopped with the app. An explicit MUKADE_DATA_DIR selects an external service
//! instead, which is only joined: never started, never stopped.
//! No shared configuration or permission is changed.
//! The credential crosses only the prepared child's environment, never argv/config.
use serde::Deserialize;
use serde_json::{Value, json};
use shinatama_state::location::StorageLocation;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, TryLockError},
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub const TOKEN_ENV: &str = "SHINATAMA_MUKADE_TOKEN";
const MAX_METADATA: usize = 8192;
const MAX_HTTP_HEADER: usize = 4096;
const MAX_HEALTH_BODY: usize = 4096;
const HEALTH_TIMEOUT: Duration = Duration::from_secs(2);
/// Mukade bounds its own start and stop at ten seconds.
const BUNDLED_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidDirectory,
    PrivateStorageRequired,
    ServiceStopped,
    InvalidMetadata,
    InvalidCredential,
    ServiceUnavailable,
    ServiceChanged,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidDirectory => "The Mukade data directory must be an absolute private directory outside projects.",
            Self::PrivateStorageRequired => "Mukade connection files could not be verified as private regular files.",
            Self::ServiceStopped => "The Mukade service is stopped.",
            Self::InvalidMetadata => "Mukade connection metadata is invalid.",
            Self::InvalidCredential => "Mukade connection credential is invalid.",
            Self::ServiceUnavailable => "The selected Mukade service did not answer its bounded health check.",
            Self::ServiceChanged => "The selected Mukade service identity changed or is stopping.",
        })
    }
}
impl std::error::Error for Error {}

/// Deliberately has no Debug or Serialize implementation: it contains a credential.
pub struct Connection {
    endpoint: String,
    service_id: String,
    instance_id: String,
    token: String,
    address: SocketAddr,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Instance {
    data_dir: PathBuf,
    endpoint: String,
    pid: u32,
    service_id: String,
    instance_id: String,
    server_version: String,
}

impl Connection {
    /// Read-only: the running service, or `None` when none was ever selected or started.
    pub fn discover(base: &StorageLocation) -> Result<Option<Self>, Error> {
        match std::env::var_os("MUKADE_DATA_DIR") {
            Some(path) => Self::discover_at(base, Path::new(&path)).map(Some),
            None => {
                let own = own_directory(base);
                match fs::symlink_metadata(own.join("bus")) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    _ => Self::discover_at(base, &own).map(Some),
                }
            }
        }
    }

    /// The running service, starting Shinatama's own one when it is stopped.
    /// `None`: no external service is selected and no Mukade program is bundled.
    /// The error is the reason to show: Mukade's own words when its start failed.
    pub fn ensure_running(base: &StorageLocation) -> Result<Option<Self>, String> {
        if std::env::var_os("MUKADE_DATA_DIR").is_some() {
            return Self::discover(base).map_err(|e| e.to_string());
        }
        if let Ok(Some(connection)) = Self::discover(base) {
            return Ok(Some(connection));
        }
        let Some(program) = bundled_program() else { return Ok(None) };
        let own = own_directory(base);
        // `chat start` is idempotent: a concurrent caller finds the same service.
        run_bundled(&program, &own, "start")?;
        Self::discover_at(base, &own).map(Some).map_err(|e| e.to_string())
    }

    fn discover_at(base: &StorageLocation, selected: &Path) -> Result<Self, Error> {
        if !selected.is_absolute() {
            return Err(Error::InvalidDirectory);
        }
        // Do not silently adopt a portable folder beside an unrelated executable.
        // Both roots must already exist and satisfy the host's local/private policy.
        reject_reparse(selected)?;
        for ancestor in selected.ancestors() {
            match fs::symlink_metadata(ancestor.join(".git")) {
                Ok(_) => return Err(Error::InvalidDirectory),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(Error::InvalidDirectory),
            }
        }
        let data = base.resolve_related_directory(selected).map_err(|_| Error::InvalidDirectory)?;
        data.verify_private_directory().map_err(|_| Error::PrivateStorageRequired)?;
        let bus_path = selected.join("bus");
        reject_reparse(&bus_path)?;
        let bus = base.resolve_related_directory(&bus_path).map_err(|_| Error::InvalidDirectory)?;
        bus.verify_private_directory().map_err(|_| Error::PrivateStorageRequired)?;
        let lock_path = bus.directory().join("instance.lock");
        let lock = bus.open_owner_only_file(&lock_path).map_err(|_| Error::PrivateStorageRequired)?;
        match lock.try_lock() {
            Ok(()) => return Err(Error::ServiceStopped), // drop releases the read-only probe lock
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(_)) => return Err(Error::PrivateStorageRequired),
        }
        let metadata = read_private(&bus, "instance.json", MAX_METADATA)?;
        let credential = read_private(&bus, "credential", 64)?;
        let connection = Self::from_files(data.directory(), &metadata, &credential)?;
        connection.check_health()?;
        // A process that released ownership during the check is not accepted.
        match lock.try_lock() {
            Err(TryLockError::WouldBlock) => Ok(connection),
            Ok(()) => Err(Error::ServiceStopped),
            Err(TryLockError::Error(_)) => Err(Error::PrivateStorageRequired),
        }
    }

    fn from_files(directory: &Path, metadata: &[u8], credential: &[u8]) -> Result<Self, Error> {
        if metadata.len() > MAX_METADATA {
            return Err(Error::InvalidMetadata);
        }
        let state: Instance = serde_json::from_slice(metadata).map_err(|_| Error::InvalidMetadata)?;
        if !same_path(directory, &state.data_dir)
            || state.pid == 0
            || state.server_version.is_empty()
            || state.server_version.len() > 128
            || state.server_version.chars().any(char::is_control)
            // Mukade chat_schema.rs uses lower(hex(randomblob(16))), not hyphenated UUIDs.
            || state.service_id.len() != 32
            || !state.service_id.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            || !canonical_uuid(&state.instance_id)
        {
            return Err(Error::InvalidMetadata);
        }
        let address = endpoint_address(&state.endpoint)?;
        if credential.len() != 64 || !credential.iter().all(u8::is_ascii_hexdigit) {
            return Err(Error::InvalidCredential);
        }
        let token = String::from_utf8(credential.to_vec()).map_err(|_| Error::InvalidCredential)?;
        Ok(Self {
            endpoint: state.endpoint,
            service_id: state.service_id,
            instance_id: state.instance_id,
            token,
            address,
        })
    }

    pub fn claude_server(&self) -> Value {
        json!({"type":"http", "url":self.endpoint, "headers":{
            "Authorization":"Bearer ${SHINATAMA_MUKADE_TOKEN}",
            "X-Mukade-Service-Id":self.service_id
        }})
    }

    /// One TOML table override keeps native launch argument count bounded.
    pub fn codex_arguments(&self) -> Vec<OsString> {
        // Endpoint and service ID validation excludes quotes and control characters;
        // JSON string encoding is also a valid TOML basic string for these values.
        let endpoint = serde_json::to_string(&self.endpoint).expect("string serialization");
        let service = serde_json::to_string(&self.service_id).expect("string serialization");
        vec!["-c".into(), format!(
            "mcp_servers.mukade={{url={endpoint},bearer_token_env_var=\"{TOKEN_ENV}\",http_headers={{\"X-Mukade-Service-Id\"={service}}},tool_timeout_sec=330}}"
        ).into()]
    }

    pub fn apply_environment(&self, environment: &mut BTreeMap<OsString, OsString>) {
        clear_environment(environment);
        environment.insert(TOKEN_ENV.into(), self.token.clone().into());
    }

    fn check_health(&self) -> Result<(), Error> {
        let deadline = Instant::now() + HEALTH_TIMEOUT;
        let mut stream = TcpStream::connect_timeout(&self.address, HEALTH_TIMEOUT)
            .map_err(|_| Error::ServiceUnavailable)?;
        stream.set_write_timeout(Some(remaining(deadline)?)).map_err(|_| Error::ServiceUnavailable)?;
        // HTTP/1.0: Mukade answers a 1.1 request chunked; a 1.0 answer is the plain
        // body, ended by its length or by the close of the connection.
        let mut request = format!(
            "GET /health HTTP/1.0\r\nHost: {}\r\nAuthorization: Bearer {}\r\nX-Mukade-Service-Id: {}\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
            self.address, self.token, self.service_id
        ).into_bytes();
        let sent = stream.write_all(&request);
        request.fill(0);
        sent.map_err(|_| Error::ServiceUnavailable)?;
        let response = read_health(&mut stream, deadline)?;
        #[derive(Deserialize)]
        struct Health { service_id: String, instance_id: String, status: String }
        let health: Health = serde_json::from_slice(&response).map_err(|_| Error::ServiceUnavailable)?;
        if health.service_id != self.service_id || health.instance_id != self.instance_id || health.status != "running" {
            return Err(Error::ServiceChanged);
        }
        Ok(())
    }
}

/// Stop the service Shinatama started. An external service is left alone.
pub fn stop_owned(base: &StorageLocation) {
    if std::env::var_os("MUKADE_DATA_DIR").is_some() {
        return;
    }
    let own = own_directory(base);
    if fs::symlink_metadata(own.join("bus")).is_ok()
        && let Some(program) = bundled_program()
    {
        let _ = run_bundled(&program, &own, "stop");
    }
}

pub(crate) fn own_directory(base: &StorageLocation) -> PathBuf {
    base.directory().join("mukade")
}

/// The Mukade program Shinatama ships (`crates/mukade`, built from the pinned revision), at its
/// one fixed place: `mukade\mukade.exe` beside the running program, or one folder up for programs
/// in `cli\`. A development build puts it at the same place (`npm run desktop`). The path is part
/// of every hook command, so of Codex's trust in those hooks: it never moves.
pub(crate) fn bundled_program() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let folder = executable.parent()?;
    [Some(folder), folder.parent()].into_iter().flatten()
        .map(|root| root.join("mukade").join("mukade.exe"))
        .find(|program| {
            reject_reparse(program).is_ok() && fs::metadata(program).is_ok_and(|m| m.is_file())
        })
}

/// One bounded `mukade chat <start|stop>` on the owned directory, without a console.
/// A failure carries the first line Mukade wrote about it. Mukade keeps its standard
/// handles out of the service it detaches, so the pipe closes when this command ends.
fn run_bundled(program: &Path, directory: &Path, operation: &str) -> Result<(), String> {
    let mut environment: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
    clear_environment(&mut environment);
    let mut command = std::process::Command::new(program);
    command.args(["chat", operation])
        .env_clear().envs(environment)
        .env("MUKADE_DATA_DIR", directory)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = command.spawn().map_err(|_| "The bundled Mukade program could not be run.".to_owned())?;
    let deadline = Instant::now() + BUNDLED_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => {
                // Mukade's message is one short line, far below the pipe's buffer.
                let mut text = String::new();
                if let Some(stream) = child.stderr.take() {
                    let _ = stream.take(2048).read_to_string(&mut text);
                }
                let line: String = text.lines().next().unwrap_or_default()
                    .chars().filter(|c| !c.is_control()).take(400).collect();
                return Err(if line.is_empty() { format!("Mukade chat {operation} failed.") } else { line });
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Mukade chat {operation} did not finish in time."));
            }
        }
    }
}

pub fn clear_environment(environment: &mut BTreeMap<OsString, OsString>) {
    // The app may itself have been started from a Mukade-managed terminal.
    // Its member/listen settings and bearer must never become the child's
    // membership. MUKADE_DATA_DIR selects the service and remains inherited.
    const SESSION_NAMES: &[&str] = &[
        TOKEN_ENV, "MUKADE_CHAT_ROOM_ID", "MUKADE_CHAT_ROLE", "MUKADE_CHAT_LISTEN",
        "MUKADE_CHAT_MAX_WAKES", "MUKADE_CHAT_AUTHORIZATION", "MUKADE_CHAT_SERVICE_ID",
    ];
    environment.retain(|name, _| !SESSION_NAMES.iter().any(|reserved| name.eq_ignore_ascii_case(reserved)));
}

fn endpoint_address(endpoint: &str) -> Result<SocketAddr, Error> {
    let port = endpoint.strip_prefix("http://127.0.0.1:")
        .and_then(|s| s.strip_suffix("/mcp"))
        .ok_or(Error::InvalidMetadata)?;
    let number: u16 = port.parse().map_err(|_| Error::InvalidMetadata)?;
    if number == 0 || number.to_string() != port { return Err(Error::InvalidMetadata); }
    Ok(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, number)))
}

fn canonical_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| id.hyphenated().to_string() == value)
}

fn same_path(expected: &Path, actual: &Path) -> bool {
    fn key(path: &Path) -> Option<String> {
        let text = path.to_str()?;
        let text = text.strip_prefix(r"\\?\").unwrap_or(text);
        Some(text.trim_end_matches(['\\', '/']).replace('/', "\\").to_ascii_lowercase())
    }
    actual.is_absolute() && key(expected).is_some() && key(expected) == key(actual)
}

pub(crate) fn reject_reparse(path: &Path) -> Result<(), Error> {
    let metadata = fs::symlink_metadata(path).map_err(|_| Error::PrivateStorageRequired)?;
    if metadata.file_type().is_symlink() { return Err(Error::PrivateStorageRequired); }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 { return Err(Error::PrivateStorageRequired); }
    }
    Ok(())
}

fn read_private(directory: &StorageLocation, name: &str, limit: usize) -> Result<Vec<u8>, Error> {
    let path = directory.directory().join(name);
    let file = directory.open_owner_only_file(&path).map_err(|_| Error::PrivateStorageRequired)?;
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64).read_to_end(&mut bytes).map_err(|_| Error::PrivateStorageRequired)?;
    if bytes.len() > limit { return Err(Error::InvalidMetadata); }
    Ok(bytes)
}

fn remaining(deadline: Instant) -> Result<Duration, Error> {
    deadline.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()).ok_or(Error::ServiceUnavailable)
}

fn read_health(stream: &mut TcpStream, deadline: Instant) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 1024];
    let (header_end, body_len) = loop {
        if let Some(at) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let end = at + 4;
            if end > MAX_HTTP_HEADER { return Err(Error::ServiceUnavailable); }
            break (end, health_content_length(&bytes[..end])?);
        }
        if bytes.len() >= MAX_HTTP_HEADER { return Err(Error::ServiceUnavailable); }
        stream.set_read_timeout(Some(remaining(deadline)?)).map_err(|_| Error::ServiceUnavailable)?;
        let size = (MAX_HTTP_HEADER - bytes.len()).min(chunk.len());
        let n = stream.read(&mut chunk[..size]).map_err(|_| Error::ServiceUnavailable)?;
        if n == 0 { return Err(Error::ServiceUnavailable); }
        bytes.extend_from_slice(&chunk[..n]);
    };
    // Without a length the body ends where the service closes the connection.
    let limit = header_end + body_len.unwrap_or(MAX_HEALTH_BODY);
    if bytes.len() > limit { return Err(Error::ServiceUnavailable); }
    while bytes.len() < limit {
        stream.set_read_timeout(Some(remaining(deadline)?)).map_err(|_| Error::ServiceUnavailable)?;
        let size = (limit - bytes.len()).min(chunk.len());
        let n = stream.read(&mut chunk[..size]).map_err(|_| Error::ServiceUnavailable)?;
        if n == 0 {
            if body_len.is_some() { return Err(Error::ServiceUnavailable); }
            break;
        }
        bytes.extend_from_slice(&chunk[..n]);
    }
    if body_len.is_none() {
        // More than the bound, or nothing at all, is not a health answer.
        let mut extra = [0u8; 1];
        stream.set_read_timeout(Some(remaining(deadline)?)).map_err(|_| Error::ServiceUnavailable)?;
        if bytes.len() == header_end || (bytes.len() == limit && stream.read(&mut extra).map_err(|_| Error::ServiceUnavailable)? != 0) {
            return Err(Error::ServiceUnavailable);
        }
    }
    Ok(bytes.split_off(header_end))
}

fn health_content_length(header: &[u8]) -> Result<Option<usize>, Error> {
    let text = std::str::from_utf8(header).map_err(|_| Error::ServiceUnavailable)?;
    let mut lines = text.split("\r\n");
    let status = lines.next().ok_or(Error::ServiceUnavailable)?;
    if !status.starts_with("HTTP/1.1 200 ") && !status.starts_with("HTTP/1.0 200 ") {
        return Err(Error::ServiceUnavailable); // redirects are never followed
    }
    let mut length = None;
    for line in lines.take_while(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':').ok_or(Error::ServiceUnavailable)?;
        if name.eq_ignore_ascii_case("transfer-encoding") { return Err(Error::ServiceUnavailable); }
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some() { return Err(Error::ServiceUnavailable); }
            let size: usize = value.trim().parse().map_err(|_| Error::ServiceUnavailable)?;
            if size == 0 || size > MAX_HEALTH_BODY { return Err(Error::ServiceUnavailable); }
            length = Some(size);
        }
    }
    Ok(length)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    const SERVICE: &str = "a234567890abcdef1234567890abcdef";
    const INSTANCE: &str = "ba85b96f-f88f-4fb0-96c3-45e0fa8ec7d0";
    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn metadata(port: u16) -> Value {
        json!({"data_dir":std::env::temp_dir(),"endpoint":format!("http://127.0.0.1:{port}/mcp"),
            "pid":12,"service_id":SERVICE,"instance_id":INSTANCE,"server_version":"0.1.0"})
    }
    fn connection(port: u16) -> Connection {
        Connection::from_files(&std::env::temp_dir(), &serde_json::to_vec(&metadata(port)).unwrap(), TOKEN.as_bytes()).unwrap()
    }

    #[test]
    fn rejects_remote_ambiguous_and_injected_endpoints_before_using_credential() {
        for endpoint in ["https://127.0.0.1:1/mcp", "http://localhost:1/mcp", "http://127.0.0.1:0/mcp",
            "http://127.0.0.1:0001/mcp", "http://127.0.0.1:1/mcp?x=1", "http://user@127.0.0.1:1/mcp",
            "http://127.0.0.1:1/mcp\r\nInjected: value", "http://example.com:1/mcp"] {
            assert!(endpoint_address(endpoint).is_err(), "{endpoint}");
        }
        assert_eq!(endpoint_address("http://127.0.0.1:33210/mcp").unwrap().port(), 33210);
    }

    #[test]
    fn validates_storage_identity_and_credentials() {
        for (field, value) in [("service_id", json!("not-a-service")), ("instance_id", json!("latest")),
            ("data_dir", json!("relative")), ("pid", json!(0)), ("extra", json!(true))] {
            let mut state = metadata(1); state[field] = value;
            assert!(Connection::from_files(&std::env::temp_dir(), &serde_json::to_vec(&state).unwrap(), TOKEN.as_bytes()).is_err());
        }
        for token in [b"short".as_slice(), b"\r\nnothex".as_slice(), &[b'g';64]] {
            assert!(Connection::from_files(&std::env::temp_dir(), &serde_json::to_vec(&metadata(1)).unwrap(), token).is_err());
        }
    }

    #[test]
    fn generated_config_contains_only_environment_reference_and_replaces_inherited_secret() {
        let connection = connection(33210);
        let claude = connection.claude_server().to_string();
        let args = connection.codex_arguments();
        assert_eq!(args.len(), 2);
        let codex = args[1].to_str().unwrap();
        assert!(!claude.contains(TOKEN) && !codex.contains(TOKEN));
        assert!(claude.contains("${SHINATAMA_MUKADE_TOKEN}"));
        assert!(codex.contains("bearer_token_env_var=\"SHINATAMA_MUKADE_TOKEN\""));
        assert!(codex.contains("tool_timeout_sec=330"));
        let mut env = BTreeMap::from([("shinatama_mukade_token".into(), "old".into()), ("KEEP".into(), "yes".into())]);
        env.insert("mukade_chat_room_id".into(), "parent-room".into());
        env.insert("MUKADE_CHAT_AUTHORIZATION".into(), "parent-bearer".into());
        connection.apply_environment(&mut env);
        assert_eq!(env.len(), 2);
        assert_eq!(env.get(&OsString::from(TOKEN_ENV)), Some(&OsString::from(TOKEN)));
        clear_environment(&mut env);
        assert_eq!(env, BTreeMap::from([("KEEP".into(), "yes".into())]));
    }

    /// `length: false` is the real service's HTTP/1.0 answer: no length, then close.
    fn health_server(body: Value, status: &str, length: bool) -> (u16, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let status = status.to_owned();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0]; stream.read_exact(&mut byte).unwrap(); request.push(byte[0]);
                assert!(request.len() < 2048);
            }
            let request = String::from_utf8(request).unwrap();
            assert!(request.starts_with("GET /health HTTP/1.0\r\n"));
            assert!(request.contains(&format!("Authorization: Bearer {TOKEN}\r\n")));
            assert!(request.contains(&format!("X-Mukade-Service-Id: {SERVICE}\r\n")));
            let body = body.to_string();
            if length {
                write!(stream,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            } else {
                write!(stream,"HTTP/1.0 {status}\r\ncontent-type: application/json\r\n\r\n{body}").unwrap();
            }
        });
        (port, handle)
    }

    #[test]
    fn health_pins_both_identities_and_requires_running_without_redirects() {
        for (body, status, expected) in [
            (json!({"service_id":SERVICE,"instance_id":INSTANCE,"status":"running"}), "200 OK", Ok(())),
            (json!({"service_id":SERVICE,"instance_id":"other","status":"running"}), "200 OK", Err(Error::ServiceChanged)),
            (json!({"service_id":SERVICE,"instance_id":INSTANCE,"status":"stopping"}), "200 OK", Err(Error::ServiceChanged)),
            (json!({}), "302 Found", Err(Error::ServiceUnavailable)),
        ] {
            let (port, worker) = health_server(body, status, true);
            assert_eq!(connection(port).check_health(), expected);
            worker.join().unwrap();
        }
    }

    #[test]
    fn health_accepts_the_service_answer_ended_by_close_within_its_bound() {
        let running = json!({"service_id":SERVICE,"instance_id":INSTANCE,"status":"running","protocol_version":"2025-11-25"});
        let mut oversized = running.clone();
        oversized["padding"] = json!("x".repeat(MAX_HEALTH_BODY));
        for (body, expected) in [
            (running, Ok(())),
            (oversized, Err(Error::ServiceUnavailable)),
        ] {
            let (port, worker) = health_server(body, "200 OK", false);
            assert_eq!(connection(port).check_health(), expected);
            worker.join().unwrap();
        }
    }

    #[test]
    fn health_response_has_bounded_unambiguous_framing() {
        assert_eq!(health_content_length(b"HTTP/1.0 200 OK\r\n\r\n"), Ok(None));
        assert_eq!(health_content_length(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\n"), Ok(Some(3)));
        for header in ["HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n", "HTTP/1.1 200 OK\r\nContent-Length: 8192\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nContent-Length: 3\r\n\r\n",
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 3\r\n\r\n"] {
            assert!(health_content_length(header.as_bytes()).is_err());
        }
    }
}
