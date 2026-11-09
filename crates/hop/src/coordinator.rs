//! C28.2-C28.3: the hop coordinator (FRD-0005 R10-R13, R35-R40, R49; ADR-0018).
//!
//! It drives one hop through the journal. Every external effect is journaled before it runs
//! and happens only after CONFIRMED. The destination and the source are traits, so the state
//! machine is tested against fake agents and the desktop plugs in real terminals.
//!
//! close: the destination continues the same thread; the source activation ends at SWITCHING
//! (`hop_out`) and the source instance closes only at COMMITTED, after a draft/running-command
//! check. fork: the destination gets a new thread whose parent is the source thread; the
//! source instance, thread and activation are never touched.
use crate::channel::Delivery;
use crate::preview::{Preview, sha256, verify_send};
use shinatama_state::hops::{HopError, HopJournal, HopMode, HopPlan, HopRecord, HopState, HopStrategy};

/// What the runtime can see of a started destination.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Evidence {
    pub alive: bool,
    pub native_session_id: Option<String>,
    /// SHA-256 of the first user entry the destination stored.
    pub first_user_sha256: Option<String>,
    /// The destination's first reply, once it exists.
    pub first_reply: Option<String>,
    /// The pane's last lines, shown when the hop fails (R44, R49).
    pub last_output: String,
}

pub trait Destination {
    /// Starts the destination with the delivery. Returns the instance id.
    fn spawn(&mut self, plan: &HopPlan, delivery: &Delivery) -> Result<String, String>;
    fn evidence(&mut self, instance: &str) -> Evidence;
    fn kill(&mut self, instance: &str);
}

pub trait Source {
    /// The source finished its turn (R39).
    fn is_idle(&mut self) -> bool;
    /// A draft in its input or a command still running (R40).
    fn has_draft_or_running(&mut self) -> bool;
    /// Close mode at SWITCHING: end the activation with `end_reason = hop_out`.
    fn end_activation(&mut self) -> Result<(), String>;
    /// Close mode at COMMITTED: close the source instance; its native session stays on disk.
    fn close_instance(&mut self) -> Result<(), String>;
}

pub trait Clock {
    fn now_ms(&mut self) -> i64;
    fn pause(&mut self, ms: u64);
}

pub struct SystemClock;
impl Clock for SystemClock {
    fn now_ms(&mut self) -> i64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
    }
    fn pause(&mut self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum CoordinatorError {
    Journal(HopError),
    /// The bytes about to be sent differ from the preview; nothing was sent.
    SnapshotChanged,
    /// The destination failed to start or exited; the source is untouched (R49).
    SpawnFailed { output: String },
    DestinationExited { output: String },
    /// The stored first user entry is not what was sent (R49).
    SeedMismatch { expected: String, found: Option<String> },
    /// No native session within the timeout; the destination was killed (R38, R49).
    NoSession { output: String },
    /// Session exists but the reply lacks the nonce; the user may switch anyway (R38).
    ReceiptMissing,
    /// The source is still working (R39, R50): wait, interrupt or cancel.
    SourceBusy,
    /// A draft or running command needs the user's answer before the source closes (R40).
    SourceNeedsConfirmation,
    SourceAction(String),
}
impl From<HopError> for CoordinatorError {
    fn from(error: HopError) -> Self {
        CoordinatorError::Journal(error)
    }
}
impl std::fmt::Display for CoordinatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoordinatorError::Journal(error) => write!(f, "{error}"),
            CoordinatorError::SnapshotChanged => write!(f, "The seed changed after the preview; nothing was sent. Prepare a new preview."),
            CoordinatorError::SpawnFailed { .. } => write!(f, "The destination did not start; the source is untouched."),
            CoordinatorError::DestinationExited { .. } => write!(f, "The destination exited while starting; the source is untouched."),
            CoordinatorError::SeedMismatch { .. } => write!(f, "The destination stored a different first message than the seed; the hop failed."),
            CoordinatorError::NoSession { .. } => write!(f, "The destination never created its session within 90 seconds; it was stopped."),
            CoordinatorError::ReceiptMissing => write!(f, "The destination did not confirm the hand-off. Ask it once more, switch anyway, or cancel."),
            CoordinatorError::SourceBusy => write!(f, "The source is still working. Wait, interrupt it, or cancel the hop."),
            CoordinatorError::SourceNeedsConfirmation => write!(f, "The source has a draft or a running command. Confirm closing it, or keep it open."),
            CoordinatorError::SourceAction(message) => write!(f, "The source could not be updated: {message}"),
        }
    }
}
pub type Outcome<T> = Result<T, CoordinatorError>;

