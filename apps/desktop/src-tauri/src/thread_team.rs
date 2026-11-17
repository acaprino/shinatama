//! The window's commands for a thread's team map. The storing logic is plain functions over
//! the database and the facts of the thread, so they are tested without a window; the commands
//! only gather those facts. Roles, checks and sentences are `shinatama_engine::team`'s: nothing
//! here computes them.
use crate::storage::{self, WorkspaceServices};
use crate::thread_chat;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use shinatama_engine::team::{self, Formation, Level, Template, Told};
use shinatama_state::database::Database;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use tauri::{State, WebviewWindow};

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
}

impl Thread {
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
    if !thread.enabled {
        return Err("This thread's sessions do not talk to each other. Turn it on first.".into());
    }
    team::validate(&formation)?;
    if formation.agents.iter().filter_map(|agent| agent.session.as_deref()).any(|session| !thread.sessions.contains_key(session)) {
        return Err("An agent's session does not belong to this thread.".into());
    }
    exclusive(|| {
        let mut record = load(store, workset)?;
        let stored = record.formation.as_ref().map_or(0, |f| f.revision);
        if stored != expected {
            return Err(format!("The team was changed elsewhere (revision {stored}). Reload it."));
        }
        if let Some(error) = team::check(&formation, &thread.stopped(&formation)).into_iter().find(|issue| issue.level == Level::Error) {
            return Err(error.text);
        }
        formation.revision = expected.checked_add(1).ok_or("The team's revision is exhausted.")?;
        record.before = record.formation.take();
        record.formation = Some(formation.clone());
        save(store, workset, &record)?;
        Ok(formation)
    })
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
    let evaluation = record.formation.as_ref().map(|f| team::evaluate(f, &thread.stopped(f)));
    json!({
        "enabled": thread.enabled,
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
    Ok(Thread { enabled, sessions })
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
        Ok(team::evaluate(&request.formation, &thread.stopped(&request.formation)))
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
/// Store the draft as the thread's team and wake the announcer, which tells the sessions.
#[tauri::command]
pub async fn team_apply(
    window: WebviewWindow,
    services: State<'_, Arc<storage::Services>>,
    request: ApplyRequest,
) -> Result<Value, String> {
    crate::local(&window)?;
    thread_chat::workset(&request.workset_id)?;
    let stored = thread_chat::blocking(services.inner(), move |state| {
        let thread = thread_of(state, &request.workset_id)?;
        apply(&state.store, &request.workset_id, &thread, request.formation, request.expected_revision)
    })
    .await?;
    crate::team_announcer::wake();
    Ok(json!({"formation": stored}))
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
/// hold when two windows save at once. A stored template that cannot be read is left out.
fn edit_templates(store: &Database, change: impl FnOnce(&mut Vec<Template>) -> Result<(), String>) -> Result<Vec<Template>, String> {
    let stored = store
        .edit_team_templates(|values| {
            let mut list: Vec<Template> = values.drain(..).filter_map(|value| serde_json::from_value(value).ok()).collect();
            change(&mut list)?;
            for template in &list {
                let value = serde_json::to_value(template).map_err(|_| "The template could not be stored.".to_owned())?;
                if value.to_string().len() > MAX_RECORD {
                    return Err("This team is too large to be kept as a template.".to_owned());
                }
                values.push(value);
            }
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
    use shinatama_engine::team::{Agent, Kind, Link, Report, Share};
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
    fn a_record_too_big_with_its_previous_formation_is_kept_without_it() {
        let store = database();
        let big = |revision: u64| {
            let mut f = formation((0..team::MAX_AGENTS).map(|k| {
                let mut a = agent(&format!("a_{}", "abcdefghijklmnopqrstuvwx".chars().nth(k).unwrap().to_string().repeat(12)), Some("t_aaaaaaaaaaaa"));
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
        apply(&store, WORKSET, &thread(), first, 0).unwrap();
        apply(&store, WORKSET, &thread(), big(0), 1).unwrap();
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
