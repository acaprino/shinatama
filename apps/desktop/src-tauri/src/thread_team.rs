//! The window's commands for a thread's team map. The storing logic is plain functions over
//! the database and the facts of the thread, so they are tested without a window; the commands
//! only gather those facts. Roles, checks and sentences are `shinatama_engine::team`'s: nothing
//! here computes them.
use crate::storage::{self, WorkspaceServices};
use crate::thread_chat;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use shinatama_engine::model_config::{self, Selection};
use shinatama_engine::team::{self, Agent, Formation, Level, ModelChoice, ModelKey, Template, Told};
use shinatama_state::database::Database;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State, WebviewWindow};

/// A setting holds 64 KiB (`Database::set_setting_json`).
const MAX_RECORD: usize = 64 * 1024;
/// A card's place is within this many units of the map's origin.
const MAX_PLACE: i32 = 1_000_000;

/// What a thread keeps (the shape `Database::thread_team` leaves to its users): the applied
/// formation, what each agent was last told, and the formation before the last apply, which the
/// notice for the other members is made from until it is sent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Record {
    #[serde(default)]
    pub formation: Option<Formation>,
    /// Per agent id; the key `team::NOTICE_KEY` holds the last revision whose notice was sent.
    #[serde(default)]
    pub told: BTreeMap<String, Told>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<Formation>,
}

/// The apply of the window and the announcer's marks both read, change and write one record:
/// they take turns, or a mark made for revision N could overwrite a formation of revision N+1.
static RECORDS: Mutex<()> = Mutex::new(());
pub(crate) fn exclusive<T>(work: impl FnOnce() -> T) -> T {
    let _turn = RECORDS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    work()
}

/// What the commands know of the thread besides its record.
#[derive(Debug, Clone, Default)]
pub(crate) struct Thread {
    /// Whether the thread's sessions talk to each other.
    pub enabled: bool,
    /// Its sessions, by id, with their state as the window names it (`running`, `stopped`, ...).
    pub sessions: BTreeMap<String, String>,
    /// The model menu as the Models panel holds it now.
    pub menu: Vec<ModelChoice>,
}

impl Thread {
    /// What the checks look a model up by.
    pub fn models(&self) -> BTreeSet<ModelKey> {
        team::model_keys(&self.menu)
    }

    /// The agents whose session exists but is not running.
    pub fn stopped(&self, f: &Formation) -> BTreeSet<String> {
        f.agents
            .iter()
            .filter(|agent| {
                agent
                    .session
                    .as_ref()
                    .and_then(|session| self.sessions.get(session))
                    .is_some_and(|state| state != "running")
            })
            .map(|agent| agent.id.clone())
            .collect()
    }
}

pub(crate) fn load(store: &Database, workset: &str) -> Result<Record, String> {
    match store.thread_team(workset).map_err(|e| e.to_string())? {
        None => Ok(Record::default()),
        Some(value) => serde_json::from_value(value).map_err(|_| "The thread's team could not be read.".to_owned()),
    }
}

/// The previous formation gives way when the record would not fit a setting: the notice for the
/// other members is then not made, the team itself is kept.
pub(crate) fn save(store: &Database, workset: &str, record: &Record) -> Result<(), String> {
    let mut value = serde_json::to_value(record).map_err(|_| "The team could not be stored.".to_owned())?;
    if value.to_string().len() > MAX_RECORD {
        value = serde_json::to_value(Record { before: None, ..record.clone() })
            .map_err(|_| "The team could not be stored.".to_owned())?;
    }
    store.set_thread_team(workset, Some(&value)).map_err(|e| e.to_string())
}

/// What an apply refuses before it opens or stores anything, whatever the stored revision: a
/// thread that is off, what cannot be valid, a session that is not the thread's.
fn admit(thread: &Thread, formation: &Formation) -> Result<(), String> {
    if !thread.enabled {
        return Err("This thread's sessions do not talk to each other. Turn it on first.".into());
    }
    team::validate(formation)?;
    if formation.agents.iter().filter_map(|agent| agent.session.as_deref()).any(|session| !thread.sessions.contains_key(session)) {
        return Err("An agent's session does not belong to this thread.".into());
    }
    Ok(())
}

/// What an apply refuses given the stored record: a revision other than `expected`, then an error
/// of the checks (warnings do not stop it).
fn admit_record(record: &Record, thread: &Thread, formation: &Formation, expected: u64) -> Result<(), String> {
    let stored = record.formation.as_ref().map_or(0, |f| f.revision);
    if stored != expected {
        return Err(format!("The team was changed elsewhere (revision {stored}). Reload it."));
    }
    if let Some(error) =
        team::check(formation, &thread.stopped(formation), &thread.models()).into_iter().find(|issue| issue.level == Level::Error)
    {
        return Err(error.text);
    }
    Ok(())
}

/// Store `formation` as the thread's team, one revision above `expected`. Refuses, in this
/// order: a thread that is off; what cannot be valid; a session that is not the thread's; a
/// stored revision other than `expected`; an error of the checks (warnings do not stop it).
pub(crate) fn apply(
    store: &Database,
    workset: &str,
    thread: &Thread,
    mut formation: Formation,
    expected: u64,
) -> Result<Formation, String> {
    admit(thread, &formation)?;
    exclusive(|| {
        let mut record = load(store, workset)?;
        admit_record(&record, thread, &formation, expected)?;
        formation.revision = expected.checked_add(1).ok_or("The team's revision is exhausted.")?;
        // The formation the notice is made from is the oldest one whose changes were not yet
        // noticed (the worker lets it go once nothing is owed): a second apply before a check
        // must not lose the first one's lines.
        record.before = record.before.take().or_else(|| record.formation.take());
        record.formation = Some(formation.clone());
        save(store, workset, &record)?;
        Ok(formation)
    })
}

