//! The worker that tells a thread's sessions their place in its team (spec section 5, ADR-0058).
//!
//! It runs `team::deliveries` for each talking thread that has a formation and executes what is
//! owed through the bundled Mukade, as the host member `shinatama`, and keeps what each agent was
//! told. It never reads from a room as a member (no `read`, `wait`, `ack`: ADR-0057 INV-001) and
//! starts no service: a thread whose room does not exist or whose service is stopped is owed
//! nothing now.
//!
//! It checks every 10 seconds, or at once when `team_apply` calls `wake`. A check reads only what
//! Shinatama holds (the record, the sessions, the adapter's files) and the one `rooms` listing the
//! Conversation panel's overview also makes; the room's members are read, and anything is sent,
//! only when what the check read differs from what the last check that found nothing owed read.
//! A send is made at most once: a refusal is kept as the agent's reason and not repeated until
//! something changes; a timeout is uncertain, so the ledger is read for the message before it is
//! tried again, and the retry carries the request id of the first try, which Mukade answers
//! without doing it twice.
use crate::storage::WorkspaceServices;
use crate::thread_chat::{self, Controller, SessionFacts};
use crate::thread_team::{self, Record};
use serde_json::Value;
use shinatama_engine::{
    mukade_chat::{self, TEAM_MEMBER},
    mukade_mcp::Connection,
    session_control::thread_room_name,
    team::{self, Delivery, Formation, NOTICE_KEY, Seat, Told},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Condvar, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const CHECK_EVERY: Duration = Duration::from_secs(10);
/// A reason that is about the service and not about one agent.
pub(crate) const SERVICE_KEY: &str = "service";

static WAKE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());
static STOP: AtomicBool = AtomicBool::new(false);
/// Why a thread's messages were last refused, by agent id (`SERVICE_KEY`, `NOTICE_KEY` or a member
/// id for what is not about one agent): what `team_get` shows on the cards.
static REFUSED: Mutex<BTreeMap<String, BTreeMap<String, String>>> = Mutex::new(BTreeMap::new());
/// Sends whose end is not known (a timeout): the request id and the time of the first try, by
/// (room, what was sent), so the retry is the same request and the ledger is read from that time.
static PENDING: Mutex<BTreeMap<(String, String), (String, String)>> = Mutex::new(BTreeMap::new());

/// Ask the worker to check the talking threads now.
pub(crate) fn wake() {
    *WAKE.0.lock().unwrap_or_else(PoisonError::into_inner) = true;
    WAKE.1.notify_all();
}

/// End the worker (at exit).
pub(crate) fn stop() {
    STOP.store(true, Ordering::Release);
    wake();
}

/// The reasons the last check found for `workset`: an agent's id, or `SERVICE_KEY` / `NOTICE_KEY`.
pub(crate) fn refusals(workset: &str) -> BTreeMap<String, String> {
    REFUSED.lock().ok().and_then(|all| all.get(workset).cloned()).unwrap_or_default()
}

fn set_refusals(workset: &str, reasons: BTreeMap<String, String>) {
    if let Ok(mut all) = REFUSED.lock() {
        if reasons.is_empty() {
            all.remove(workset);
        } else {
            all.insert(workset.to_owned(), reasons);
        }
    }
}

// ---- What a service call's end means ----

/// How one call ended, as far as the worker can tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    Done,
    /// Mukade refused it (a message that looks like a secret, a closed room, a member that left).
    Refused(String),
    /// The service could not be reached: nothing was refused, nothing was done.
    Unavailable(String),
    /// The call did not finish in time: it may have been done.
    Uncertain,
}

pub(crate) fn outcome(result: &Result<Value, String>) -> Outcome {
    match result {
        Ok(_) => Outcome::Done,
        Err(reason) if reason.contains("did not finish in time") => Outcome::Uncertain,
        Err(reason)
            if ["service is stopped", "starting or unreachable", "transport_failure", "could not be run", "has no Mukade program"]
                .iter()
                .any(|sign| reason.contains(sign)) =>
        {
            Outcome::Unavailable(reason.clone())
        }
        Err(reason) => Outcome::Refused(reason.clone()),
    }
}

/// What an outcome means for what the agent was told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Settled {
    Told,
    NotTold(String),
    /// Not known to have happened and not known not to: left for the next check.
    Later,
}

