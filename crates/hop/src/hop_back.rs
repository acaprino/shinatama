//! C29.3: hopping back to an earlier segment and UNDO (FRD-0005 R14, R41, R45-R46).
//!
//! When the destination agent already has a segment of the thread, the hop resumes that
//! native session (its id unchanged) and sends only what it missed as a catch-up. If that
//! session is gone (for example the agent's retention deleted it) the hop falls back to a
//! seed and says why. A segment that is already live is never activated twice. UNDO is
//! offered for ten minutes after a close hop: it is that same hop back, and once it commits
//! the original hop is marked ROLLED_BACK.
use crate::catchup::NativeCursor;
use shinatama_state::hops::{HopError, HopJournal, HopMode, HopRecord, HopState};

pub const UNDO_WINDOW_MS: i64 = 10 * 60 * 1000;

/// One earlier segment of the thread, as the runtime sees it now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentView {
    pub segment_id: String,
    pub agent: String,
    pub native_id: String,
    /// The segment has an open activation (a live process writes it).
    pub live: bool,
    /// The native session still exists in its store.
    pub exists: bool,
    /// What that segment had seen when it was left.
    pub cursor: Option<NativeCursor>,
    pub left_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HopBack {
    /// Resume the earlier native session; deliver turns after `since` as a catch-up.
    NativeResume {
        segment_id: String,
        native_id: String,
        since: Option<NativeCursor>,
    },
    /// Seed a new native session.
    SeedPrompt { reason: &'static str },
    /// The destination is already live in this thread: focus it instead.
    AlreadyLive { segment_id: String },
}

/// Chooses how to reach `destination_agent` for a thread with these segments (R14).
pub fn plan(destination_agent: &str, segments: &[SegmentView]) -> HopBack {
    let Some(latest) = segments
        .iter()
        .filter(|s| s.agent == destination_agent)
        .max_by_key(|s| s.left_at_ms)
    else {
        return HopBack::SeedPrompt {
            reason: "This agent has no earlier part of the conversation.",
        };
    };
    if latest.live {
        return HopBack::AlreadyLive {
            segment_id: latest.segment_id.clone(),
        };
    }
    if !latest.exists {
        return HopBack::SeedPrompt {
            reason: "The agent's earlier session no longer exists (its own retention removed it); a seed is used instead.",
        };
    }
    HopBack::NativeResume {
        segment_id: latest.segment_id.clone(),
        native_id: latest.native_id.clone(),
        since: latest.cursor.clone(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UndoOffer {
    pub hop_id: String,
    pub back_to_agent: String,
    pub back_to_native_id: String,
    pub expires_at_ms: i64,
}

/// UNDO is offered only for a committed close hop, for ten minutes (R41).
pub fn undo_offer(hop: &HopRecord, now_ms: i64) -> Option<UndoOffer> {
    let expires_at_ms = hop.updated_at_ms + UNDO_WINDOW_MS;
    (hop.state == HopState::Committed && hop.plan.mode == HopMode::Close && now_ms <= expires_at_ms)
        .then(|| UndoOffer {
            hop_id: hop.plan.id.clone(),
            back_to_agent: hop.plan.source_agent.clone(),
            back_to_native_id: hop.plan.source_native_id.clone(),
            expires_at_ms,
        })
}

/// After the hop back committed, the original hop becomes ROLLED_BACK.
pub fn complete_undo(
    journal: &HopJournal,
    original: &str,
    back: &str,
    now_ms: i64,
) -> Result<HopRecord, HopError> {
    let hop = journal.hop(original)?.ok_or(HopError::NotFound)?;
    let back_hop = journal.hop(back)?.ok_or(HopError::NotFound)?;
    if back_hop.state != HopState::Committed
        || back_hop.plan.thread_id != hop.plan.thread_id
        || back_hop.plan.destination_agent != hop.plan.source_agent
        || undo_offer(&hop, back_hop.created_at_ms).is_none()
    {
        return Err(HopError::Invalid("undo"));
    }
    journal.transition(
        original,
        hop.state,
        hop.revision,
        HopState::RolledBack,
        Some(&format!("undone by {back}")),
        now_ms,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catchup::{items_since, render};
    use crate::history::tests::item;
    use shinatama_export::ItemKind;
    use shinatama_state::hops::{HopPlan, HopStrategy};

    fn segment(agent: &str, native: &str, live: bool, exists: bool, at: i64) -> SegmentView {
        SegmentView {
            segment_id: format!("s_{native}"),
            agent: agent.into(),
            native_id: native.into(),
            live,
            exists,
            cursor: None,
            left_at_ms: at,
        }
    }

    #[test]
    fn chooses_native_resume_seed_or_already_live() {
        let segments = [
            segment("claude_code", "old", false, true, 1),
            segment("claude_code", "newer", false, true, 5),
            segment("codex", "c1", false, true, 9),
        ];
        assert!(
            matches!(plan("claude_code", &segments), HopBack::NativeResume { ref native_id, .. } if native_id == "newer")
        );
        assert!(matches!(plan("pi", &segments), HopBack::SeedPrompt { .. }));
        let gone = [segment("claude_code", "old", false, false, 1)];
        assert!(
            matches!(plan("claude_code", &gone), HopBack::SeedPrompt { reason } if reason.contains("no longer exists"))
        );
        let live = [segment("claude_code", "old", true, true, 1)];
        assert_eq!(
            plan("claude_code", &live),
            HopBack::AlreadyLive {
                segment_id: "s_old".into()
            },
            "never a second activation"
        );
    }

    #[test]
    fn hop_back_sends_only_the_missed_turns_while_the_history_canary_stays_native() {
        // Claude segment saw the canary; Codex continued afterwards.
        let claude = vec![
            item(ItemKind::UserText, "the canary is ZEBRA-7"),
            item(ItemKind::AssistantText, "noted"),
        ];
        let mut thread = claude.clone();
        thread.push(item(ItemKind::UserText, "codex: refactor"));
        thread.push(item(ItemKind::AssistantText, "codex: done"));
        let mut seg = segment("claude_code", "claude-native", false, true, 1);
        seg.cursor = Some(NativeCursor::at_end(&claude));
        let HopBack::NativeResume {
            native_id, since, ..
        } = plan("claude_code", &[seg])
        else {
            panic!()
        };
        assert_eq!(
            native_id, "claude-native",
            "the earlier native session id is unchanged"
        );
        let missed = items_since(&since.unwrap(), &thread).unwrap();
        let catchup = render(missed, "codex", "n", None).unwrap();
        assert!(catchup.text.contains("codex: refactor") && catchup.text.contains("via codex"));
        assert!(
            !catchup.text.contains("ZEBRA-7"),
            "the canary is already in the resumed session"
        );
    }

    fn hop(id: char, thread: &str, mode: HopMode, from: &str, to: &str) -> HopPlan {
        HopPlan {
            id: std::iter::repeat_n(id, 32).collect(),
            thread_id: thread.into(),
            destination_thread_id: None,
            mode,
            strategy: HopStrategy::SeedPrompt,
            source_agent: from.into(),
            source_native_id: format!("{from}-native"),
            source_instance: Some("i_s".into()),
            destination_agent: to.into(),
            nonce: "n".into(),
            snapshot_sha256: "a".repeat(64),
            bootstrap_sha256: "b".repeat(64),
            seed_path: "C:/s".into(),
        }
    }

    fn commit(journal: &HopJournal, plan: &HopPlan, at: i64) -> HopRecord {
        let mut record = journal.plan(plan, at).unwrap();
        for next in [
            HopState::Previewed,
            HopState::Confirmed,
            HopState::Preparing,
            HopState::Ready,
            HopState::Switching,
            HopState::Committed,
        ] {
            record = journal
                .transition(
                    &record.plan.id,
                    record.state,
                    record.revision,
                    next,
                    None,
                    at,
                )
                .unwrap();
        }
        record
    }

    #[test]
    fn undo_is_offered_for_ten_minutes_after_a_close_hop_and_marks_it_rolled_back() {
        let journal = HopJournal::open_in_memory().unwrap();
        let original = commit(
            &journal,
            &hop('1', "t_u", HopMode::Close, "claude_code", "codex"),
            1_000,
        );
        let offer = undo_offer(&original, 1_000 + UNDO_WINDOW_MS).unwrap();
        assert_eq!(
            (
                offer.back_to_agent.as_str(),
                offer.back_to_native_id.as_str()
            ),
            ("claude_code", "claude_code-native")
        );
        assert!(undo_offer(&original, 1_001 + UNDO_WINDOW_MS).is_none());
        let back = commit(
            &journal,
            &hop('2', "t_u", HopMode::Close, "codex", "claude_code"),
            60_000,
        );
        let rolled = complete_undo(&journal, &original.plan.id, &back.plan.id, 61_000).unwrap();
        assert_eq!(rolled.state, HopState::RolledBack);
        let fork = commit(
            &journal,
            &hop('3', "t_f", HopMode::Fork, "claude_code", "codex"),
            1_000,
        );
        assert!(
            undo_offer(&fork, 2_000).is_none(),
            "UNDO does not apply to a fork (R13)"
        );
        let late = commit(
            &journal,
            &hop('4', "t_l", HopMode::Close, "claude_code", "codex"),
            0,
        );
        let late_back = commit(
            &journal,
            &hop('5', "t_l", HopMode::Close, "codex", "claude_code"),
            UNDO_WINDOW_MS + 5,
        );
        assert_eq!(
            complete_undo(
                &journal,
                &late.plan.id,
                &late_back.plan.id,
                UNDO_WINDOW_MS + 6
            ),
            Err(HopError::Invalid("undo"))
        );
    }
}