/// An agent whose session could not be opened, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Failure {
    pub agent: String,
    pub reason: String,
}

/// What `team_apply` answers: the stored formation, and the agent whose opening stopped it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Applied {
    pub formation: Formation,
    pub failed: Option<Failure>,
}

/// `apply`, after opening a session for each agent that has none, one after another, with
/// `open` (which answers the new session's thread id). Nothing is opened for a team the apply
/// would refuse, or while `reach` (the Mukade service, which the sessions join) fails; an agent
/// that has a session is not opened again, so a retry after a failure opens only what is left.
/// A launch that fails stops the opening: the formation is stored with the sessions opened so far
/// and the agent is named. The formation is stored after its sessions exist, never before.
pub(crate) fn apply_opening(
    store: &Database,
    workset: &str,
    thread: &Thread,
    mut formation: Formation,
    expected: u64,
    reach: impl FnOnce() -> Result<(), String>,
    mut open: impl FnMut(&Agent) -> Result<String, String>,
) -> Result<Applied, String> {
    admit(thread, &formation)?;
    admit_record(&load(store, workset)?, thread, &formation, expected)?;
    let lacking: Vec<usize> = (0..formation.agents.len()).filter(|&at| formation.agents[at].session.is_none()).collect();
    let mut thread = thread.clone();
    let mut failed = None;
    if !lacking.is_empty() {
        reach()?;
        for at in lacking {
            match open(&formation.agents[at]) {
                Ok(session) => {
                    thread.sessions.insert(session.clone(), "running".to_owned());
                    formation.agents[at].session = Some(session);
                }
                Err(reason) => {
                    failed = Some(Failure { agent: formation.agents[at].id.clone(), reason });
                    break;
                }
            }
        }
    }
    let formation = apply(store, workset, &thread, formation, expected)?;
    Ok(Applied { formation, failed })
}

/// Move cards: only `x` and `y` change, and no revision. An agent the team no longer has is
/// skipped (another window removed it while this one dragged).
pub(crate) fn place(store: &Database, workset: &str, places: &BTreeMap<String, [i32; 2]>) -> Result<(), String> {
    if places.values().flatten().any(|at| !(-MAX_PLACE..=MAX_PLACE).contains(at)) {
        return Err("A card is placed out of the map.".into());
    }
    exclusive(|| {
        let mut record = load(store, workset)?;
        let formation = record.formation.as_mut().ok_or("This thread has no team.")?;
        for agent in &mut formation.agents {
            if let Some([x, y]) = places.get(&agent.id) {
                (agent.x, agent.y) = (*x, *y);
            }
        }
        save(store, workset, &record)
    })
}

/// What `team_get` answers. `told` leaves out the digests and the notice's mark: the window
/// needs to know who was told which revision and by which member, nothing else. `refused`: the
/// reason a message to an agent was refused, as the announcer last found it.
pub(crate) fn answer(record: &Record, thread: &Thread, sessions: Value, refused: &BTreeMap<String, String>) -> Value {
    let told: BTreeMap<&String, Value> = record
        .told
        .iter()
        .filter(|(id, _)| id.as_str() != team::NOTICE_KEY)
        .map(|(id, told)| (id, json!({"revision": told.revision, "member": told.member})))
        .collect();
    let evaluation = record.formation.as_ref().map(|f| team::evaluate(f, &thread.stopped(f), &thread.models()));
    json!({
        "enabled": thread.enabled,
        "models": thread.menu,
        "formation": record.formation,
        "told": told,
        "refused": refused,
        "sessions": sessions,
        "evaluation": evaluation,
    })
}

