//! Pi sign-in status contract (PI9). The only command is
//! `auth check --json --no-refresh --provider <id>`: `--no-refresh` keeps the
//! check from refreshing (writing) OAuth credentials, and `--credentials` or the
//! `print-*` commands are never produced. Parsing keeps only the status and a
//! closed reason; every other field, including any token-shaped value, is dropped.
use serde::Serialize;
use std::ffi::OsString;

/// Pi provider identifiers are lowercase slugs such as `anthropic` or
/// `openai-codex`. A provider is a check argument, never a path or a flag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderId(String);
impl ProviderId {
    pub fn new(value: &str) -> Option<Self> {
        let bytes = value.as_bytes();
        (!bytes.is_empty()
            && bytes.len() <= 64
            && bytes[0].is_ascii_lowercase()
            && bytes.iter().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(byte)
            }))
        .then(|| Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Arguments after the script token.
pub fn check_arguments(provider: &ProviderId) -> Vec<OsString> {
    [
        "auth",
        "check",
        "--json",
        "--no-refresh",
        "--provider",
        provider.as_str(),
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotReadyReason {
    CredentialsNotConfigured,
    ProviderNotFound,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AuthStatus {
    Ready,
    NotReady {
        reason: NotReadyReason,
    },
    /// Malformed, mismatched, oversized or ambiguous output. Never "signed out".
    Unknown,
}

/// Observed 1.4.0 shape: exit 0 for ready, exit 1 with
/// `{"status":"not_ready","provider":"<id>","reason":"<reason>"}` otherwise.
/// The provider echo must match the request; exit code and status must agree.
pub fn parse_check(provider: &ProviderId, exit_code: Option<u32>, stdout: &[u8]) -> AuthStatus {
    if stdout.len() > 4096 {
        return AuthStatus::Unknown;
    }
    let Ok(text) = std::str::from_utf8(stdout) else {
        return AuthStatus::Unknown;
    };
    let text = text.trim();
    if text.lines().count() != 1 {
        return AuthStatus::Unknown;
    }
    let Ok(serde_json::Value::Object(record)) = serde_json::from_str::<serde_json::Value>(text)
    else {
        return AuthStatus::Unknown;
    };
    if record.get("provider").and_then(serde_json::Value::as_str) != Some(provider.as_str()) {
        return AuthStatus::Unknown;
    }
    match (
        exit_code,
        record.get("status").and_then(serde_json::Value::as_str),
    ) {
        (Some(0), Some("ready")) => AuthStatus::Ready,
        (Some(1), Some("not_ready")) => AuthStatus::NotReady {
            reason: match record.get("reason").and_then(serde_json::Value::as_str) {
                Some("credentials_not_configured") => NotReadyReason::CredentialsNotConfigured,
                Some("provider_not_found") => NotReadyReason::ProviderNotFound,
                _ => NotReadyReason::Other,
            },
        },
        _ => AuthStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anthropic() -> ProviderId {
        ProviderId::new("anthropic").unwrap()
    }

    #[test]
    fn check_never_refreshes_or_requests_credentials() {
        let arguments = check_arguments(&anthropic());
        assert_eq!(
            arguments,
            [
                "auth",
                "check",
                "--json",
                "--no-refresh",
                "--provider",
                "anthropic"
            ]
            .map(OsString::from)
        );
        assert!(
            !arguments
                .iter()
                .any(|token| token == "--credentials"
                    || token.to_string_lossy().starts_with("print-"))
        );
        for invalid in ["", "Anthropic", "--credentials", "a b", "../x", "-x", "é"] {
            assert!(ProviderId::new(invalid).is_none(), "{invalid:?}");
        }
        assert!(ProviderId::new("openai-codex").is_some());
    }

    #[test]
    fn installed_1_4_0_fixtures_map_to_closed_statuses() {
        // Captured from pi 1.4.0 in an empty scratch PI_CODING_AGENT_DIR.
        assert_eq!(
            parse_check(
                &anthropic(),
                Some(1),
                br#"{"status":"not_ready","provider":"anthropic","reason":"credentials_not_configured"}"#
            ),
            AuthStatus::NotReady {
                reason: NotReadyReason::CredentialsNotConfigured
            }
        );
        let bogus = ProviderId::new("bogus-provider").unwrap();
        assert_eq!(
            parse_check(
                &bogus,
                Some(1),
                b"{\"status\":\"not_ready\",\"provider\":\"bogus-provider\",\"reason\":\"provider_not_found\"}\n"
            ),
            AuthStatus::NotReady {
                reason: NotReadyReason::ProviderNotFound
            }
        );
    }

    #[test]
    fn secrets_are_dropped_and_disagreements_are_unknown() {
        let ready = br#"{"status":"ready","provider":"anthropic","credential":"sk-ant-SECRET"}"#;
        let status = parse_check(&anthropic(), Some(0), ready);
        assert_eq!(status, AuthStatus::Ready);
        let projected = format!("{status:?}{}", serde_json::to_string(&status).unwrap());
        assert!(!projected.contains("SECRET"));
        for (exit, output) in [
            (Some(1), ready.as_slice()),
            (
                Some(0),
                br#"{"status":"not_ready","provider":"anthropic"}"#.as_slice(),
            ),
            (
                Some(0),
                br#"{"status":"ready","provider":"openai"}"#.as_slice(),
            ),
            (Some(2), b"Error: Auth checks require --provider".as_slice()),
            (
                None,
                br#"{"status":"ready","provider":"anthropic"}"#.as_slice(),
            ),
            (
                Some(0),
                b"{\"status\":\"ready\",\"provider\":\"anthropic\"}\n{\"status\":\"ready\"}"
                    .as_slice(),
            ),
        ] {
            assert_eq!(parse_check(&anthropic(), exit, output), AuthStatus::Unknown);
        }
    }
}
