//! C28 hop run in the desktop (FRD-0005 R35-R40, R42).
//!
//! The destination opens through the qualified Open path in a new pane; the verified bootstrap
//! is delivered by the `paste` channel (bracketed paste, R31's last resort) because no
//! harness has a qualified argv prompt recipe yet. Every arrow and effect is journaled in
//! `hops.sqlite` before it runs. A hop from the session library has no live source, so it is
//! a fork: the source session is never touched.
#![cfg(windows)]
use crate::sessions::hop::{KeptPreview, kept_preview};
use crate::{Host, agents};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shinatama_hop::preview::verify_send;
use shinatama_hop::recovery::{RecoveryRuntime, reconcile};
use shinatama_state::hops::{HopJournal, HopMode, HopPlan, HopRecord, HopState, HopStrategy};
use std::sync::{Arc, OnceLock};
use tauri::{Manager, State, WebviewWindow};

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

struct HostRuntime<'a>(&'a Host);
impl RecoveryRuntime for HostRuntime<'_> {
    fn destination_alive(&mut self, hop: &HopRecord) -> bool {
        hop.destination_instance.as_deref().is_some_and(|id| self.0.info(id).is_ok())
    }
    fn kill_destination(&mut self, instance: &str) {
        let _ = self.0.stop(instance);
    }
    fn reopen_source(&mut self, _: &HopRecord) -> Result<(), String> {
        // Hops from the desktop start from history and never end a live source activation.
        Err("no live source activation is owned by desktop hops".into())
    }
}