fn thread_of(state: &WorkspaceServices, workset: &str) -> Result<Thread, String> {
    let enabled = state.store.thread_chat_enabled(workset).map_err(|e| e.to_string())?;
    let sessions = state
        .store
        .workset_threads(workset)
        .map_err(|_| "The thread no longer exists.".to_owned())?
        .into_iter()
        .map(|session| (session.id, thread_chat::session_state(session.status).to_owned()))
        .collect();
    // A Models panel that cannot be read holds no model: an agent that names one then reads as lost.
    let menu = team::model_menu(&model_config::current().map(|snapshot| snapshot.configuration).unwrap_or_default());
    Ok(Thread { enabled, sessions, menu })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetRequest {
    workset_id: String,
}
/// The thread's team, the state of its sessions as the Conversation panel shows them, and the
/// evaluation of the formation. A read: it starts no service and sends nothing.
#[tauri::command]
pub async fn team_get(
    window: WebviewWindow,
    services: State<'_, Arc<storage::Services>>,
    request: GetRequest,
) -> Result<Value, String> {
    crate::local(&window)?;
    thread_chat::workset(&request.workset_id)?;
    thread_chat::blocking(services.inner(), move |state| {
        let thread = thread_of(state, &request.workset_id)?;
        let record = load(&state.store, &request.workset_id)?;
        let room = if thread.enabled { thread_chat::find_room(state, &request.workset_id) } else { None };
        let members = room
            .as_deref()
            .and_then(|room| thread_chat::listing(state, "members", &[("room-id", room)]).ok())
            .map(|(members, _)| members)
            .unwrap_or_default();
        let facts = thread_chat::session_facts(state, &request.workset_id, room.as_deref())?;
        let sessions = thread_chat::project(room.as_deref().unwrap_or_default(), &members, &[], &facts)["sessions"].clone();
        Ok(answer(&record, &thread, sessions, &crate::team_announcer::refusals(&request.workset_id)))
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluateRequest {
    workset_id: String,
    formation: Formation,
}
/// The evaluation of a draft, which the window asks for at each edit. Nothing is stored.
#[tauri::command]
pub async fn team_evaluate(
    window: WebviewWindow,
    services: State<'_, Arc<storage::Services>>,
    request: EvaluateRequest,
) -> Result<team::Evaluation, String> {
    crate::local(&window)?;
    thread_chat::workset(&request.workset_id)?;
    thread_chat::blocking(services.inner(), move |state| {
        team::validate(&request.formation)?;
        let thread = thread_of(state, &request.workset_id)?;
        Ok(team::evaluate(&request.formation, &thread.stopped(&request.formation), &thread.models()))
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyRequest {
    workset_id: String,
    formation: Formation,
    expected_revision: u64,
}
/// The folder of the thread's project, which the sessions opened for its agents run in.
fn project_of(state: &crate::storage::WorkspaceServices, workset: &str) -> Result<PathBuf, String> {
    let record = state
        .store
        .workset_record(workset)
        .map_err(|e| e.to_string())?
        .filter(|record| !record.archived)
        .ok_or("The thread no longer exists.")?;
    state
        .store
        .recent()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|project| project.id == record.project_id && project.available)
        .map(|project| PathBuf::from(project.path))
        .ok_or_else(|| "The thread's project folder is not available. Open the project first.".to_owned())
}

/// A session for one agent, in its thread, with its model and its first message; its tab opens
/// as for any session another one opened (`collaboration-session-opened`). Answers the new
/// session's thread id, or why it could not be opened.
fn open_for(
    app: &tauri::AppHandle,
    state: &crate::storage::WorkspaceServices,
    owner: &Arc<crate::agents::Controller>,
    host: &crate::Host,
    workset: &str,
    project: &Path,
    agent: &Agent,
) -> Result<String, String> {
    let model = agent.model.as_ref().ok_or("The agent has no model.")?;
    let selection = match (&model.connection, &model.model) {
        (Some(connection), Some(model)) => Some(Selection { connection_id: connection.clone(), model_id: model.clone() }),
        (None, None) => None,
        _ => return Err("The agent's model is incomplete.".into()),
    };
    let room = shinatama_engine::session_control::thread_room_name(workset).ok_or("The thread is invalid.")?;
    let started = tauri::async_runtime::block_on(owner.open_agent(
        &model.program,
        host.clone(),
        project.to_path_buf(),
        selection.clone(),
        selection.is_none(),
        team::opening(&agent.name, &room),
        workset.to_owned(),
    ))
    .map_err(str::to_owned)?;
    // The host keeps the process even if the page is reloading; a failed notice opens nothing twice.
    let _ = app.emit_to(
        "main",
        "collaboration-session-opened",
        json!({
            "requestId": format!("team-{}", agent.id),
            "parentInstanceId": Value::Null,
            "harness": model.program,
            "mode": "team",
            "joinedParentThread": true,
            "started": started,
        }),
    );
    if state.store.workset_of_thread(&started.thread_id).ok().flatten().as_deref() != Some(workset) {
        return Err("The session opened outside this thread, so it was not tied to the agent.".into());
    }
    Ok(started.thread_id)
}

/// Store the draft as the thread's team, after opening a session for each agent that has none,
/// and wake the announcer, which tells the sessions. Answers `{ formation, failed }`: `failed`
/// names the agent whose session could not be opened, and what is stored holds the sessions
/// opened before it.
#[tauri::command]
pub async fn team_apply(
    window: WebviewWindow,
    services: State<'_, Arc<storage::Services>>,
    owner: State<'_, Arc<crate::agents::Controller>>,
    host: State<'_, crate::Host>,
    request: ApplyRequest,
) -> Result<Value, String> {
    crate::local(&window)?;
    thread_chat::workset(&request.workset_id)?;
    let (app, owner, host) = (window.app_handle().clone(), owner.inner().clone(), host.inner().clone());
    let applied = thread_chat::blocking(services.inner(), move |state| {
        let thread = thread_of(state, &request.workset_id)?;
        let workset = request.workset_id.as_str();
        apply_opening(
            &state.store,
            workset,
            &thread,
            request.formation,
            request.expected_revision,
            || {
                // The sessions join the thread's room: the service must run before the first launch.
                thread_chat::room_of(state, workset)
                    .map(|_| ())
                    .map_err(|reason| format!("The Mukade chat service could not be reached or started, so no session was opened: {reason}"))
            },
            |agent| {
                let project = project_of(state, workset)?;
                open_for(&app, state, &owner, &host, workset, &project, agent)
            },
        )
    })
    .await?;
    crate::team_announcer::wake();
    Ok(json!({"formation": applied.formation, "failed": applied.failed}))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlaceRequest {
    workset_id: String,
    places: BTreeMap<String, [i32; 2]>,
}
/// Where the cards were dragged to. Not a change of the team: no revision, nothing is told.
#[tauri::command]
pub async fn team_place(
    window: WebviewWindow,
    services: State<'_, Arc<storage::Services>>,
    request: PlaceRequest,
) -> Result<Value, String> {
    crate::local(&window)?;
    thread_chat::workset(&request.workset_id)?;
    thread_chat::blocking(services.inner(), move |state| place(&state.store, &request.workset_id, &request.places)).await?;
    Ok(json!({}))
}

/// The templates, which belong to no thread. The store runs `change` in the transaction that
/// writes the list (`Database::edit_team_templates`), so the 64 limit and the name's uniqueness
/// hold when two windows save at once. A stored template that cannot be read is left out of the
/// list and kept in the store.
fn edit_templates(store: &Database, change: impl FnOnce(&mut Vec<Template>) -> Result<(), String>) -> Result<Vec<Template>, String> {
    let stored = store
        .edit_team_templates(|values| {
            let mut list: Vec<Template> = Vec::new();
            let mut unreadable = Vec::new();
            for value in values.drain(..) {
                match serde_json::from_value(value.clone()) {
                    Ok(template) => list.push(template),
                    Err(_) => unreadable.push(value),
                }
            }
            change(&mut list)?;
            for template in &list {
                let value = serde_json::to_value(template).map_err(|_| "The template could not be stored.".to_owned())?;
                if value.to_string().len() > MAX_RECORD {
                    return Err("This team is too large to be kept as a template.".to_owned());
                }
                values.push(value);
            }
            // What this build cannot read (a template of another format) is kept as it is, after
            // the others and within the bound, so that one save does not delete it.
            let room = shinatama_state::database::MAX_TEMPLATES.saturating_sub(values.len());
            values.extend(unreadable.into_iter().take(room));
            Ok(())
        })
        .map_err(|e| e.to_string())??;
    Ok(stored.into_iter().filter_map(|value| serde_json::from_value(value).ok()).collect())
}

pub(crate) fn templates(store: &Database) -> Result<Vec<Template>, String> {
    let stored = store.team_templates().map_err(|e| e.to_string())?;
    Ok(stored.into_iter().filter_map(|value| serde_json::from_value(value).ok()).collect())
}

/// Keep the formation's team as a template of that name, at the end of the list.
pub(crate) fn save_template(store: &Database, name: &str, formation: &Formation, id: String) -> Result<Vec<Template>, String> {
    let template = team::template_of(id, name, formation)?;
    edit_templates(store, |list| team::add_template(list, template))
}

pub(crate) fn delete_template(store: &Database, id: &str) -> Result<Vec<Template>, String> {
    edit_templates(store, |list| {
        team::remove_template(list, id);
        Ok(())
    })
}

fn new_template_id() -> String {
    let random = uuid::Uuid::new_v4();
    let bytes = random.as_bytes();
    team::template_id(&[bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13]])
}

/// The saved templates, every project's and every thread's.
#[tauri::command]
pub async fn team_templates(window: WebviewWindow, services: State<'_, Arc<storage::Services>>) -> Result<Vec<Template>, String> {
    crate::local(&window)?;
    thread_chat::blocking(services.inner(), move |state| templates(&state.store)).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveTemplateRequest {
    name: String,
    formation: Formation,
}
/// Keep a team as a template. Answers the list.
#[tauri::command]
pub async fn team_template_save(
    window: WebviewWindow,
    services: State<'_, Arc<storage::Services>>,
    request: SaveTemplateRequest,
) -> Result<Vec<Template>, String> {
    crate::local(&window)?;
    thread_chat::blocking(services.inner(), move |state| {
        save_template(&state.store, &request.name, &request.formation, new_template_id())
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteTemplateRequest {
    id: String,
}
/// Delete a template. Answers the list.
#[tauri::command]
pub async fn team_template_delete(
    window: WebviewWindow,
    services: State<'_, Arc<storage::Services>>,
    request: DeleteTemplateRequest,
) -> Result<Vec<Template>, String> {
    crate::local(&window)?;
    thread_chat::blocking(services.inner(), move |state| delete_template(&state.store, &request.id)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use shinatama_engine::model_config::Configuration;
    use shinatama_engine::team::{Kind, Link, ModelRef, Report, Share};
    use serde_json::json;
    use shinatama_state::location::{LocationRequest, StorageLocation};

    const WORKSET: &str = "w_aaaaaaaaaaaa";
    const OTHER: &str = "w_bbbbbbbbbbbb";

    fn database() -> Database {
        let root = std::env::temp_dir().join(format!("shinatama-team-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("projects")).unwrap();
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: root.clone(),
            home_override: Some(root.join("data")),
            project_roots: vec![root.join("projects")],
            synchronized_roots: Vec::new(),
        })
        .unwrap();
        Database::open(&location, None).unwrap()
    }

    fn thread() -> Thread {
        Thread {
            enabled: true,
            sessions: BTreeMap::from([
                ("t_aaaaaaaaaaaa".to_owned(), "running".to_owned()),
                ("t_bbbbbbbbbbbb".to_owned(), "stopped".to_owned()),
            ]),
            menu: team::model_menu(&Configuration::default()),
        }
    }

    fn agent(id: &str, session: Option<&str>) -> Agent {
        Agent {
            id: id.into(),
            session: session.map(Into::into),
            model: None,
            name: id.to_uppercase(),
            notes: String::new(),
            share: Share::Split,
            x: 0,
            y: 0,
        }
    }

    fn formation(agents: Vec<Agent>) -> Formation {
        Formation { revision: 0, agents, links: Vec::new() }
    }

    fn two() -> Formation {
        formation(vec![agent("a_aaaaaaaaaaaa", Some("t_aaaaaaaaaaaa")), agent("a_bbbbbbbbbbbb", Some("t_bbbbbbbbbbbb"))])
    }

    #[test]
    fn a_stale_revision_is_refused() {
        let store = database();
        let first = apply(&store, WORKSET, &thread(), two(), 0).unwrap();
        assert_eq!(first.revision, 1);
        let second = apply(&store, WORKSET, &thread(), two(), 1).unwrap();
        assert_eq!(second.revision, 2);
        // A second window still holds revision 1.
        let refused = apply(&store, WORKSET, &thread(), two(), 1).unwrap_err();
        assert_eq!(refused, "The team was changed elsewhere (revision 2). Reload it.");
        assert_eq!(load(&store, WORKSET).unwrap().formation.unwrap().revision, 2);
        // A team that was never applied is revision 0.
        assert_eq!(apply(&database(), WORKSET, &thread(), two(), 3).unwrap_err(), "The team was changed elsewhere (revision 0). Reload it.");
    }

    #[test]
    fn a_formation_with_an_error_is_not_stored() {
        let store = database();
        // No session and no model: it cannot be opened.
        let empty = formation(vec![agent("a_aaaaaaaaaaaa", None)]);
        let refused = apply(&store, WORKSET, &thread(), empty, 0).unwrap_err();
        assert!(refused.contains("no session and no model"), "{refused}");
        assert_eq!(load(&store, WORKSET).unwrap(), Record::default());
        // What cannot be valid is refused before it is checked.
        let mut at = formation(vec![agent("a_aaaaaaaaaaaa", Some("t_aaaaaaaaaaaa"))]);
        at.agents[0].name = "all @all".into();
        assert!(apply(&store, WORKSET, &thread(), at, 0).unwrap_err().contains('@'));
        // A thread that is off keeps nothing.
        let off = Thread { enabled: false, ..thread() };
        assert!(apply(&store, WORKSET, &off, two(), 0).unwrap_err().contains("Turn it on"));
        assert_eq!(load(&store, WORKSET).unwrap(), Record::default());
        // A warning does not stop it: work given to a stopped session waits.
        let mut warned = two();
        warned.links.push(Link { from: "a_aaaaaaaaaaaa".into(), to: "a_bbbbbbbbbbbb".into(), kind: Kind::Work, report: Report::Done, words: String::new() });
        assert!(apply(&store, WORKSET, &thread(), warned, 0).is_ok());
    }

    #[test]
    fn a_session_of_another_thread_is_refused() {
        let store = database();
        let foreign = formation(vec![agent("a_aaaaaaaaaaaa", Some("t_cccccccccccc"))]);
        let refused = apply(&store, WORKSET, &thread(), foreign, 0).unwrap_err();
        assert_eq!(refused, "An agent's session does not belong to this thread.");
        assert_eq!(load(&store, WORKSET).unwrap(), Record::default());
        // Another thread's record is not touched by either.
        apply(&store, OTHER, &thread(), two(), 0).unwrap();
        assert_eq!(load(&store, WORKSET).unwrap(), Record::default());
    }

    #[test]
    fn placing_changes_no_revision() {
        let store = database();
        apply(&store, WORKSET, &thread(), two(), 0).unwrap();
        place(&store, WORKSET, &BTreeMap::from([
            ("a_aaaaaaaaaaaa".to_owned(), [120, -40]),
            ("a_zzzzzzzzzzzz".to_owned(), [1, 1]),
        ]))
        .unwrap();
        let kept = load(&store, WORKSET).unwrap().formation.unwrap();
        assert_eq!(kept.revision, 1);
        assert_eq!((kept.agents[0].x, kept.agents[0].y), (120, -40));
        assert_eq!((kept.agents[1].x, kept.agents[1].y), (0, 0));
        assert_eq!(kept.agents.len(), 2);
        assert!(place(&store, WORKSET, &BTreeMap::from([("a_aaaaaaaaaaaa".to_owned(), [i32::MAX, 0])])).is_err());
        assert!(place(&database(), WORKSET, &BTreeMap::new()).is_err());
    }

    #[test]
    fn an_apply_keeps_what_was_told_and_the_formation_it_replaces() {
        let store = database();
        apply(&store, WORKSET, &thread(), two(), 0).unwrap();
        let mut record = load(&store, WORKSET).unwrap();
        record.told.insert("a_aaaaaaaaaaaa".into(), Told { revision: 1, member: "m1".into(), digest: "d".into() });
        save(&store, WORKSET, &record).unwrap();
        apply(&store, WORKSET, &thread(), two(), 1).unwrap();
        let kept = load(&store, WORKSET).unwrap();
        assert_eq!(kept.formation.as_ref().unwrap().revision, 2);
        assert_eq!(kept.before.as_ref().unwrap().revision, 1);
        assert_eq!(kept.told["a_aaaaaaaaaaaa"].member, "m1");
    }

    #[test]
    fn two_applies_before_a_check_keep_the_oldest_formation_the_notice_is_made_from() {
        let store = database();
        apply(&store, WORKSET, &thread(), two(), 0).unwrap();
        // The first formation was noticed (the worker found nothing owed and let `before` go).
        let mut record = load(&store, WORKSET).unwrap();
        record.before = None;
        save(&store, WORKSET, &record).unwrap();
        let mut changed = two();
        changed.agents[0].name = "Ada".into();
        apply(&store, WORKSET, &thread(), changed.clone(), 1).unwrap();
        let mut again = changed;
        again.agents[1].name = "Bo".into();
        apply(&store, WORKSET, &thread(), again, 2).unwrap();
        let kept = load(&store, WORKSET).unwrap();
        assert_eq!(kept.formation.as_ref().unwrap().revision, 3);
        let oldest = kept.before.expect("the notice is still owed");
        assert_eq!(oldest.revision, 1, "not the formation of revision 2: its change would lose its notice");
        let changes = shinatama_engine::team::changes(Some(&oldest), kept.formation.as_ref().unwrap());
        let lines = changes.lines.join("\n");
        assert!(lines.contains("Ada") && lines.contains("Bo"), "both changes are in the one notice: {lines}");
    }

    #[test]
    fn two_agents_cannot_hold_one_session() {
        let mut f = two();
        f.agents[1].session = f.agents[0].session.clone();
        assert!(apply(&database(), WORKSET, &thread(), f, 0).is_err());
    }

    #[test]
    fn a_record_too_big_with_its_previous_formation_is_kept_without_it() {
        let store = database();
        let big = |revision: u64| {
            let mut f = formation((0..team::MAX_AGENTS).map(|k| {
                let letter = "abcdefghijklmnopqrstuvwx".chars().nth(k).unwrap().to_string().repeat(12);
                let mut a = agent(&format!("a_{letter}"), Some(&format!("t_{letter}")));
                a.notes = "n".repeat(team::MAX_NOTES);
                a
            }).collect());
            f.revision = revision;
            let ids: Vec<String> = f.agents.iter().map(|a| a.id.clone()).collect();
            'pairs: for (i, from) in ids.iter().enumerate() {
                for to in &ids[i + 1..] {
                    if f.links.len() == team::MAX_LINKS {
                        break 'pairs;
                    }
                    f.links.push(Link { from: from.clone(), to: to.clone(), kind: Kind::Observes, report: Report::Done, words: "w".repeat(team::MAX_WORDS) });
                }
            }
            f
        };
        let first = big(0);
        let mut crowded = thread();
        crowded.sessions = big(0).agents.iter().filter_map(|a| a.session.clone()).map(|s| (s, "running".to_owned())).collect();
        apply(&store, WORKSET, &crowded, first, 0).unwrap();
        apply(&store, WORKSET, &crowded, big(0), 1).unwrap();
        let kept = load(&store, WORKSET).unwrap();
        assert_eq!(kept.formation.unwrap().revision, 2);
        assert!(kept.before.is_none());
    }

    #[test]
    fn the_answer_shows_who_was_told_and_what_the_formation_evaluates_to() {
        let store = database();
        let applied = apply(&store, WORKSET, &thread(), two(), 0).unwrap();
        let mut record = load(&store, WORKSET).unwrap();
        record.told.insert("a_aaaaaaaaaaaa".into(), Told { revision: 1, member: "m1".into(), digest: "secret-digest".into() });
        record.told.insert(team::NOTICE_KEY.into(), Told { revision: 1, member: String::new(), digest: String::new() });
        let rows = json!([{"sessionId": "t_aaaaaaaaaaaa", "connection": "in_room"}]);
        let got = answer(&record, &thread(), rows.clone(), &BTreeMap::from([("a_bbbbbbbbbbbb".to_owned(), "The message was refused.".to_owned())]));
        assert_eq!(got["enabled"], true);
        assert_eq!(got["formation"]["revision"], applied.revision);
        assert_eq!(got["told"], json!({"a_aaaaaaaaaaaa": {"revision": 1, "member": "m1"}}));
        assert_eq!(got["refused"], json!({"a_bbbbbbbbbbbb": "The message was refused."}));
        assert_eq!(got["sessions"], rows);
        // The stopped session of the second agent is on the evaluation's side of the answer.
        assert!(got["evaluation"]["roles"]["a_bbbbbbbbbbbb"].is_object());
        assert!(!got.to_string().contains("secret-digest"));
        // A thread without a team has no formation and no evaluation.
        let none = answer(&Record::default(), &thread(), json!([]), &BTreeMap::new());
        assert_eq!((none["formation"].is_null(), none["evaluation"].is_null()), (true, true));
    }

    fn waiting(id: &str, program: &str) -> Agent {
        Agent { model: Some(ModelRef { program: program.into(), connection: None, model: None }), ..agent(id, None) }
    }

    /// One running agent and three that wait for a session.
    fn crowd() -> Formation {
        formation(vec![
            agent("a_aaaaaaaaaaaa", Some("t_aaaaaaaaaaaa")),
            waiting("a_bbbbbbbbbbbb", "opencode"),
            waiting("a_cccccccccccc", "pi"),
            waiting("a_dddddddddddd", "codex"),
        ])
    }

    fn reachable() -> Result<(), String> {
        Ok(())
    }

    #[test]
    fn sessions_are_opened_before_the_formation_is_stored() {
        let store = database();
        let mut seen = Vec::new();
        let applied = apply_opening(&store, WORKSET, &thread(), crowd(), 0, reachable, |agent| {
            // Nothing is stored while the sessions are being opened.
            assert_eq!(load(&store, WORKSET).unwrap(), Record::default());
            seen.push(agent.id.clone());
            Ok(format!("t_{}", agent.id.trim_start_matches("a_")))
        })
        .unwrap();
        assert_eq!(seen, ["a_bbbbbbbbbbbb", "a_cccccccccccc", "a_dddddddddddd"], "in order, never the running one");
        assert_eq!(applied.failed, None);
        let stored = load(&store, WORKSET).unwrap().formation.unwrap();
        assert_eq!(stored, applied.formation);
        assert_eq!(stored.revision, 1);
        let sessions: Vec<_> = stored.agents.iter().map(|a| a.session.as_deref().unwrap()).collect();
        assert_eq!(sessions, ["t_aaaaaaaaaaaa", "t_bbbbbbbbbbbb", "t_cccccccccccc", "t_dddddddddddd"]);
    }

    #[test]
    fn a_failed_opening_keeps_what_was_opened_and_names_the_agent() {
        let store = database();
        let mut calls = 0;
        let applied = apply_opening(&store, WORKSET, &thread(), crowd(), 0, reachable, |agent| {
            calls += 1;
            if agent.id == "a_cccccccccccc" { Err("Pi did not start.".to_owned()) } else { Ok("t_eeeeeeeeeeee".to_owned()) }
        })
        .unwrap();
        assert_eq!(calls, 2, "the opening stops at the failure: the last agent is not tried");
        assert_eq!(applied.failed, Some(Failure { agent: "a_cccccccccccc".into(), reason: "Pi did not start.".into() }));
        let stored = load(&store, WORKSET).unwrap().formation.unwrap();
        assert_eq!(stored.revision, 1);
        let sessions: Vec<_> = stored.agents.iter().map(|a| a.session.as_deref()).collect();
        assert_eq!(sessions, [Some("t_aaaaaaaaaaaa"), Some("t_eeeeeeeeeeee"), None, None]);
        assert_eq!(stored, applied.formation);
    }

    #[test]
    fn an_agent_opened_once_is_not_opened_again_on_retry() {
        let store = database();
        let mut first = 0;
        let failed = apply_opening(&store, WORKSET, &thread(), crowd(), 0, reachable, |agent| {
            first += 1;
            if agent.id == "a_dddddddddddd" { Err("Codex did not start.".to_owned()) } else { Ok(format!("t_{}", agent.id.trim_start_matches("a_"))) }
        })
        .unwrap();
        assert_eq!((first, failed.failed.is_some()), (3, true));
        // The window answers with the formation it was given back; the sessions are in it.
        // The thread now holds the sessions that were opened.
        let mut now = thread();
        now.sessions.insert("t_bbbbbbbbbbbb".into(), "running".into());
        now.sessions.insert("t_cccccccccccc".into(), "running".into());
        let mut again = Vec::new();
        let retried = apply_opening(&store, WORKSET, &now, failed.formation, 1, reachable, |agent| {
            again.push(agent.id.clone());
            Ok("t_ffffffffffff".to_owned())
        })
        .unwrap();
        assert_eq!(again, ["a_dddddddddddd"], "only the agent that has none");
        assert_eq!(retried.failed, None);
        assert_eq!(retried.formation.revision, 2);
        assert!(retried.formation.agents.iter().all(|a| a.session.is_some()));
    }

    #[test]
    fn nothing_is_opened_while_the_service_is_unreachable() {
        let store = database();
        let refused = apply_opening(
            &store,
            WORKSET,
            &thread(),
            crowd(),
            0,
            || Err("The Mukade service is stopped.".to_owned()),
            |_| panic!("a session was opened with the service unreachable"),
        )
        .unwrap_err();
        assert_eq!(refused, "The Mukade service is stopped.");
        assert_eq!(load(&store, WORKSET).unwrap(), Record::default());
        // A team with a session for every agent opens nothing, so it never needs the service.
        let applied = apply_opening(
            &store,
            WORKSET,
            &thread(),
            two(),
            0,
            || Err("The Mukade service is stopped.".to_owned()),
            |_| panic!("nothing to open"),
        )
        .unwrap();
        assert_eq!(applied.formation.revision, 1);
    }

    #[test]
    fn nothing_is_opened_for_a_team_the_apply_would_refuse() {
        let store = database();
        let never = |_: &Agent| -> Result<String, String> { panic!("a session was opened for a refused team") };
        // An agent with neither session nor model.
        let mut bare = crowd();
        bare.agents[1].model = None;
        assert!(apply_opening(&store, WORKSET, &thread(), bare, 0, reachable, never).unwrap_err().contains("no session and no model"));
        // A model the Models panel no longer holds, named.
        let mut gone = crowd();
        gone.agents[1].model = Some(ModelRef { program: "pi".into(), connection: Some("home".into()), model: Some("qwen-9".into()) });
        assert!(apply_opening(&store, WORKSET, &thread(), gone, 0, reachable, never).unwrap_err().contains("qwen-9"));
        // A thread that is off, and a stale revision.
        let off = Thread { enabled: false, ..thread() };
        assert!(apply_opening(&store, WORKSET, &off, crowd(), 0, reachable, never).unwrap_err().contains("Turn it on"));
        assert!(apply_opening(&store, WORKSET, &thread(), crowd(), 5, reachable, never).unwrap_err().contains("revision 0"));
        // A session that left the thread.
        let mut moved = crowd();
        moved.agents[0].session = Some("t_zzzzzzzzzzzz".into());
        assert!(apply_opening(&store, WORKSET, &thread(), moved, 0, reachable, never).unwrap_err().contains("does not belong"));
        assert_eq!(load(&store, WORKSET).unwrap(), Record::default());
    }

    #[test]
    fn the_answer_lists_the_model_menu_with_the_own_sign_ins_first() {
        let got = answer(&Record::default(), &thread(), json!([]), &BTreeMap::new());
        let menu = got["models"].as_array().unwrap();
        assert_eq!(menu.len(), 4);
        assert_eq!(menu[0], json!({"program": "claude_code", "connection": null, "model": null, "label": "Claude Code \u{b7} its own sign-in and model"}));
    }

    fn linked() -> Formation {
        let mut f = two();
        f.revision = 4;
        f.agents[0].notes = "plans".into();
        f.links.push(Link { from: "a_aaaaaaaaaaaa".into(), to: "a_bbbbbbbbbbbb".into(), kind: Kind::Work, report: Report::Steps, words: "go".into() });
        f
    }

    fn id(n: usize) -> String {
        format!("tm_{}{}", "a".repeat(10), ["aa", "ab", "ac", "ad", "ae"][n])
    }

    #[test]
    fn a_template_this_build_cannot_read_survives_a_save_and_a_delete() {
        let store = database();
        let list = save_template(&store, "Review", &linked(), id(0)).unwrap();
        let mut odd = serde_json::to_value(&list[0]).unwrap();
        odd["id"] = json!("tm_zzzzzzzzzzzz");
        odd["fromANewerBuild"] = json!(true);
        let mut raw = store.team_templates().unwrap();
        raw.push(odd.clone());
        store.set_team_templates(&raw).unwrap();
        assert_eq!(templates(&store).unwrap().len(), 1, "not listed: this build cannot read it");
        save_template(&store, "Ship", &linked(), id(1)).unwrap();
        assert!(store.team_templates().unwrap().contains(&odd), "kept by a save");
        delete_template(&store, &list[0].id).unwrap();
        assert!(store.team_templates().unwrap().contains(&odd), "kept by a delete");
        assert_eq!(templates(&store).unwrap().len(), 1);
    }

    #[test]
    fn a_saved_template_is_listed_for_every_thread_and_keeps_no_session() {
        let store = database();
        assert!(templates(&store).unwrap().is_empty());
        let list = save_template(&store, "Review", &linked(), id(0)).unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].agents.iter().all(|a| a.session.is_none()));
        assert_eq!((list[0].agents[0].notes.as_str(), list[0].links.len()), ("plans", 1));
        // A thread's team does not touch it, and removing the team leaves it.
        apply(&store, WORKSET, &thread(), two(), 0).unwrap();
        store.set_thread_team(WORKSET, None).unwrap();
        assert_eq!(templates(&store).unwrap(), list);
        // The saved text holds no session and no revision.
        let kept = store.team_templates().unwrap();
        assert!(kept[0].get("revision").is_none());
        assert!(!kept[0].to_string().contains("t_aaaaaaaaaaaa"));
    }

    #[test]
    fn a_name_twice_a_loop_and_the_sixty_fifth_are_refused_and_change_nothing() {
        let store = database();
        save_template(&store, "Review", &linked(), id(0)).unwrap();
        assert_eq!(
            save_template(&store, "  review", &linked(), id(1)).unwrap_err(),
            "A template named Review already exists."
        );
        let mut circle = linked();
        circle.links.push(Link { from: "a_bbbbbbbbbbbb".into(), to: "a_aaaaaaaaaaaa".into(), kind: Kind::Work, report: Report::Done, words: String::new() });
        assert!(save_template(&store, "Circle", &circle, id(2)).is_err());
        assert!(save_template(&store, "Nothing", &formation(Vec::new()), id(2)).is_err());
        assert_eq!(templates(&store).unwrap().len(), 1);
        // 64 are kept; the next is refused.
        for i in 1..64 {
            let tail: String = [(b'a' + (i / 26) as u8) as char, (b'a' + (i % 26) as u8) as char].iter().collect();
            save_template(&store, &format!("Team {i}"), &linked(), format!("tm_{}{tail}", "b".repeat(10))).unwrap();
        }
        assert_eq!(save_template(&store, "One more", &linked(), id(3)).unwrap_err(), "A team keeps at most 64 templates. Delete one first.");
        assert_eq!(templates(&store).unwrap().len(), 64);
    }

    #[test]
    fn deleting_a_template_keeps_the_others_in_order_and_an_unknown_one_is_not_an_error() {
        let store = database();
        for (n, name) in ["One", "Two", "Three"].iter().enumerate() {
            save_template(&store, name, &linked(), id(n)).unwrap();
        }
        let left = delete_template(&store, &id(1)).unwrap();
        assert_eq!(left.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["One", "Three"]);
        assert_eq!(delete_template(&store, "tm_zzzzzzzzzzzz").unwrap().len(), 2);
        // The freed name can be used again.
        assert_eq!(save_template(&store, "Two", &linked(), id(1)).unwrap().len(), 3);
    }

    #[test]
    fn a_made_up_id_is_a_valid_one() {
        let first = new_template_id();
        assert_ne!(first, new_template_id());
        assert!(save_template(&database(), "Made", &linked(), first).is_ok());
    }
}
