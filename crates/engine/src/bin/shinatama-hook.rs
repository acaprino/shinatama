//! Minimal Claude command-hook bridge. No discovery, proxy, files or stdout.
//! The local receiver authenticates the instance and validates its exact cwd.
use serde::{Deserialize, Serialize};
use std::{
    env,
    ffi::OsString,
    io::{Read, Write},
    net::{Ipv4Addr, Shutdown, SocketAddr, SocketAddrV4, TcpStream},
    path::{Component, Path},
    process::ExitCode,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const INPUT_LIMIT: usize = 16 * 1024;
const RESPONSE_LIMIT: usize = 1024;
const CWD_LIMIT: usize = 4096;
const TIME_LIMIT: Duration = Duration::from_secs(2);
type Result<T> = std::result::Result<T, ()>;

#[derive(Deserialize, Serialize)]
enum EventName {
    SessionStart,
    SessionEnd,
}

// Unknown Claude fields are ignored, never forwarded. Duplicate known fields
// are rejected by serde, so there is only one interpretation of the binding.
#[derive(Deserialize, Serialize)]
struct SessionEvent {
    hook_event_name: EventName,
    session_id: String,
    cwd: String,
}

fn port_argument(mut arguments: impl Iterator<Item = OsString>) -> Result<u16> {
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new("claude-session")) {
        return Err(());
    }
    let port = arguments.next().ok_or(())?;
    if arguments.next().is_some() {
        return Err(());
    }
    let port = port.to_str().ok_or(())?;
    if port.is_empty() || port.len() > 5 || !port.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(());
    }
    port.parse::<u16>().ok().filter(|value| *value != 0).ok_or(())
}

fn valid_instance(instance: &str) -> bool {
    instance.strip_prefix("i_").is_some_and(|suffix| {
        suffix.len() == 12
            && suffix.bytes().all(|byte| byte.is_ascii_lowercase() || (b'2'..=b'7').contains(&byte))
    })
}

fn valid_token(token: &str) -> bool {
    token.len() == 64
        && token.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn event_body(input: &[u8]) -> Result<Vec<u8>> {
    if input.is_empty() || input.len() > INPUT_LIMIT {
        return Err(());
    }
    let event: SessionEvent = serde_json::from_slice(input).map_err(|_| ())?;
    let session = uuid::Uuid::parse_str(&event.session_id).map_err(|_| ())?;
    if session.hyphenated().to_string() != event.session_id
        || session.get_variant() != uuid::Variant::RFC4122
        || session.get_version().is_none()
        || event.cwd.is_empty()
        || event.cwd.len() > CWD_LIMIT
        || event.cwd.chars().any(char::is_control)
        || !Path::new(&event.cwd).is_absolute()
        || Path::new(&event.cwd).components().any(|part| matches!(part, Component::ParentDir))
    {
        return Err(());
    }
    serde_json::to_vec(&event).map_err(|_| ())
}

fn read_input() -> Result<Vec<u8>> {
    let deadline = Instant::now() + TIME_LIMIT;
    let (send, receive) = mpsc::sync_channel(1);
    // A stalled producer must not hold the hook open. Returning from main ends
    // the process, including this detached reader; it never owns other resources.
    thread::Builder::new().name("hook-input".into()).spawn(move || {
        let mut input = Vec::new();
        let result = std::io::stdin().lock().take((INPUT_LIMIT + 1) as u64)
            .read_to_end(&mut input)
            .map_err(|_| ())
            .and_then(|_| if input.len() <= INPUT_LIMIT { Ok(input) } else { Err(()) });
        let _ = send.send(result);
    }).map_err(|_| ())?;
    receive.recv_timeout(remaining(deadline)?).map_err(|_| ())?
}

fn remaining(deadline: Instant) -> Result<Duration> {
    deadline.checked_duration_since(Instant::now()).filter(|value| !value.is_zero()).ok_or(())
}

fn accepted_response(response: &[u8]) -> bool {
    if response.len() > RESPONSE_LIMIT || !response.ends_with(b"\r\n\r\n") {
        return false;
    }
    let Ok(response) = std::str::from_utf8(response) else { return false; };
    if !response.is_ascii() {
        return false;
    }
    let mut lines = response[..response.len() - 4].split("\r\n");
    let Some(status) = lines.next() else { return false; };
    let Some(reason) = status.strip_prefix("HTTP/1.1 204") else { return false; };
    if (!reason.is_empty() && !reason.starts_with(' ')) || status.bytes().any(|byte| byte.is_ascii_control()) {
        return false;
    }
    let mut content_length_seen = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { return false; };
        if name.is_empty()
            || !name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
            || value.bytes().any(|byte| byte.is_ascii_control() && byte != b'\t')
            || name.eq_ignore_ascii_case("transfer-encoding")
        {
            return false;
        }
        if name.eq_ignore_ascii_case("content-length") {
            if content_length_seen || value.trim() != "0" { return false; }
            content_length_seen = true;
        }
    }
    true
}

