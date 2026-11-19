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
    session_control::{member_name, thread_member_name, thread_room_name},
    team::{self, Formation, MukadeRole},
};
use shinatama_state::threads::{ThreadStatus, ThreadSummary};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{State, WebviewWindow};

/// Harnesses whose sessions Shinatama launches into a thread's room: Claude Code and Codex through
/// Mukade's hooks, OpenCode and Pi through Mukade's plugin kit.
pub(crate) const CHAT_HARNESSES: &[&str] = &["claude_code", "codex", "opencode", "pi"];
/// Those that also have Shinatama's own tools (`shinatama_thread_peers`) and a stable member name
/// to join with by hand, and whose native session Shinatama records, so a member is tied to them.
pub(crate) const MANUAL_HARNESSES: &[&str] = &["claude_code", "codex"];
const MAX_EVENTS: usize = 400;
const PAGE: &str = "500";
const MAX_PAGES: usize = 8;
const MAX_TEXT: usize = 16 * 1024;
const MAX_OVERVIEW: usize = 64;
/// Events the window draws, and the ones among them that are news for the user.
const DRAWN: &[&str] = &["message", "signal", "join", "rejoin", "leave", "close", "reopen"];
const NEWS: &[&str] = &["message", "signal"];

/// The handles of the host members (the person's `user` and Shinatama's `shinatama`) in each room,
/// by (room, member name), for this run only: a handle is a credential and stays in memory.
/// After a restart it is recovered with `rejoin`, which rotates it.
/// `unread` remembers, per room, the answer for the last (revision, seen event) pair.
#[derive(Default)]
pub struct Controller {
    hosts: Mutex<BTreeMap<(String, String), String>>,
    unread: Mutex<BTreeMap<String, (String, String, bool)>>,
}

/// A thread's room ID never changes for a service's data folder: it is opened once and kept.
type RoomKey = (PathBuf, String);
static ROOMS: Mutex<BTreeMap<RoomKey, String>> = Mutex::new(BTreeMap::new());
fn room_key(state: &WorkspaceServices, workset: &str) -> RoomKey {
    (mukade_chat::data_directory(&state.location), workset.to_owned())
}
fn cached_room(state: &WorkspaceServices, workset: &str) -> Option<String> {
    ROOMS.lock().ok()?.get(&room_key(state, workset)).cloned()
}
/// After a failed call the next one starts the service again if needed and reopens the room.
fn forget_room(state: &WorkspaceServices, workset: &str) {
    if let Ok(mut rooms) = ROOMS.lock() {
        rooms.remove(&room_key(state, workset));
    }
}

/// The hooks for a launch into `workset`, when that thread is on and the harness has
/// them. A failure leaves the session an ordinary one; the reason goes to the log. `session` is
/// the session being resumed: it joins in the Mukade role its agent's arrows give it; a new
/// session, one not on the map and a thread without a formation join as a peer, as before.
pub(crate) fn launch_hooks(state: &WorkspaceServices, harness: &str, workset: &str, session: Option<&str>) -> Option<ChatHooks> {
    if !CHAT_HARNESSES.contains(&harness) || !state.store.thread_chat_enabled(workset).unwrap_or(false) {
        return None;
    }
    match room_of(state, workset) {
        Ok(room) => {
            let formation = crate::thread_team::load(&state.store, workset).ok().and_then(|record| record.formation);
            ChatHooks::new(&state.location, &room).map(|hooks| hooks.with_role(launch_role(formation.as_ref(), session).as_str()))
        }
        Err(reason) => {
            eprintln!("A session of a talking thread opens without its room: {reason}");
            None
        }
    }
}

/// The Mukade role of the agent of `formation` whose session is `session`; a peer when there is
/// no formation, no session yet, or the session is not on the map.
pub(crate) fn launch_role(formation: Option<&Formation>, session: Option<&str>) -> MukadeRole {
    let (Some(formation), Some(session)) = (formation, session) else { return MukadeRole::Peer };
    formation
        .agents
        .iter()
        .find(|agent| agent.session.as_deref() == Some(session))
        .map_or(MukadeRole::Peer, |agent| team::mukade_role(formation, &agent.id))
}

/// The thread's room ID, starting Shinatama's own service when it is stopped and opening the
/// room the first time. Kept for the run; `forget_room` after a failure.
pub(crate) fn room_of(state: &WorkspaceServices, workset: &str) -> Result<String, String> {
    if let Some(room) = cached_room(state, workset) {
        return Ok(room);
    }
    let name = thread_room_name(workset).ok_or("The thread is invalid.")?;
    Connection::ensure_running(&state.location)?
        .ok_or("This Shinatama has no Mukade program in its mukade folder.")?;
    let room = mukade_chat::open_room(&state.location, &name)?;
    if let Ok(mut rooms) = ROOMS.lock() {
        rooms.insert(room_key(state, workset), room.clone());
    }
    Ok(room)
}

