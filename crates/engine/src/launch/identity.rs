//! Per-launch identity and a hash-only record; no persistence or process launch.
//! The secret crosses only the explicit, redacted child-environment override.

use serde::Serialize;
use sha2::{Digest, Sha256};
use shinatama_adapter::launch::environment::{ChildOverride, OverrideName};
use std::fmt::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;

const BASE32: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
const ID_BYTES: usize = 12;
const TOKEN_BYTES: usize = 32;
const WIRE_BYTES: usize = TOKEN_BYTES * 2;
const HASH_DOMAIN: &[u8] = b"shinatama.instance-token.v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityError {
    InvalidThreadId,
    InvalidInstanceId,
    InvalidTimestamp,
    EntropyUnavailable,
    InvalidRecord,
    UnknownInstance,
    EndedInstance,
    InvalidToken,
}

/// Persistible projection. The authentication hash binds the instance and thread
/// to the canonical wire token; it contains neither a token nor a child manifest.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchIdentityRecord {
    pub instance_id: String,
    pub thread_id: String,
    pub started_at_utc_ms: i64,
    pub ended_at_utc_ms: Option<i64>,
    pub token_sha256: [u8; 32],
}

impl fmt::Debug for LaunchIdentityRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LaunchIdentityRecord")
            .field("instance_id", &self.instance_id)
            .field("thread_id", &self.thread_id)
            .field("started_at_utc_ms", &self.started_at_utc_ms)
            .field("ended_at_utc_ms", &self.ended_at_utc_ms)
            .field("token_sha256", &"[redacted]")
            .finish()
    }
}

/// Deliberately not Serialize or Clone. Use record() for persistence and the
/// explicit environment-only accessor for the secret needed by one launch.
pub struct LaunchIdentity {
    record: LaunchIdentityRecord,
    child_secret: ChildOverride,
}

impl fmt::Debug for LaunchIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LaunchIdentity")
            .field("record", &self.record)
            .field("child_secret", &"[redacted]")
            .finish()
    }
}

impl LaunchIdentity {
    pub fn generate(thread_id: &str) -> Result<Self, IdentityError> {
        Self::generate_with(thread_id, SystemTime::now(), |bytes| {
            getrandom::fill(bytes).map_err(|_| ())
        })
    }

    pub fn instance_id(&self) -> &str {
        &self.record.instance_id
    }

    pub fn record(&self) -> LaunchIdentityRecord {
        self.record.clone()
    }

    /// This typed override has a redacted Debug and is not serializable. Its
    /// value is 64 lowercase hexadecimal characters encoding 32 random bytes.
    /// Pass it to the child environment plan, never argv or a persisted manifest.
    pub fn child_secret_override(&self) -> ChildOverride {
        self.child_secret.clone()
    }

    // Private deterministic seam: entropy failure has no public weak fallback.
    fn generate_with(
        thread_id: &str,
        now: SystemTime,
        fill: impl FnOnce(&mut [u8]) -> Result<(), ()>,
    ) -> Result<Self, IdentityError> {
        if !canonical_id(thread_id, "t_") {
            return Err(IdentityError::InvalidThreadId);
        }
        let milliseconds = now
            .duration_since(UNIX_EPOCH)
            .map_err(|_| IdentityError::InvalidTimestamp)?
            .as_millis();
        let started_at_utc_ms =
            i64::try_from(milliseconds).map_err(|_| IdentityError::InvalidTimestamp)?;
        let mut entropy = [0_u8; ID_BYTES + TOKEN_BYTES];
        fill(&mut entropy).map_err(|_| IdentityError::EntropyUnavailable)?;

        let mut instance_id = String::with_capacity(14);
        instance_id.push_str("i_");
        for byte in &entropy[..ID_BYTES] {
            // Uniform formatting: 256 is divisible by the 32-character alphabet.
            instance_id.push(char::from(BASE32[usize::from(byte & 31)]));
        }
        let mut wire = String::with_capacity(WIRE_BYTES);
        for byte in &entropy[ID_BYTES..] {
            write!(&mut wire, "{byte:02x}").expect("Writing into a String cannot fail.");
        }
        let token_sha256 = bound_hash(&instance_id, thread_id, &wire);
        let child_secret = ChildOverride::new(OverrideName::InstanceToken, wire.into())
            .map_err(|_| IdentityError::InvalidToken)?;
        Ok(Self {
            record: LaunchIdentityRecord {
                instance_id,
                thread_id: thread_id.to_owned(),
                started_at_utc_ms,
                ended_at_utc_ms: None,
                token_sha256,
            },
            child_secret,
        })
    }
}

