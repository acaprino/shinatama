//! `shinatama_thread_peers`: the sessions of the caller's thread (a workset, ADR-0055)
//! and the Mukade names they talk to each other with. Every session of one thread
//! opens the same room by name, so the room needs no record of its own here.
//! Reads only: it starts no service, creates no room, joins and sends nothing.
use crate::storage;
use serde_json::{Value, json};
use shinatama_engine::session_control::{Caller, ThreadTools, thread_member_name, thread_room_name};
use shinatama_engine::team::{self, Formation, Role};
use crate::thread_chat::{CHAT_HARNESSES, MANUAL_HARNESSES, find_room, hooked_member, session_state};
use shinatama_state::threads::ThreadSummary;
use std::{collections::BTreeMap, sync::Arc};

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
        // ADR-0057: in a thread that talks, the sessions launched in this run with Mukade's
        // hooks were joined by its adapter under names it chose; those are the names to
        // address. The caller has the hooks only when its own launch composed them.
        let mut names = BTreeMap::new();
        let mut hooks = false;
        if store.thread_chat_enabled(&workset).unwrap_or(false)
            && let Some(room) = find_room(state, &workset)
        {
            hooks = shinatama_engine::mukade_chat::launched_room(caller.session).as_deref() == Some(room.as_str());
            for (session, harness, native) in store.workset_native_sessions(&workset).unwrap_or_default() {
                if let Some(member) = hooked_member(state, &session, &harness, &native, &room) {
                    names.insert(session, member.name);
                }
            }
        }
        // The team map the user applied, when the thread talks: the caller's place in it and its peers'.
        let team = if store.thread_chat_enabled(&workset).unwrap_or(false) {
            crate::thread_team::load(store, &workset)
                .ok()
                .and_then(|record| record.formation)
                .and_then(|formation| TeamView::of(&formation, caller.session))
        } else {
            None
        };
        projection_with_team(&record.id, record.title.as_deref(), &sessions, caller, service, &names, hooks, team.as_ref())
    }
}

/// The caller's place in the thread's team and its peers', as the engine computed them.
pub(crate) struct TeamView {
    revision: u64,
    name: String,
    role: Role,
    /// One sentence per relation of the caller, from its own side (`team::told`).
    relations: Vec<String>,
    /// The other agents that have a session, by session id.
    peers: BTreeMap<String, (String, Role)>,
}

impl TeamView {
    /// `None` when `session` is not an agent's session on the map: it has no place in the team.
    pub(crate) fn of(formation: &Formation, session: &str) -> Option<Self> {
        let me = formation.agents.iter().find(|agent| agent.session.as_deref() == Some(session))?;
        let relations = formation
            .links
            .iter()
            .enumerate()
            .filter(|(_, link)| link.from == me.id || link.to == me.id)
            .map(|(k, link)| team::told(formation, k, link.from == me.id))
            .collect();
        let peers = formation
            .agents
            .iter()
            .filter(|agent| agent.id != me.id)
            .filter_map(|agent| Some((agent.session.clone()?, (agent.name.clone(), team::role(formation, &agent.id)))))
            .collect();
        Some(Self { revision: formation.revision, name: me.name.clone(), role: team::role(formation, &me.id), relations, peers })
    }

    fn to_json(&self) -> Value {
        json!({
            "revision": self.revision,
            "you": {"name": self.name, "role": self.role, "relations": self.relations},
            "peers": self.peers.iter().map(|(session, (name, role))| (session.clone(), json!({"name": name, "role": role}))).collect::<serde_json::Map<_, _>>(),
            "note": "Set by the user in Shinatama's Team map; the member shinatama relays it.",
        })
    }
}

/// The tool's answer. The caller's own harness is the receiver's, not the stored
/// segment's: a session that has just started may have no segment yet.
#[cfg(test)]
pub(crate) fn projection(
    workset_id: &str,
    title: Option<&str>,
    sessions: &[ThreadSummary],
    caller: &Caller<'_>,
    service: &str,
    names: &BTreeMap<String, String>,
    hooks: bool,
) -> Result<Value, &'static str> {
    projection_with_team(workset_id, title, sessions, caller, service, names, hooks, None)
}