/// The thread's room ID without changing anything: no service is started and no room is
/// created. `None` while the service is not running or the room was never opened.
pub(crate) fn find_room(state: &WorkspaceServices, workset: &str) -> Option<String> {
    if let Some(room) = cached_room(state, workset) {
        return Some(room);
    }
    let name = thread_room_name(workset)?;
    Connection::discover(&state.location).ok()??;
    let (rooms, _) = listing(state, "rooms", &[]).ok()?;
    let room = rooms
        .iter()
        .find(|room| room["name"] == name.as_str())?
        .get("room_id")?
        .as_str()
        .filter(|id| mukade_chat::valid_room_id(id))?
        .to_owned();
    if let Ok(mut cache) = ROOMS.lock() {
        cache.insert(room_key(state, workset), room.clone());
    }
    Some(room)
}

/// The room of a session that was launched in this run with Mukade's hooks into `room`, and
/// whose adapter then joined it: only then is it a member through its hooks.
pub(crate) fn hooked_member(state: &WorkspaceServices, session: &str, harness: &str, native: &str, room: &str) -> Option<AdapterMember> {
    if mukade_chat::launched_room(session).as_deref() != Some(room) {
        return None;
    }
    mukade_chat::adapter_member(&state.location, harness, native).filter(|member| member.room_id == room)
}

pub(crate) fn session_state(status: ThreadStatus) -> &'static str {
    match status {
        ThreadStatus::RecordedOpen => "running",
        ThreadStatus::Inactive => "stopped",
        ThreadStatus::Unbound => "not_started",
        // The interface calls an archived session a closed one (ADR-0055).
        ThreadStatus::Archived => "closed",
    }
}

pub(crate) async fn blocking<T: Send + 'static>(
    services: &Arc<storage::Services>,
    work: impl FnOnce(&WorkspaceServices) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let services = Arc::clone(services);
    tauri::async_runtime::spawn_blocking(move || work(services.load()?))
        .await
        .map_err(|_| "The chat worker is unavailable.".to_owned())?
}

pub(crate) fn workset(value: &str) -> Result<(), String> {
    thread_room_name(value).map(|_| ()).ok_or_else(|| "The thread is invalid.".to_owned())
}

/// The pages of a listing (`rooms`, `members`, `ledger`), within a bound; `true` when the
/// listing ended before the bound.
pub(crate) fn listing(state: &WorkspaceServices, operation: &str, arguments: &[(&str, &str)]) -> Result<(Vec<Value>, bool), String> {
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
            None => return Ok((items, true)),
        }
    }
    Ok((items, false))
}

