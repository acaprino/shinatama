//! Minimal harness command-hook bridge. No discovery, proxy, files or stdout.
//! The local receiver authenticates the instance and validates its exact cwd.
use shinatama_engine::harness_session_events::{
    HarnessKind, MAX_BODY_BYTES as INPUT_LIMIT, normalize_hook_payload,
};
#[cfg(test)]
use std::path::Path;
use std::{
    env,
    ffi::OsString,
    io::{Read, Write},
    net::{Ipv4Addr, Shutdown, SocketAddr, SocketAddrV4, TcpStream},
    process::ExitCode,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const RESPONSE_LIMIT: usize = 1024;
const TIME_LIMIT: Duration = Duration::from_secs(2);
type Result<T> = std::result::Result<T, ()>;

fn port_argument(mut arguments: impl Iterator<Item = OsString>) -> Result<(HarnessKind, u16)> {
    let command = arguments.next().ok_or(())?;
    let harness = match command.to_str() {
        Some("claude-session") => HarnessKind::ClaudeCode,
        Some("codex-session") => HarnessKind::Codex,
        _ => return Err(()),
    };
    let port = arguments.next().ok_or(())?;
    if arguments.next().is_some() {
        return Err(());
    }
    let port = port.to_str().ok_or(())?;
    if port.is_empty() || port.len() > 5 || !port.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(());
    }
    port.parse::<u16>()
        .ok()
        .filter(|value| *value != 0)
        .map(|port| (harness, port))
        .ok_or(())
}

fn valid_instance(instance: &str) -> bool {
    instance.strip_prefix("i_").is_some_and(|suffix| {
        suffix.len() == 12
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || (b'2'..=b'7').contains(&byte))
    })
}

fn valid_token(token: &str) -> bool {
    token.len() == 64
        && token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn event_body(harness: HarnessKind, input: &[u8]) -> Result<Vec<u8>> {
    normalize_hook_payload(harness, input).map_err(|_| ())
}

fn read_input() -> Result<Vec<u8>> {
    let deadline = Instant::now() + TIME_LIMIT;
    let (send, receive) = mpsc::sync_channel(1);
    // A stalled producer must not hold the hook open. Returning from main ends
    // the process, including this detached reader; it never owns other resources.
    thread::Builder::new()
        .name("hook-input".into())
        .spawn(move || {
            let mut input = Vec::new();
            let result = std::io::stdin()
                .lock()
                .take((INPUT_LIMIT + 1) as u64)
                .read_to_end(&mut input)
                .map_err(|_| ())
                .and({
                    if input.len() <= INPUT_LIMIT {
                        Ok(input)
                    } else {
                        Err(())
                    }
                });
            let _ = send.send(result);
        })
        .map_err(|_| ())?;
    receive.recv_timeout(remaining(deadline)?).map_err(|_| ())?
}

fn remaining(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|value| !value.is_zero())
        .ok_or(())
}

fn accepted_response(response: &[u8]) -> bool {
    if response.len() > RESPONSE_LIMIT || !response.ends_with(b"\r\n\r\n") {
        return false;
    }
    let Ok(response) = std::str::from_utf8(response) else {
        return false;
    };
    if !response.is_ascii() {
        return false;
    }
    let mut lines = response[..response.len() - 4].split("\r\n");
    let Some(status) = lines.next() else {
        return false;
    };
    let Some(reason) = status.strip_prefix("HTTP/1.1 204") else {
        return false;
    };
    if (!reason.is_empty() && !reason.starts_with(' '))
        || status.bytes().any(|byte| byte.is_ascii_control())
    {
        return false;
    }
    let mut content_length_seen = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return false;
        };
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
            || value
                .bytes()
                .any(|byte| byte.is_ascii_control() && byte != b'\t')
            || name.eq_ignore_ascii_case("transfer-encoding")
        {
            return false;
        }
        if name.eq_ignore_ascii_case("content-length") {
            if content_length_seen || value.trim() != "0" {
                return false;
            }
            content_length_seen = true;
        }
    }
    true
}

fn post(harness: HarnessKind, port: u16, instance: &str, token: &str, body: &[u8]) -> Result<()> {
    let deadline = Instant::now() + TIME_LIMIT;
    let address = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&address, remaining(deadline)?).map_err(|_| ())?;
    // The validated token appears only in this in-memory request, never argv,
    // files, logs or stdout. TcpStream uses no proxy configuration or redirects.
    let header = format!(
        "POST {}{instance} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        harness.route_prefix(),
        body.len()
    );
    for bytes in [header.as_bytes(), body] {
        let mut rest = bytes;
        while !rest.is_empty() {
            stream
                .set_write_timeout(Some(remaining(deadline)?))
                .map_err(|_| ())?;
            let written = stream.write(rest).map_err(|_| ())?;
            if written == 0 {
                return Err(());
            }
            rest = &rest[written..];
        }
    }
    stream.shutdown(Shutdown::Write).map_err(|_| ())?;
    let mut response = [0_u8; RESPONSE_LIMIT + 1];
    let mut count = 0;
    loop {
        stream
            .set_read_timeout(Some(remaining(deadline)?))
            .map_err(|_| ())?;
        let read = stream.read(&mut response[count..]).map_err(|_| ())?;
        if read == 0 {
            break;
        }
        count += read;
        if count > RESPONSE_LIMIT {
            return Err(());
        }
    }
    if accepted_response(&response[..count]) {
        Ok(())
    } else {
        Err(())
    }
}