/// An uncertain send is looked up (`found`: the ledger holds the message) before anything else is
/// done; only a message that was found counts as told, and one that was not found is left for the
/// next check, which sends it again. A refusal is never looked up and never repeated.
pub(crate) fn settle(outcome: Outcome, found: impl FnOnce() -> Result<bool, String>) -> Settled {
    match outcome {
        Outcome::Done => Settled::Told,
        Outcome::Refused(reason) => Settled::NotTold(reason),
        Outcome::Unavailable(_) => Settled::Later,
        Outcome::Uncertain => match found() {
            Ok(true) => Settled::Told,
            _ => Settled::Later,
        },
    }
}

/// The first line of a team message with its line break: what the ledger is searched for. The
/// text starts `TEAM revision <n>`, and a notice `TEAM revision <n> changes`.
fn head(text: &str) -> String {
    format!("{}\n", text.lines().next().unwrap_or_default())
}

/// Whether the ledger's `events` hold a message of the member `shinatama` that starts with
/// `head` and is addressed to `to` (a member name; none for an unaddressed notice). `names` gives
/// the name of each member id an event lists as a recipient.
pub(crate) fn ledger_has(events: &[Value], names: &BTreeMap<String, String>, head: &str, to: Option<&str>) -> bool {
    events.iter().any(|event| {
        event["kind"] == "message"
            && event["actor"]["name"] == TEAM_MEMBER
            && event["payload"]["text"].as_str().is_some_and(|text| text.starts_with(head))
            && {
                let recipients: Vec<&str> = event["recipient_ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|id| id.as_str().and_then(|id| names.get(id)).map(String::as_str))
                    .collect();
                match to {
                    Some(name) => recipients == [name],
                    None => recipients.is_empty(),
                }
            }
    })
}

// ---- Executing what is owed ----

/// The room, as far as the worker needs it; the service in the real worker, a script in tests.
pub(crate) trait Room {
    fn send(&self, delivery: &Delivery) -> Result<Value, String>;
    /// Whether the ledger holds `delivery`'s message (an uncertain send).
    fn found(&self, delivery: &Delivery) -> Result<bool, String>;
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Report {
    /// Why a delivery was refused, by agent id (`NOTICE_KEY`, or a member id for a retire).
    pub reasons: BTreeMap<String, String>,
    /// Deliveries that went through.
    pub done: usize,
    /// Something is left for the next check (a timeout not found in the ledger, no service).
    pub later: bool,
    /// Members whose retire was refused or not known to be done.
    pub failed_retires: Vec<String>,
}

/// Run `deliveries` in order. `marked` is told of each one that went through, at once, so a crash
/// does not make the next run send it again. A role that could not be set blocks the message
/// that explains it; a service that cannot be reached ends the run.
pub(crate) fn execute(room: &impl Room, seats: &BTreeMap<String, Seat>, deliveries: &[Delivery], mut marked: impl FnMut(&Delivery)) -> Report {
    let mut report = Report::default();
    let mut blocked: BTreeSet<&str> = BTreeSet::new();
    for delivery in deliveries {
        if let Delivery::Personal { agent, .. } = delivery
            && seats.get(agent).is_some_and(|seat| blocked.contains(seat.member_id.as_str()))
        {
            continue;
        }
        let result = room.send(delivery);
        let ended = outcome(&result);
        if let Outcome::Unavailable(reason) = &ended {
            report.later = true;
            report.reasons.insert(SERVICE_KEY.into(), reason.clone());
            break;
        }
        let key = match delivery {
            Delivery::SetRole { member_id, .. } => seats
                .iter()
                .find(|(_, seat)| &seat.member_id == member_id)
                .map_or_else(|| member_id.clone(), |(agent, _)| agent.clone()),
            Delivery::Personal { agent, .. } => agent.clone(),
            Delivery::Notice { .. } => NOTICE_KEY.to_owned(),
            Delivery::Retire { member_id } => member_id.clone(),
        };
        match settle(ended, || room.found(delivery)) {
            Settled::Told => {
                report.done += 1;
                marked(delivery);
            }
            Settled::NotTold(reason) => {
                report.reasons.insert(key, reason);
                match delivery {
                    Delivery::SetRole { member_id, .. } => {
                        blocked.insert(member_id);
                    }
                    Delivery::Retire { member_id } => report.failed_retires.push(member_id.clone()),
                    _ => {}
                }
            }
            Settled::Later => {
                report.later = true;
                match delivery {
                    Delivery::SetRole { member_id, .. } => {
                        blocked.insert(member_id);
                    }
                    Delivery::Retire { member_id } => report.failed_retires.push(member_id.clone()),
                    _ => {}
                }
            }
        }
    }
    report
}

// ---- Who is in the room ----

/// The seat of each agent of the formation whose session is in the thread, of each agent that was
/// told and is no longer on the map (found by the member it was told as), and the active members
/// of closed sessions of agents on the map. `projected` is `thread_chat::project`'s answer. An
/// agent whose session left the thread, or has no member, has no seat: it is owed nothing.
pub(crate) fn seats_of(formation: &Formation, told: &BTreeMap<String, Told>, projected: &Value) -> (BTreeMap<String, Seat>, Vec<Seat>) {
    let empty = Vec::new();
    let members = projected["members"].as_array().unwrap_or(&empty);
    let rows = projected["sessions"].as_array().unwrap_or(&empty);
    let row_of = |session: &str| rows.iter().find(|row| row["sessionId"] == session);
    let seat = |member: &Value, row: &Value| {
        Some(Seat {
            member_id: member["memberId"].as_str()?.to_owned(),
            member_name: member["name"].as_str()?.to_owned(),
            role: member["role"].as_str().unwrap_or("peer").to_owned(),
            in_room: row["connection"] == "in_room",
        })
    };
    let active_of = |session: &str| members.iter().find(|member| member["sessionId"] == session && member["active"] == true);
    let mut seats = BTreeMap::new();
    let mut closed = Vec::new();
    for agent in &formation.agents {
        let Some(session) = agent.session.as_deref() else { continue };
        let (Some(row), Some(member)) = (row_of(session), active_of(session)) else { continue };
        let Some(found) = seat(member, row) else { continue };
        if row["state"] == "closed" {
            closed.push(found);
        } else {
            seats.insert(agent.id.clone(), found);
        }
    }
    for (agent, was) in told {
        if agent == NOTICE_KEY || formation.agents.iter().any(|a| &a.id == agent) {
            continue;
        }
        let Some(member) = members.iter().find(|member| member["memberId"] == was.member.as_str() && member["active"] == true) else { continue };
        let Some(row) = member["sessionId"].as_str().and_then(row_of) else { continue };
        if row["state"] == "closed" {
            continue;
        }
        if let Some(found) = seat(member, row) {
            seats.insert(agent.clone(), found);
        }
    }
    (seats, closed)
}

// ---- The service ----

struct ServiceRoom<'a> {
    state: &'a WorkspaceServices,
    controller: &'a Controller,
    room: &'a str,
}

impl ServiceRoom<'_> {
    fn pending_key(&self, what: &str) -> (String, String) {
        (self.room.to_owned(), what.to_owned())
    }

    /// One call as the host member `shinatama`. A retry of an uncertain send carries the first
    /// try's request id; the id is dropped once the call ended in any other way.
    fn host_call(&self, what: &str, operation: &str, extra: &[(&str, &str)], text: Option<&str>) -> Result<Value, String> {
        let key = self.pending_key(what);
        let (id, _) = PENDING
            .lock()
            .ok()
            .and_then(|pending| pending.get(&key).cloned())
            .unwrap_or_else(|| (uuid::Uuid::new_v4().to_string(), since(5)));
        let first_try = since(5);
        let result = thread_chat::as_host(self.state, self.controller, self.room, TEAM_MEMBER, |handle| {
            let mut arguments = vec![("request-id", id.as_str()), ("room-id", self.room), ("member-handle", handle)];
            arguments.extend_from_slice(extra);
            mukade_chat::call(&self.state.location, operation, &arguments, text)
        });
        if let Ok(mut pending) = PENDING.lock() {
            if matches!(outcome(&result), Outcome::Uncertain) {
                let kept = pending.get(&key).map_or_else(|| first_try, |(_, at)| at.clone());
                pending.insert(key, (id, kept));
            } else {
                pending.remove(&key);
            }
        }
        result
    }
}