/// Authenticate only a known, live registry record. Record lookup is the future
/// registry's responsibility; None and records for a different ID are refused.
pub fn verify_record(
    instance_id: &str,
    wire_token: &str,
    record: Option<&LaunchIdentityRecord>,
) -> Result<(), IdentityError> {
    if wire_token.len() != WIRE_BYTES
        || !wire_token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(IdentityError::InvalidToken);
    }
    if !canonical_id(instance_id, "i_") {
        return Err(IdentityError::InvalidInstanceId);
    }
    let record = record.ok_or(IdentityError::UnknownInstance)?;
    if record.instance_id != instance_id {
        return Err(IdentityError::UnknownInstance);
    }
    if !canonical_id(&record.thread_id, "t_") || record.started_at_utc_ms < 0 {
        return Err(IdentityError::InvalidRecord);
    }
    if record.ended_at_utc_ms.is_some() {
        return Err(IdentityError::EndedInstance);
    }
    let candidate = bound_hash(instance_id, &record.thread_id, wire_token);
    if bool::from(candidate.ct_eq(&record.token_sha256)) {
        Ok(())
    } else {
        Err(IdentityError::InvalidToken)
    }
}

fn canonical_id(value: &str, prefix: &str) -> bool {
    value.len() == 14
        && value.starts_with(prefix)
        && value.as_bytes()[2..]
            .iter()
            .all(|byte| BASE32.contains(byte))
}

