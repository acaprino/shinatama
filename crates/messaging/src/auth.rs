//! C32.2: who is calling the messaging tools (FRD-0007 R18-R22, FRD-0002 R30-R34).
//!
//! The desktop registers each launched instance with a random token handed to that process
//! only; the store keeps the token's SHA-256. A caller is accepted when its instance is
//! registered, not ended, has a heartbeat newer than [`STALE_MS`], and the token hashes to the
//! stored value (constant-time compare). A peer cannot act as another instance.
use crate::{Channel, Directory, Instance};
use sha2::{Digest, Sha256};
use shinatama_state::messages::MessageStore;
use subtle::ConstantTimeEq;

pub const STALE_MS: i64 = 30_000;

pub fn token_hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A random launch token (256 bits, hex).
pub fn new_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// Live instances from the store, as the messaging directory.
pub struct StoreDirectory {
    pub instances: Vec<Instance>,
    hashes: Vec<(String, String)>,
}

impl StoreDirectory {
    pub fn load(store: &MessageStore, now_ms: i64) -> Result<Self, String> {
        let rows = store
            .live_instances(now_ms, STALE_MS)
            .map_err(|e| e.to_string())?;
        let mut instances = Vec::with_capacity(rows.len());
        let mut hashes = Vec::with_capacity(rows.len());
        for (id, agent, project, thread, roundtable, hash, channels) in rows {
            let channels: Vec<Channel> =
                serde_json::from_str(&channels).unwrap_or_else(|_| vec![Channel::McpPull]);
            hashes.push((id.clone(), hash));
            instances.push(Instance {
                id,
                agent,
                project,
                thread,
                roundtable,
                online: true,
                channels,
                idle_quiet_ms: 0,
                prompt_or_draft_active: true,
            });
        }
        Ok(Self { instances, hashes })
    }

    /// The instance id when `token` belongs to that live instance.
    pub fn authenticate(&self, instance_id: &str, token: &str) -> Option<&Instance> {
        let (_, stored) = self.hashes.iter().find(|(id, _)| id == instance_id)?;
        let offered = token_hash(token);
        bool::from(offered.as_bytes().ct_eq(stored.as_bytes()))
            .then(|| self.instances.iter().find(|i| i.id == instance_id))
            .flatten()
    }
}

impl Directory for StoreDirectory {
    fn instances(&self) -> Vec<Instance> {
        self.instances.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_live_instance_with_its_own_token_is_accepted() {
        let store = MessageStore::open_in_memory().unwrap();
        let (ta, tb) = (new_token(), new_token());
        store
            .register_instance(
                "a",
                "claude_code",
                "C:/p",
                None,
                None,
                &token_hash(&ta),
                "[\"mcp_pull\"]",
                1_000,
            )
            .unwrap();
        store
            .register_instance(
                "b",
                "codex",
                "C:/p",
                None,
                None,
                &token_hash(&tb),
                "[\"mcp_pull\"]",
                1_000,
            )
            .unwrap();
        let directory = StoreDirectory::load(&store, 2_000).unwrap();
        assert_eq!(
            directory.authenticate("a", &ta).unwrap().agent,
            "claude_code"
        );
        assert!(
            directory.authenticate("a", &tb).is_none(),
            "b's token cannot act as a"
        );
        assert!(directory.authenticate("a", "").is_none());
        assert!(directory.authenticate("ghost", &ta).is_none());
        store.end_instance("b").unwrap();
        assert!(
            StoreDirectory::load(&store, 2_000)
                .unwrap()
                .authenticate("b", &tb)
                .is_none(),
            "ended"
        );
        assert!(
            StoreDirectory::load(&store, 1_000 + STALE_MS + 1)
                .unwrap()
                .authenticate("a", &ta)
                .is_none(),
            "stale heartbeat"
        );
        assert!(store.heartbeat("a", 40_000).unwrap());
        assert!(
            StoreDirectory::load(&store, 40_001)
                .unwrap()
                .authenticate("a", &ta)
                .is_some()
        );
        assert!(
            store
                .register_instance("c", "pi", "C:/p", None, None, "plaintext-token", "[]", 0)
                .is_err(),
            "only hashes are stored"
        );
    }
}