/// The time `seconds` ago, as the ledger's `since` takes it.
fn since(seconds: i64) -> String {
    use chrono::{Duration, SecondsFormat, Utc};
    (Utc::now() - Duration::seconds(seconds)).to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn sent_what(delivery: &Delivery) -> String {
    match delivery {
        Delivery::SetRole { member_id, role } => format!("set-role\n{member_id}\n{}", role.as_str()),
        Delivery::Personal { member_name, text, .. } => format!("send\n{member_name}\n{text}"),
        Delivery::Notice { text } => format!("send\n\n{text}"),
        Delivery::Retire { member_id } => format!("retire\n{member_id}"),
    }
}

impl Room for ServiceRoom<'_> {
    fn send(&self, delivery: &Delivery) -> Result<Value, String> {
        let what = sent_what(delivery);
        match delivery {
            Delivery::SetRole { member_id, role } => {
                self.host_call(&what, "set-role", &[("member-id", member_id), ("role", role.as_str())], None)
            }
            Delivery::Personal { member_name, text, .. } => {
                self.host_call(&what, "send", &[("recipients", member_name)], Some(text))
            }
            Delivery::Notice { text } => self.host_call(&what, "send", &[], Some(text)),
            Delivery::Retire { member_id } => self.host_call(&what, "retire", &[("member-id", member_id)], None),
        }
    }

    fn found(&self, delivery: &Delivery) -> Result<bool, String> {
        let (text, to) = match delivery {
            Delivery::Personal { member_name, text, .. } => (text, Some(member_name.as_str())),
            Delivery::Notice { text } => (text, None),
            // A role and a retire have no message to look for; they are repeated, which is harmless.
            Delivery::SetRole { .. } | Delivery::Retire { .. } => return Ok(false),
        };
        let from = PENDING
            .lock()
            .ok()
            .and_then(|pending| pending.get(&self.pending_key(&sent_what(delivery))).map(|(_, at)| at.clone()))
            .unwrap_or_else(|| since(60));
        let (members, _) = thread_chat::listing(self.state, "members", &[("room-id", self.room)])?;
        let names: BTreeMap<String, String> = members
            .iter()
            .filter_map(|member| Some((member["member_id"].as_str()?.to_owned(), member["name"].as_str()?.to_owned())))
            .collect();
        let (events, _) = thread_chat::listing(self.state, "ledger", &[("room-id", self.room), ("since", &from)])?;
        Ok(ledger_has(&events, &names, &head(text), to))
    }
}

