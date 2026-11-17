//! The formation of a thread: the agents the user places on the team map and the arrows between
//! them. This module alone computes roles, checks and the sentences sent to sessions; the window
//! asks it and never works them out again (spec `2026-11-17-thread-team-design.md`).
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_AGENTS: usize = 24;
pub const MAX_LINKS: usize = 64;
pub const MAX_NAME: usize = 40;
pub const MAX_WORDS: usize = 120;
pub const MAX_NOTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Formation {
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub agents: Vec<Agent>,
    #[serde(default)]
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Agent {
    /// `a_` and 12 of `[a-z2-7]`, made by the window.
    pub id: String,
    /// A session of the thread (`t_…`), or none yet.
    #[serde(default)]
    pub session: Option<String>,
    /// What an agent without a session is opened with.
    #[serde(default)]
    pub model: Option<ModelRef>,
    pub name: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub share: Share,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
}

/// A choice of the model menu. `connection` and `model` absent: the program's own sign-in and model.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelRef {
    /// `claude_code`, `codex`, `opencode` or `pi`.
    pub program: String,
    #[serde(default)]
    pub connection: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

/// How an agent that gives work to two or more hands it out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Share {
    #[default]
    Split,
    Same,
    Free,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Link {
    pub from: String,
    pub to: String,
    pub kind: Kind,
    #[serde(default)]
    pub report: Report,
    #[serde(default)]
    pub words: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Work,
    Observes,
}

/// How the receiver of work reports; meaningful for `Kind::Work` only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Report {
    #[default]
    Done,
    Steps,
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    Lead,
    Worker,
    LeadWorker,
    Observer,
    Peer,
}

/// The part of a role Mukade enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MukadeRole {
    Peer,
    Lead,
    Worker,
    Observer,
}

impl MukadeRole {
    pub fn as_str(self) -> &'static str {
        match self {
            MukadeRole::Peer => "peer",
            MukadeRole::Lead => "lead",
            MukadeRole::Worker => "worker",
            MukadeRole::Observer => "observer",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub level: Level,
    pub code: &'static str,
    pub text: String,
    pub agents: Vec<String>,
    pub links: Vec<usize>,
}

fn has_control(text: &str) -> bool {
    text.chars().any(char::is_control)
}

fn valid_id(id: &str) -> bool {
    id.strip_prefix("a_").is_some_and(|tail| {
        tail.len() == 12 && tail.bytes().all(|b| matches!(b, b'a'..=b'z' | b'2'..=b'7'))
    })
}

/// What the formation may hold before it is checked for sense: sizes, characters, ends of arrows.
/// `@` is refused everywhere a person types: Mukade turns `@name` in a message into a recipient.
pub fn validate(f: &Formation) -> Result<(), String> {
    if f.agents.len() > MAX_AGENTS {
        return Err(format!("A team holds at most {MAX_AGENTS} agents."));
    }
    if f.links.len() > MAX_LINKS {
        return Err(format!("A team holds at most {MAX_LINKS} relations."));
    }
    let mut ids = BTreeSet::new();
    for agent in &f.agents {
        if !valid_id(&agent.id) {
            return Err("An agent has an invalid id.".into());
        }
        if !ids.insert(agent.id.as_str()) {
            return Err("Two agents have the same id.".into());
        }
        if agent.name.trim().is_empty()
            || agent.name.chars().count() > MAX_NAME
            || has_control(&agent.name)
        {
            return Err(format!("An agent's name takes 1 to {MAX_NAME} characters, on one line."));
        }
        if agent.name.contains('@') {
            return Err("An agent's name cannot hold @.".into());
        }
        if agent.notes.len() > MAX_NOTES {
            return Err(format!("An agent's notes take at most {MAX_NOTES} bytes."));
        }
        if agent.notes.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
            return Err("An agent's notes cannot hold control characters.".into());
        }
        if agent.notes.contains('@') {
            return Err("An agent's notes cannot hold @.".into());
        }
        if agent.session.as_deref().is_some_and(str::is_empty) {
            return Err("An agent's session cannot be empty.".into());
        }
    }
    let mut pairs = BTreeSet::new();
    for link in &f.links {
        if !ids.contains(link.from.as_str()) || !ids.contains(link.to.as_str()) {
            return Err("A relation joins an agent that is not on the map.".into());
        }
        if link.from == link.to {
            return Err("A relation cannot join an agent to itself.".into());
        }
        let pair = if link.from < link.to {
            (&link.from, &link.to)
        } else {
            (&link.to, &link.from)
        };
        if !pairs.insert(pair) {
            return Err("Two agents have at most one relation between them.".into());
        }
        if link.words.chars().count() > MAX_WORDS || has_control(&link.words) {
            return Err(format!("The words of a relation take at most {MAX_WORDS} characters, on one line."));
        }
        if link.words.contains('@') {
            return Err("The words of a relation cannot hold @.".into());
        }
    }
    Ok(())
}