pub struct HopRequest<'a> {
    pub preview: &'a Preview,
    pub thread_id: &'a str,
    pub mode: HopMode,
    /// For fork: the id of the new thread the caller created with parent = `thread_id`.
    pub destination_thread_id: Option<&'a str>,
    pub source_agent: &'a str,
    pub source_native_id: &'a str,
    pub source_instance: Option<&'a str>,
}

/// Bytes the destination stores as its first user entry for each channel.
pub fn first_entry_bytes(preview: &Preview) -> &str {
    match &preview.delivery {
        Delivery::ProtocolPrompt { prompt } => prompt,
        Delivery::QueryFile { .. } => &preview.seed,
        Delivery::ArgvPrompt { prompt } | Delivery::FileArg { prompt, .. } => prompt,
    }
}

pub struct Coordinator<'a, C: Clock> {
    pub journal: &'a HopJournal,
    pub clock: C,
    pub ready_timeout_ms: i64,
    pub poll_ms: u64,
}

impl<'a, C: Clock> Coordinator<'a, C> {
    pub fn new(journal: &'a HopJournal, clock: C) -> Self {
        Self { journal, clock, ready_timeout_ms: shinatama_state::hops::READY_TIMEOUT_MS, poll_ms: 250 }
    }

    fn step(&mut self, hop: &HopRecord, next: HopState, detail: Option<&str>) -> Outcome<HopRecord> {
        let now = self.clock.now_ms();
        Ok(self.journal.transition(&hop.plan.id, hop.state, hop.revision, next, detail, now)?)
    }

    fn current(&self, hop_id: &str) -> Outcome<HopRecord> {
        self.journal.hop(hop_id)?.ok_or(CoordinatorError::Journal(HopError::NotFound))
    }

    /// PLANNED then PREVIEWED: takes the thread (I1) and freezes the preview's hashes.
    pub fn plan(&mut self, request: &HopRequest<'_>) -> Outcome<HopRecord> {
        let preview = request.preview;
        let plan = HopPlan {
            id: preview.hop_id.clone(),
            thread_id: request.thread_id.to_owned(),
            destination_thread_id: request.destination_thread_id.map(str::to_owned),
            mode: request.mode,
            strategy: HopStrategy::SeedPrompt,
            source_agent: request.source_agent.to_owned(),
            source_native_id: request.source_native_id.to_owned(),
            source_instance: request.source_instance.map(str::to_owned),
            destination_agent: preview.destination.clone(),
            nonce: preview.nonce.clone(),
            snapshot_sha256: preview.seed_sha256.clone(),
            bootstrap_sha256: preview.bootstrap_sha256.clone(),
            seed_path: preview.seed_path.clone(),
        };
        let now = self.clock.now_ms();
        let hop = self.journal.plan(&plan, now)?;
        self.step(&hop, HopState::Previewed, Some(&preview.seed_sha256))
    }

    /// CONFIRMED is the user's Enter. Nothing has run in the destination yet (R36).
    pub fn confirm(&mut self, hop_id: &str) -> Outcome<HopRecord> {
        let hop = self.current(hop_id)?;
        self.step(&hop, HopState::Confirmed, None)
    }

    pub fn abort(&mut self, hop_id: &str, destination: Option<&mut dyn Destination>, reason: &str) -> Outcome<HopRecord> {
        let hop = self.current(hop_id)?;
        if let (Some(destination), Some(instance)) = (destination, hop.destination_instance.as_deref()) {
            // Only an instance this hop provably started is stopped.
            destination.kill(instance);
        }
        self.step(&hop, HopState::Aborted, Some(reason))
    }

    fn fail(&mut self, hop_id: &str, destination: &mut dyn Destination, kill: bool, detail: &str) -> Outcome<()> {
        let hop = self.current(hop_id)?;
        if kill && let Some(instance) = hop.destination_instance.as_deref() {
            destination.kill(instance);
        }
        self.step(&hop, HopState::Failed, Some(detail))?;
        Ok(())
    }