// ---- The worker ----

/// What the worker remembers between two checks.
#[derive(Default)]
struct Memo {
    /// Per thread, what the last check read when it found nothing it could do: while the next
    /// check reads the same, it does nothing and calls nothing.
    parked: BTreeMap<String, String>,
    /// Members whose retire did not go through: not offered again in this run, or the notice of
    /// their closed session would be sent at every check.
    retire_failed: BTreeSet<String>,
}

/// Everything a check reads without a service call, as one string: it differs when the formation,
/// what was told, the room's last event or a session's state or member changed.
fn key_of(record: &Record, room_revision: &str, facts: &[SessionFacts]) -> String {
    let mut key = format!(
        "{}\n{}\n{room_revision}",
        serde_json::to_string(&record.formation).unwrap_or_default(),
        serde_json::to_string(&record.told).unwrap_or_default()
    );
    for fact in facts {
        key.push_str(&format!(
            "\n{} {} {}",
            fact.summary.id,
            thread_chat::session_state(fact.summary.status),
            fact.adapter.as_ref().map(|member| format!("{} {}", member.member_id, member.role)).unwrap_or_default()
        ));
    }
    key
}

/// Take the previous formation out of the record once nothing is owed: it only serves the notice.
fn forget_before(state: &WorkspaceServices, workset: &str, used: &Formation) {
    thread_team::exclusive(|| {
        if let Ok(mut record) = thread_team::load(&state.store, workset)
            && record.before.is_some()
            && record.formation.as_ref().is_some_and(|stored| stored.revision == used.revision)
        {
            record.before = None;
            let _ = thread_team::save(&state.store, workset, &record);
        }
    });
}