/// What an agent's arrows add up to.
#[derive(Default)]
struct Ties {
    gives: usize,
    gives_strict: usize,
    takes: usize,
    takes_strict: usize,
    /// Arrows of *observes* it draws.
    observes: usize,
}

fn ties(f: &Formation, agent: &str) -> Ties {
    let mut t = Ties::default();
    for link in &f.links {
        match link.kind {
            Kind::Work if link.from == agent => {
                t.gives += 1;
                t.gives_strict += usize::from(link.report == Report::Strict);
            }
            Kind::Work if link.to == agent => {
                t.takes += 1;
                t.takes_strict += usize::from(link.report == Report::Strict);
            }
            Kind::Observes if link.from == agent => t.observes += 1,
            _ => {}
        }
    }
    t
}

pub fn role(f: &Formation, agent: &str) -> Role {
    let t = ties(f, agent);
    if t.observes > 0 {
        Role::Observer
    } else if t.gives > 0 && t.takes > 0 {
        Role::LeadWorker
    } else if t.gives > 0 {
        Role::Lead
    } else if t.takes > 0 {
        Role::Worker
    } else {
        Role::Peer
    }
}

/// The part of the role Mukade enforces: an observer and a worker cannot send messages, and a
/// worker answers a lead's instruction with done or blocked, so whoever gives "only done /
/// blocked" work must be a `lead`.
pub fn mukade_role(f: &Formation, agent: &str) -> MukadeRole {
    let t = ties(f, agent);
    if t.observes > 0 {
        MukadeRole::Observer
    } else if t.gives_strict > 0 {
        MukadeRole::Lead
    } else if t.takes_strict > 0 && t.gives == 0 {
        MukadeRole::Worker
    } else {
        MukadeRole::Peer
    }
}