fn post(port: u16, instance: &str, token: &str, body: &[u8]) -> Result<()> {
    let deadline = Instant::now() + TIME_LIMIT;
    let address = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&address, remaining(deadline)?).map_err(|_| ())?;
    // The validated token appears only in this in-memory request, never argv,
    // files, logs or stdout. TcpStream uses no proxy configuration or redirects.
    let header = format!(
        "POST /v1/claude/session/{instance} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    for bytes in [header.as_bytes(), body] {
        let mut rest = bytes;
        while !rest.is_empty() {
            stream.set_write_timeout(Some(remaining(deadline)?)).map_err(|_| ())?;
            let written = stream.write(rest).map_err(|_| ())?;
            if written == 0 { return Err(()); }
            rest = &rest[written..];
        }
    }
    stream.shutdown(Shutdown::Write).map_err(|_| ())?;
    let mut response = [0_u8; RESPONSE_LIMIT + 1];
    let mut count = 0;
    loop {
        stream.set_read_timeout(Some(remaining(deadline)?)).map_err(|_| ())?;
        let read = stream.read(&mut response[count..]).map_err(|_| ())?;
        if read == 0 { break; }
        count += read;
        if count > RESPONSE_LIMIT { return Err(()); }
    }
    if accepted_response(&response[..count]) { Ok(()) } else { Err(()) }
}

fn run() -> Result<()> {
    let port = port_argument(env::args_os().skip(1))?;
    // These are the only environment reads. In particular, no proxy/home/auth
    // settings are discovered and no agent token is accepted from arguments.
    let instance = env::var("SHINATAMA_INSTANCE").map_err(|_| ())?;
    let token = env::var("SHINATAMA_INSTANCE_TOKEN").map_err(|_| ())?;
    if !valid_instance(&instance) || !valid_token(&token) { return Err(()); }
    let body = event_body(&read_input()?)?;
    post(port, &instance, &token, &body)
}