    /// PREPARING: verify the snapshot, journal the spawn, start the destination, wait for the
    /// technical evidence and the receipt, then READY (R38).
    pub fn prepare(&mut self, hop_id: &str, preview: &Preview, destination: &mut dyn Destination) -> Outcome<HopRecord> {
        let hop = self.current(hop_id)?;
        if hop.state != HopState::Confirmed {
            return Err(CoordinatorError::Journal(HopError::InvalidTransition { from: hop.state, to: HopState::Preparing }));
        }
        if preview.hop_id != hop.plan.id
            || preview.seed_sha256 != hop.plan.snapshot_sha256
            || !verify_send(preview, preview.bootstrap.as_bytes())
        {
            self.step(&hop, HopState::Aborted, Some("snapshot changed after preview"))?;
            return Err(CoordinatorError::SnapshotChanged);
        }
        let hop = self.step(&hop, HopState::Preparing, None)?;
        let now = self.clock.now_ms();
        self.journal.record_effect(&hop.plan.id, "spawn_attempted", Some(&preview.destination), now)?;
        let instance = match destination.spawn(&hop.plan, &preview.delivery) {
            Ok(instance) => instance,
            Err(output) => {
                self.fail(hop_id, destination, false, &format!("spawn failed: {}", tail(&output)))?;
                return Err(CoordinatorError::SpawnFailed { output });
            }
        };
        let now = self.clock.now_ms();
        self.journal.set_destination(hop_id, Some(&instance), None, now)?;
        self.journal.record_effect(hop_id, "spawn_observed", Some(&instance), now)?;
        self.await_ready(hop_id, preview, destination, &instance)
    }

    fn await_ready(&mut self, hop_id: &str, preview: &Preview, destination: &mut dyn Destination, instance: &str) -> Outcome<HopRecord> {
        let expected = sha256(first_entry_bytes(preview).as_bytes());
        let receipt = format!("ready {}", preview.nonce);
        let started = self.clock.now_ms();
        let mut session_recorded = false;
        loop {
            let evidence = destination.evidence(instance);
            if !evidence.alive {
                self.fail(hop_id, destination, false, &format!("destination exited: {}", tail(&evidence.last_output)))?;
                return Err(CoordinatorError::DestinationExited { output: evidence.last_output });
            }
            if let Some(native) = evidence.native_session_id.as_deref()
                && !session_recorded
            {
                let now = self.clock.now_ms();
                self.journal.set_destination(hop_id, None, Some(native), now)?;
                session_recorded = true;
            }
            if let Some(found) = evidence.first_user_sha256.as_deref()
                && found != expected
            {
                self.fail(hop_id, destination, true, &format!("seed mismatch: expected {expected}, found {found}"))?;
                return Err(CoordinatorError::SeedMismatch { expected, found: Some(found.to_owned()) });
            }
            let technical = evidence.native_session_id.is_some() && evidence.first_user_sha256.as_deref() == Some(expected.as_str());
            if technical && evidence.first_reply.as_deref().is_some_and(|reply| reply.contains(&receipt)) {
                let hop = self.current(hop_id)?;
                return self.step(&hop, HopState::Ready, Some("evidence and receipt"));
            }
            if self.clock.now_ms() - started >= self.ready_timeout_ms {
                if technical {
                    // The hop stays PREPARING: the user asks again, switches anyway or cancels.
                    let now = self.clock.now_ms();
                    self.journal.record_effect(hop_id, "receipt_missing", None, now)?;
                    return Err(CoordinatorError::ReceiptMissing);
                }
                self.fail(hop_id, destination, true, &format!("no session in time: {}", tail(&evidence.last_output)))?;
                return Err(CoordinatorError::NoSession { output: evidence.last_output });
            }
            self.clock.pause(self.poll_ms);
        }
    }

    /// After ReceiptMissing: the user chose to switch anyway (R38).
    pub fn accept_without_receipt(&mut self, hop_id: &str) -> Outcome<HopRecord> {
        let hop = self.current(hop_id)?;
        if !self.journal.events(hop_id)?.iter().any(|e| e.kind == "receipt_missing") {
            return Err(CoordinatorError::Journal(HopError::InvalidTransition { from: hop.state, to: HopState::Ready }));
        }
        self.step(&hop, HopState::Ready, Some("user switched without receipt"))
    }