/// The room's most recent events. The ledger lists oldest first, so the first read asks for
/// a widening window ending now (10 minutes, an hour, a day, a week, a month, everything) and
/// keeps the widest one it could read whole, or the first that already holds enough.
fn recent_events(state: &WorkspaceServices, room: &str) -> Result<Vec<Value>, String> {
    use chrono::{Duration, SecondsFormat, Utc};
    let now = Utc::now();
    let mut kept: Option<Vec<Value>> = None;
    for span in [Some(Duration::minutes(10)), Some(Duration::hours(1)), Some(Duration::days(1)),
        Some(Duration::days(7)), Some(Duration::days(30)), None]
    {
        let since = span.map(|span| (now - span).to_rfc3339_opts(SecondsFormat::Millis, true));
        let mut arguments = vec![("room-id", room)];
        if let Some(since) = &since {
            arguments.push(("since", since));
        }
        let (events, whole) = listing(state, "ledger", &arguments)?;
        if !whole {
            // Too many to read whole: a narrower window already read is newer and complete.
            return Ok(kept.unwrap_or(events));
        }
        let enough = events.len() >= MAX_EVENTS;
        kept = Some(events);
        if enough {
            break;
        }
    }
    Ok(kept.unwrap_or_default())
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
        let warning = if request.enabled { room_of(state, &request.workset_id).inspect_err(|_| forget_room(state, &request.workset_id)).err() } else { None };
        Ok(ChatSwitch { enabled, warning })
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SeenEvent {
    event_id: String,
    timestamp: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatOverviewRequest {
    workset_ids: Vec<String>,
    /// The last event the user saw in each thread's room, as the window keeps it.
    #[serde(default)]
    seen: BTreeMap<String, SeenEvent>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomMark {
    /// The room's latest event and its time: where a thread seen for the first time starts.
    latest: String,
    last_activity: Option<String>,
    /// A message or signal after the event the user saw.
    unread: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatOverview {
    /// `running`, `not_started` or `unavailable`: a read never starts the service.
    service: &'static str,
    /// Per talking thread whose room exists.
    rooms: BTreeMap<String, RoomMark>,
    /// Per thread, its sessions that are members of the room through hooks launched in this
    /// run. Read from Shinatama's launch record and the adapter's files: no service call.
    connected: BTreeMap<String, Vec<String>>,
    /// Per thread, its OpenCode and Pi sessions launched in this run with the room's kit.
    launched: BTreeMap<String, Vec<String>>,
}
fn later(a: &str, b: &str) -> bool {
    a.len() > b.len() || (a.len() == b.len() && a > b)
}
/// One `rooms` call for every talking thread the window shows; a room whose latest event is
/// past what the user saw is read from that point to tell news (a message or a signal) from
/// a join or a binding. Each answer is kept until the room or the seen event changes.
#[tauri::command]
pub async fn thread_chat_overview(
    window: WebviewWindow,
    services: State<'_, Arc<storage::Services>>,
    controller: State<'_, Arc<Controller>>,
    request: ChatOverviewRequest,
) -> Result<ChatOverview, String> {
    crate::local(&window)?;
    if request.workset_ids.len() > MAX_OVERVIEW || request.seen.len() > MAX_OVERVIEW {
        return Err("Too many threads in one request.".into());
    }
    for id in request.workset_ids.iter().chain(request.seen.keys()) {
        workset(id)?;
    }
    for seen in request.seen.values() {
        if !seen.event_id.bytes().all(|c| c.is_ascii_digit()) || seen.event_id.len() > 20 || seen.timestamp.len() > 64 || seen.timestamp.chars().any(char::is_control) {
            return Err("A read position is invalid.".into());
        }
    }
    let controller = Arc::clone(controller.inner());
    blocking(services.inner(), move |state| {
        let service = match Connection::discover(&state.location) {
            Ok(Some(_)) => "running",
            Ok(None) => "not_started",
            Err(_) => "unavailable",
        };
        let mut rooms = BTreeMap::new();
        let mut connected = BTreeMap::new();
        let mut launched = BTreeMap::new();
        if service != "running" || request.workset_ids.is_empty() {
            return Ok(ChatOverview { service, rooms, connected, launched });
        }
        let names: BTreeMap<String, &String> = request
            .workset_ids
            .iter()
            .filter_map(|id| thread_room_name(id).map(|name| (name, id)))
            .collect();
        for room in listing(state, "rooms", &[])?.0 {
            let (Some(name), Some(room_id), Some(revision)) = (room["name"].as_str(), room["room_id"].as_str(), room["revision"].as_str()) else { continue };
            let Some(id) = names.get(name) else { continue };
            let unread = match request.seen.get(*id) {
                Some(seen) if later(revision, &seen.event_id) => {
                    let known = controller.unread.lock().ok().and_then(|cache| cache.get(room_id).cloned());
                    match known {
                        Some((at, from, answer)) if at == revision && from == seen.event_id => answer,
                        _ => {
                            let (events, _) = listing(state, "ledger", &[("room-id", room_id), ("since", &seen.timestamp)])?;
                            let answer = events.iter().any(|event| {
                                event["kind"].as_str().is_some_and(|kind| NEWS.contains(&kind))
                                    && event["event_id"].as_str().is_some_and(|event| later(event, &seen.event_id))
                            });
                            if let Ok(mut cache) = controller.unread.lock() {
                                cache.insert(room_id.to_owned(), (revision.to_owned(), seen.event_id.clone(), answer));
                            }
                            answer
                        }
                    }
                }
                _ => false,
            };
            rooms.insert((*id).clone(), RoomMark {
                latest: revision.to_owned(),
                last_activity: room["last_activity"].as_str().map(str::to_owned),
                unread,
            });
            let members: Vec<String> = state
                .store
                .workset_native_sessions(id)
                .unwrap_or_default()
                .into_iter()
                .filter(|(session, harness, native)| hooked_member(state, session, harness, native, room_id).is_some())
                .map(|(session, _, _)| session)
                .collect();
            connected.insert((*id).clone(), members);
            // OpenCode and Pi: launched into this room in this run (their member is not tied).
            let started: Vec<String> = state
                .store
                .workset_threads(id)
                .unwrap_or_default()
                .into_iter()
                .filter(|session| {
                    session.harness.as_deref().is_some_and(|harness| !MANUAL_HARNESSES.contains(&harness))
                        && mukade_chat::launched_room(&session.id).as_deref() == Some(room_id)
                })
                .map(|session| session.id)
                .collect();
            launched.insert((*id).clone(), started);
        }
        Ok(ChatOverview { service, rooms, connected, launched })
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
        // Reads repeat safely: after a failure the room is opened again, once.
        let read = |room: &str| -> Result<Value, String> {
            let (members, _) = listing(state, "members", &[("room-id", room)])?;
            let events = match &request.since {
                Some(since) => listing(state, "ledger", &[("room-id", room), ("since", since)])?.0,
                None => recent_events(state, room)?,
            };
            let sessions = session_facts(state, &request.workset_id, Some(room))?;
            Ok(project(room, &members, &events, &sessions))
        };
        let room = room_of(state, &request.workset_id)?;
        read(&room).or_else(|_| {
            forget_room(state, &request.workset_id);
            read(&room_of(state, &request.workset_id)?)
        })
    })
    .await
}

/// The thread's sessions with what is known of them without a service call: their record, the
/// adapter's record of the member each holds in `room` and whether this run launched them into it.
/// Without a room no session is tied to a member.
pub(crate) fn session_facts(state: &WorkspaceServices, workset: &str, room: Option<&str>) -> Result<Vec<SessionFacts>, String> {
    let summaries = state.store.workset_threads(workset).map_err(|e| e.to_string())?;
    let natives = state.store.workset_native_sessions(workset).map_err(|e| e.to_string())?;
    Ok(summaries
        .into_iter()
        .map(|summary| {
            let adapter = room.and_then(|room| {
                natives
                    .iter()
                    .find(|(id, _, _)| id == &summary.id)
                    .and_then(|(session, harness, native)| hooked_member(state, session, harness, native, room))
            });
            let launched = room.is_some_and(|room| mukade_chat::launched_room(&summary.id).as_deref() == Some(room));
            SessionFacts { summary, adapter, launched }
        })
        .collect())
}

pub(crate) struct SessionFacts {
    pub summary: ThreadSummary,
    /// Mukade's adapter record of the session's native conversation, in this room, when the
    /// session was launched in this run with the hooks (`hooked_member`).
    pub adapter: Option<AdapterMember>,
    /// Launched in this run with this room's hooks or kit. For OpenCode and Pi, whose native
    /// session Shinatama does not record, this is all that is known: their member is in the
    /// room's list, not tied to the session.
    pub launched: bool,
}

/// The window's view of a room. A member belongs to a session when the session's
/// adapter record names it, or (a session that joined by hand) by the stable name
/// `shinatama_thread_peers` gives it. A session is in the room when it is a member through
/// its hooks, or, joined by hand, while it runs.
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
                    .filter(|harness| MANUAL_HARNESSES.contains(harness))
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
                "host": member["host"] == true,
                // The person at the window: Shinatama's own harness under `user` (or `user-<n>`, when an
                // agent held the name); an agent that joined as `user` is not.
                "user": member["harness"] == USER_HARNESS && mukade_chat::is_host_name(USER_MEMBER, name),
            }))
        })
        .collect();
    let session_rows: Vec<Value> = sessions
        .iter()
        .map(|session| {
            let harness = session.summary.harness.as_deref();
            let member = members.iter().find(|member| member["sessionId"].as_str() == Some(session.summary.id.as_str()));
            let present = member.is_some_and(|member| member["active"] == true)
                && (session.adapter.is_some() || session.summary.status == ThreadStatus::RecordedOpen);
            let connection = if !harness.is_some_and(|harness| CHAT_HARNESSES.contains(&harness)) {
                "unsupported"
            } else if present {
                "in_room"
            } else if session.launched && !harness.is_some_and(|harness| MANUAL_HARNESSES.contains(&harness)) {
                "launched"
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
            if !DRAWN.contains(&kind) {
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
                "actorHost": event["actor"]["host"] == true,
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
/// Mukade's refusal of a handle another run rotated or that no longer names a member.
fn stale_handle(reason: &str) -> bool {
    reason.starts_with("stale_member") || reason.starts_with("member_not_found")
}
/// The person's message, sent as the room's `user` member. A message addressed to a
/// session wakes it when it is listening; otherwise it reads it at its next turn.
/// It is sent at most once: only a refused handle is recovered and the send repeated, since
/// then nothing was recorded; any other failure is reported, a timeout as uncertain.
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
        let sent = as_host(state, &controller, &room, USER_MEMBER, send);
        let sent = sent.map_err(|reason| {
            forget_room(state, &request.workset_id);
            if reason.contains("did not finish in time") {
                "Mukade did not answer in time: the message may have been sent. Check the conversation before sending it again.".to_owned()
            } else {
                reason
            }
        })?;
        Ok(json!({"eventId": sent["event_id"]}))
    })
    .await
}

/// Act as the host member `name` (`user` or `shinatama`) of `room`: with the handle kept
/// for this run, or one made by `host_handle`. See `with_recovery` for what is repeated.
pub(crate) fn as_host<T>(
    state: &WorkspaceServices,
    controller: &Controller,
    room: &str,
    name: &str,
    act: impl Fn(&str) -> Result<T, String>,
) -> Result<T, String> {
    let cached = controller.hosts.lock().ok().and_then(|hosts| hosts.get(&(room.to_owned(), name.to_owned())).cloned());
    with_recovery(cached, act, || host_handle(state, controller, room, name))
}

/// `act` with `cached`; a handle Mukade refused (another run rotated it, or it no longer names a
/// member) is replaced once by `recover` and `act` repeated, since nothing was recorded. Any other
/// failure is returned as it is, a timeout too: whether it happened is for the caller to find out.
fn with_recovery<T>(
    cached: Option<String>,
    act: impl Fn(&str) -> Result<T, String>,
    recover: impl FnOnce() -> Result<String, String>,
) -> Result<T, String> {
    match cached {
        Some(handle) => match act(&handle) {
            Err(reason) if stale_handle(&reason) => act(&recover()?),
            other => other,
        },
        None => act(&recover()?),
    }
}

/// What `mukade chat join` is given for a host member: the flag that makes it the host's (the
/// program reads the host's credential itself) and the harness name Mukade asks every member for.
fn host_join<'a>(request_id: &'a str, room: &'a str, name: &'a str) -> [(&'a str, &'a str); 6] {
    [("request-id", request_id), ("room-id", room), ("harness", USER_HARNESS), ("role", "peer"), ("name", name), ("host", "")]
}

