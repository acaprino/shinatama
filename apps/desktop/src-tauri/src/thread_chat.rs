//! ADR-0057: a thread whose sessions talk to each other through its Mukade room.
//! The switch is per thread (workset). Sessions of a thread that is on are launched
//! with Mukade's hooks (`mukade_chat::ChatHooks`); the window reads the room through
//! the bundled Mukade program without consuming any member's stream (`members`,
//! `ledger`, `rooms`), and writes to it only as the person's own member, `user`.
use crate::storage::{self, WorkspaceServices};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use shinatama_engine::{
    mukade_chat::{self, AdapterMember, ChatHooks, USER_HARNESS, USER_MEMBER},
    mukade_mcp::Connection,
    session_control::{thread_member_name, thread_room_name},
};
use shinatama_state::threads::{ThreadStatus, ThreadSummary};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tauri::{State, WebviewWindow};

/// Harnesses Shinatama gives Mukade's hooks and tools (`claude_launch`, `codex_launch`).
const CHAT_HARNESSES: &[&str] = &["claude_code", "codex"];
const MAX_EVENTS: usize = 400;
const PAGE: &str = "500";
const MAX_PAGES: usize = 8;
const MAX_TEXT: usize = 16 * 1024;
const MAX_OVERVIEW: usize = 64;

/// The person's member handle in each room, for this run only: it is a credential and
/// stays in memory. After a restart it is recovered with `rejoin`, which rotates it.
#[derive(Default)]
pub struct Controller {
    user: Mutex<BTreeMap<String, String>>,
}

/// The hooks for a launch into `workset`, when that thread is on and the harness has
/// them. A failure leaves the session an ordinary one; the reason goes to the log.
pub(crate) fn launch_hooks(state: &WorkspaceServices, harness: &str, workset: &str) -> Option<ChatHooks> {
    if !CHAT_HARNESSES.contains(&harness) || !state.store.thread_chat_enabled(workset).unwrap_or(false) {
        return None;
    }
    match room_of(state, workset) {
        Ok(room) => ChatHooks::new(&state.location, &room),
        Err(reason) => {
            eprintln!("A session of a talking thread opens without its room: {reason}");
            None
        }
    }
}

/// The thread's room ID, starting Shinatama's own service when it is stopped.
pub(crate) fn room_of(state: &WorkspaceServices, workset: &str) -> Result<String, String> {
    let name = thread_room_name(workset).ok_or("The thread is invalid.")?;
    Connection::ensure_running(&state.location)?
        .ok_or("This Shinatama has no Mukade program in its mukade folder.")?;
    mukade_chat::open_room(&state.location, &name)
}

async fn blocking<T: Send + 'static>(
    services: &Arc<storage::Services>,
    work: impl FnOnce(&WorkspaceServices) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let services = Arc::clone(services);
    tauri::async_runtime::spawn_blocking(move || work(services.load()?))
        .await
        .map_err(|_| "The chat worker is unavailable.".to_owned())?
}

fn workset(value: &str) -> Result<(), String> {
    thread_room_name(value).map(|_| ()).ok_or_else(|| "The thread is invalid.".to_owned())
}

/// Every page of a listing (`rooms`, `members`, `ledger`), within a bound.
fn listing(state: &WorkspaceServices, operation: &str, arguments: &[(&str, &str)]) -> Result<Vec<Value>, String> {
    let mut items = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let mut page: Vec<(&str, &str)> = arguments.to_vec();
        page.push(("limit", PAGE));
        if let Some(cursor) = &cursor {
            page.push(("cursor", cursor));
        }
        let answer = mukade_chat::call(&state.location, operation, &page, None)?;
        items.extend(answer["items"].as_array().cloned().unwrap_or_default());
        match answer["next_cursor"].as_str() {
            Some(next) => cursor = Some(next.to_owned()),
            None => return Ok(items),
        }
    }
    Ok(items)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSwitch {
    enabled: Vec<String>,
    /// Why the room could not be opened now; the switch is saved either way.
    warning: Option<String>,
}