    /// SWITCHING: the source must be idle; close ends its activation, fork leaves it (R39).
    pub fn switch(&mut self, hop_id: &str, source: &mut dyn Source) -> Outcome<HopRecord> {
        let hop = self.current(hop_id)?;
        if hop.state != HopState::Ready {
            return Err(CoordinatorError::Journal(HopError::InvalidTransition { from: hop.state, to: HopState::Switching }));
        }
        let live_source = hop.plan.source_instance.is_some();
        if live_source && hop.plan.mode == HopMode::Close && !source.is_idle() {
            return Err(CoordinatorError::SourceBusy);
        }
        let hop = self.step(&hop, HopState::Switching, None)?;
        if live_source && hop.plan.mode == HopMode::Close {
            let now = self.clock.now_ms();
            self.journal.record_effect(hop_id, "source_activation_end", Some("hop_out"), now)?;
            source.end_activation().map_err(CoordinatorError::SourceAction)?;
        }
        Ok(hop)
    }

    /// COMMITTED: close mode closes the source after the draft check; fork never touches it (R40).
    pub fn commit(&mut self, hop_id: &str, source: &mut dyn Source, close_confirmed: bool) -> Outcome<HopRecord> {
        let hop = self.current(hop_id)?;
        if hop.state != HopState::Switching {
            return Err(CoordinatorError::Journal(HopError::InvalidTransition { from: hop.state, to: HopState::Committed }));
        }
        let close = hop.plan.mode == HopMode::Close && hop.plan.source_instance.is_some();
        if close && !close_confirmed && source.has_draft_or_running() {
            return Err(CoordinatorError::SourceNeedsConfirmation);
        }
        let hop = self.step(&hop, HopState::Committed, None)?;
        if close {
            let now = self.clock.now_ms();
            // The hop is committed; a failed close leaves the source open and visible, never lost.
            let _ = self.journal.record_effect(hop_id, "source_close", None, now);
            source.close_instance().map_err(CoordinatorError::SourceAction)?;
        }
        Ok(hop)
    }
}

fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(40);
    let joined = lines[start..].join("\n");
    crate::history::clip(&joined, 8 * 1024)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::history::tests::item;
    use crate::preview::{PreviewInput, prepare};
    use crate::seed::HeaderFacts;
    use shinatama_export::{ItemKind, Redactions};
    use std::path::PathBuf;

    struct FakeClock(i64);
    impl Clock for FakeClock {
        fn now_ms(&mut self) -> i64 {
            self.0
        }
        fn pause(&mut self, ms: u64) {
            self.0 += ms as i64;
        }
    }

    #[derive(Default)]
    struct FakeDestination {
        spawned: Vec<Delivery>,
        killed: Vec<String>,
        fail_spawn: bool,
        exit: bool,
        stored: Option<String>,
        reply: Option<String>,
        session: bool,
    }
    impl Destination for FakeDestination {
        fn spawn(&mut self, _: &HopPlan, delivery: &Delivery) -> Result<String, String> {
            if self.fail_spawn {
                return Err("codex: not signed in".into());
            }
            self.spawned.push(delivery.clone());
            Ok("i_dest".into())
        }
        fn evidence(&mut self, _: &str) -> Evidence {
            Evidence {
                alive: !self.exit,
                native_session_id: self.session.then(|| "dest-native".into()),
                first_user_sha256: self.stored.as_deref().map(|s| sha256(s.as_bytes())),
                first_reply: self.reply.clone(),
                last_output: "last line".into(),
            }
        }
        fn kill(&mut self, instance: &str) {
            self.killed.push(instance.into());
        }
    }
    impl FakeDestination {
        /// Behaves like a destination that stored exactly what it got and confirmed.
        fn honest(preview: &Preview) -> Self {
            Self { stored: Some(first_entry_bytes(preview).to_owned()), reply: Some(format!("ready {}\ncontinuing", preview.nonce)), session: true, ..Self::default() }
        }
    }

    #[derive(Default)]
    struct FakeSource {
        busy: bool,
        draft: bool,
        ended: u32,
        closed: u32,
    }
    impl Source for FakeSource {
        fn is_idle(&mut self) -> bool {
            !self.busy
        }
        fn has_draft_or_running(&mut self) -> bool {
            self.draft
        }
        fn end_activation(&mut self) -> Result<(), String> {
            self.ended += 1;
            Ok(())
        }
        fn close_instance(&mut self) -> Result<(), String> {
            self.closed += 1;
            Ok(())
        }
    }

    struct Fixture {
        base: PathBuf,
        preview: Preview,
        journal: HopJournal,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }
    fn fixture() -> Fixture {
        let base = std::env::temp_dir().join(format!("shinatama-coord-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(base.join("data")).unwrap();
        let items = [item(ItemKind::UserText, "do the thing"), item(ItemKind::AssistantText, "ok")];
        let preview = prepare(PreviewInput {
            items: &items,
            facts: HeaderFacts { source_agent: "claude_code".into(), source_session: "src".into(), destination_agent: "codex".into(), time: "t".into(), ..Default::default() },
            window_tokens: None,
            export_redactions: &Redactions::new(),
            project: None,
            data_dir: &base.join("data"),
        })
        .unwrap();
        Fixture { base, preview, journal: HopJournal::open_in_memory().unwrap() }
    }
    fn request<'a>(preview: &'a Preview, mode: HopMode) -> HopRequest<'a> {
        HopRequest {
            preview,
            thread_id: "t_source",
            mode,
            destination_thread_id: (mode == HopMode::Fork).then_some("t_forked"),
            source_agent: "claude_code",
            source_native_id: "src",
            source_instance: Some("i_source"),
        }
    }

    #[test]
    fn close_hop_runs_effects_only_after_confirm_and_closes_the_source_last() {
        let f = fixture();
        let mut coordinator = Coordinator::new(&f.journal, FakeClock(0));
        let mut destination = FakeDestination::honest(&f.preview);
        let mut source = FakeSource::default();
        let hop = coordinator.plan(&request(&f.preview, HopMode::Close)).unwrap();
        assert_eq!(hop.state, HopState::Previewed);
        assert!(destination.spawned.is_empty(), "nothing runs before CONFIRMED");
        assert!(coordinator.prepare(&hop.plan.id, &f.preview, &mut destination).is_err(), "prepare needs CONFIRMED");
        assert!(destination.spawned.is_empty());
        coordinator.confirm(&hop.plan.id).unwrap();
        let ready = coordinator.prepare(&hop.plan.id, &f.preview, &mut destination).unwrap();
        assert_eq!(ready.state, HopState::Ready);
        assert_eq!(ready.destination_native_id.as_deref(), Some("dest-native"));
        assert_eq!(destination.spawned, [f.preview.delivery.clone()]);
        assert_eq!((source.ended, source.closed), (0, 0));
        coordinator.switch(&hop.plan.id, &mut source).unwrap();
        assert_eq!((source.ended, source.closed), (1, 0), "the source runs until COMMITTED");
        let committed = coordinator.commit(&hop.plan.id, &mut source, false).unwrap();
        assert_eq!(committed.state, HopState::Committed);
        assert_eq!((source.ended, source.closed), (1, 1));
        let kinds: Vec<_> = f.journal.events(&hop.plan.id).unwrap().into_iter().map(|e| e.kind).collect();
        let attempted = kinds.iter().position(|k| k == "spawn_attempted").unwrap();
        assert!(kinds.iter().position(|k| k == "confirmed").unwrap() < attempted, "{kinds:?}");
    }

    #[test]
    fn fork_never_touches_the_source() {
        let f = fixture();
        let mut coordinator = Coordinator::new(&f.journal, FakeClock(0));
        let mut destination = FakeDestination::honest(&f.preview);
        let mut source = FakeSource { busy: true, draft: true, ..Default::default() };
        let hop = coordinator.plan(&request(&f.preview, HopMode::Fork)).unwrap();
        assert_eq!(hop.plan.destination_thread_id.as_deref(), Some("t_forked"));
        coordinator.confirm(&hop.plan.id).unwrap();
        coordinator.prepare(&hop.plan.id, &f.preview, &mut destination).unwrap();
        coordinator.switch(&hop.plan.id, &mut source).unwrap();
        coordinator.commit(&hop.plan.id, &mut source, false).unwrap();
        assert_eq!((source.ended, source.closed), (0, 0));
    }

    #[test]
    fn failures_leave_the_source_untouched_and_stop_only_what_the_hop_started() {
        // Spawn refused.
        let f = fixture();
        let mut coordinator = Coordinator::new(&f.journal, FakeClock(0));
        let hop = coordinator.plan(&request(&f.preview, HopMode::Close)).unwrap();
        coordinator.confirm(&hop.plan.id).unwrap();
        let mut destination = FakeDestination { fail_spawn: true, ..Default::default() };
        assert!(matches!(coordinator.prepare(&hop.plan.id, &f.preview, &mut destination), Err(CoordinatorError::SpawnFailed { .. })));
        assert_eq!(f.journal.hop(&hop.plan.id).unwrap().unwrap().state, HopState::Failed);
        assert!(destination.killed.is_empty());
        // Seed mismatch: failed and the destination stopped.
        let g = fixture();
        let mut coordinator = Coordinator::new(&g.journal, FakeClock(0));
        let hop = coordinator.plan(&request(&g.preview, HopMode::Close)).unwrap();
        coordinator.confirm(&hop.plan.id).unwrap();
        let mut destination = FakeDestination { stored: Some("something else".into()), session: true, ..Default::default() };
        assert!(matches!(coordinator.prepare(&hop.plan.id, &g.preview, &mut destination), Err(CoordinatorError::SeedMismatch { .. })));
        assert_eq!(destination.killed, ["i_dest"]);
        // No session in 90 s: failed and stopped.
        let h = fixture();
        let mut coordinator = Coordinator::new(&h.journal, FakeClock(0));
        let hop = coordinator.plan(&request(&h.preview, HopMode::Close)).unwrap();
        coordinator.confirm(&hop.plan.id).unwrap();
        let mut destination = FakeDestination::default();
        assert!(matches!(coordinator.prepare(&hop.plan.id, &h.preview, &mut destination), Err(CoordinatorError::NoSession { .. })));
        assert_eq!(destination.killed, ["i_dest"]);
        assert_eq!(h.journal.hop(&hop.plan.id).unwrap().unwrap().state, HopState::Failed);
    }

    #[test]
    fn missing_receipt_waits_for_the_user_and_busy_source_blocks_the_switch() {
        let f = fixture();
        let mut coordinator = Coordinator::new(&f.journal, FakeClock(0));
        let hop = coordinator.plan(&request(&f.preview, HopMode::Close)).unwrap();
        coordinator.confirm(&hop.plan.id).unwrap();
        let mut destination = FakeDestination { reply: Some("hello".into()), ..FakeDestination::honest(&f.preview) };
        assert_eq!(coordinator.prepare(&hop.plan.id, &f.preview, &mut destination), Err(CoordinatorError::ReceiptMissing));
        assert_eq!(f.journal.hop(&hop.plan.id).unwrap().unwrap().state, HopState::Preparing);
        coordinator.accept_without_receipt(&hop.plan.id).unwrap();
        let mut source = FakeSource { busy: true, draft: true, ..Default::default() };
        assert_eq!(coordinator.switch(&hop.plan.id, &mut source), Err(CoordinatorError::SourceBusy));
        assert_eq!(f.journal.hop(&hop.plan.id).unwrap().unwrap().state, HopState::Ready, "unchanged");
        source.busy = false;
        coordinator.switch(&hop.plan.id, &mut source).unwrap();
        assert_eq!(coordinator.commit(&hop.plan.id, &mut source, false), Err(CoordinatorError::SourceNeedsConfirmation));
        assert_eq!(source.closed, 0);
        coordinator.commit(&hop.plan.id, &mut source, true).unwrap();
        assert_eq!(source.closed, 1);
    }

    #[test]
    fn a_changed_seed_file_aborts_before_any_spawn_and_a_second_hop_of_the_thread_loses() {
        let f = fixture();
        let mut coordinator = Coordinator::new(&f.journal, FakeClock(0));
        let hop = coordinator.plan(&request(&f.preview, HopMode::Close)).unwrap();
        let g = fixture();
        assert!(matches!(
            coordinator.plan(&request(&g.preview, HopMode::Close)),
            Err(CoordinatorError::Journal(HopError::ThreadBusy { .. }))
        ));
        coordinator.confirm(&hop.plan.id).unwrap();
        let mut tampered = f.preview.clone();
        tampered.bootstrap.push('x');
        let mut destination = FakeDestination::honest(&f.preview);
        assert_eq!(coordinator.prepare(&hop.plan.id, &tampered, &mut destination), Err(CoordinatorError::SnapshotChanged));
        assert!(destination.spawned.is_empty());
        assert_eq!(f.journal.hop(&hop.plan.id).unwrap().unwrap().state, HopState::Aborted);
    }
}