fn names(f: &Formation, ids: &[String]) -> String {
    let named: Vec<&str> = ids
        .iter()
        .filter_map(|id| f.agents.iter().find(|a| &a.id == id))
        .map(|a| a.name.as_str())
        .collect();
    match named.split_last() {
        None => String::new(),
        Some((only, [])) => (*only).to_string(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

fn name_of<'a>(f: &'a Formation, id: &'a str) -> &'a str {
    f.agents.iter().find(|a| a.id == id).map_or(id, |a| a.name.as_str())
}

/// The groups of agents that give each other work in a circle, each with the arrows inside it.
fn loops(f: &Formation) -> Vec<(Vec<String>, Vec<usize>)> {
    let index = |id: &str| f.agents.iter().position(|a| a.id == id);
    let n = f.agents.len();
    let mut reach = vec![vec![false; n]; n];
    for link in f.links.iter().filter(|l| l.kind == Kind::Work) {
        if let (Some(a), Some(b)) = (index(&link.from), index(&link.to)) {
            reach[a][b] = true;
        }
    }
    // Transitive closure: at most 24 agents.
    for k in 0..n {
        for i in 0..n {
            if reach[i][k] {
                for j in 0..n {
                    if reach[k][j] {
                        reach[i][j] = true;
                    }
                }
            }
        }
    }
    let mut seen = vec![false; n];
    let mut found = Vec::new();
    for i in 0..n {
        if seen[i] || !reach[i][i] {
            continue;
        }
        let group: Vec<usize> = (0..n).filter(|&j| j == i || (reach[i][j] && reach[j][i])).collect();
        for &j in &group {
            seen[j] = true;
        }
        let links = f
            .links
            .iter()
            .enumerate()
            .filter(|(_, l)| {
                l.kind == Kind::Work
                    && index(&l.from).is_some_and(|a| group.contains(&a))
                    && index(&l.to).is_some_and(|b| group.contains(&b))
            })
            .map(|(k, _)| k)
            .collect();
        found.push((group.iter().map(|&j| f.agents[j].id.clone()).collect(), links));
    }
    found
}

/// What cannot work, or may not, with the arrows as drawn. `stopped`: agents whose session
/// exists but is not running.
pub fn check(f: &Formation, stopped: &BTreeSet<String>) -> Vec<Issue> {
    let mut issues = Vec::new();
    for (agents, links) in loops(f) {
        issues.push(Issue {
            level: Level::Error,
            code: "loop",
            text: format!(
                "{} give each other work in a circle. Each would wait for the next and none would finish: remove one of the arrows.",
                names(f, &agents)
            ),
            agents,
            links,
        });
    }
    let touching = |id: &str| -> Vec<usize> {
        f.links
            .iter()
            .enumerate()
            .filter(|(_, l)| (l.from == id || l.to == id) && !(l.to == id && l.kind == Kind::Observes))
            .map(|(k, _)| k)
            .collect()
    };
    let incoming = |id: &str| -> Vec<usize> {
        f.links
            .iter()
            .enumerate()
            .filter(|(_, l)| l.to == id && l.kind == Kind::Work)
            .map(|(k, _)| k)
            .collect()
    };
    for agent in &f.agents {
        let t = ties(f, &agent.id);
        if t.takes_strict > 0 && (t.gives > 0 || t.observes > 0 || t.takes > t.takes_strict) {
            issues.push(Issue {
                level: Level::Error,
                code: "only_executes",
                text: format!(
                    "{} only executes work and answers done or blocked, so it can hold no other relation, except being observed.",
                    agent.name
                ),
                agents: vec![agent.id.clone()],
                links: touching(&agent.id),
            });
        }
    }
    for agent in &f.agents {
        let t = ties(f, &agent.id);
        if t.observes > 0 && (t.gives > 0 || t.takes > 0) {
            issues.push(Issue {
                level: Level::Error,
                code: "observer_writes",
                text: format!(
                    "{} observes, so it cannot give or take work: an observer does not write.",
                    agent.name
                ),
                agents: vec![agent.id.clone()],
                links: touching(&agent.id),
            });
        }
    }
    for agent in &f.agents {
        let t = ties(f, &agent.id);
        if t.takes_strict >= 2 {
            issues.push(Issue {
                level: Level::Error,
                code: "two_givers",
                text: format!(
                    "Two agents give {} work it answers only with done or blocked, and it answers one lead.",
                    agent.name
                ),
                agents: vec![agent.id.clone()],
                links: incoming(&agent.id),
            });
        } else if t.takes >= 2 && t.takes_strict == 0 {
            issues.push(Issue {
                level: Level::Warning,
                code: "two_givers",
                text: format!("Two agents give work to {}: their instructions may conflict.", agent.name),
                agents: vec![agent.id.clone()],
                links: incoming(&agent.id),
            });
        }
    }
    for agent in &f.agents {
        if agent.session.is_none() && agent.model.is_none() {
            issues.push(Issue {
                level: Level::Error,
                code: "no_model",
                text: format!("{} has no session and no model, so it cannot be opened.", agent.name),
                agents: vec![agent.id.clone()],
                links: Vec::new(),
            });
        }
    }
    for (k, link) in f.links.iter().enumerate() {
        if link.kind == Kind::Work && stopped.contains(&link.to) {
            issues.push(Issue {
                level: Level::Warning,
                code: "stopped",
                text: format!(
                    "{} has a stopped session: the work waits until it is resumed.",
                    name_of(f, &link.to)
                ),
                agents: vec![link.to.clone()],
                links: vec![k],
            });
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use Kind::{Observes, Work};
    use Report::{Done, Steps, Strict};

    fn agent(id: &str) -> Agent {
        Agent {
            id: id.into(),
            session: Some("t_session".into()),
            model: None,
            name: id.to_uppercase(),
            notes: String::new(),
            share: Share::Split,
            x: 0,
            y: 0,
        }
    }

    fn team(ids: &[&str], links: &[(&str, &str, Kind, Report)]) -> Formation {
        Formation {
            revision: 1,
            agents: ids.iter().map(|id| agent(id)).collect(),
            links: links
                .iter()
                .map(|(from, to, kind, report)| Link {
                    from: (*from).into(),
                    to: (*to).into(),
                    kind: *kind,
                    report: *report,
                    words: String::new(),
                })
                .collect(),
        }
    }

    fn none() -> BTreeSet<String> {
        BTreeSet::new()
    }

    fn codes(f: &Formation) -> Vec<(Level, &'static str)> {
        check(f, &none()).iter().map(|i| (i.level, i.code)).collect()
    }

    /// A formation that passes `validate`: real ids, `n` agents named by letter, no arrows.
    fn proper(n: usize) -> Formation {
        let id = |i: usize| {
            let tail: String = [(b'a' + (i / 26) as u8) as char, (b'a' + (i % 26) as u8) as char].iter().collect();
            format!("a_{}{tail}", "a".repeat(10))
        };
        Formation {
            revision: 0,
            agents: (0..n)
                .map(|i| Agent { name: format!("Agent {i}"), ..agent(&id(i)) })
                .collect(),
            links: Vec::new(),
        }
    }

    #[test]
    fn roles_follow_the_arrows() {
        let f = team(
            &["a", "b", "c", "d"],
            &[("a", "b", Work, Done), ("b", "c", Work, Strict), ("d", "a", Observes, Done)],
        );
        assert_eq!(role(&f, "a"), Role::Lead);
        assert_eq!(mukade_role(&f, "a"), MukadeRole::Peer);
        assert_eq!(role(&f, "b"), Role::LeadWorker);
        assert_eq!(mukade_role(&f, "b"), MukadeRole::Lead);
        assert_eq!(role(&f, "c"), Role::Worker);
        assert_eq!(mukade_role(&f, "c"), MukadeRole::Worker);
        assert_eq!(role(&f, "d"), Role::Observer);
        assert_eq!(mukade_role(&f, "d"), MukadeRole::Observer);
    }

    #[test]
    fn an_agent_without_arrows_and_one_taking_plain_work_are_peers_to_mukade() {
        let f = team(&["a", "b", "c"], &[("a", "b", Work, Steps)]);
        assert_eq!((role(&f, "c"), mukade_role(&f, "c")), (Role::Peer, MukadeRole::Peer));
        assert_eq!((role(&f, "b"), mukade_role(&f, "b")), (Role::Worker, MukadeRole::Peer));
    }

    #[test]
    fn a_loop_of_work_is_an_error_naming_its_arrows() {
        let f = team(
            &["a", "b", "c"],
            &[("a", "b", Work, Done), ("b", "c", Work, Done), ("c", "a", Work, Done)],
        );
        let issue = &check(&f, &none())[0];
        assert_eq!((issue.level, issue.code), (Level::Error, "loop"));
        assert_eq!(issue.links, vec![0, 1, 2]);
        assert_eq!(issue.agents, vec!["a", "b", "c"]);
    }

    #[test]
    fn a_chain_and_a_diamond_are_not_loops() {
        let f = team(
            &["a", "b", "c", "d"],
            &[("a", "b", Work, Done), ("a", "c", Work, Done), ("b", "d", Work, Done), ("c", "d", Work, Done)],
        );
        assert!(!codes(&f).iter().any(|(_, code)| *code == "loop"));
    }

    #[test]
    fn who_only_executes_holds_nothing_else_but_being_observed() {
        let f = team(&["a", "b", "c"], &[("a", "b", Work, Strict), ("b", "c", Work, Done)]);
        assert!(codes(&f).contains(&(Level::Error, "only_executes")));
        let f = team(&["a", "b", "c"], &[("a", "b", Work, Strict), ("c", "b", Observes, Done)]);
        assert!(!codes(&f).iter().any(|(level, _)| *level == Level::Error), "{:?}", codes(&f));
        let f = team(&["a", "b", "c"], &[("a", "b", Work, Strict), ("c", "b", Work, Done)]);
        assert!(codes(&f).contains(&(Level::Error, "only_executes")), "another giver is another relation");
    }

    #[test]
    fn an_observer_does_not_work() {
        let f = team(&["a", "b", "c"], &[("a", "b", Observes, Done), ("c", "a", Work, Done)]);
        assert!(codes(&f).contains(&(Level::Error, "observer_writes")));
        let f = team(&["a", "b", "c"], &[("a", "b", Observes, Done), ("a", "c", Work, Done)]);
        assert!(codes(&f).contains(&(Level::Error, "observer_writes")));
        let f = team(&["a", "b", "c"], &[("a", "b", Observes, Done), ("c", "b", Observes, Done)]);
        assert!(check(&f, &none()).is_empty(), "two observers of one agent");
        let f = team(&["a", "b", "c"], &[("a", "b", Observes, Done), ("b", "c", Work, Done)]);
        assert!(!codes(&f).contains(&(Level::Error, "observer_writes")), "being observed is allowed");
    }

    #[test]
    fn two_strict_givers_is_an_error_two_plain_givers_a_warning() {
        let f = team(&["a", "b", "c"], &[("a", "c", Work, Strict), ("b", "c", Work, Strict)]);
        assert!(codes(&f).contains(&(Level::Error, "two_givers")));
        let f = team(&["a", "b", "c"], &[("a", "c", Work, Done), ("b", "c", Work, Steps)]);
        assert_eq!(codes(&f), vec![(Level::Warning, "two_givers")]);
    }

    #[test]
    fn an_agent_with_neither_session_nor_model_cannot_be_opened() {
        let mut f = team(&["a", "b"], &[]);
        f.agents[0].session = None;
        let issues = check(&f, &none());
        assert_eq!((issues[0].level, issues[0].code), (Level::Error, "no_model"));
        assert_eq!(issues[0].agents, vec!["a"]);
        f.agents[0].model = Some(ModelRef { program: "pi".into(), connection: None, model: None });
        assert!(check(&f, &none()).is_empty());
    }

    #[test]
    fn work_for_a_stopped_session_is_a_warning() {
        let f = team(&["a", "b"], &[("a", "b", Work, Done)]);
        let stopped: BTreeSet<String> = ["b".to_string()].into();
        let issues = check(&f, &stopped);
        assert_eq!(issues.len(), 1);
        assert_eq!((issues[0].level, issues[0].code), (Level::Warning, "stopped"));
        assert_eq!(issues[0].links, vec![0]);
        let stopped: BTreeSet<String> = ["a".to_string()].into();
        assert!(check(&f, &stopped).is_empty(), "the giver being stopped is not the receiver's wait");
    }

    #[test]
    fn a_sound_formation_validates() {
        let f = proper(3);
        assert_eq!(validate(&f), Ok(()));
        assert_eq!(validate(&Formation { revision: 0, agents: vec![], links: vec![] }), Ok(()));
    }

    #[test]
    fn validate_refuses_at_signs_long_words_blank_names_unknown_ends_and_two_arrows_between_two_agents() {
        let link = |f: &Formation, a: usize, b: usize| Link {
            from: f.agents[a].id.clone(),
            to: f.agents[b].id.clone(),
            kind: Work,
            report: Done,
            words: String::new(),
        };
        let refused = |f: &Formation, why: &str| {
            let error = validate(f).expect_err(why);
            assert!(!error.is_empty(), "{why}");
        };
        let mut f = proper(3);
        f.links.push(Link { words: "tell @all".into(), ..link(&f, 0, 1) });
        refused(&f, "@all in words");
        let mut f = proper(3);
        f.links.push(Link { words: "x".repeat(121), ..link(&f, 0, 1) });
        refused(&f, "121 characters of words");
        let mut f = proper(3);
        f.links.push(Link { words: "é".repeat(120), ..link(&f, 0, 1) });
        assert_eq!(validate(&f), Ok(()), "120 characters, not bytes");
        let mut f = proper(3);
        f.agents[1].name = "   ".into();
        refused(&f, "a name of spaces");
        let mut f = proper(3);
        f.agents[1].name = "x".repeat(41);
        refused(&f, "a name of 41 characters");
        let mut f = proper(3);
        f.agents[1].name = "@claude-1".into();
        refused(&f, "@ in a name");
        let mut f = proper(3);
        f.agents[1].notes = "ask @codex-1".into();
        refused(&f, "@ in notes");
        let mut f = proper(3);
        f.agents[1].notes = "x".repeat(1025);
        refused(&f, "notes over 1024 bytes");
        let mut f = proper(3);
        f.links.push(Link { to: "a_missing".into(), ..link(&f, 0, 1) });
        refused(&f, "a link to a missing agent");
        let mut f = proper(3);
        f.links.push(link(&f, 0, 1));
        f.links.push(link(&f, 1, 0));
        refused(&f, "a→b and b→a together");
        let mut f = proper(3);
        f.links.push(link(&f, 0, 1));
        f.links.push(Link { kind: Observes, ..link(&f, 0, 1) });
        refused(&f, "two arrows between the same two agents");
        let mut f = proper(3);
        f.links.push(link(&f, 0, 0));
        refused(&f, "an arrow from an agent to itself");
        refused(&proper(25), "25 agents");
        let mut f = proper(24);
        assert_eq!(validate(&f), Ok(()));
        f.links = (0..65).map(|i| link(&f, i % 24, (i + 1 + i / 24) % 24)).collect();
        refused(&f, "65 links");
        let mut f = proper(3);
        f.agents[2].id = f.agents[0].id.clone();
        refused(&f, "a repeated id");
        let mut f = proper(3);
        f.agents[0].id = "claude-1".into();
        refused(&f, "an id the window did not make");
        let mut f = proper(3);
        f.agents[0].name = "line\nbreak".into();
        refused(&f, "a control character in a name");
    }
}