#[tauri::command]
pub async fn thread_chat_list(window: WebviewWindow, services: State<'_, Arc<storage::Services>>) -> Result<Vec<String>, String> {
    crate::local(&window)?;
    blocking(services.inner(), |state| state.store.chat_threads().map_err(|e| e.to_string())).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatSet {
    workset_id: String,
    enabled: bool,
}
/// The switch. Turning it on opens the room at once, so a problem shows now and not at
/// the next launch; sessions already running are not changed (they need a resume).
#[tauri::command]
pub async fn thread_chat_set(
    window: WebviewWindow,
    services: State<'_, Arc<storage::Services>>,
    request: ChatSet,
) -> Result<ChatSwitch, String> {
    crate::local(&window)?;
    workset(&request.workset_id)?;
    blocking(services.inner(), move |state| {
        if state.store.workset_record(&request.workset_id).map_err(|e| e.to_string())?.is_none() {
            return Err("The thread no longer exists.".into());
        }
        let enabled = state.store.set_thread_chat(&request.workset_id, request.enabled).map_err(|e| e.to_string())?;
        let warning = if request.enabled { room_of(state, &request.workset_id).err() } else { None };
        Ok(ChatSwitch { enabled, warning })
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatOverviewRequest {
    workset_ids: Vec<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatOverview {
    /// `running`, `not_started` or `unavailable`: a read never starts the service.
    service: &'static str,
    /// The latest event of each thread's room, as a decimal string; absent while the
    /// room has never been opened.
    latest: BTreeMap<String, String>,
    /// Per thread, its sessions whose native conversation Mukade's hooks made a member
    /// of the thread's room. Read from the adapter's own records: no service call.
    connected: BTreeMap<String, Vec<String>>,
}
/// One `rooms` call for every talking thread the window shows: enough for its marks.
#[tauri::command]
pub async fn thread_chat_overview(
    window: WebviewWindow,
    services: State<'_, Arc<storage::Services>>,
    request: ChatOverviewRequest,
) -> Result<ChatOverview, String> {
    crate::local(&window)?;
    if request.workset_ids.len() > MAX_OVERVIEW {
        return Err("Too many threads in one request.".into());
    }
    for id in &request.workset_ids {
        workset(id)?;
    }
    blocking(services.inner(), move |state| {
        let service = match Connection::discover(&state.location) {
            Ok(Some(_)) => "running",
            Ok(None) => "not_started",
            Err(_) => "unavailable",
        };
        let mut latest = BTreeMap::new();
        let mut connected = BTreeMap::new();
        if service == "running" && !request.workset_ids.is_empty() {
            let names: BTreeMap<String, &String> = request
                .workset_ids
                .iter()
                .filter_map(|id| thread_room_name(id).map(|name| (name, id)))
                .collect();
            for room in listing(state, "rooms", &[])? {
                let (Some(name), Some(room_id)) = (room["name"].as_str(), room["room_id"].as_str()) else { continue };
                let Some(id) = names.get(name) else { continue };
                if let Some(revision) = room["revision"].as_str() {
                    latest.insert((*id).clone(), revision.to_owned());
                }
                let members: Vec<String> = state
                    .store
                    .workset_native_sessions(id)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|(_, harness, native)| {
                        mukade_chat::adapter_member(&state.location, harness, native).is_some_and(|member| member.room_id == room_id)
                    })
                    .map(|(session, _, _)| session)
                    .collect();
                connected.insert((*id).clone(), members);
            }
        }
        Ok(ChatOverview { service, latest, connected })
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatSnapshotRequest {
    workset_id: String,
    /// The timestamp of the last event the window holds: later reads ask from there.
    since: Option<String>,
}
/// The room as the window draws it: members tied to the thread's sessions, and the
/// deliberate events (messages, signals, joins). Reads only; consumes nothing.
#[tauri::command]
pub async fn thread_chat_snapshot(
    window: WebviewWindow,
    services: State<'_, Arc<storage::Services>>,
    request: ChatSnapshotRequest,
) -> Result<Value, String> {
    crate::local(&window)?;
    workset(&request.workset_id)?;
    if request.since.as_ref().is_some_and(|since| since.len() > 64 || since.chars().any(char::is_control)) {
        return Err("The read position is invalid.".into());
    }
    blocking(services.inner(), move |state| {
        if !state.store.thread_chat_enabled(&request.workset_id).map_err(|e| e.to_string())? {
            return Ok(json!({"enabled": false}));
        }
        let room = room_of(state, &request.workset_id)?;
        let members = listing(state, "members", &[("room-id", &room)])?;
        let mut ledger_arguments = vec![("room-id", room.as_str())];
        if let Some(since) = &request.since {
            ledger_arguments.push(("since", since));
        }
        let events = listing(state, "ledger", &ledger_arguments)?;
        let summaries = state.store.workset_threads(&request.workset_id).map_err(|e| e.to_string())?;
        let natives = state.store.workset_native_sessions(&request.workset_id).map_err(|e| e.to_string())?;
        let sessions: Vec<SessionFacts> = summaries
            .into_iter()
            .map(|summary| {
                let adapter = natives
                    .iter()
                    .find(|(id, _, _)| id == &summary.id)
                    .and_then(|(_, harness, native)| mukade_chat::adapter_member(&state.location, harness, native))
                    .filter(|member| member.room_id == room);
                SessionFacts { summary, adapter }
            })
            .collect();
        Ok(project(&room, &members, &events, &sessions))
    })
    .await
}

pub(crate) struct SessionFacts {
    pub summary: ThreadSummary,
    /// Mukade's adapter record of the session's native conversation, in this room.
    pub adapter: Option<AdapterMember>,
}

fn session_state(status: ThreadStatus) -> &'static str {
    match status {
        ThreadStatus::RecordedOpen => "running",
        ThreadStatus::Inactive => "stopped",
        ThreadStatus::Unbound => "not_started",
        ThreadStatus::Archived => "closed",
    }
}

/// The window's view of a room. A member belongs to a session when the session's
/// adapter record names it, or (a session that joined by hand) by the stable name
/// `shinatama_thread_peers` gives it.
pub(crate) fn project(room: &str, members: &[Value], events: &[Value], sessions: &[SessionFacts]) -> Value {
    let session_of = |member: &Value| -> Option<&SessionFacts> {
        let id = member["member_id"].as_str()?;
        let name = member["name"].as_str()?;
        sessions.iter().find(|session| {
            session.adapter.as_ref().is_some_and(|adapter| adapter.member_id == id)
                || session
                    .summary
                    .harness
                    .as_deref()
                    .and_then(|harness| thread_member_name(harness, &session.summary.id))
                    .is_some_and(|stable| stable == name)
        })
    };
    let mut names = BTreeMap::new();
    let members: Vec<Value> = members
        .iter()
        .filter_map(|member| {
            let id = member["member_id"].as_str()?;
            let name = member["name"].as_str()?;
            names.insert(id.to_owned(), name.to_owned());
            Some(json!({
                "memberId": id,
                "name": name,
                "harness": member["harness"],
                "role": member["role"],
                "active": member["left_at"].is_null(),
                "sessionId": session_of(member).map(|session| session.summary.id.clone()),
                "user": name == USER_MEMBER,
            }))
        })
        .collect();
    let session_rows: Vec<Value> = sessions
        .iter()
        .map(|session| {
            let harness = session.summary.harness.as_deref();
            let member = members.iter().find(|member| member["sessionId"].as_str() == Some(session.summary.id.as_str()));
            let connection = if !harness.is_some_and(|harness| CHAT_HARNESSES.contains(&harness)) {
                "unsupported"
            } else if member.is_some_and(|member| member["active"] == true) {
                "in_room"
            } else {
                "not_connected"
            };
            json!({
                "sessionId": session.summary.id,
                "harness": harness,
                "title": session.summary.title,
                "state": session_state(session.summary.status),
                "memberName": member.map(|member| member["name"].clone()),
                "connection": connection,
                "wakes": session.adapter.as_ref().map(|adapter| adapter.wakes),
            })
        })
        .collect();
    let mut events: Vec<Value> = events
        .iter()
        .filter_map(|event| {
            let kind = event["kind"].as_str()?;
            if !matches!(kind, "message" | "signal" | "join" | "rejoin" | "leave" | "close" | "reopen") {
                return None;
            }
            let payload = &event["payload"];
            let recipients: Vec<String> = event["recipient_ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|id| id.as_str().and_then(|id| names.get(id)).cloned())
                .collect();
            Some(json!({
                "eventId": event["event_id"].as_str()?,
                "timestamp": event["timestamp"],
                "kind": kind,
                "actor": event["actor"]["name"],
                "recipients": recipients,
                "text": payload["text"],
                "signal": payload["signal"],
                "reason": payload["reason"],
            }))
        })
        .collect();
    if events.len() > MAX_EVENTS {
        events.drain(..events.len() - MAX_EVENTS);
    }
    json!({"enabled": true, "roomId": room, "members": members, "sessions": session_rows, "events": events})
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatSendRequest {
    workset_id: String,
    /// Member names; none writes to the whole room, which wakes nobody.
    recipients: Vec<String>,
    text: String,
}
fn member_name(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value != "all"
        && value.bytes().next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && value.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'_' | b'-'))
}
/// The person's message, sent as the room's `user` member. A message addressed to a
/// session wakes it when it is listening; otherwise it reads it at its next turn.
#[tauri::command]
pub async fn thread_chat_send(
    window: WebviewWindow,
    services: State<'_, Arc<storage::Services>>,
    controller: State<'_, Arc<Controller>>,
    request: ChatSendRequest,
) -> Result<Value, String> {
    crate::local(&window)?;
    workset(&request.workset_id)?;
    if request.text.trim().is_empty() || request.text.len() > MAX_TEXT || request.text.contains('\0') {
        return Err("Write a message of at most 16 KB.".into());
    }
    if request.recipients.len() > 16 || !request.recipients.iter().all(|name| member_name(name) && name != USER_MEMBER) {
        return Err("A recipient is invalid.".into());
    }
    let controller = Arc::clone(controller.inner());
    blocking(services.inner(), move |state| {
        if !state.store.thread_chat_enabled(&request.workset_id).map_err(|e| e.to_string())? {
            return Err("This thread's sessions do not talk to each other. Turn it on first.".into());
        }
        let room = room_of(state, &request.workset_id)?;
        let recipients = request.recipients.join(",");
        let send = |handle: &str| {
            let request_id = uuid::Uuid::new_v4().to_string();
            let mut arguments = vec![("request-id", request_id.as_str()), ("room-id", room.as_str()), ("member-handle", handle)];
            if !recipients.is_empty() {
                arguments.push(("recipients", recipients.as_str()));
            }
            mukade_chat::call(&state.location, "send", &arguments, Some(&request.text))
        };
        let cached = controller.user.lock().ok().and_then(|users| users.get(&room).cloned());
        let sent = match cached {
            // A handle another run rotated is refused: recover once.
            Some(handle) => send(&handle).or_else(|_| send(&user_handle(state, &controller, &room)?)),
            None => send(&user_handle(state, &controller, &room)?),
        }?;
        Ok(json!({"eventId": sent["event_id"]}))
    })
    .await
}

/// Join as `user`, or recover that member after a restart. Never another member.
fn user_handle(state: &WorkspaceServices, controller: &Controller, room: &str) -> Result<String, String> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let joined = mukade_chat::call(
        &state.location,
        "join",
        &[("request-id", &request_id), ("room-id", room), ("harness", USER_HARNESS), ("role", "peer"), ("name", USER_MEMBER)],
        None,
    );
    let answer = match joined {
        Ok(answer) => answer,
        Err(reason) if reason.contains("member_name_taken") => {
            let members = listing(state, "members", &[("room-id", room)])?;
            let member_id = members
                .iter()
                .find(|member| member["name"] == USER_MEMBER && member["harness"] == USER_HARNESS)
                .and_then(|member| member["member_id"].as_str())
                .ok_or("The room's user member belongs to someone else.")?
                .to_owned();
            let request_id = uuid::Uuid::new_v4().to_string();
            mukade_chat::call(&state.location, "rejoin", &[("request-id", &request_id), ("room-id", room), ("member-id", &member_id)], None)?
        }
        Err(reason) => return Err(reason),
    };
    let handle = answer["member_handle"]
        .as_str()
        .filter(|handle| uuid::Uuid::parse_str(handle).is_ok())
        .ok_or("Mukade did not return the member.")?
        .to_owned();
    if let Ok(mut users) = controller.user.lock() {
        users.insert(room.to_owned(), handle.clone());
    }
    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shinatama_state::records::ThreadKind;

    fn summary(id: &str, harness: Option<&str>, status: ThreadStatus) -> ThreadSummary {
        ThreadSummary {
            id: id.into(), project_id: "p_aaaaaaaaaaaa".into(), kind: ThreadKind::Work, title: None,
            revision: 1, updated_at: 1, harness: harness.map(Into::into), native_available: Some(true), status,
        }
    }

    #[test]
    fn members_are_tied_to_sessions_and_events_name_their_recipients() {
        let members = [
            json!({"member_id":"m1","name":"claude-1","harness":"claude_code","role":"peer","left_at":null}),
            json!({"member_id":"m2","name":"codex-bbbbbbbbbbbb","harness":"codex","role":"peer","left_at":null}),
            json!({"member_id":"m3","name":"user","harness":"shinatama","role":"peer","left_at":null}),
        ];
        let events = [
            json!({"event_id":"7","timestamp":"2026-11-16T10:00:00.000Z","kind":"message","actor":{"name":"user"},"payload":{"text":"Rivedi il diff"},"recipient_ids":["m2"]}),
            json!({"event_id":"8","timestamp":"2026-11-16T10:00:01.000Z","kind":"bind","actor":{"name":"claude-1"},"payload":{},"recipient_ids":[]}),
        ];
        let sessions = [
            SessionFacts {
                summary: summary("t_aaaaaaaaaaaa", Some("claude_code"), ThreadStatus::RecordedOpen),
                adapter: Some(AdapterMember { room_id: "r".into(), member_id: "m1".into(), name: "claude-1".into(), role: "peer".into(), wakes: 1 }),
            },
            SessionFacts { summary: summary("t_bbbbbbbbbbbb", Some("codex"), ThreadStatus::Inactive), adapter: None },
            SessionFacts { summary: summary("t_cccccccccccc", Some("pi"), ThreadStatus::RecordedOpen), adapter: None },
            SessionFacts { summary: summary("t_dddddddddddd", Some("claude_code"), ThreadStatus::RecordedOpen), adapter: None },
        ];
        let view = project("r", &members, &events, &sessions);
        assert_eq!(view["members"][0]["sessionId"], "t_aaaaaaaaaaaa");
        assert_eq!(view["members"][1]["sessionId"], "t_bbbbbbbbbbbb");
        assert_eq!(view["members"][2]["user"], true);
        assert_eq!(view["sessions"][0]["connection"], "in_room");
        assert_eq!(view["sessions"][0]["wakes"], 1);
        assert_eq!(view["sessions"][1]["memberName"], "codex-bbbbbbbbbbbb");
        assert_eq!(view["sessions"][2]["connection"], "unsupported");
        assert_eq!(view["sessions"][3]["connection"], "not_connected");
        // Only deliberate events are drawn; a bind is not one.
        assert_eq!(view["events"].as_array().unwrap().len(), 1);
        assert_eq!(view["events"][0]["recipients"], json!(["codex-bbbbbbbbbbbb"]));
        assert_eq!(view["events"][0]["text"], "Rivedi il diff");
    }

    #[test]
    fn recipients_are_member_names_and_never_the_person_s_own() {
        assert!(member_name("codex-1") && !member_name("all") && !member_name("Codex") && !member_name(""));
    }
}
