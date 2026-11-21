//! C29.1: reconcile hops after a crash (FRD-0005 R42, R51).
//!
//! The journal decides the state per R42; this module performs the follow-up the runtime owns:
//! stopping a destination Shinatama provably started (its instance is in the journal) and
//! reopening the source activation when a close hop died while SWITCHING with no live
//! destination. An uncertain spawn is only observed: nothing here can start an agent, and no
//! path deletes anything an agent owns.
use shinatama_state::hops::{
    HopError, HopJournal, HopMode, HopRecord, HopState, Recovered, RecoveryProbe,
};

pub trait RecoveryRuntime {
    fn destination_alive(&mut self, hop: &HopRecord) -> bool;
    /// Stops an instance this hop started (only ever called with a journaled instance id).
    fn kill_destination(&mut self, instance: &str);
    /// Reopens the source activation a close hop ended at SWITCHING.
    fn reopen_source(&mut self, hop: &HopRecord) -> Result<(), String>;
}

struct Probe<'a>(&'a mut dyn RecoveryRuntime);
impl RecoveryProbe for Probe<'_> {
    fn destination_alive(&mut self, hop: &HopRecord) -> bool {
        self.0.destination_alive(hop)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Uncertain spawn inside its 90 s window: watched, never relaunched.
    Observing,
    KilledDestination(String),
    ReopenedSource,
    ReopenFailed(String),
    /// FAILED with a manual retry offered.
    OfferRetry,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reconciled {
    pub recovered: Recovered,
    pub actions: Vec<Action>,
}

pub fn reconcile(
    journal: &HopJournal,
    runtime: &mut dyn RecoveryRuntime,
    now_ms: i64,
) -> Result<Vec<Reconciled>, HopError> {
    let recovered = journal.recover(&mut Probe(runtime), now_ms)?;
    let mut out = Vec::with_capacity(recovered.len());
    for item in recovered {
        let hop = journal.hop(&item.hop_id)?.ok_or(HopError::NotFound)?;
        let mut actions = Vec::new();
        match (item.from, item.to) {
            (_, None) => actions.push(Action::Observing),
            (
                HopState::Preparing | HopState::Ready,
                Some(to @ (HopState::Aborted | HopState::Failed)),
            ) => {
                if let Some(instance) = hop.destination_instance.as_deref()
                    && runtime.destination_alive(&hop)
                {
                    runtime.kill_destination(instance);
                    actions.push(Action::KilledDestination(instance.to_owned()));
                }
                if to == HopState::Failed {
                    actions.push(Action::OfferRetry);
                }
            }
            (HopState::Switching, Some(HopState::Aborted))
                if hop.plan.mode == HopMode::Close && hop.plan.source_instance.is_some() =>
            {
                actions.push(match runtime.reopen_source(&hop) {
                    Ok(()) => Action::ReopenedSource,
                    Err(message) => Action::ReopenFailed(message),
                });
            }
            _ => {}
        }
        out.push(Reconciled {
            recovered: item,
            actions,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shinatama_state::hops::{HopPlan, HopStrategy, READY_TIMEOUT_MS};
    use std::path::{Path, PathBuf};

    #[derive(Default)]
    struct Runtime {
        alive: bool,
        killed: Vec<String>,
        reopened: u32,
    }
    impl RecoveryRuntime for Runtime {
        fn destination_alive(&mut self, hop: &HopRecord) -> bool {
            self.alive && hop.destination_instance.is_some()
        }
        fn kill_destination(&mut self, instance: &str) {
            self.killed.push(instance.into());
            self.alive = false;
        }
        fn reopen_source(&mut self, _: &HopRecord) -> Result<(), String> {
            self.reopened += 1;
            Ok(())
        }
    }

    fn plan() -> HopPlan {
        HopPlan {
            id: "c".repeat(32),
            thread_id: "t_matrix".into(),
            destination_thread_id: None,
            mode: HopMode::Close,
            strategy: HopStrategy::SeedPrompt,
            source_agent: "claude_code".into(),
            source_native_id: "s".into(),
            source_instance: Some("i_src".into()),
            destination_agent: "codex".into(),
            nonce: "n".into(),
            snapshot_sha256: "a".repeat(64),
            bootstrap_sha256: "b".repeat(64),
            seed_path: "C:/seed.md".into(),
        }
    }

    /// The happy path as the coordinator journals it; a kill may land after any of these.
    fn apply(journal: &HopJournal, steps: usize) {
        use HopState::*;
        let id = "c".repeat(32);
        let mut done = 0;
        let mut go = |f: &mut dyn FnMut()| {
            if done < steps {
                f();
                done += 1;
            }
        };
        let arrow = |to: HopState| {
            let hop = journal.hop(&id).unwrap().unwrap();
            journal
                .transition(&id, hop.state, hop.revision, to, None, 0)
                .unwrap();
        };
        go(&mut || {
            journal.plan(&plan(), 0).unwrap();
        });
        go(&mut || arrow(Previewed));
        go(&mut || arrow(Confirmed));
        go(&mut || arrow(Preparing));
        go(&mut || {
            journal
                .record_effect(&id, "spawn_attempted", None, 0)
                .unwrap()
        });
        go(&mut || {
            journal
                .set_destination(&id, Some("i_dest"), Some("dest-native"), 0)
                .unwrap();
            journal
                .record_effect(&id, "spawn_observed", None, 0)
                .unwrap();
        });
        go(&mut || arrow(Ready));
        go(&mut || arrow(Switching));
        go(&mut || {
            journal
                .record_effect(&id, "source_activation_end", None, 0)
                .unwrap()
        });
        go(&mut || arrow(Committed));
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "shinatama-recovery-{tag}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn crashed_journal(dir: &Path, steps: usize) -> HopJournal {
        let path = dir.join("hops.sqlite");
        let journal = HopJournal::open_path(&path).unwrap();
        apply(&journal, steps);
        drop(journal); // the process dies here
        HopJournal::open_path(&path).unwrap()
    }

    #[test]
    fn kill_at_each_boundary_matrix() {
        use HopState::*;
        // (steps applied before the kill, destination alive?, expected state, kills, reopens)
        let cases: &[(usize, bool, Option<HopState>, usize, u32)] = &[
            (1, false, Some(Aborted), 0, 0),
            (2, false, Some(Aborted), 0, 0),
            (3, false, Some(Aborted), 0, 0),
            (4, false, Some(Aborted), 0, 0),
            (5, false, Some(Preparing), 0, 0),
            (6, true, Some(Failed), 1, 0),
            (6, false, Some(Failed), 0, 0),
            (7, true, Some(Aborted), 1, 0),
            (8, true, Some(Committed), 0, 0),
            (8, false, Some(Aborted), 0, 1),
            (9, true, Some(Committed), 0, 0),
            (9, false, Some(Aborted), 0, 1),
            (10, false, Some(Committed), 0, 0),
        ];
        for &(steps, alive, expected, kills, reopens) in cases {
            let dir = scratch("matrix");
            let journal = crashed_journal(&dir, steps);
            assert!(journal.integrity_check().unwrap(), "steps {steps}");
            let mut runtime = Runtime {
                alive,
                ..Runtime::default()
            };
            let first = reconcile(&journal, &mut runtime, 1_000).unwrap();
            let hop = journal.hop(&"c".repeat(32)).unwrap().unwrap();
            assert_eq!(
                Some(hop.state),
                expected,
                "steps {steps} alive {alive}: {first:?}"
            );
            assert_eq!(runtime.killed.len(), kills, "steps {steps}");
            assert_eq!(runtime.reopened, reopens, "steps {steps}");
            // Recovery runs once: a second pass from another process changes nothing more.
            let mut again = Runtime {
                alive,
                ..Runtime::default()
            };
            let second = reconcile(&journal, &mut again, 1_000).unwrap();
            assert!(
                again.killed.is_empty() && again.reopened == 0,
                "steps {steps}: {second:?}"
            );
            assert!(journal.integrity_check().unwrap());
            drop(journal);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn an_uncertain_spawn_is_observed_then_failed_never_relaunched() {
        let dir = scratch("uncertain");
        let journal = crashed_journal(&dir, 5);
        let mut runtime = Runtime::default();
        let watched = reconcile(&journal, &mut runtime, 1_000).unwrap();
        assert_eq!(watched[0].actions, [Action::Observing]);
        let failed = reconcile(&journal, &mut runtime, READY_TIMEOUT_MS + 1).unwrap();
        assert_eq!(failed[0].recovered.to, Some(HopState::Failed));
        assert_eq!(failed[0].actions, [Action::OfferRetry]);
        assert!(
            runtime.killed.is_empty(),
            "no instance was ever observed, so nothing is killed"
        );
        drop(journal);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