/// What `mukade chat rejoin` is given to recover a host member: since Mukade 0.1.2 only the bare
/// `--host` flag (the program reads the host's credential itself) can recover a host member.
fn host_rejoin<'a>(request_id: &'a str, room: &'a str, member_id: &'a str) -> [(&'a str, &'a str); 4] {
    [("request-id", request_id), ("room-id", room), ("member-id", member_id), ("host", "")]
}

/// Join as the host member `preferred`, or recover that member after a restart (`rejoin` keeps
/// its mark). Never a member that is not a host (under Shinatama's harness): when an agent that
/// joined by hand holds the name, the host joins as `<name>-2`, `-3` … (`mukade_chat::host_name`),
/// at most `MAX_HOST_NAMES` names. `join(candidate)`, `members()` and `rejoin(member id)` are the
/// room's three calls.
fn host_member(
    preferred: &str,
    join: impl Fn(&str) -> Result<Value, String>,
    members: impl Fn() -> Result<Vec<Value>, String>,
    rejoin: impl Fn(&str) -> Result<Value, String>,
) -> Result<Value, String> {
    for attempt in 1..=mukade_chat::MAX_HOST_NAMES {
        let candidate = mukade_chat::host_name(preferred, attempt);
        match join(&candidate) {
            Ok(answer) => return Ok(answer),
            Err(reason) if reason.contains("member_name_taken") => {
                let held = members()?;
                let ours = held.iter().find(|member| {
                    member["name"] == candidate.as_str() && member["harness"] == USER_HARNESS && member["host"] == true
                });
                if let Some(member_id) = ours.and_then(|member| member["member_id"].as_str()) {
                    return rejoin(member_id);
                }
            }
            Err(reason) => return Err(reason),
        }
    }
    Err(format!("The room's {preferred} member, and the names it could take instead, belong to someone else."))
}