// SHA-256(domain || NUL || instance ID || NUL || thread ID || NUL || wire ASCII).
// Fields are canonical and length-bounded before this standard Digest operation.
fn bound_hash(instance_id: &str, thread_id: &str, wire: &str) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(HASH_DOMAIN);
    digest.update(b"\0");
    digest.update(instance_id.as_bytes());
    digest.update(b"\0");
    digest.update(thread_id.as_bytes());
    digest.update(b"\0");
    digest.update(wire.as_bytes());
    digest.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::time::Duration;

    const THREAD: &str = "t_abcdefghijkl";

    fn identity(seed: u8) -> LaunchIdentity {
        LaunchIdentity::generate_with(
            THREAD,
            UNIX_EPOCH + Duration::from_millis(1_700_000_000_123),
            |bytes| {
                bytes.fill(seed);
                Ok(())
            },
        )
        .unwrap()
    }

    fn wire(identity: &LaunchIdentity) -> String {
        identity
            .child_secret_override()
            .value()
            .to_str()
            .expect("Generated wire token must be ASCII.")
            .to_owned()
    }

    #[test]
    fn generated_ids_and_integer_utc_milliseconds_are_canonical() {
        let identity = identity(31);
        assert!(canonical_id(identity.instance_id(), "i_"));
        let record = identity.record();
        assert_eq!(record.thread_id, THREAD);
        assert_eq!(record.started_at_utc_ms, 1_700_000_000_123);
        assert!(record.ended_at_utc_ms.is_none());
        let live = LaunchIdentity::generate(THREAD).unwrap();
        assert!(canonical_id(live.instance_id(), "i_"));
        assert!(live.record().started_at_utc_ms >= 0);
    }

    #[test]
    fn malformed_thread_or_clock_is_refused_before_requesting_entropy() {
        for thread in [
            "",
            "p_abcdefghijkl",
            "t_abc",
            "t_ABCDEFGHIJKL",
            "t_abcdefghijk0",
            "t_abcdefghijkl\0tail",
            " t_abcdefghijkl",
            "t_abcdefghi=kl",
        ] {
            assert!(matches!(
                LaunchIdentity::generate_with(thread, UNIX_EPOCH, |_| panic!(
                    "Entropy must not be requested for an invalid thread."
                )),
                Err(IdentityError::InvalidThreadId)
            ));
        }
        assert!(matches!(
            LaunchIdentity::generate_with(
                THREAD,
                UNIX_EPOCH - Duration::from_millis(1),
                |_| panic!("Entropy must not be requested for an invalid clock.")
            ),
            Err(IdentityError::InvalidTimestamp)
        ));
    }

    #[test]
    fn entropy_failure_has_no_partial_identity_or_fallback_token() {
        assert!(matches!(
            LaunchIdentity::generate_with(THREAD, UNIX_EPOCH, |bytes| {
                bytes.fill(42);
                Err(())
            }),
            Err(IdentityError::EntropyUnavailable)
        ));
    }

    #[test]
    fn only_the_hash_record_serializes_and_all_secret_debug_is_redacted() {
        let identity = identity(42);
        let secret = wire(&identity);
        let record = identity.record();
        let serialized = serde_json::to_string(&record).unwrap();
        let fields = serde_json::to_value(&record)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            fields,
            [
                "instanceId",
                "threadId",
                "startedAtUtcMs",
                "endedAtUtcMs",
                "tokenSha256"
            ]
            .into_iter()
            .map(String::from)
            .collect()
        );
        assert!(
            !serialized.contains(&secret),
            "A wire token must never be serialized."
        );
        assert_eq!(
            serde_json::to_value(&record).unwrap()["tokenSha256"]
                .as_array()
                .unwrap()
                .len(),
            32
        );
        for debug in [
            format!("{identity:?}"),
            format!("{record:?}"),
            format!("{:?}", identity.child_secret_override()),
        ] {
            assert!(
                !debug.contains(&secret),
                "Debug must not expose a wire token."
            );
            assert!(debug.contains("[redacted]"));
        }
        assert_eq!(
            identity.child_secret_override().name(),
            OverrideName::InstanceToken
        );
    }

    #[test]
    fn verification_accepts_only_the_own_canonical_token() {
        let first = identity(1);
        let other = identity(2);
        let secret = wire(&first);
        assert!(verify_record(first.instance_id(), &secret, Some(&first.record())).is_ok());
        assert_eq!(
            verify_record(other.instance_id(), &secret, Some(&other.record())),
            Err(IdentityError::InvalidToken)
        );
        for invalid in [
            String::new(),
            "0".repeat(63),
            "0".repeat(65),
            "G".repeat(64),
            "A".repeat(64),
            "0".repeat(64),
        ] {
            assert_eq!(
                verify_record(first.instance_id(), &invalid, Some(&first.record())),
                Err(IdentityError::InvalidToken)
            );
        }
    }

    #[test]
    fn unknown_other_or_ended_instance_is_never_authenticated() {
        let identity = identity(3);
        let secret = wire(&identity);
        assert_eq!(
            verify_record(identity.instance_id(), &secret, None),
            Err(IdentityError::UnknownInstance)
        );
        assert_eq!(
            verify_record("i_aaaaaaaaaaaa", &secret, Some(&identity.record())),
            Err(IdentityError::UnknownInstance)
        );
        assert_eq!(
            verify_record("i_INVALID", &secret, Some(&identity.record())),
            Err(IdentityError::InvalidInstanceId)
        );
        let mut ended = identity.record();
        ended.ended_at_utc_ms = Some(ended.started_at_utc_ms);
        assert_eq!(
            verify_record(identity.instance_id(), &secret, Some(&ended)),
            Err(IdentityError::EndedInstance)
        );
    }

    #[test]
    fn copied_hash_cannot_be_rebound_to_another_instance_or_thread() {
        let identity = identity(4);
        let secret = wire(&identity);
        let mut rebound = identity.record();
        rebound.instance_id = "i_aaaaaaaaaaaa".into();
        assert_eq!(
            verify_record(&rebound.instance_id, &secret, Some(&rebound)),
            Err(IdentityError::InvalidToken)
        );
        let mut rebound = identity.record();
        rebound.thread_id = "t_aaaaaaaaaaaa".into();
        assert_eq!(
            verify_record(identity.instance_id(), &secret, Some(&rebound)),
            Err(IdentityError::InvalidToken)
        );
    }
}