fn check_thread(state: &WorkspaceServices, controller: &Controller, memo: &mut Memo, workset: &str, record: &Record, room: &str, revision: &str) {
    let Some(formation) = &record.formation else { return };
    let Ok(facts) = thread_chat::session_facts(state, workset, Some(room)) else { return };
    let key = key_of(record, revision, &facts);
    if memo.parked.get(workset) == Some(&key) {
        return;
    }
    let members = match thread_chat::listing(state, "members", &[("room-id", room)]) {
        Ok((members, _)) => members,
        Err(reason) => {
            set_refusals(workset, BTreeMap::from([(SERVICE_KEY.to_owned(), reason)]));
            return;
        }
    };
    let projected = thread_chat::project(room, &members, &[], &facts);
    let (seats, mut closed) = seats_of(formation, &record.told, &projected);
    closed.retain(|seat| !memo.retire_failed.contains(&seat.member_id));
    let owed = team::deliveries(record.before.as_ref(), formation, &record.told, &seats, &closed);
    if owed.is_empty() {
        forget_before(state, workset, formation);
        set_refusals(workset, BTreeMap::new());
        memo.parked.insert(workset.to_owned(), key);
        return;
    }
    let report = execute(&ServiceRoom { state, controller, room }, &seats, &owed, |delivery| {
        // What was told is merged into the record as it is now, not replaced by what this check
        // read: the window may have applied a newer formation meanwhile. The mark is for the
        // formation the message was made from, and says so with its digest.
        thread_team::exclusive(|| {
            if let Ok(mut stored) = thread_team::load(&state.store, workset) {
                team::mark(formation, &seats, delivery, &mut stored.told);
                let _ = thread_team::save(&state.store, workset, &stored);
            }
        });
    });
    memo.retire_failed.extend(report.failed_retires);
    // Something went through: read again at the next check, which verifies that nothing is left.
    // Only refusals: the same refusals would come again, so wait until something changes.
    if report.done == 0 && !report.later {
        memo.parked.insert(workset.to_owned(), key);
    } else {
        memo.parked.remove(workset);
    }
    set_refusals(workset, report.reasons);
}

fn check_all(state: &WorkspaceServices, controller: &Controller, memo: &mut Memo) {
    let mut teams: Vec<(String, Record)> = Vec::new();
    for workset in state.store.chat_threads().unwrap_or_default() {
        if let Ok(record) = thread_team::load(&state.store, &workset)
            && record.formation.is_some()
        {
            teams.push((workset, record));
        }
    }
    memo.parked.retain(|workset, _| teams.iter().any(|(id, _)| id == workset));
    if teams.is_empty() {
        return;
    }
    if !matches!(Connection::discover(&state.location), Ok(Some(_))) {
        for (workset, _) in &teams {
            set_refusals(workset, BTreeMap::from([(SERVICE_KEY.to_owned(), "Mukade chat is not running.".to_owned())]));
        }
        return;
    }
    let rooms = match thread_chat::listing(state, "rooms", &[]) {
        Ok((rooms, _)) => rooms,
        Err(reason) => {
            for (workset, _) in &teams {
                set_refusals(workset, BTreeMap::from([(SERVICE_KEY.to_owned(), reason.clone())]));
            }
            return;
        }
    };
    for (workset, record) in &teams {
        let Some(name) = thread_room_name(workset) else { continue };
        let Some(found) = rooms.iter().find(|room| room["name"] == name.as_str()) else { continue };
        let (Some(room), Some(revision)) = (found["room_id"].as_str().filter(|id| mukade_chat::valid_room_id(id)), found["revision"].as_str()) else { continue };
        check_thread(state, controller, memo, workset, record, room, revision);
    }
}