fn main() -> ExitCode {
    // A command hook must never add context or expose an error/secret on stdout.
    if run().is_ok() { ExitCode::SUCCESS } else { ExitCode::from(1) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selectors_cannot_change_the_endpoint_or_inject_headers() {
        let port = |values: &[&str]| port_argument(values.iter().map(|value| OsString::from(*value)));
        assert_eq!(port(&["claude-session", "65535"]), Ok(65535));
        for values in [vec!["claude-session", "0"], vec!["claude-session", "65536"], vec!["claude-session", "+80"], vec!["claude-session", "80", "extra"], vec!["other", "80"]] {
            assert!(port(&values).is_err());
        }
        assert!(valid_instance("i_abcdefghijkl"));
        assert!(!valid_instance("i_abcdefghijk1"));
        assert!(valid_token(&"a".repeat(64)));
        assert!(!valid_token(&"A".repeat(64)));
        assert!(!valid_token(&format!("{}\r\n", "a".repeat(62))));
    }

    #[test]
    fn payload_is_reduced_and_ambiguous_or_noncanonical_bindings_are_rejected() {
        let cwd = if cfg!(windows) { r"C:\synthetic-project" } else { "/synthetic-project" };
        let source = serde_json::json!({"hook_event_name":"SessionStart", "session_id":"01234567-89ab-4def-8123-456789abcdef", "cwd":cwd, "transcript_path":"omitted", "prompt":"omitted"});
        let parsed: serde_json::Value = serde_json::from_slice(&event_body(&serde_json::to_vec(&source).unwrap()).unwrap()).unwrap();
        assert_eq!(parsed.as_object().unwrap().len(), 3);
        assert!(parsed.get("prompt").is_none());
        for (key, value) in [("hook_event_name", "Stop"), ("session_id", "01234567-89AB-4DEF-8123-456789ABCDEF"), ("session_id", "0123456789ab4def8123456789abcdef"), ("session_id", "00000000-0000-0000-0000-000000000000"), ("cwd", "relative/project")] {
            let mut invalid = source.clone(); invalid[key] = value.into();
            assert!(event_body(&serde_json::to_vec(&invalid).unwrap()).is_err());
        }
        assert!(event_body(br#"{"hook_event_name":"SessionStart","hook_event_name":"SessionEnd","session_id":"01234567-89ab-4def-8123-456789abcdef","cwd":"/synthetic"}"#).is_err());
        assert!(event_body(&vec![b' '; INPUT_LIMIT + 1]).is_err());
    }

    #[test]
    fn response_must_be_a_bounded_bodyless_204() {
        assert!(accepted_response(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n"));
        for response in [b"HTTP/1.1 200 OK\r\n\r\n".as_slice(), b"HTTP/1.1 2040\r\n\r\n", b"HTTP/1.1 204 No Content\r\nContent-Length: 1\r\n\r\nx", b"HTTP/1.1 204 No Content\r\nTransfer-Encoding: chunked\r\n\r\n", b"HTTP/1.1 204 No Content\r\n\r\nextra", b"HTTP/1.1 204 No Content\r\nInvalid\r\n\r\n"] {
            assert!(!accepted_response(response));
        }
    }

    #[test]
    fn real_receiver_accepts_bridge_protocol_and_rejects_wrong_binding() {
        use shinatama_engine::{
            claude_session_events::{ClaudeSessionEventReceiver, SessionEventKind},
            launch::identity::LaunchIdentity,
        };

        // No child executable, environment lookup, filesystem or Claude API.
        // Exercise the production serializer/socket client against the actual
        // receiver rather than a mock that might repeat the same wire mistake.
        let identity = LaunchIdentity::generate("t_abcdefghijkl").unwrap();
        let secret = identity.child_secret_override();
        let token = secret.value().to_str().unwrap();
        let project = Path::new(if cfg!(windows) {
            r"C:\shinatama-hook-synthetic"
        } else {
            "/shinatama-hook-synthetic"
        });
        let receiver = ClaudeSessionEventReceiver::start(identity.record(), project).unwrap();
        let session_id = "01234567-89ab-4def-8123-456789abcdef";
        for (name, kind) in [
            ("SessionStart", SessionEventKind::SessionStart),
            ("SessionEnd", SessionEventKind::SessionEnd),
        ] {
            let input = serde_json::to_vec(&serde_json::json!({
                "hook_event_name": name,
                "session_id": session_id,
                "cwd": project,
                "transcript_path": "must-not-be-forwarded",
                "prompt": "must-not-be-forwarded",
            })).unwrap();
            let body = event_body(&input).unwrap();
            assert!(post(receiver.port(), identity.instance_id(), token, &body).is_ok());
            // 204 means queued, not durable storage or a launch authorization.
            let event = receiver.try_next().unwrap().unwrap();
            assert_eq!(event.instance_id, identity.instance_id());
            assert_eq!(event.thread_id, "t_abcdefghijkl");
            assert_eq!(event.session_id, session_id);
            assert_eq!(event.kind, kind);
            assert_eq!(event.cwd, project.to_str().unwrap());
            assert!(receiver.try_next().unwrap().is_none());
        }

        let body = event_body(&serde_json::to_vec(&serde_json::json!({
            "hook_event_name": "SessionStart", "session_id": session_id,
            "cwd": project.join("different-project"),
        })).unwrap()).unwrap();
        assert!(post(receiver.port(), identity.instance_id(), token, &body).is_err());
        let body = event_body(&serde_json::to_vec(&serde_json::json!({
            "hook_event_name": "SessionStart", "session_id": session_id, "cwd": project,
        })).unwrap()).unwrap();
        let other = LaunchIdentity::generate("t_abcdefghijkl").unwrap();
        let other_secret = other.child_secret_override();
        assert!(post(receiver.port(), identity.instance_id(), other_secret.value().to_str().unwrap(), &body).is_err());
        assert!(receiver.try_next().unwrap().is_none());
        assert_eq!(receiver.status().rejected_requests, 2);
        receiver.stop();
        let deadline = Instant::now() + Duration::from_secs(1);
        while !receiver.status().stopped && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(receiver.status().stopped);
        assert!(!receiver.status().failed);
    }
}