/// `projection`, with the thread's team map when the caller has a place in it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn projection_with_team(
    workset_id: &str,
    title: Option<&str>,
    sessions: &[ThreadSummary],
    caller: &Caller<'_>,
    service: &str,
    // Member names Mukade's hooks gave sessions, by session id; `hooks`: the caller is one.
    names: &BTreeMap<String, String>,
    hooks: bool,
    team: Option<&TeamView>,
) -> Result<Value, &'static str> {
    let room = thread_room_name(workset_id).ok_or("thread_not_found")?;
    // With the hooks the adapter chose the name; before its first turn has joined, the name is
    // the one its `[mukade chat]` note gives. Without them it is the stable name.
    let known = match names.get(caller.session) {
        Some(name) => Some(name.clone()),
        None if hooks => None,
        None => Some(thread_member_name(caller.harness, caller.session).ok_or("thread_not_found")?),
    };
    let me = known.clone().unwrap_or_else(|| "the name your [mukade chat] note gives".to_owned());
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
                // OpenCode and Pi join through Mukade's kit under a name it chooses, not tied to the
                // session here: chat_members lists it.
                "member_name": if chat {
                    names.get(&session.id).cloned().or_else(|| {
                        harness
                            .filter(|harness| MANUAL_HARNESSES.contains(harness))
                            .and_then(|harness| thread_member_name(harness, &session.id))
                    })
                } else {
                    None
                },
                "chat": if chat { "mukade" } else { "none" },
            })
        })
        .collect();
    let manual = [
        format!("1. chat_open with a fresh UUID request_id and name \"{room}\". It returns the same room_id to every session of this thread; keep it."),
        format!("2. chat_join with a fresh UUID request_id, that room_id, harness \"{}\", role \"peer\" and name \"{me}\". Keep the returned member_handle to yourself. If the answer is member_name_taken, an earlier run of this session joined: find the member_id named \"{me}\" with chat_members and call chat_rejoin with it instead.", caller.harness),
        "3. chat_send with recipients set to the peer's member_name. Refer to files by path; never send credentials or file contents.".to_owned(),
        "4. Another session sees a message only when it reads the room: an idle one does not wake by itself. To wait for an answer, call chat_wait in slices of at most 30 seconds for a bounded time and chat_ack each batch after handling it; otherwise tell the user to ask that session to read the thread's room.".to_owned(),
        format!("5. To bring a new session into this thread, call shinatama_session_open with this room_id and coordinator \"{me}\"."),
    ];
    let joined = [
        format!("1. You are already the member \"{me}\" of this thread's room \"{room}\": Shinatama's hooks joined you, and the [mukade chat] note at the start of your session gives your room_id and member_handle. Do not call chat_join again."),
        "2. chat_send with recipients set to the peer's member_name. Refer to files by path; never send credentials or file contents.".to_owned(),
        "3. Do not call chat_read or chat_wait: what other members write reaches you at your turns, and after each turn you wait a few minutes for a message addressed to you. A peer that listens the same way answers on its own; one with state other than running answers when resumed.".to_owned(),
        format!("4. To bring a new session into this thread, call shinatama_session_open with your room_id and coordinator \"{me}\"."),
    ];
    let how_to: Vec<String> = if hooks { joined.to_vec() } else { manual.to_vec() };
    let mut note = String::from("A peer with chat \"none\" has no mukade tools in Shinatama and cannot be reached this way. An OpenCode or Pi peer joins the room under the name Mukade gives it (for example pi-1): find it with chat_members.");
    if service != "running" {
        note.push_str(" The mukade chat service is not running now: chat calls will fail until Shinatama starts it.");
    }
    note.push_str(" A session started before the service ran has no mukade tools: resuming it, or a new session, gives them.");
    let mut answer = json!({
        "thread": {"id": workset_id, "title": title},
        "mukade": {"service": service, "room_name": room, "delivery": if hooks { "hooks" } else { "manual" }},
        "you": {"session_id": caller.session, "harness": caller.harness, "member_name": known},
        "peers": peers,
        "how_to": how_to,
        "note": note,
    });
    if let (Some(team), Some(object)) = (team, answer.as_object_mut()) {
        object.insert("team".into(), team.to_json());
    }
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shinatama_state::records::ThreadKind;
    use shinatama_state::threads::ThreadStatus;

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
        let value = projection("w_dddddddddddd", Some("Billing"), &sessions, &caller, "running", &BTreeMap::new(), false).unwrap();
        assert_eq!(value["mukade"]["room_name"], "shinatama-thread-dddddddddddd");
        assert_eq!(value["you"]["member_name"], "claude-aaaaaaaaaaaa");
        let peers = value["peers"].as_array().unwrap();
        assert_eq!(peers.len(), 2);
        assert_eq!(peers[0]["member_name"], "codex-bbbbbbbbbbbb");
        assert_eq!(peers[0]["state"], "running");
        assert_eq!(peers[0]["chat"], "mukade");
        // Pi takes part through Mukade's kit, under a name not tied to the session here.
        assert_eq!(peers[1]["member_name"], Value::Null);
        assert_eq!(peers[1]["chat"], "mukade");
        assert!(value["how_to"][1].as_str().unwrap().contains("claude-aaaaaaaaaaaa"));
        assert!(!value["note"].as_str().unwrap().contains("not running now"));
    }

    #[test]
    fn with_hooks_the_adapter_names_are_used_and_no_second_join_is_asked() {
        let caller = Caller { instance: "i_aaaaaaaaaaaa", session: "t_aaaaaaaaaaaa", harness: "claude_code" };
        let sessions = [
            session("t_aaaaaaaaaaaa", Some("claude_code"), ThreadStatus::RecordedOpen),
            session("t_bbbbbbbbbbbb", Some("codex"), ThreadStatus::RecordedOpen),
        ];
        let names = BTreeMap::from([
            ("t_aaaaaaaaaaaa".to_owned(), "claude-1".to_owned()),
            ("t_bbbbbbbbbbbb".to_owned(), "codex-1".to_owned()),
        ]);
        let value = projection("w_dddddddddddd", None, &sessions, &caller, "running", &names, true).unwrap();
        assert_eq!(value["you"]["member_name"], "claude-1");
        assert_eq!(value["peers"][0]["member_name"], "codex-1");
        assert_eq!(value["mukade"]["delivery"], "hooks");
        let steps = value["how_to"].to_string();
        assert!(steps.contains("Do not call chat_join again") && steps.contains("Do not call chat_read"));
    }
    #[test]
    fn a_stopped_service_is_said_and_invalid_ids_are_refused() {
        let caller = Caller { instance: "i_aaaaaaaaaaaa", session: "t_aaaaaaaaaaaa", harness: "codex" };
        let value = projection("w_dddddddddddd", None, &[], &caller, "not_started", &BTreeMap::new(), false).unwrap();
        assert!(value["note"].as_str().unwrap().contains("not running now"));
        assert!(projection("bad", None, &[], &caller, "running", &BTreeMap::new(), false).is_err());
        let stranger = Caller { session: "x", ..caller };
        assert!(projection("w_dddddddddddd", None, &[], &stranger, "running", &BTreeMap::new(), false).is_err());
    }

    fn formation() -> shinatama_engine::team::Formation {
        use shinatama_engine::team::{Agent, Formation, Kind, Link, Report, Share};
        let agent = |id: &str, name: &str, session: Option<&str>| Agent {
            id: id.into(), session: session.map(Into::into), model: None, name: name.into(),
            notes: String::new(), share: Share::Split, x: 0, y: 0,
        };
        Formation {
            revision: 3,
            agents: vec![
                agent("a_aaaaaaaaaaaa", "Planner", Some("t_aaaaaaaaaaaa")),
                agent("a_bbbbbbbbbbbb", "Builder", Some("t_bbbbbbbbbbbb")),
                agent("a_cccccccccccc", "Reviewer", None),
            ],
            links: vec![Link {
                from: "a_aaaaaaaaaaaa".into(), to: "a_bbbbbbbbbbbb".into(), kind: Kind::Work,
                report: Report::Done, words: "Keep the diff small.".into(),
            }],
        }
    }

    #[test]
    fn peers_report_the_team() {
        let caller = Caller { instance: "i_aaaaaaaaaaaa", session: "t_aaaaaaaaaaaa", harness: "claude_code" };
        let sessions = [
            session("t_aaaaaaaaaaaa", Some("claude_code"), ThreadStatus::RecordedOpen),
            session("t_bbbbbbbbbbbb", Some("codex"), ThreadStatus::RecordedOpen),
        ];
        let team = TeamView::of(&formation(), "t_aaaaaaaaaaaa").unwrap();
        let value = projection_with_team("w_dddddddddddd", None, &sessions, &caller, "running", &BTreeMap::new(), false, Some(&team)).unwrap();
        assert_eq!(value["team"]["revision"], 3);
        assert_eq!(value["team"]["you"]["name"], "Planner");
        assert_eq!(value["team"]["you"]["role"], "lead");
        let relations = value["team"]["you"]["relations"].as_array().unwrap();
        assert_eq!(relations.len(), 1);
        assert!(relations[0].as_str().unwrap().contains("You give work to Builder"), "{relations:?}");
        assert!(relations[0].as_str().unwrap().contains("Keep the diff small."));
        // Peers by session id: an agent without a session is not a peer to talk to.
        assert_eq!(value["team"]["peers"], json!({"t_bbbbbbbbbbbb": {"name": "Builder", "role": "worker"}}));
        assert!(value["team"]["note"].as_str().unwrap().contains("Team map"));
        // A session that is not on the map has no place in it.
        assert!(TeamView::of(&formation(), "t_cccccccccccc").is_none());
    }

    #[test]
    fn peers_without_a_team_are_unchanged() {
        let caller = Caller { instance: "i_aaaaaaaaaaaa", session: "t_aaaaaaaaaaaa", harness: "claude_code" };
        let sessions = [session("t_aaaaaaaaaaaa", Some("claude_code"), ThreadStatus::RecordedOpen), session("t_bbbbbbbbbbbb", Some("codex"), ThreadStatus::RecordedOpen)];
        let names = BTreeMap::new();
        let plain = projection("w_dddddddddddd", Some("Billing"), &sessions, &caller, "running", &names, false).unwrap();
        let none = projection_with_team("w_dddddddddddd", Some("Billing"), &sessions, &caller, "running", &names, false, None).unwrap();
        assert_eq!(plain, none);
        assert!(plain.get("team").is_none());
    }
}