/// The host member `name`'s handle: see `host_member` for the name it may end up with.
fn host_handle(state: &WorkspaceServices, controller: &Controller, room: &str, name: &str) -> Result<String, String> {
    let answer = host_member(
        name,
        |candidate| {
            let request_id = uuid::Uuid::new_v4().to_string();
            mukade_chat::call(&state.location, "join", &host_join(&request_id, room, candidate), None)
        },
        || listing(state, "members", &[("room-id", room)]).map(|(members, _)| members),
        |member_id| {
            let request_id = uuid::Uuid::new_v4().to_string();
            mukade_chat::call(&state.location, "rejoin", &host_rejoin(&request_id, room, member_id), None)
        },
    )?;
    let handle = answer["member_handle"]
        .as_str()
        .filter(|handle| uuid::Uuid::parse_str(handle).is_ok())
        .ok_or("Mukade did not return the member.")?
        .to_owned();
    if let Ok(mut hosts) = controller.hosts.lock() {
        hosts.insert((room.to_owned(), name.to_owned()), handle.clone());
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
                launched: true,
            },
            // Joined by hand, under its stable name, and running.
            SessionFacts { summary: summary("t_bbbbbbbbbbbb", Some("codex"), ThreadStatus::RecordedOpen), adapter: None, launched: false },
            SessionFacts { summary: summary("t_cccccccccccc", Some("shell"), ThreadStatus::RecordedOpen), adapter: None, launched: false },
            SessionFacts { summary: summary("t_dddddddddddd", Some("claude_code"), ThreadStatus::RecordedOpen), adapter: None, launched: false },
        ];
        let view = project("r", &members, &events, &sessions);
        assert_eq!(view["members"][0]["sessionId"], "t_aaaaaaaaaaaa");
        assert_eq!(view["members"][1]["sessionId"], "t_bbbbbbbbbbbb");
        assert_eq!(view["members"][2]["user"], true);
        assert_eq!(view["sessions"][0]["connection"], "in_room");
        assert_eq!(view["sessions"][0]["wakes"], 1);
        assert_eq!(view["sessions"][1]["memberName"], "codex-bbbbbbbbbbbb");
        assert_eq!(view["sessions"][1]["connection"], "in_room");
        assert_eq!(view["sessions"][2]["connection"], "unsupported");
        assert_eq!(view["sessions"][3]["connection"], "not_connected");
        // Only deliberate events are drawn; a bind is not one.
        assert_eq!(view["events"].as_array().unwrap().len(), 1);
        assert_eq!(view["events"][0]["recipients"], json!(["codex-bbbbbbbbbbbb"]));
        assert_eq!(view["events"][0]["text"], "Rivedi il diff");
    }

    #[test]
    fn a_host_that_took_another_name_is_marked_as_the_host_and_an_agent_called_user_is_no_user() {
        let members = [
            json!({"member_id":"m1","name":"shinatama","harness":"claude_code","role":"peer","left_at":null,"host":false}),
            json!({"member_id":"m2","name":"shinatama-2","harness":"shinatama","role":"peer","left_at":null,"host":true}),
            json!({"member_id":"m3","name":"user","harness":"codex","role":"peer","left_at":null,"host":false}),
            json!({"member_id":"m4","name":"user-2","harness":"shinatama","role":"peer","left_at":null,"host":true}),
        ];
        let events = [
            json!({"event_id":"1","timestamp":"2026-11-16T10:00:00.000Z","kind":"message","actor":{"name":"shinatama-2","host":true},"payload":{"text":"TEAM revision 1"},"recipient_ids":[]}),
            json!({"event_id":"2","timestamp":"2026-11-16T10:00:01.000Z","kind":"message","actor":{"name":"shinatama","host":false},"payload":{"text":"hi"},"recipient_ids":[]}),
        ];
        let view = project("r", &members, &events, &[]);
        let flags: Vec<(bool, bool)> = (0..4).map(|i| (view["members"][i]["host"] == true, view["members"][i]["user"] == true)).collect();
        assert_eq!(flags, [(false, false), (true, false), (false, false), (true, true)]);
        assert_eq!(view["events"][0]["actorHost"], true);
        assert_eq!(view["events"][1]["actorHost"], false);
    }

    #[test]
    fn an_opencode_member_is_tied_by_its_recorded_native_session() {
        // The launch chose the native id and recorded it, so Mukade's adapter record for it names the
        // member: the session is in the room, with that member, and the member shows its tab.
        let members = [
            json!({"member_id":"m8","name":"opencode-1","harness":"opencode","role":"peer","left_at":null}),
            json!({"member_id":"m9","name":"pi-1","harness":"pi","role":"peer","left_at":null}),
        ];
        let tied = |member: &str, name: &str| Some(AdapterMember { room_id: "r".into(), member_id: member.into(), name: name.into(), role: "peer".into(), wakes: 2 });
        let sessions = [
            SessionFacts { summary: summary("t_oooooooooooo", Some("opencode"), ThreadStatus::RecordedOpen), adapter: tied("m8", "opencode-1"), launched: true },
            SessionFacts { summary: summary("t_pppppppppppp", Some("pi"), ThreadStatus::RecordedOpen), adapter: tied("m9", "pi-1"), launched: true },
        ];
        let view = project("r", &members, &[], &sessions);
        for (row, session, member) in [(0, "t_oooooooooooo", "opencode-1"), (1, "t_pppppppppppp", "pi-1")] {
            assert_eq!(view["sessions"][row]["connection"], "in_room");
            assert_eq!(view["sessions"][row]["memberName"], member);
            assert_eq!(view["sessions"][row]["wakes"], 2);
            assert_eq!(view["members"][row]["sessionId"], session);
        }
    }

    #[test]
    fn an_opencode_or_pi_session_without_a_recorded_native_id_claims_no_member() {
        // A session opened before its id was recorded: its member in the room is not tied to it.
        let members = [json!({"member_id":"m9","name":"pi-1","harness":"pi","role":"peer","left_at":null})];
        let sessions = [
            SessionFacts { summary: summary("t_pppppppppppp", Some("pi"), ThreadStatus::RecordedOpen), adapter: None, launched: true },
            SessionFacts { summary: summary("t_oooooooooooo", Some("opencode"), ThreadStatus::RecordedOpen), adapter: None, launched: false },
        ];
        let view = project("r", &members, &[], &sessions);
        assert_eq!(view["sessions"][0]["connection"], "launched");
        assert_eq!(view["sessions"][0]["memberName"], Value::Null);
        assert_eq!(view["sessions"][1]["connection"], "not_connected");
        assert_eq!(view["members"][0]["sessionId"], Value::Null);
    }

    #[test]
    fn a_member_that_joined_by_hand_is_not_in_the_room_once_its_session_stops() {
        let members = [json!({"member_id":"m2","name":"codex-bbbbbbbbbbbb","harness":"codex","role":"peer","left_at":null})];
        let sessions = [SessionFacts { summary: summary("t_bbbbbbbbbbbb", Some("codex"), ThreadStatus::Inactive), adapter: None, launched: false }];
        assert_eq!(project("r", &members, &[], &sessions)["sessions"][0]["connection"], "not_connected");
    }

    #[test]
    fn a_session_launches_in_the_role_the_arrows_of_its_agent_give_it() {
        use shinatama_engine::team::{Agent, Formation, Kind, Link, MukadeRole, Report, Share};
        let agent = |id: &str, session: Option<&str>| Agent {
            id: id.into(), session: session.map(Into::into), model: None, name: id.into(),
            notes: String::new(), share: Share::Split, x: 0, y: 0,
        };
        let link = |from: &str, to: &str, kind: Kind, report: Report| Link { from: from.into(), to: to.into(), kind, report, words: String::new() };
        let formation = Formation {
            revision: 1,
            agents: vec![
                agent("a_aaaaaaaaaaaa", Some("t_aaaaaaaaaaaa")),
                agent("a_bbbbbbbbbbbb", Some("t_bbbbbbbbbbbb")),
                agent("a_cccccccccccc", Some("t_cccccccccccc")),
                agent("a_dddddddddddd", None),
            ],
            links: vec![
                link("a_aaaaaaaaaaaa", "a_bbbbbbbbbbbb", Kind::Work, Report::Strict),
                link("a_cccccccccccc", "a_bbbbbbbbbbbb", Kind::Observes, Report::Done),
            ],
        };
        let role = |session: Option<&str>| launch_role(Some(&formation), session);
        assert_eq!(role(Some("t_aaaaaaaaaaaa")), MukadeRole::Lead);
        assert_eq!(role(Some("t_bbbbbbbbbbbb")), MukadeRole::Worker);
        assert_eq!(role(Some("t_cccccccccccc")), MukadeRole::Observer);
        // A session that is not on the map, a new session and a thread without a formation: a peer.
        assert_eq!(role(Some("t_eeeeeeeeeeee")), MukadeRole::Peer);
        assert_eq!(role(None), MukadeRole::Peer);
        assert_eq!(launch_role(None, Some("t_aaaaaaaaaaaa")), MukadeRole::Peer);
    }

    #[test]
    fn a_host_member_joins_with_the_host_flag_under_the_name_it_is_given() {
        let joined = host_join("req", "room", "shinatama");
        for pair in [("host", ""), ("name", "shinatama"), ("harness", "shinatama"), ("role", "peer"), ("room-id", "room")] {
            assert!(joined.contains(&pair), "{pair:?}");
        }
        assert!(host_join("req", "room", "user").contains(&("name", "user")));
    }

    #[test]
    fn a_host_member_is_recovered_with_the_host_flag() {
        // Mukade's `rejoin` of a member the host created needs the host's credential, which the
        // program reads from its own folder when it is given the bare `--host` flag.
        let rejoined = host_rejoin("req", "room", "m0");
        for pair in [("host", ""), ("member-id", "m0"), ("room-id", "room"), ("request-id", "req")] {
            assert!(rejoined.contains(&pair), "{pair:?}");
        }
    }

    /// A scripted room for `host_member`: the names held by members (name, harness, host), by id.
    struct Held(Vec<(&'static str, &'static str, bool)>);
    impl Held {
        fn run(&self, name: &str) -> (Result<Value, String>, Vec<String>) {
            let log = std::cell::RefCell::new(Vec::new());
            let members = || -> Result<Vec<Value>, String> {
                Ok(self.0.iter().enumerate().map(|(i, (n, h, host))| json!({"member_id": format!("m{i}"), "name": n, "harness": h, "host": host})).collect())
            };
            let joined = host_member(
                name,
                |candidate| {
                    log.borrow_mut().push(format!("join {candidate}"));
                    if self.0.iter().any(|(held, _, _)| *held == candidate) {
                        Err("member_name_taken: The name is taken".to_owned())
                    } else {
                        Ok(json!({"member_handle": candidate}))
                    }
                },
                members,
                |member_id| {
                    log.borrow_mut().push(format!("rejoin {member_id}"));
                    Ok(json!({"member_handle": member_id}))
                },
            );
            (joined, log.into_inner())
        }
    }

    #[test]
    fn a_host_whose_name_a_hand_joined_agent_took_joins_under_another_and_never_adopts_that_agent() {
        // An agent joined by hand as `shinatama` (not a host, and under another harness).
        let (joined, log) = Held(vec![("shinatama", "claude_code", false)]).run("shinatama");
        assert_eq!(joined.unwrap()["member_handle"], "shinatama-2");
        assert_eq!(log, ["join shinatama", "join shinatama-2"], "no rejoin of the squatter");
        // Even under Shinatama's harness: only a member the room marks as a host is recovered.
        let (joined, log) = Held(vec![("shinatama", "shinatama", false), ("shinatama-2", "claude_code", false)]).run("shinatama");
        assert_eq!(joined.unwrap()["member_handle"], "shinatama-3");
        assert!(log.iter().all(|line| !line.starts_with("rejoin")), "{log:?}");
        // The host that holds the name after a restart is recovered, not duplicated.
        let (joined, log) = Held(vec![("shinatama", "shinatama", true)]).run("shinatama");
        assert_eq!(joined.unwrap()["member_handle"], "m0");
        assert_eq!(log, ["join shinatama", "rejoin m0"]);
        // ...and so is the host that already moved to the second name.
        let (joined, _) = Held(vec![("shinatama", "claude_code", false), ("shinatama-2", "shinatama", true)]).run("shinatama");
        assert_eq!(joined.unwrap()["member_handle"], "m1");
        // Every name taken by others is a refusal naming the member.
        let mut all = vec![("shinatama", "claude_code", false)];
        all.extend((2..=9).map(|n| (&*Box::leak(format!("shinatama-{n}").into_boxed_str()), "claude_code", false)));
        let (joined, log) = Held(all).run("shinatama");
        assert!(joined.unwrap_err().contains("belong to someone else"));
        assert_eq!(log.len(), 9, "bounded");
        // Another error is returned as it is.
        let failed = host_member("shinatama", |_| Err("Mukade chat join failed.".into()), || Ok(Vec::new()), |_| Ok(json!({})));
        assert_eq!(failed.unwrap_err(), "Mukade chat join failed.");
    }

    #[test]
    fn a_host_name_is_the_preferred_one_then_numbered_from_two() {
        assert_eq!(mukade_chat::host_name("shinatama", 1), "shinatama");
        assert_eq!(mukade_chat::host_name("shinatama", 3), "shinatama-3");
        assert!(mukade_chat::is_host_name("shinatama", "shinatama") && mukade_chat::is_host_name("shinatama", "shinatama-12"));
        assert!(!mukade_chat::is_host_name("shinatama", "shinatama-") && !mukade_chat::is_host_name("shinatama", "shinatama-x") && !mukade_chat::is_host_name("shinatama", "shinatamas"));
        assert!(!mukade_chat::is_host_name("shinatama", "user-2"));
    }

    #[test]
    fn a_refused_host_handle_is_recovered_once_and_nothing_else_is_repeated() {
        use std::cell::Cell;
        let stale = || Err::<String, String>("stale_member: The member handle is stale".to_owned());
        // A handle another run rotated: recovered, and the act repeated once with the new one.
        let recovered = Cell::new(0);
        let acted = with_recovery(Some("old".into()), |handle| if handle == "old" { stale() } else { Ok(handle.to_owned()) }, || {
            recovered.set(recovered.get() + 1);
            Ok("new".into())
        });
        assert_eq!((acted, recovered.get()), (Ok("new".to_owned()), 1));
        // A recovered handle that is refused again is reported, not recovered again.
        let recovered = Cell::new(0);
        let acted = with_recovery(Some("old".into()), |_| stale(), || {
            recovered.set(recovered.get() + 1);
            Ok("new".into())
        });
        assert!(acted.is_err());
        assert_eq!(recovered.get(), 1);
        // Any other failure, a timeout among them, is returned as it is: nothing is sent twice.
        let tries = Cell::new(0);
        let acted: Result<String, String> = with_recovery(Some("h".into()), |_| {
            tries.set(tries.get() + 1);
            Err("Mukade chat send did not finish in time.".into())
        }, || Ok("new".into()));
        assert_eq!((acted.is_err(), tries.get()), (true, 1));
        // No handle yet: one is made, and used once.
        let tries = Cell::new(0);
        let acted = with_recovery(None, |handle| {
            tries.set(tries.get() + 1);
            Ok(handle.to_owned())
        }, || Ok("made".into()));
        assert_eq!((acted, tries.get()), (Ok("made".to_owned()), 1));
    }

    #[test]
    fn only_a_refused_handle_is_recovered_and_event_ids_compare_as_numbers() {
        assert!(stale_handle("stale_member: The member handle is stale; recover the member with chat_rejoin"));
        assert!(stale_handle("member_not_found: No member has that ID in this room"));
        assert!(!stale_handle("Mukade chat send did not finish in time."));
        assert!(!stale_handle("room_closed: The room is closed"));
        assert!(later("10", "9") && !later("9", "10") && !later("7", "7") && later("8", "7"));
    }
}