/// Start the worker. It ends with `stop` or with the application.
pub fn start(app: &tauri::AppHandle) -> Result<(), String> {
    use crate::{ExitIntent, storage};
    use std::sync::Arc;
    use tauri::Manager;
    let app = app.clone();
    std::thread::Builder::new()
        .name("team-announcer".into())
        .spawn(move || {
            let mut memo = Memo::default();
            while !STOP.load(Ordering::Acquire) && !app.state::<ExitIntent>().0.load(Ordering::Acquire) {
                if let Some(state) = app.state::<Arc<storage::Services>>().ready() {
                    let controller = app.state::<Arc<Controller>>();
                    check_all(state, controller.inner(), &mut memo);
                }
                let woken = WAKE.0.lock().unwrap_or_else(PoisonError::into_inner);
                let (mut woken, _) = WAKE.1.wait_timeout_while(woken, CHECK_EVERY, |woken| !*woken).unwrap_or_else(PoisonError::into_inner);
                *woken = false;
            }
        })
        .map(|_| ())
        .map_err(|_| "The team announcer could not start.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::RefCell;
    use team::{Agent, Kind, Link, Report as Hand, Share};

    fn agent(id: &str, session: &str) -> Agent {
        Agent { id: id.into(), session: Some(session.into()), model: None, name: id.to_uppercase(), notes: String::new(), share: Share::Split, x: 0, y: 0 }
    }

    /// A lead A giving work that is answered only with done or blocked to a worker B, applied as
    /// revision 2; both in the room as peers.
    fn lead_and_worker() -> (Formation, BTreeMap<String, Seat>) {
        let formation = Formation {
            revision: 2,
            agents: vec![agent("a_aaaaaaaaaaaa", "t_aaaaaaaaaaaa"), agent("a_bbbbbbbbbbbb", "t_bbbbbbbbbbbb")],
            links: vec![Link { from: "a_aaaaaaaaaaaa".into(), to: "a_bbbbbbbbbbbb".into(), kind: Kind::Work, report: Hand::Strict, words: String::new() }],
        };
        let seat = |id: &str, name: &str| Seat { member_id: id.into(), member_name: name.into(), role: "peer".into(), in_room: true };
        let seats = BTreeMap::from([
            ("a_aaaaaaaaaaaa".to_owned(), seat("m1", "claude-1")),
            ("a_bbbbbbbbbbbb".to_owned(), seat("m2", "codex-1")),
        ]);
        (formation, seats)
    }

    /// A room that answers from a script and remembers what it was asked, in order.
    struct Scripted {
        log: RefCell<Vec<String>>,
        send: Box<dyn Fn(&Delivery) -> Result<Value, String>>,
        found: Box<dyn Fn(&Delivery) -> Result<bool, String>>,
    }
    impl Scripted {
        fn new(send: impl Fn(&Delivery) -> Result<Value, String> + 'static, found: impl Fn(&Delivery) -> Result<bool, String> + 'static) -> Self {
            Self { log: RefCell::new(Vec::new()), send: Box::new(send), found: Box::new(found) }
        }
    }
    fn label(delivery: &Delivery) -> String {
        match delivery {
            Delivery::SetRole { member_id, role } => format!("role {member_id} {}", role.as_str()),
            Delivery::Personal { member_name, .. } => format!("personal {member_name}"),
            Delivery::Notice { .. } => "notice".to_owned(),
            Delivery::Retire { member_id } => format!("retire {member_id}"),
        }
    }
    impl Room for Scripted {
        fn send(&self, delivery: &Delivery) -> Result<Value, String> {
            self.log.borrow_mut().push(format!("send {}", label(delivery)));
            (self.send)(delivery)
        }
        fn found(&self, delivery: &Delivery) -> Result<bool, String> {
            self.log.borrow_mut().push(format!("found {}", label(delivery)));
            (self.found)(delivery)
        }
    }

    fn owed() -> (Formation, BTreeMap<String, Seat>, Vec<Delivery>) {
        let (formation, seats) = lead_and_worker();
        let owed = team::deliveries(None, &formation, &BTreeMap::new(), &seats, &[]);
        (formation, seats, owed)
    }

    #[test]
    fn the_first_run_sets_roles_then_tells_each_agent() {
        let (_, seats, owed) = owed();
        let labels: Vec<String> = owed.iter().map(label).collect();
        assert_eq!(labels, ["role m1 lead", "role m2 worker", "personal claude-1", "personal codex-1"]);
        let room = Scripted::new(|_| Ok(json!({"event_id": "1"})), |_| Ok(false));
        let mut marked = Vec::new();
        let report = execute(&room, &seats, &owed, |delivery| marked.push(label(delivery)));
        assert_eq!((report.done, report.later, report.reasons.is_empty()), (4, false, true));
        assert_eq!(marked, labels);
    }

    #[test]
    fn an_uncertain_send_is_looked_up_before_it_is_repeated() {
        let (_, seats, owed) = owed();
        let timeout = |delivery: &Delivery| match delivery {
            Delivery::Personal { member_name, .. } if member_name == "claude-1" => Err("Mukade chat send did not finish in time.".to_owned()),
            _ => Ok(json!({})),
        };
        // The message is in the ledger: told, and sent once.
        let room = Scripted::new(timeout, |_| Ok(true));
        let mut marked = Vec::new();
        let report = execute(&room, &seats, &owed, |delivery| marked.push(label(delivery)));
        assert!(marked.contains(&"personal claude-1".to_owned()));
        assert!(!report.later);
        let log = room.log.borrow();
        let sends = log.iter().filter(|line| *line == "send personal claude-1").count();
        let at = |what: &str| log.iter().position(|line| line == what).unwrap();
        assert_eq!(sends, 1, "{log:?}");
        assert!(at("send personal claude-1") < at("found personal claude-1"));
        drop(log);

        // The message is not there: not told, not sent again in this run, left for the next check.
        let room = Scripted::new(timeout, |_| Ok(false));
        let mut marked = Vec::new();
        let report = execute(&room, &seats, &owed, |delivery| marked.push(label(delivery)));
        assert!(!marked.contains(&"personal claude-1".to_owned()));
        assert!(marked.contains(&"personal codex-1".to_owned()));
        assert!(report.later && report.reasons.is_empty());
        assert_eq!(room.log.borrow().iter().filter(|line| *line == "send personal claude-1").count(), 1);

        // The ledger cannot be read: the same.
        let room = Scripted::new(timeout, |_| Err("Mukade did not answer.".into()));
        let report = execute(&room, &seats, &owed, |_| {});
        assert!(report.later);
        // A refusal and a success are never looked up.
        let calls = room.log.borrow().iter().filter(|line| line.starts_with("found")).count();
        assert_eq!(calls, 1);

        // The decision alone.
        let looked = std::cell::Cell::new(0);
        let lookup = |answer: bool| {
            let looked = &looked;
            move || {
                looked.set(looked.get() + 1);
                Ok(answer)
            }
        };
        assert_eq!(settle(Outcome::Uncertain, lookup(true)), Settled::Told);
        assert_eq!(settle(Outcome::Uncertain, lookup(false)), Settled::Later);
        assert_eq!(looked.get(), 2);
        assert_eq!(settle(Outcome::Done, lookup(true)), Settled::Told);
        assert_eq!(settle(Outcome::Refused("no".into()), lookup(true)), Settled::NotTold("no".into()));
        assert_eq!(settle(Outcome::Unavailable("down".into()), lookup(true)), Settled::Later);
        assert_eq!(looked.get(), 2);
    }

    #[test]
    fn a_refused_message_marks_the_agent_not_told() {
        let (_, seats, owed) = owed();
        let refuse = |delivery: &Delivery| match delivery {
            Delivery::Personal { member_name, .. } if member_name == "codex-1" => {
                Err("secret_refused: The message looks like it holds a secret".to_owned())
            }
            _ => Ok(json!({})),
        };
        let room = Scripted::new(refuse, |_| Ok(false));
        let mut marked = Vec::new();
        let report = execute(&room, &seats, &owed, |delivery| marked.push(label(delivery)));
        // The other agent is still told; this one is not marked, and says why.
        assert!(marked.contains(&"personal claude-1".to_owned()));
        assert!(!marked.contains(&"personal codex-1".to_owned()));
        assert_eq!(report.reasons["a_bbbbbbbbbbbb"], "secret_refused: The message looks like it holds a secret");
        // Nothing to try again later: the same message would be refused again.
        assert!(!report.later);
        assert_eq!(room.log.borrow().iter().filter(|line| *line == "send personal codex-1").count(), 1);
        assert!(room.log.borrow().iter().all(|line| !line.starts_with("found")));
    }

    #[test]
    fn a_role_that_could_not_be_set_holds_back_the_message_that_explains_it() {
        let (_, seats, owed) = owed();
        let room = Scripted::new(
            |delivery| match delivery {
                Delivery::SetRole { member_id, .. } if member_id == "m2" => Err("not_host: Only a host member may change a role".to_owned()),
                _ => Ok(json!({})),
            },
            |_| Ok(false),
        );
        let mut marked = Vec::new();
        let report = execute(&room, &seats, &owed, |delivery| marked.push(label(delivery)));
        assert!(marked.contains(&"personal claude-1".to_owned()));
        assert!(!marked.iter().any(|line| line == "personal codex-1"));
        assert!(!room.log.borrow().iter().any(|line| line == "send personal codex-1"));
        assert!(report.reasons["a_bbbbbbbbbbbb"].starts_with("not_host"));
    }

    #[test]
    fn an_unreachable_service_ends_the_run_and_blames_no_agent() {
        let (_, seats, owed) = owed();
        let room = Scripted::new(|_| Err("Chat service is stopped; run `mukade chat start`".to_owned()), |_| Ok(false));
        let report = execute(&room, &seats, &owed, |_| panic!("nothing went through"));
        assert!(report.later);
        assert_eq!(report.reasons.len(), 1);
        assert!(report.reasons.contains_key(SERVICE_KEY));
        assert_eq!(room.log.borrow().len(), 1);
    }

    #[test]
    fn a_refused_retire_is_reported_so_its_notice_is_not_sent_at_every_check() {
        let (_, seats) = lead_and_worker();
        let gone = Seat { member_id: "m9".into(), member_name: "pi-1".into(), role: "peer".into(), in_room: true };
        let owed = vec![Delivery::Notice { text: "TEAM revision 2 changes\n- x".into() }, Delivery::Retire { member_id: gone.member_id.clone() }];
        let room = Scripted::new(
            |delivery| match delivery {
                Delivery::Retire { .. } => Err("member_not_found: No member has that ID".to_owned()),
                _ => Ok(json!({})),
            },
            |_| Ok(false),
        );
        let report = execute(&room, &seats, &owed, |_| {});
        assert_eq!(report.failed_retires, ["m9"]);
        assert_eq!(report.done, 1);
    }

    #[test]
    fn the_ledger_is_searched_for_the_message_by_its_first_line_and_recipient() {
        let names = BTreeMap::from([("m1".to_owned(), "claude-1".to_owned()), ("m2".to_owned(), "codex-1".to_owned())]);
        let event = |actor: &str, text: &str, to: &[&str]| json!({"kind": "message", "actor": {"name": actor}, "payload": {"text": text}, "recipient_ids": to});
        let events = [
            event("user", "TEAM revision 2\nyou", &["m1"]),
            event("shinatama", "TEAM revision 1\nold", &["m1"]),
            event("shinatama", "TEAM revision 2\nYou are A.", &["m2"]),
            event("shinatama", "TEAM revision 2 changes\n- B joined", &[]),
        ];
        assert!(ledger_has(&events, &names, &head("TEAM revision 2\nanything"), Some("codex-1")));
        // Another recipient, another revision, another author: not the message.
        assert!(!ledger_has(&events, &names, "TEAM revision 2\n", Some("claude-1")));
        assert!(!ledger_has(&events, &names, "TEAM revision 3\n", Some("codex-1")));
        // A notice is unaddressed.
        assert!(ledger_has(&events, &names, "TEAM revision 2 changes\n", None));
        assert!(!ledger_has(&events, &names, "TEAM revision 2\n", None));
    }

    #[test]
    fn seats_follow_the_sessions_in_the_thread_and_the_members_that_hold_them() {
        let (formation, _) = lead_and_worker();
        let member = |id: &str, name: &str, session: Option<&str>, active: bool| json!({"memberId": id, "name": name, "role": "peer", "active": active, "sessionId": session});
        let row = |session: &str, state: &str, connection: &str| json!({"sessionId": session, "state": state, "connection": connection});
        // B's session left the thread (no row); C was told and taken off the map; D closed.
        let mut formation = formation;
        formation.agents.push(agent("a_dddddddddddd", "t_dddddddddddd"));
        let projected = json!({
            "members": [
                member("m1", "claude-1", Some("t_aaaaaaaaaaaa"), true),
                member("m2", "codex-1", Some("t_bbbbbbbbbbbb"), true),
                member("m3", "pi-1", Some("t_cccccccccccc"), true),
                member("m4", "codex-2", Some("t_dddddddddddd"), true),
            ],
            "sessions": [
                row("t_aaaaaaaaaaaa", "running", "in_room"),
                row("t_cccccccccccc", "running", "in_room"),
                row("t_dddddddddddd", "closed", "in_room"),
            ],
        });
        let told = BTreeMap::from([
            ("a_cccccccccccc".to_owned(), Told { revision: 1, member: "m3".into(), digest: "d".into() }),
            (NOTICE_KEY.to_owned(), Told { revision: 1, member: String::new(), digest: String::new() }),
        ]);
        let (seats, closed) = seats_of(&formation, &told, &projected);
        assert_eq!(seats.keys().map(String::as_str).collect::<Vec<_>>(), ["a_aaaaaaaaaaaa", "a_cccccccccccc"]);
        assert!(seats["a_aaaaaaaaaaaa"].in_room);
        assert_eq!(closed.iter().map(|seat| seat.member_name.as_str()).collect::<Vec<_>>(), ["codex-2"]);
        // A member that left is nobody's seat.
        let left = json!({"members": [member("m1", "claude-1", Some("t_aaaaaaaaaaaa"), false)], "sessions": [row("t_aaaaaaaaaaaa", "running", "in_room")]});
        assert!(seats_of(&formation, &BTreeMap::new(), &left).0.is_empty());
    }

    #[test]
    fn an_outcome_is_read_from_the_error_text() {
        assert_eq!(outcome(&Ok(json!({}))), Outcome::Done);
        assert_eq!(outcome(&Err("Mukade chat send did not finish in time.".into())), Outcome::Uncertain);
        assert!(matches!(outcome(&Err("Chat service is stopped; run `mukade chat start`".into())), Outcome::Unavailable(_)));
        assert!(matches!(outcome(&Err("This Shinatama has no Mukade program in its mukade folder.".into())), Outcome::Unavailable(_)));
        assert!(matches!(outcome(&Err("secret_refused: looks like a secret".into())), Outcome::Refused(_)));
    }
}