/// Opens the journal once per process and runs startup recovery (R42) against live panes.
fn journal(window: &WebviewWindow) -> Result<&'static HopJournal, String> {
    static JOURNAL: OnceLock<Result<HopJournal, String>> = OnceLock::new();
    JOURNAL
        .get_or_init(|| {
            let services = window.state::<Arc<crate::storage::Services>>();
            let state = services.load()?;
            let journal = HopJournal::open(&state.location).map_err(|e| e.to_string())?;
            let host = window.state::<Host>();
            let _ = reconcile(&journal, &mut HostRuntime(host.inner()), now_ms());
            Ok(journal)
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// The journal's thread key for a library session: stable per source session, so two hops of
/// the same session never run at once (I1).
fn source_key(kept: &KeptPreview) -> String {
    let mut hash = Sha256::new();
    for part in [kept.source_root.store_root_id.as_str(), kept.source_agent.as_str(), kept.source_native_id.as_str()] {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part.as_bytes());
    }
    let digest = hash.finalize();
    format!("session:{}", shinatama_export::stx::hex(&digest[..16]))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchRequest {
    hop_id: String,
    rows: u16,
    cols: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Launched {
    hop_id: String,
    nonce: String,
    started: agents::AgentStarted,
}

/// PLANNED -> PREVIEWED -> CONFIRMED (the user pressed Hop) -> PREPARING, then the destination
/// opens. Nothing reaches it until `hop_deliver`.
#[tauri::command]
pub async fn hop_launch(window: WebviewWindow, host: State<'_, Host>, request: LaunchRequest) -> Result<Launched, String> {
    crate::local(&window)?;
    let kept = kept_preview(&request.hop_id).ok_or("This preview is no longer available; prepare it again.")?;
    let cwd = kept.cwd.clone().ok_or("The session's project folder is not available here, so the destination cannot open in it.")?;
    let preview = &kept.preview;
    if !verify_send(preview, preview.bootstrap.as_bytes()) {
        return Err("The seed file changed after the preview; prepare it again.".into());
    }
    let journal = journal(&window)?;
    let plan = HopPlan {
        id: preview.hop_id.clone(),
        thread_id: source_key(&kept),
        destination_thread_id: None,
        mode: HopMode::Fork,
        strategy: HopStrategy::SeedPrompt,
        source_agent: kept.source_agent.clone(),
        source_native_id: kept.source_native_id.clone(),
        source_instance: None,
        destination_agent: preview.destination.clone(),
        nonce: preview.nonce.clone(),
        snapshot_sha256: preview.seed_sha256.clone(),
        bootstrap_sha256: preview.bootstrap_sha256.clone(),
        seed_path: preview.seed_path.clone(),
    };
    let mut hop = journal.plan(&plan, now_ms()).map_err(|e| e.to_string())?;
    for next in [HopState::Previewed, HopState::Confirmed, HopState::Preparing] {
        hop = journal.transition(&hop.plan.id, hop.state, hop.revision, next, None, now_ms()).map_err(|e| e.to_string())?;
    }
    journal.record_effect(&hop.plan.id, "spawn_attempted", Some(&preview.destination), now_ms()).map_err(|e| e.to_string())?;
    let owner = window.state::<Arc<agents::Controller>>().inner().clone();
    let started = match owner.open(&preview.destination, host.inner().clone(), cwd, request.rows, request.cols).await {
        Ok(started) => started,
        Err(message) => {
            let _ = journal.transition(&hop.plan.id, hop.state, hop.revision, HopState::Failed, Some(message), now_ms());
            return Err(format!("{message} The source session is untouched."));
        }
    };
    journal
        .set_destination_with_thread(&hop.plan.id, Some(&started.session.id), None, Some(&started.thread_id), now_ms())
        .map_err(|e| e.to_string())?;
    journal.record_effect(&hop.plan.id, "spawn_observed", Some(&started.session.id), now_ms()).map_err(|e| e.to_string())?;
    Ok(Launched { hop_id: hop.plan.id, nonce: preview.nonce.clone(), started })
}

/// Delivers the exact previewed bootstrap into the destination's input as one bracketed paste.
#[tauri::command]
pub async fn hop_deliver(window: WebviewWindow, host: State<'_, Host>, hop_id: String) -> Result<(), String> {
    crate::local(&window)?;
    let journal = journal(&window)?;
    let hop = journal.hop(&hop_id).map_err(|e| e.to_string())?.ok_or("The hop is not in the journal.")?;
    if hop.state != HopState::Preparing {
        return Err("The hop is not waiting for delivery.".into());
    }
    let kept = kept_preview(&hop_id).ok_or("This preview is no longer available.")?;
    let instance = hop.destination_instance.as_deref().ok_or("The destination was not recorded.")?;
    let preview = &kept.preview;
    if preview.bootstrap_sha256 != hop.plan.bootstrap_sha256 || !verify_send(preview, preview.bootstrap.as_bytes()) {
        let _ = journal.transition(&hop_id, hop.state, hop.revision, HopState::Aborted, Some("snapshot changed before delivery"), now_ms());
        return Err("The seed changed after the preview; nothing was sent.".into());
    }
    journal.record_effect(&hop_id, "seed_delivery", Some("paste"), now_ms()).map_err(|e| e.to_string())?;
    let mut bytes = b"\x1b[200~".to_vec();
    bytes.extend_from_slice(preview.bootstrap.as_bytes());
    bytes.extend_from_slice(b"\x1b[201~");
    host.write(instance, bytes).map_err(|e| e.to_string())?;
    std::thread::sleep(std::time::Duration::from_millis(150));
    host.write(instance, b"\r".to_vec()).map_err(|e| e.to_string())?;
    journal.record_effect(&hop_id, "seed_delivered", Some(&preview.bootstrap_sha256), now_ms()).map_err(|e| e.to_string())
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FinishOutcome {
    /// The destination replied with `ready <nonce>`.
    Receipt,
    /// No receipt; the user chose to keep the destination anyway (R38).
    AcceptWithoutReceipt,
    /// The user cancelled or the destination never answered.
    Fail,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finished {
    state: HopState,
}

/// READY -> SWITCHING -> COMMITTED for a fork from history: there is no live source to end or
/// close. FAILED leaves the destination pane open so its last output stays visible (R44).
#[tauri::command]
pub async fn hop_finish(window: WebviewWindow, hop_id: String, outcome: FinishOutcome) -> Result<Finished, String> {
    crate::local(&window)?;
    let journal = journal(&window)?;
    let hop = journal.hop(&hop_id).map_err(|e| e.to_string())?.ok_or("The hop is not in the journal.")?;
    if hop.state != HopState::Preparing {
        return Err("The hop is not waiting for its receipt.".into());
    }
    let events = journal.events(&hop_id).map_err(|e| e.to_string())?;
    if !events.iter().any(|e| e.kind == "seed_delivered") && outcome != FinishOutcome::Fail {
        return Err("The seed was not delivered.".into());
    }
    if outcome == FinishOutcome::Fail {
        let failed = journal.transition(&hop_id, hop.state, hop.revision, HopState::Failed, Some("no receipt; user cancelled"), now_ms()).map_err(|e| e.to_string())?;
        return Ok(Finished { state: failed.state });
    }
    let detail = match outcome {
        FinishOutcome::Receipt => "receipt seen; paste channel, first-entry hash not verified",
        _ => "user switched without receipt; paste channel",
    };
    let mut hop = journal.transition(&hop_id, hop.state, hop.revision, HopState::Ready, Some(detail), now_ms()).map_err(|e| e.to_string())?;
    for next in [HopState::Switching, HopState::Committed] {
        hop = journal.transition(&hop_id, hop.state, hop.revision, next, None, now_ms()).map_err(|e| e.to_string())?;
    }
    Ok(Finished { state: hop.state })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalView {
    record: HopRecord,
    events: Vec<shinatama_state::hops::HopEvent>,
}

/// Read-only view of one hop for the dialog and proofs.
#[tauri::command]
pub async fn hop_status(window: WebviewWindow, hop_id: String) -> Result<JournalView, String> {
    crate::local(&window)?;
    let journal = journal(&window)?;
    let record = journal.hop(&hop_id).map_err(|e| e.to_string())?.ok_or("The hop is not in the journal.")?;
    let events = journal.events(&hop_id).map_err(|e| e.to_string())?;
    Ok(JournalView { record, events })
}
