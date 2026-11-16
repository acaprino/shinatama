//! `shinatama_thread_peers`: the sessions of the caller's thread (a workset, ADR-0055)
//! and the Mukade names they talk to each other with. Every session of one thread
//! opens the same room by name, so the room needs no record of its own here.
//! Reads the shared state only: it starts, stops, joins and sends nothing.
use crate::storage;
use serde_json::{Value, json};
use shinatama_engine::session_control::{Caller, ThreadTools, thread_member_name, thread_room_name};
use shinatama_state::threads::{ThreadStatus, ThreadSummary};
use std::sync::Arc;

/// Harnesses Shinatama gives the Mukade chat tools at launch (`claude_launch`, `codex_launch`).
const CHAT_HARNESSES: &[&str] = &["claude_code", "codex"];

pub struct Peers {
    pub services: Arc<storage::Services>,
}

impl ThreadTools for Peers {
    fn peers(&self, caller: &Caller<'_>) -> Result<Value, &'static str> {
        let state = self.services.load().map_err(|_| "state_unavailable")?;
        let store = &state.store;
        let workset = store
            .workset_of_thread(caller.session)
            .map_err(|_| "state_unavailable")?
            .ok_or("thread_not_found")?;
        let record = store
            .workset_record(&workset)
            .map_err(|_| "state_unavailable")?
            .ok_or("thread_not_found")?;
        let sessions = store.workset_threads(&workset).map_err(|_| "state_unavailable")?;
        let service = match shinatama_engine::mukade_mcp::Connection::discover(&state.location) {
            Ok(Some(_)) => "running",
            Ok(None) => "not_started",
            Err(_) => "unavailable",
        };
        projection(&record.id, record.title.as_deref(), &sessions, caller, service)
    }
}

fn session_state(status: ThreadStatus) -> &'static str {
    match status {
        ThreadStatus::RecordedOpen => "running",
        ThreadStatus::Inactive => "stopped",
        ThreadStatus::Unbound => "not_started",
        // The interface calls an archived session a closed one (ADR-0055).
        ThreadStatus::Archived => "closed",
    }
}

/// The tool's answer. The caller's own harness is the receiver's, not the stored
/// segment's: a session that has just started may have no segment yet.
pub(crate) fn projection(
    workset_id: &str,
    title: Option<&str>,
    sessions: &[ThreadSummary],
    caller: &Caller<'_>,
    service: &str,
) -> Result<Value, &'static str> {
    let room = thread_room_name(workset_id).ok_or("thread_not_found")?;
    let me = thread_member_name(caller.harness, caller.session).ok_or("thread_not_found")?;
    let peers: Vec<Value> = sessions
        .iter()
        .filter(|session| session.id != caller.session)
        .map(|session| {
            let harness = session.harness.as_deref();
            let chat = harness.is_some_and(|harness| CHAT_HARNESSES.contains(&harness));
            json!({
                "session_id": session.id,
                "harness": harness,
                "title": session.title,
                "state": session_state(session.status),
                "member_name": if chat { harness.and_then(|harness| thread_member_name(harness, &session.id)) } else { None },
                "chat": if chat { "mukade" } else { "none" },
            })
        })
        .collect();
    let how_to = [
        format!("1. chat_open with a fresh UUID request_id and name \"{room}\". It returns the same room_id to every session of this thread; keep it."),
        format!("2. chat_join with a fresh UUID request_id, that room_id, harness \"{}\", role \"peer\" and name \"{me}\". Keep the returned member_handle to yourself. If the answer is member_name_taken, an earlier run of this session joined: find the member_id named \"{me}\" with chat_members and call chat_rejoin with it instead.", caller.harness),
        "3. chat_send with recipients set to the peer's member_name. Refer to files by path; never send credentials or file contents.".to_owned(),
        "4. Another session sees a message only when it reads the room: an idle one does not wake by itself. To wait for an answer, call chat_wait in slices of at most 30 seconds for a bounded time and chat_ack each batch after handling it; otherwise tell the user to ask that session to read the thread's room.".to_owned(),
        format!("5. To bring a new session into this thread, call shinatama_session_open with this room_id and coordinator \"{me}\"."),
    ];
    let mut note = String::from("A peer with chat \"none\" has no mukade tools in Shinatama and cannot be reached this way.");
    if service != "running" {
        note.push_str(" The mukade chat service is not running now: chat calls will fail until Shinatama starts it.");
    }
    note.push_str(" A session started before the service ran has no mukade tools: resuming it, or a new session, gives them.");
    Ok(json!({
        "thread": {"id": workset_id, "title": title},
        "mukade": {"service": service, "room_name": room},
        "you": {"session_id": caller.session, "harness": caller.harness, "member_name": me},
        "peers": peers,
        "how_to": how_to,
        "note": note,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shinatama_state::records::ThreadKind;

    fn session(id: &str, harness: Option<&str>, status: ThreadStatus) -> ThreadSummary {
        ThreadSummary {
            id: id.into(),
            project_id: "p_aaaaaaaaaaaa".into(),
            kind: ThreadKind::Work,
            title: None,
            revision: 1,
            updated_at: 1,
            harness: harness.map(Into::into),
            native_available: Some(true),
            status,
        }
    }

    #[test]
    fn lists_the_other_sessions_with_their_stable_member_names() {
        let caller = Caller { instance: "i_aaaaaaaaaaaa", session: "t_aaaaaaaaaaaa", harness: "claude_code" };
        let sessions = [
            // The caller's own record, still without a segment.
            session("t_aaaaaaaaaaaa", None, ThreadStatus::Unbound),
            session("t_bbbbbbbbbbbb", Some("codex"), ThreadStatus::RecordedOpen),
            session("t_cccccccccccc", Some("pi"), ThreadStatus::Inactive),
        ];
        let value = projection("w_dddddddddddd", Some("Billing"), &sessions, &caller, "running").unwrap();
        assert_eq!(value["mukade"]["room_name"], "shinatama-thread-dddddddddddd");
        assert_eq!(value["you"]["member_name"], "claude-aaaaaaaaaaaa");
        let peers = value["peers"].as_array().unwrap();
        assert_eq!(peers.len(), 2);
        assert_eq!(peers[0]["member_name"], "codex-bbbbbbbbbbbb");
        assert_eq!(peers[0]["state"], "running");
        assert_eq!(peers[0]["chat"], "mukade");
        assert_eq!(peers[1]["member_name"], Value::Null);
        assert_eq!(peers[1]["chat"], "none");
        assert!(value["how_to"][1].as_str().unwrap().contains("claude-aaaaaaaaaaaa"));
        assert!(!value["note"].as_str().unwrap().contains("not running now"));
    }

    #[test]
    fn a_stopped_service_is_said_and_invalid_ids_are_refused() {
        let caller = Caller { instance: "i_aaaaaaaaaaaa", session: "t_aaaaaaaaaaaa", harness: "codex" };
        let value = projection("w_dddddddddddd", None, &[], &caller, "not_started").unwrap();
        assert!(value["note"].as_str().unwrap().contains("not running now"));
        assert!(projection("bad", None, &[], &caller, "running").is_err());
        let stranger = Caller { session: "x", ..caller };
        assert!(projection("w_dddddddddddd", None, &[], &stranger, "running").is_err());
    }
}
