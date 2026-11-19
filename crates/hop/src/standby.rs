//! C29.4: warm standby and hop latency (FRD-0005 R06, R34).
//!
//! Off by default. For the agents the user names, at most one idle, un-seeded instance per
//! project in use is kept in a hidden pane: it costs memory, never tokens, because nothing is
//! ever typed into it before a hop takes it. It is discarded after 30 minutes without use of
//! its project, when the agent's version changes, or when the setting is turned off.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const IDLE_EXPIRY_MS: i64 = 30 * 60 * 1000;
/// R06 targets, reported against measurements and never promised without them.
pub const WARM_TARGET_MS: u64 = 8_000;
pub const COLD_TARGET_MS: u64 = 25_000;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandbySettings {
    pub enabled: bool,
    pub agents: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Standby {
    pub instance: String,
    pub project: String,
    pub agent: String,
    pub agent_version: String,
    pub created_at_ms: i64,
    pub project_used_at_ms: i64,
    /// Bytes ever written into the instance's input. A standby with input is never handed out.
    pub input_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Discard {
    Idle,
    VersionChanged,
    Disabled,
    InputSeen,
}

#[derive(Default)]
pub struct StandbyPool {
    pub settings: StandbySettings,
    entries: Vec<Standby>,
}

impl StandbyPool {
    pub fn new(settings: StandbySettings) -> Self {
        Self { settings, entries: Vec::new() }
    }
    pub fn entries(&self) -> &[Standby] {
        &self.entries
    }
    fn allowed(&self, agent: &str) -> bool {
        self.settings.enabled && self.settings.agents.contains(agent)
    }
    /// The runtime should start a hidden instance for this project and agent.
    pub fn wanted(&self, project: &str, agent: &str) -> bool {
        self.allowed(agent) && !self.entries.iter().any(|e| e.project == project && e.agent == agent)
    }
    pub fn add(&mut self, standby: Standby) -> Result<(), &'static str> {
        if !self.wanted(&standby.project, &standby.agent) {
            return Err("standby is off for this agent or one already exists for the project");
        }
        if standby.input_bytes != 0 {
            return Err("a standby must never have received input");
        }
        self.entries.push(standby);
        Ok(())
    }
    pub fn note_input(&mut self, instance: &str, bytes: u64) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.instance == instance) {
            entry.input_bytes += bytes;
        }
    }
    pub fn project_used(&mut self, project: &str, now_ms: i64) {
        for entry in self.entries.iter_mut().filter(|e| e.project == project) {
            entry.project_used_at_ms = now_ms;
        }
    }
    /// A fresh, input-free standby for the hop, removed from the pool.
    pub fn take(&mut self, project: &str, agent: &str, version: &str, now_ms: i64) -> Option<Standby> {
        let index = self.entries.iter().position(|e| {
            e.project == project
                && e.agent == agent
                && e.agent_version == version
                && e.input_bytes == 0
                && now_ms - e.project_used_at_ms <= IDLE_EXPIRY_MS
        })?;
        self.allowed(agent).then(|| self.entries.remove(index))
    }
    /// Entries the runtime must stop, with the reason.
    pub fn expire(&mut self, now_ms: i64, installed_version: &dyn Fn(&str) -> Option<String>) -> Vec<(Standby, Discard)> {
        let mut gone = Vec::new();
        let mut kept = Vec::new();
        for entry in self.entries.drain(..) {
            let reason = if !self.settings.enabled || !self.settings.agents.contains(&entry.agent) {
                Some(Discard::Disabled)
            } else if entry.input_bytes != 0 {
                Some(Discard::InputSeen)
            } else if installed_version(&entry.agent).as_deref() != Some(entry.agent_version.as_str()) {
                Some(Discard::VersionChanged)
            } else if now_ms - entry.project_used_at_ms > IDLE_EXPIRY_MS {
                Some(Discard::Idle)
            } else {
                None
            };
            match reason {
                Some(reason) => gone.push((entry, reason)),
                None => kept.push(entry),
            }
        }
        self.entries = kept;
        gone
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatencySample {
    pub warm: bool,
    /// From the user's Enter to the destination's receipt.
    pub ready_ms: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatencySummary {
    pub warm_samples: usize,
    pub warm_median_ms: Option<u64>,
    pub warm_within_target: Option<bool>,
    pub cold_samples: usize,
    pub cold_median_ms: Option<u64>,
    pub cold_within_target: Option<bool>,
}

fn median(mut values: Vec<u64>) -> Option<u64> {
    values.sort_unstable();
    let n = values.len();
    match n {
        0 => None,
        _ if n % 2 == 1 => Some(values[n / 2]),
        _ => Some((values[n / 2 - 1] + values[n / 2]) / 2),
    }
}

/// Medians against the R06 targets; no sample means no claim.
pub fn summarize(samples: &[LatencySample]) -> LatencySummary {
    let pick = |warm: bool| samples.iter().filter(|s| s.warm == warm).map(|s| s.ready_ms).collect::<Vec<_>>();
    let (warm, cold) = (pick(true), pick(false));
    let warm_median_ms = median(warm.clone());
    let cold_median_ms = median(cold.clone());
    LatencySummary {
        warm_samples: warm.len(),
        warm_median_ms,
        warm_within_target: warm_median_ms.map(|m| m < WARM_TARGET_MS),
        cold_samples: cold.len(),
        cold_median_ms,
        cold_within_target: cold_median_ms.map(|m| m < COLD_TARGET_MS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standby(instance: &str, version: &str, at: i64) -> Standby {
        Standby { instance: instance.into(), project: "p".into(), agent: "codex".into(), agent_version: version.into(), created_at_ms: at, project_used_at_ms: at, input_bytes: 0 }
    }
    fn on() -> StandbySettings {
        StandbySettings { enabled: true, agents: ["codex".to_owned()].into() }
    }

    #[test]
    fn off_by_default_and_one_per_project_and_agent() {
        let mut pool = StandbyPool::default();
        assert!(!pool.wanted("p", "codex"));
        assert!(pool.add(standby("i1", "1.0", 0)).is_err());
        let mut pool = StandbyPool::new(on());
        assert!(pool.wanted("p", "codex") && !pool.wanted("p", "claude_code"));
        pool.add(standby("i1", "1.0", 0)).unwrap();
        assert!(!pool.wanted("p", "codex"));
        assert!(pool.add(standby("i2", "1.0", 0)).is_err());
    }

    #[test]
    fn a_standby_with_any_input_is_never_handed_out() {
        let mut pool = StandbyPool::new(on());
        pool.add(standby("i1", "1.0", 0)).unwrap();
        pool.note_input("i1", 3);
        assert!(pool.take("p", "codex", "1.0", 10).is_none());
        let gone = pool.expire(10, &|_| Some("1.0".into()));
        assert_eq!(gone[0].1, Discard::InputSeen);
    }

    #[test]
    fn expires_after_thirty_idle_minutes_a_version_change_or_turning_it_off() {
        let mut pool = StandbyPool::new(on());
        pool.add(standby("i1", "1.0", 0)).unwrap();
        pool.project_used("p", 1_000);
        assert!(pool.expire(1_000 + IDLE_EXPIRY_MS, &|_| Some("1.0".into())).is_empty());
        assert_eq!(pool.expire(1_001 + IDLE_EXPIRY_MS, &|_| Some("1.0".into()))[0].1, Discard::Idle);
        pool.add(standby("i2", "1.0", 0)).unwrap();
        assert_eq!(pool.expire(5, &|_| Some("1.1".into()))[0].1, Discard::VersionChanged);
        pool.add(standby("i3", "1.0", 0)).unwrap();
        pool.settings.enabled = false;
        assert_eq!(pool.expire(5, &|_| Some("1.0".into()))[0].1, Discard::Disabled);
        assert!(pool.entries().is_empty());
    }

    #[test]
    fn take_returns_only_a_fresh_matching_version() {
        let mut pool = StandbyPool::new(on());
        pool.add(standby("i1", "1.0", 0)).unwrap();
        assert!(pool.take("p", "codex", "2.0", 1).is_none());
        assert!(pool.take("p", "codex", "1.0", IDLE_EXPIRY_MS + 1).is_none());
        assert_eq!(pool.take("p", "codex", "1.0", 1).unwrap().instance, "i1");
        assert!(pool.entries().is_empty());
    }

    #[test]
    fn summary_reports_medians_against_targets_and_claims_nothing_without_samples() {
        assert_eq!(summarize(&[]), LatencySummary::default());
        let summary = summarize(&[
            LatencySample { warm: true, ready_ms: 5_000 },
            LatencySample { warm: true, ready_ms: 9_000 },
            LatencySample { warm: false, ready_ms: 20_000 },
        ]);
        assert_eq!((summary.warm_median_ms, summary.warm_within_target), (Some(7_000), Some(true)));
        assert_eq!((summary.cold_median_ms, summary.cold_within_target), (Some(20_000), Some(true)));
    }
}