fn run() -> Result<()> {
    let (harness, port) = port_argument(env::args_os().skip(1))?;
    // These are the only environment reads. In particular, no proxy/home/auth
    // settings are discovered and no agent token is accepted from arguments.
    let instance = env::var("SHINATAMA_INSTANCE").map_err(|_| ())?;
    let token = env::var("SHINATAMA_INSTANCE_TOKEN").map_err(|_| ())?;
    if !valid_instance(&instance) || !valid_token(&token) {
        return Err(());
    }
    let body = event_body(harness, &read_input()?)?;
    post(harness, port, &instance, &token, &body)
}

fn main() -> ExitCode {
    // A command hook must never add context or expose an error/secret on stdout.
    if run().is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selectors_cannot_change_the_endpoint_or_inject_headers() {
        let port =
            |values: &[&str]| port_argument(values.iter().map(|value| OsString::from(*value)));
        assert_eq!(
            port(&["claude-session", "65535"]),
            Ok((HarnessKind::ClaudeCode, 65535))
        );
        assert_eq!(
            port(&["codex-session", "65535"]),
            Ok((HarnessKind::Codex, 65535))
        );
        for values in [
            vec!["claude-session", "0"],
            vec!["claude-session", "65536"],
            vec!["claude-session", "+80"],
            vec!["claude-session", "80", "extra"],
            vec!["other", "80"],
        ] {
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
        let cwd = if cfg!(windows) {
            r"C:\synthetic-project"
        } else {
            "/synthetic-project"
        };
        let source = serde_json::json!({"hook_event_name":"SessionStart", "session_id":"01234567-89ab-4def-8123-456789abcdef", "cwd":cwd, "transcript_path":"omitted", "prompt":"omitted"});
        let parsed: serde_json::Value = serde_json::from_slice(
            &event_body(
                HarnessKind::ClaudeCode,
                &serde_json::to_vec(&source).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(parsed.as_object().unwrap().len(), 3);
        assert!(parsed.get("prompt").is_none());
        for (key, value) in [
            ("hook_event_name", "Stop"),
            ("session_id", "01234567-89AB-4DEF-8123-456789ABCDEF"),
            ("session_id", "0123456789ab4def8123456789abcdef"),
            ("session_id", "00000000-0000-0000-0000-000000000000"),
            ("cwd", "relative/project"),
        ] {
            let mut invalid = source.clone();
            invalid[key] = value.into();
            assert!(
                event_body(
                    HarnessKind::ClaudeCode,
                    &serde_json::to_vec(&invalid).unwrap()
                )
                .is_err()
            );
        }
        assert!(event_body(HarnessKind::ClaudeCode, br#"{"hook_event_name":"SessionStart","hook_event_name":"SessionEnd","session_id":"01234567-89ab-4def-8123-456789abcdef","cwd":"/synthetic"}"#).is_err());
        assert!(event_body(HarnessKind::ClaudeCode, &vec![b' '; INPUT_LIMIT + 1]).is_err());

        let mut codex = source.clone();
        for kind in ["startup", "resume", "clear", "compact", "fork"] {
            codex["source"] = kind.into();
            let body =
                event_body(HarnessKind::Codex, &serde_json::to_vec(&codex).unwrap()).unwrap();
            let normalized: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(normalized.as_object().unwrap().len(), 4);
            assert_eq!(normalized["source"], kind);
            assert!(normalized.get("transcript_path").is_none());
            assert!(normalized.get("prompt").is_none());
        }
        for invalid_source in [
            serde_json::Value::Null,
            "Startup".into(),
            "unknown".into(),
            1.into(),
        ] {
            codex["source"] = invalid_source;
            assert!(event_body(HarnessKind::Codex, &serde_json::to_vec(&codex).unwrap()).is_err());
        }
        assert!(event_body(HarnessKind::Codex, &serde_json::to_vec(&source).unwrap()).is_err());
        let duplicate = format!(
            r#"{{"hook_event_name":"SessionStart","session_id":"01234567-89ab-4def-8123-456789abcdef","cwd":{},"source":"startup","source":"resume"}}"#,
            serde_json::to_string(cwd).unwrap()
        );
        assert!(event_body(HarnessKind::Codex, duplicate.as_bytes()).is_err());
    }

    #[test]
    fn response_must_be_a_bounded_bodyless_204() {
        assert!(accepted_response(
            b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n"
        ));
        for response in [
            b"HTTP/1.1 200 OK\r\n\r\n".as_slice(),
            b"HTTP/1.1 2040\r\n\r\n",
            b"HTTP/1.1 204 No Content\r\nContent-Length: 1\r\n\r\nx",
            b"HTTP/1.1 204 No Content\r\nTransfer-Encoding: chunked\r\n\r\n",
            b"HTTP/1.1 204 No Content\r\n\r\nextra",
            b"HTTP/1.1 204 No Content\r\nInvalid\r\n\r\n",
        ] {
            assert!(!accepted_response(response));
        }
    }

    #[test]
    fn real_receivers_accept_both_dialects_and_refuse_foreign_routes_and_bindings() {
        use shinatama_engine::{
            harness_session_events::{SessionEventKind, SessionEventReceiver, SessionStartSource},
            launch::identity::LaunchIdentity,
        };
        // The production serializer/socket client talks to the actual worker.
        // No child executable, environment lookup, files or external provider.
        for harness in [HarnessKind::ClaudeCode, HarnessKind::Codex] {
            let identity = LaunchIdentity::generate("t_abcdefghijkl").unwrap();
            let secret = identity.child_secret_override();
            let token = secret.value().to_str().unwrap();
            let project = Path::new(if cfg!(windows) {
                r"C:\shinatama-hook-synthetic"
            } else {
                "/shinatama-hook-synthetic"
            });
            let receiver =
                SessionEventReceiver::start_for(harness, identity.record(), project).unwrap();
            assert_eq!(receiver.harness(), harness);
            let session_id = "01234567-89ab-4def-8123-456789abcdef";
            for (name, kind) in [
                ("SessionStart", SessionEventKind::SessionStart),
                ("SessionEnd", SessionEventKind::SessionEnd),
            ] {
                let mut input = serde_json::json!({"hook_event_name":name, "session_id":session_id, "cwd":project,
                    "transcript_path":"must-not-be-forwarded", "prompt":"must-not-be-forwarded"});
                let source =
                    if harness == HarnessKind::Codex && kind == SessionEventKind::SessionStart {
                        input["source"] = "startup".into();
                        Some(SessionStartSource::Startup)
                    } else {
                        None
                    };
                let body = event_body(harness, &serde_json::to_vec(&input).unwrap()).unwrap();
                assert!(
                    post(
                        harness,
                        receiver.port(),
                        identity.instance_id(),
                        token,
                        &body
                    )
                    .is_ok()
                );
                // 204 means queued, not storage or launch authorization.
                let event = receiver.try_next().unwrap().unwrap();
                assert_eq!(event.instance_id, identity.instance_id());
                assert_eq!(event.thread_id, "t_abcdefghijkl");
                assert_eq!(event.session_id, session_id);
                assert_eq!(event.kind, kind);
                assert_eq!(event.source, source);
                assert_eq!(event.cwd, project.to_str().unwrap());
                assert!(receiver.try_next().unwrap().is_none());
            }
            let input = |cwd: &Path| {
                serde_json::to_vec(&serde_json::json!({
                "hook_event_name":"SessionStart", "session_id":session_id, "cwd":cwd, "source":"startup"
            })).unwrap()
            };
            let wrong_project =
                event_body(harness, &input(&project.join("different-project"))).unwrap();
            assert!(
                post(
                    harness,
                    receiver.port(),
                    identity.instance_id(),
                    token,
                    &wrong_project
                )
                .is_err()
            );
            let body = event_body(harness, &input(project)).unwrap();
            let other = LaunchIdentity::generate("t_abcdefghijkl").unwrap();
            let other_secret = other.child_secret_override();
            assert!(
                post(
                    harness,
                    receiver.port(),
                    identity.instance_id(),
                    other_secret.value().to_str().unwrap(),
                    &body
                )
                .is_err()
            );
            assert!(post(harness, receiver.port(), other.instance_id(), token, &body).is_err());
            let foreign_harness = if harness == HarnessKind::ClaudeCode {
                HarnessKind::Codex
            } else {
                HarnessKind::ClaudeCode
            };
            assert!(
                post(
                    foreign_harness,
                    receiver.port(),
                    identity.instance_id(),
                    token,
                    &body
                )
                .is_err()
            );
            assert!(receiver.try_next().unwrap().is_none());
            assert_eq!(receiver.status().rejected_requests, 4);
            receiver.stop();
            let deadline = Instant::now() + Duration::from_secs(1);
            while !receiver.status().stopped && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            assert!(receiver.status().stopped);
            assert!(!receiver.status().failed);
        }
    }
}
