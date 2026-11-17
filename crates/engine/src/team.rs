//! The formation of a thread: the agents the user places on the team map and the arrows between
//! them. This module alone computes roles, checks and the sentences sent to sessions; the window
//! asks it and never works them out again (spec `2026-11-17-thread-team-design.md`).
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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

// ---- What each session is told ----

/// What one message to a session may hold (Mukade refuses more than it can carry; two kilobytes
/// is what a model reads without losing the rest of its turn).
pub const MESSAGE_LIMIT: usize = 2048;

fn report_words(report: Report) -> &'static str {
    match report {
        Report::Done => "when done",
        Report::Steps => "at every step",
        Report::Strict => "only done or blocked",
    }
}

fn role_label(role: Role) -> &'static str {
    match role {
        Role::Lead => "Lead",
        Role::Worker => "Worker",
        Role::LeadWorker => "Lead and worker",
        Role::Observer => "Observer",
        Role::Peer => "Peer",
    }
}

fn user_adds(link: &Link, max_words: usize) -> String {
    if link.words.is_empty() || max_words == 0 {
        String::new()
    } else {
        format!(" The user adds: \u{201c}{}\u{201d}.", cut(&link.words, max_words))
    }
}

/// The sentence for one end of a relation. `giver`: the end the arrow starts from.
pub fn told(f: &Formation, link: usize, giver: bool) -> String {
    told_with(f, link, giver, usize::MAX)
}

/// `told` with the user's words cut to `max_words` bytes (none at 0).
fn told_with(f: &Formation, link: usize, giver: bool, max_words: usize) -> String {
    let Some(l) = f.links.get(link) else {
        return String::new();
    };
    let (a, b) = (name_of(f, &l.from), name_of(f, &l.to));
    let body = match (l.kind, l.report, giver) {
        (Kind::Work, Report::Done, true) => {
            format!("You give work to {b}; {b} reports back to you when it is done.")
        }
        (Kind::Work, Report::Done, false) => {
            format!("You wait for work from {a}, do it, and report back to {a} when it is done.")
        }
        (Kind::Work, Report::Steps, true) => {
            format!("You give work to {b}; {b} tells you what it is doing at every step.")
        }
        (Kind::Work, Report::Steps, false) => format!(
            "You wait for work from {a}, do it, and tell {a} what you are doing at every step."
        ),
        (Kind::Work, Report::Strict, true) => {
            format!("You give work to {b}; {b} answers only done or blocked.")
        }
        (Kind::Work, Report::Strict, false) => format!(
            "You wait for instructions from {a}, execute them, and answer only done or blocked."
        ),
        (Kind::Observes, _, true) => format!(
            "You observe {b}: follow what it does, to explain it to the user when asked. Do not write to it."
        ),
        (Kind::Observes, _, false) => format!("{a} observes you and may read what you do."),
    };
    body + &user_adds(l, max_words)
}

/// How an agent that gives work to two or more hands it out, said to it.
fn hand_out(f: &Formation, agent: &str) -> Option<String> {
    let receivers: Vec<String> = f
        .links
        .iter()
        .filter(|l| l.kind == Kind::Work && l.from == agent)
        .map(|l| l.to.clone())
        .collect();
    if receivers.len() < 2 {
        return None;
    }
    let share = f.agents.iter().find(|a| a.id == agent)?.share;
    let how = match share {
        Share::Split => "split it among them, a different part to each, and put their results together.",
        Share::Same => "give the same task to each, then compare their results.",
        Share::Free => "give each task to whichever of them is free.",
    };
    Some(format!("You give work to {}: {how}", names(f, &receivers)))
}

/// The most of `text` that fits in `max` bytes, cut at a character and marked.
fn cut(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max.saturating_sub('\u{2026}'.len_utf8());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\u{2026}", &text[..end])
}

/// A titled list that fits `budget` bytes, saying how many entries it left out.
fn listed(title: &str, lines: &[String], budget: usize) -> Option<String> {
    let total = lines.len();
    let mut out = title.to_string();
    let mut shown = 0;
    for (i, line) in lines.iter().enumerate() {
        let item = format!("\n- {line}");
        let left = total - i - 1;
        let reserve = if left > 0 { format!("\n\u{2026} and {left} more").len() } else { 0 };
        if out.len() + item.len() + reserve > budget {
            break;
        }
        out.push_str(&item);
        shown += 1;
    }
    if shown < total {
        let tail = format!("\n\u{2026} and {} more", total - shown);
        if out.len() + tail.len() > budget {
            return None;
        }
        out.push_str(&tail);
    }
    (out.len() <= budget).then_some(out)
}

fn relation_line(f: &Formation, l: &Link) -> String {
    let (a, b) = (name_of(f, &l.from), name_of(f, &l.to));
    match l.kind {
        Kind::Work => format!("{a} gives work to {b} ({})", report_words(l.report)),
        Kind::Observes => format!("{a} observes {b}"),
    }
}

/// The message that tells one agent its place: addressed to its member, so that it wakes a
/// listening session. `members`: the Mukade member name by agent id, for those in the room. It
/// starts with `TEAM revision <n>` on its own line, which is how a send that timed out is found
/// again in the room's ledger. The agent's own sentences are never cut; notes and the two lists of
/// the formation give way to the size limit.
pub fn personal(f: &Formation, agent: &str, members: &BTreeMap<String, String>) -> String {
    let Some(me) = f.agents.iter().find(|a| a.id == agent) else {
        return String::new();
    };
    let in_room = |id: &str| members.get(id).cloned().unwrap_or_else(|| "not in the room yet".into());
    let view = role(f, agent);
    let mut out = format!(
        "TEAM revision {}\nYou are {} in this thread's team; in the room you are the member {}. Your place: {}.",
        f.revision,
        me.name,
        in_room(agent),
        role_label(view)
    );
    let own_with = |max_words: usize| -> Vec<String> {
        f.links
            .iter()
            .enumerate()
            .filter(|(_, l)| l.from == agent || l.to == agent)
            .map(|(k, l)| told_with(f, k, l.from == agent, max_words))
            .collect()
    };
    let hand = hand_out(f, agent);
    let fixed = out.len() + hand.as_ref().map_or(0, |h| h.len() + 1);
    let size = |own: &[String]| fixed + own.iter().map(|s| s.len() + 3).sum::<usize>();
    // shortcut: when the agent's own sentences alone pass the limit, the user's words in them
    // are cut (every sentence keeps its meaning); upgrade by sending a second message.
    let mut own = own_with(usize::MAX);
    for max_words in [96, 72, 48, 24, 0] {
        if size(&own) <= MESSAGE_LIMIT {
            break;
        }
        own = own_with(max_words);
    }
    if own.is_empty() {
        out.push_str("\nYou have no relation: you work as an equal with everyone.");
    }
    for sentence in own {
        out.push_str("\n- ");
        out.push_str(&sentence);
    }
    if let Some(sentence) = hand {
        out.push('\n');
        out.push_str(&sentence);
    }
    let left = |out: &str| MESSAGE_LIMIT.saturating_sub(out.len());
    if !me.notes.trim().is_empty() {
        let label = "\nNotes from the user: ";
        let room = left(&out).saturating_sub(label.len()) / 2;
        if room > 8 {
            out.push_str(label);
            out.push_str(&cut(me.notes.trim(), room));
        }
    }
    let team: Vec<String> = f
        .agents
        .iter()
        .map(|a| format!("{}: {}, {}", a.name, in_room(&a.id), role_label(role(f, &a.id))))
        .collect();
    let relations: Vec<String> = f.links.iter().map(|l| relation_line(f, l)).collect();
    // The members first, then what is left for the arrows.
    if let Some(list) = listed("\nThe team:", &team, left(&out) / 2) {
        out.push_str(&list);
    }
    if let Some(list) = listed("\nRelations:", &relations, left(&out)) {
        out.push_str(&list);
    }
    out
}

/// What a change of formation concerns.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    /// The agents to tell personally: those whose own settings or whose relations changed, in
    /// the order of the map.
    pub touched: Vec<String>,
    /// Agents that were on the map and are not any more.
    pub removed: Vec<Agent>,
    /// One line per change, for the notice the other members get.
    pub lines: Vec<String>,
}

fn pair_of(l: &Link) -> (&str, &str) {
    if l.from < l.to { (&l.from, &l.to) } else { (&l.to, &l.from) }
}

fn said(f: &Formation, l: &Link) -> String {
    match l.kind {
        Kind::Work => format!(
            "{} now gives work to {} ({})",
            name_of(f, &l.from),
            name_of(f, &l.to),
            report_words(l.report)
        ),
        Kind::Observes => format!("{} now observes {}", name_of(f, &l.from), name_of(f, &l.to)),
    }
}

/// Places (`x`, `y`) are not a change.
pub fn changes(before: Option<&Formation>, after: &Formation) -> Changes {
    let Some(before) = before else {
        return Changes {
            touched: after.agents.iter().map(|a| a.id.clone()).collect(),
            ..Changes::default()
        };
    };
    let mut touched = BTreeSet::new();
    let mut lines = Vec::new();
    let mut renamed = BTreeSet::new();
    for agent in &after.agents {
        match before.agents.iter().find(|a| a.id == agent.id) {
            None => {
                touched.insert(agent.id.clone());
                lines.push(format!("{} joined the team.", agent.name));
            }
            Some(old) => {
                let mut changed = false;
                if old.name != agent.name {
                    changed = true;
                    renamed.insert(agent.id.clone());
                    lines.push(format!("{} is now called {}.", old.name, agent.name));
                }
                if old.notes != agent.notes {
                    changed = true;
                    lines.push(format!("{}'s notes changed.", agent.name));
                }
                if old.model != agent.model {
                    changed = true;
                    lines.push(format!("{} now uses another model.", agent.name));
                }
                if old.share != agent.share {
                    changed = true;
                    lines.push(format!("{} now hands work out another way.", agent.name));
                }
                if changed {
                    touched.insert(agent.id.clone());
                }
            }
        }
    }
    let removed: Vec<Agent> = before
        .agents
        .iter()
        .filter(|a| !after.agents.iter().any(|b| b.id == a.id))
        .cloned()
        .collect();
    for gone in &removed {
        lines.push(format!("{} left the team.", gone.name));
    }
    let survives = |id: &str| after.agents.iter().any(|a| a.id == id);
    for l in &after.links {
        let old = before.links.iter().find(|o| pair_of(o) == pair_of(l));
        let renamed_end = renamed.contains(&l.from) || renamed.contains(&l.to);
        let line = match old {
            None => Some(format!("{}.", said(after, l))),
            Some(o) if o == l => None,
            Some(o) if o.from == l.from && o.to == l.to && o.kind == l.kind && o.report == l.report => Some(format!(
                "The words of the relation between {} and {} changed.",
                name_of(after, &l.from),
                name_of(after, &l.to)
            )),
            Some(_) => Some(format!(
                "The relation between {} and {} changed: {}.",
                name_of(after, &l.from),
                name_of(after, &l.to),
                said(after, l)
            )),
        };
        if line.is_some() || renamed_end {
            touched.insert(l.from.clone());
            touched.insert(l.to.clone());
        }
        lines.extend(line);
    }
    for o in &before.links {
        if after.links.iter().any(|l| pair_of(l) == pair_of(o)) {
            continue;
        }
        for end in [&o.from, &o.to] {
            if survives(end) {
                touched.insert(end.clone());
            }
        }
        if survives(&o.from) && survives(&o.to) {
            lines.push(match o.kind {
                Kind::Work => format!("{} no longer gives work to {}.", name_of(before, &o.from), name_of(before, &o.to)),
                Kind::Observes => format!("{} no longer observes {}.", name_of(before, &o.from), name_of(before, &o.to)),
            });
        }
    }
    Changes {
        touched: after.agents.iter().filter(|a| touched.contains(&a.id)).map(|a| a.id.clone()).collect(),
        removed,
        lines,
    }
}

/// The unaddressed message that lists a change for the members it does not concern: it wakes
/// nobody.
pub fn notice(revision: u64, lines: &[String]) -> String {
    let footer = "\nThis is for your information; no answer is needed.";
    let title = format!("TEAM revision {revision} changes");
    let mut out = listed(&title, lines, MESSAGE_LIMIT - footer.len()).unwrap_or(title);
    out.push_str(footer);
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleView {
    pub role: Role,
    /// The role as the card says it, with how the work is handed out when it is.
    pub detail: String,
    pub mukade: MukadeRole,
    pub hand_out: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Evaluation {
    pub roles: BTreeMap<String, RoleView>,
    pub issues: Vec<Issue>,
    /// For each arrow, the sentence its giver is told and the one its receiver is told.
    pub sentences: Vec<[String; 2]>,
}

/// Everything the window shows about a formation, computed once, here.
pub fn evaluate(f: &Formation, stopped: &BTreeSet<String>) -> Evaluation {
    let roles = f
        .agents
        .iter()
        .map(|a| {
            let kind = role(f, &a.id);
            let hand = hand_out(f, &a.id);
            let mut detail = role_label(kind).to_string();
            if hand.is_some() {
                detail.push_str(match a.share {
                    Share::Split => " \u{b7} splits the work",
                    Share::Same => " \u{b7} gives the same task to all",
                    Share::Free => " \u{b7} gives work to whoever is free",
                });
            }
            (a.id.clone(), RoleView { role: kind, detail, mukade: mukade_role(f, &a.id), hand_out: hand })
        })
        .collect();
    Evaluation {
        roles,
        issues: check(f, stopped),
        sentences: (0..f.links.len()).map(|k| [told(f, k, true), told(f, k, false)]).collect(),
    }
}


// ---- What is owed to the room ----

/// One agent as the room sees it now: the member its session holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seat {
    pub member_id: String,
    pub member_name: String,
    /// The Mukade role the member has now.
    pub role: String,
    /// Whether the member is in the room (a session that is not running or never joined is not).
    pub in_room: bool,
}

/// What an agent was last told: the member that was told, and a fingerprint of everything the
/// message said about the agent itself. A new revision that changes nothing about it is not a
/// reason to tell it again (and to wake it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Told {
    pub revision: u64,
    pub member: String,
    #[serde(default)]
    pub digest: String,
}

/// The key under which `told` remembers the last revision whose notice was sent. Not an agent id.
pub const NOTICE_KEY: &str = "notice";
const REMOVED: &str = "removed";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    SetRole { member_id: String, role: MukadeRole },
    Personal { agent: String, member_name: String, text: String },
    Notice { text: String },
    Retire { member_id: String },
}

/// FNV-1a: stable across Rust versions, which a stored fingerprint needs.
fn fingerprint(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Everything a personal message says about the agent itself: its name, notes, model, how it
/// hands work out, its Mukade role and the sentences of its relations.
fn digest(f: &Formation, agent: &str) -> String {
    let Some(me) = f.agents.iter().find(|a| a.id == agent) else {
        return REMOVED.into();
    };
    let sentences: Vec<String> = f
        .links
        .iter()
        .enumerate()
        .filter(|(_, l)| l.from == agent || l.to == agent)
        .map(|(k, l)| told(f, k, l.from == agent))
        .collect();
    fingerprint(&format!(
        "{}\n{}\n{}\n{:?}\n{}\n{}\n{}",
        me.name,
        me.notes,
        serde_json::to_string(&me.model).unwrap_or_default(),
        me.share,
        mukade_role(f, agent).as_str(),
        sentences.join("\n"),
        hand_out(f, agent).unwrap_or_default()
    ))
}

/// The message to an agent taken off the map.
fn no_place(revision: u64) -> String {
    format!(
        "TEAM revision {revision}\nYou have no place in this thread's team any more: the user took you off the map. Go on as an equal with everyone."
    )
}

/// What is owed to the room, given what was stored, what was told and who is in it. Pure and
/// idempotent: with `told` updated by `mark` for each delivery that succeeded, a second call
/// returns nothing. Order: the roles that changed, then the personal messages that explain them,
/// then one notice for the members the change does not concern (those it does are told
/// personally), then the sessions that closed.
///
/// `seats`: the agents of the thread's sessions that are in this thread (an agent whose session
/// left the thread has none and is owed nothing). `closed`: members of sessions that were closed.
pub fn deliveries(
    before: Option<&Formation>,
    now: &Formation,
    told: &BTreeMap<String, Told>,
    seats: &BTreeMap<String, Seat>,
    closed: &[Seat],
) -> Vec<Delivery> {
    let mut out = Vec::new();
    let present = |id: &str| seats.get(id).filter(|seat| seat.in_room);
    let on_map = |id: &str| now.agents.iter().any(|a| a.id == id);
    let gone: Vec<&String> = seats
        .keys()
        .filter(|id| !on_map(id) && told.get(*id).is_some_and(|t| t.digest != REMOVED))
        .collect();
    for agent in &now.agents {
        if let Some(seat) = present(&agent.id) {
            let wanted = mukade_role(now, &agent.id);
            if seat.role != wanted.as_str() {
                out.push(Delivery::SetRole { member_id: seat.member_id.clone(), role: wanted });
            }
        }
    }
    for id in &gone {
        if let Some(seat) = present(id).filter(|seat| seat.role != MukadeRole::Peer.as_str()) {
            out.push(Delivery::SetRole { member_id: seat.member_id.clone(), role: MukadeRole::Peer });
        }
    }
    let members: BTreeMap<String, String> = seats
        .iter()
        .filter(|(_, seat)| seat.in_room)
        .map(|(id, seat)| (id.clone(), seat.member_name.clone()))
        .collect();
    for agent in &now.agents {
        let Some(seat) = present(&agent.id) else { continue };
        let owed = match told.get(&agent.id) {
            None => true,
            Some(t) => t.member != seat.member_id || t.digest != digest(now, &agent.id),
        };
        if owed {
            out.push(Delivery::Personal {
                agent: agent.id.clone(),
                member_name: seat.member_name.clone(),
                text: personal(now, &agent.id, &members),
            });
        }
    }
    for id in &gone {
        if let Some(seat) = present(id) {
            out.push(Delivery::Personal {
                agent: (*id).clone(),
                member_name: seat.member_name.clone(),
                text: no_place(now.revision),
            });
        }
    }
    let change = changes(before, now);
    let noticed = told.get(NOTICE_KEY).is_some_and(|t| t.revision >= now.revision);
    let others = now
        .agents
        .iter()
        .any(|a| present(&a.id).is_some() && !change.touched.contains(&a.id));
    if !change.lines.is_empty() && !noticed && others {
        out.push(Delivery::Notice { text: notice(now.revision, &change.lines) });
    }
    if !closed.is_empty() {
        let lines: Vec<String> = closed
            .iter()
            .map(|seat| format!("{} closed its session and left the room.", seat.member_name))
            .collect();
        out.push(Delivery::Notice { text: notice(now.revision, &lines) });
        for seat in closed {
            out.push(Delivery::Retire { member_id: seat.member_id.clone() });
        }
    }
    out
}

/// Record that one delivery succeeded, so that `deliveries` does not owe it again. A notice of a
/// closed session shares the change notice's mark: both say "the room was told at this revision".
pub fn mark(now: &Formation, seats: &BTreeMap<String, Seat>, delivery: &Delivery, told: &mut BTreeMap<String, Told>) {
    match delivery {
        Delivery::Personal { agent, .. } => {
            if let Some(seat) = seats.get(agent) {
                told.insert(
                    agent.clone(),
                    Told { revision: now.revision, member: seat.member_id.clone(), digest: digest(now, agent) },
                );
            }
        }
        Delivery::Notice { .. } => {
            told.insert(
                NOTICE_KEY.into(),
                Told { revision: now.revision, member: String::new(), digest: String::new() },
            );
        }
        Delivery::SetRole { .. } | Delivery::Retire { .. } => {}
    }
}


/// How many templates are kept (the store's own bound).
pub const MAX_TEMPLATES: usize = shinatama_state::database::MAX_TEMPLATES;
pub const MAX_TEMPLATE_NAME: usize = 60;

/// A team kept to be used again, in any thread: its agents (none has a session), their relations
/// and their places. It carries no revision: a team made from it starts at revision 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Template {
    /// `tm_` and 12 of `[a-z2-7]`, made by `template_id`.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub agents: Vec<Agent>,
    #[serde(default)]
    pub links: Vec<Link>,
}

fn valid_template_id(id: &str) -> bool {
    id.strip_prefix("tm_").is_some_and(|tail| {
        tail.len() == 12 && tail.bytes().all(|b| matches!(b, b'a'..=b'z' | b'2'..=b'7'))
    })
}

/// An id for a new template from 12 random bytes.
pub fn template_id(random: &[u8; 12]) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let tail: String = random.iter().map(|b| char::from(ALPHABET[usize::from(b % 32)])).collect();
    format!("tm_{tail}")
}

/// The template a formation makes. Its sessions and its revision are left out. It is refused when
/// the formation could not be valid, is empty, or has an error of the checks, except `no_model`:
/// an agent with no session and no model is what a template's agents are until the team is used.
pub fn template_of(id: String, name: &str, f: &Formation) -> Result<Template, String> {
    if !valid_template_id(&id) {
        return Err("A template has an invalid id.".into());
    }
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_TEMPLATE_NAME || has_control(name) {
        return Err(format!("A template's name takes 1 to {MAX_TEMPLATE_NAME} characters, on one line."));
    }
    if name.contains('@') {
        return Err("A template's name cannot hold @.".into());
    }
    validate(f)?;
    if f.agents.is_empty() {
        return Err("An empty team is not saved.".into());
    }
    if let Some(error) = check(f, &BTreeSet::new())
        .into_iter()
        .find(|issue| issue.level == Level::Error && issue.code != "no_model")
    {
        return Err(error.text);
    }
    Ok(Template {
        id,
        name: name.to_owned(),
        agents: f.agents.iter().map(|agent| Agent { session: None, ..agent.clone() }).collect(),
        links: f.links.clone(),
    })
}

fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Add a template to the end of the list, refusing the sixty-fifth, a name the list already has
/// (without regard to case) and an id it already has. The store runs this inside the transaction
/// that writes the list, so two windows cannot both take the last place or the same name.
pub fn add_template(list: &mut Vec<Template>, template: Template) -> Result<(), String> {
    if list.len() >= MAX_TEMPLATES {
        return Err(format!("A team keeps at most {MAX_TEMPLATES} templates. Delete one first."));
    }
    if let Some(taken) = list.iter().find(|kept| same_name(&kept.name, &template.name)) {
        return Err(format!("A template named {} already exists.", taken.name));
    }
    if list.iter().any(|kept| kept.id == template.id) {
        return Err("A template with this id already exists.".into());
    }
    list.push(template);
    Ok(())
}

/// Delete a template; one that is not there (another window deleted it) is not an error.
pub fn remove_template(list: &mut Vec<Template>, id: &str) {
    list.retain(|kept| kept.id != id);
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

    // ---- What each session is told ----

    fn named(mut f: Formation, names: &[(&str, &str)]) -> Formation {
        for (id, name) in names {
            f.agents.iter_mut().find(|a| a.id == *id).unwrap().name = (*name).into();
        }
        f
    }

    fn two(kind: Kind, report: Report) -> Formation {
        named(team(&["a", "b"], &[("a", "b", kind, report)]), &[("a", "Ada"), ("b", "Bo")])
    }

    #[test]
    fn work_when_done_is_told_to_both_ends() {
        let f = two(Work, Done);
        assert_eq!(told(&f, 0, true), "You give work to Bo; Bo reports back to you when it is done.");
        assert_eq!(told(&f, 0, false), "You wait for work from Ada, do it, and report back to Ada when it is done.");
    }

    #[test]
    fn work_at_every_step_is_told_to_both_ends() {
        let f = two(Work, Steps);
        assert_eq!(told(&f, 0, false), "You wait for work from Ada, do it, and tell Ada what you are doing at every step.");
        assert_eq!(told(&f, 0, true), "You give work to Bo; Bo tells you what it is doing at every step.");
    }

    #[test]
    fn work_only_done_or_blocked_is_told_to_both_ends() {
        let f = two(Work, Strict);
        assert_eq!(told(&f, 0, false), "You wait for instructions from Ada, execute them, and answer only done or blocked.");
        assert_eq!(told(&f, 0, true), "You give work to Bo; Bo answers only done or blocked.");
    }

    #[test]
    fn observing_is_told_to_both_ends() {
        let f = two(Observes, Done);
        assert_eq!(told(&f, 0, true), "You observe Bo: follow what it does, to explain it to the user when asked. Do not write to it.");
        assert_eq!(told(&f, 0, false), "Ada observes you and may read what you do.");
    }

    #[test]
    fn the_users_words_are_appended_to_both_ends() {
        let mut f = two(Work, Done);
        f.links[0].words = "plan first".into();
        assert!(told(&f, 0, true).ends_with(" The user adds: \u{201c}plan first\u{201d}."));
        assert!(told(&f, 0, false).ends_with(" The user adds: \u{201c}plan first\u{201d}."));
        let mut f = two(Observes, Done);
        f.links[0].words = "only the tests".into();
        assert!(told(&f, 0, true).ends_with(" The user adds: \u{201c}only the tests\u{201d}."));
    }

    #[test]
    fn handing_work_out_is_told_in_one_of_three_ways() {
        let mut f = named(
            team(&["a", "b", "c", "d"], &[("a", "b", Work, Done), ("a", "c", Work, Done), ("a", "d", Work, Done)]),
            &[("a", "Ada"), ("b", "Bo"), ("c", "Cy"), ("d", "Di")],
        );
        let view = |f: &Formation| evaluate(f, &none()).roles["a"].clone();
        assert_eq!(
            view(&f).hand_out.as_deref(),
            Some("You give work to Bo, Cy and Di: split it among them, a different part to each, and put their results together.")
        );
        assert_eq!(view(&f).detail, "Lead \u{b7} splits the work");
        f.agents[0].share = Share::Same;
        assert_eq!(
            view(&f).hand_out.as_deref(),
            Some("You give work to Bo, Cy and Di: give the same task to each, then compare their results.")
        );
        f.agents[0].share = Share::Free;
        assert_eq!(
            view(&f).hand_out.as_deref(),
            Some("You give work to Bo, Cy and Di: give each task to whichever of them is free.")
        );
        let one = two(Work, Done);
        assert_eq!(evaluate(&one, &none()).roles["a"].hand_out, None, "one receiver is not a hand-out");
    }

    #[test]
    fn personal_names_the_agent_its_member_its_role_then_the_formation() {
        let mut f = named(
            team(&["a", "b", "c"], &[("a", "b", Work, Done), ("c", "a", Observes, Done)]),
            &[("a", "Ada"), ("b", "Bo"), ("c", "Cy")],
        );
        f.revision = 7;
        f.agents[0].notes = "Keep it short.".into();
        let members: BTreeMap<String, String> =
            [("a".to_string(), "claude-1".to_string()), ("b".to_string(), "opencode-1".to_string())].into();
        let text = personal(&f, "a", &members);
        assert!(text.starts_with("TEAM revision 7\n"), "{text}");
        let at = |needle: &str| text.find(needle).unwrap_or_else(|| panic!("{needle} in {text}"));
        assert!(at("Ada") < at("claude-1") && at("claude-1") < at("Lead"), "{text}");
        assert!(at("Lead") < at("You give work to Bo; Bo reports back to you when it is done."), "{text}");
        assert!(text.contains("Cy observes you and may read what you do."), "{text}");
        assert!(text.contains("Keep it short."), "{text}");
        assert!(at("Keep it short.") < at("The team:"), "{text}");
        assert!(text.contains("Bo: opencode-1") && text.contains("Cy: not in the room yet"), "{text}");
        assert!(at("The team:") < at("Relations:"), "{text}");
        assert!(text.contains("Ada gives work to Bo (when done)"), "{text}");
    }

    #[test]
    fn an_agent_with_no_arrow_is_told_it_is_an_equal() {
        let f = team(&["a"], &[]);
        let members: BTreeMap<String, String> = [("a".to_string(), "pi-1".to_string())].into();
        assert!(personal(&f, "a", &members).contains("equal"), "{}", personal(&f, "a", &members));
    }

    #[test]
    fn a_personal_message_stays_within_2048_bytes() {
        let mut f = proper(24);
        for (i, agent) in f.agents.iter_mut().enumerate() {
            agent.name = format!("{:0>2}{}", i, "n".repeat(38));
            agent.notes = "k".repeat(1024);
        }
        // 64 arrows spread over the 24 agents (none holds more than six), every one with the
        // longest words and the longest sentence.
        let id = |i: usize| f.agents[i].id.clone();
        let links: Vec<Link> = (0..64)
            .map(|i| Link {
                from: id(i % 24),
                to: id((i % 24 + 1 + i / 24) % 24),
                kind: Work,
                report: Steps,
                words: "w".repeat(120),
            })
            .collect();
        f.links = links;
        assert_eq!(validate(&f), Ok(()));
        let members: BTreeMap<String, String> =
            f.agents.iter().map(|a| (a.id.clone(), format!("member-{}", &a.id[2..])).clone()).collect();
        for (i, agent) in f.agents.iter().enumerate() {
            let text = personal(&f, &agent.id, &members);
            assert!(text.len() <= 2048, "agent {i}: {} bytes", text.len());
        }
        // The agent's own sentences are never the part that is cut, when they fit at all.
        let plain = Formation { links: f.links.iter().map(|l| Link { words: String::new(), ..l.clone() }).collect(), ..f.clone() };
        for agent in &f.agents {
            let text = personal(&f, &agent.id, &members);
            for (k, link) in f.links.iter().enumerate().filter(|(_, l)| l.from == agent.id || l.to == agent.id) {
                let sentence = told(&plain, k, link.from == agent.id);
                assert!(text.contains(&sentence), "arrow {k} of {} is whole in its message", agent.name);
            }
        }
        let text = personal(&f, &f.agents[0].id, &members);
        assert!(text.contains('\u{2026}'), "what was left out is said");
    }

    #[test]
    fn moving_a_card_is_not_a_change() {
        let before = two(Work, Done);
        let mut after = before.clone();
        after.revision = 2;
        after.agents[0].x = 400;
        after.agents[1].y = -30;
        let changes = changes(Some(&before), &after);
        assert!(changes.touched.is_empty() && changes.removed.is_empty() && changes.lines.is_empty(), "{changes:?}");
    }

    #[test]
    fn a_changed_arrow_touches_both_its_ends_and_no_one_else() {
        let before = named(
            team(&["a", "b", "c"], &[("a", "b", Work, Done)]),
            &[("a", "Ada"), ("b", "Bo"), ("c", "Cy")],
        );
        let mut after = before.clone();
        after.links[0].report = Steps;
        let changes = changes(Some(&before), &after);
        assert_eq!(changes.touched, vec!["a", "b"]);
        assert!(changes.removed.is_empty());
        assert_eq!(changes.lines.len(), 1, "{:?}", changes.lines);
        assert!(changes.lines[0].contains("Ada") && changes.lines[0].contains("Bo"), "{:?}", changes.lines);
    }

    #[test]
    fn a_new_arrow_a_new_agent_and_a_removed_agent_are_changes() {
        let before = named(team(&["a", "b", "c"], &[]), &[("a", "Ada"), ("b", "Bo"), ("c", "Cy")]);
        let mut after = before.clone();
        after.links.push(Link { from: "a".into(), to: "b".into(), kind: Work, report: Done, words: String::new() });
        after.agents.remove(2);
        after.agents.push(Agent { name: "Di".into(), ..agent("d") });
        let changes = changes(Some(&before), &after);
        assert_eq!(changes.touched, vec!["a", "b", "d"]);
        assert_eq!(changes.removed.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), vec!["c"]);
        let all = changes.lines.join("\n");
        assert!(all.contains("Ada now gives work to Bo"), "{all}");
        assert!(all.contains("Cy left the team") && all.contains("Di joined the team"), "{all}");
    }

    #[test]
    fn a_first_formation_touches_everyone_and_notices_nothing() {
        let after = two(Work, Done);
        let changes = changes(None, &after);
        assert_eq!(changes.touched, vec!["a", "b"]);
        assert!(changes.lines.is_empty());
    }

    #[test]
    fn a_renamed_agent_a_new_model_and_new_notes_touch_that_agent_and_its_neighbours_are_told_the_name() {
        let before = two(Work, Done);
        let mut after = before.clone();
        after.agents[1].name = "Bea".into();
        let c = changes(Some(&before), &after);
        assert_eq!(c.touched, vec!["a", "b"], "a rename changes what the other end's sentences say");
        assert!(c.lines.iter().any(|l| l.contains("Bo") && l.contains("Bea")), "{:?}", c.lines);
        let mut after = before.clone();
        after.agents[0].notes = "new".into();
        assert_eq!(changes(Some(&before), &after).touched, vec!["a"]);
        let mut after = before.clone();
        after.agents[1].model = Some(ModelRef { program: "pi".into(), connection: None, model: None });
        assert_eq!(changes(Some(&before), &after).touched, vec!["b"]);
    }

    #[test]
    fn the_notice_counts_what_it_leaves_out_and_stays_within_2048_bytes() {
        let lines: Vec<String> = (0..80).map(|i| format!("Agent number {i} now gives work to another agent, in a long sentence of words.")).collect();
        let text = notice(9, &lines);
        assert!(text.starts_with("TEAM revision 9 changes\n"), "{text}");
        assert!(text.len() <= 2048, "{}", text.len());
        assert!(text.contains("\u{2026} and "), "{text}");
        let short = notice(3, &["Ada now gives work to Bo.".to_string()]);
        assert!(short.contains("- Ada now gives work to Bo."), "{short}");
        assert!(!short.contains("\u{2026}"), "{short}");
    }

    #[test]
    fn evaluate_gives_every_agent_its_role_and_every_arrow_its_two_sentences() {
        let f = two(Work, Strict);
        let e = evaluate(&f, &none());
        assert_eq!(e.roles["a"].role, Role::Lead);
        assert_eq!(e.roles["a"].mukade, MukadeRole::Lead);
        assert_eq!(e.roles["b"].role, Role::Worker);
        assert_eq!(e.roles["b"].mukade, MukadeRole::Worker);
        assert_eq!(e.sentences, vec![[told(&f, 0, true), told(&f, 0, false)]]);
        assert!(e.issues.is_empty());
    }

    // ---- What is owed to the room ----

    fn seat(member: &str, role: &str, in_room: bool) -> Seat {
        Seat { member_id: format!("id-{member}"), member_name: member.into(), role: role.into(), in_room }
    }

    /// The seats of every agent of `f`, all in the room, with the Mukade role the formation asks for.
    fn seated(f: &Formation) -> BTreeMap<String, Seat> {
        f.agents
            .iter()
            .map(|a| (a.id.clone(), seat(&format!("m-{}", a.id), mukade_role(f, &a.id).as_str(), true)))
            .collect()
    }

    /// What the worker does with a run: every delivery succeeds.
    fn run(before: Option<&Formation>, now: &Formation, told: &mut BTreeMap<String, Told>, seats: &BTreeMap<String, Seat>) -> Vec<Delivery> {
        let owed = deliveries(before, now, told, seats, &[]);
        for delivery in &owed {
            mark(now, seats, delivery, told);
        }
        owed
    }

    fn kinds(owed: &[Delivery]) -> Vec<&'static str> {
        owed.iter()
            .map(|d| match d {
                Delivery::SetRole { .. } => "role",
                Delivery::Personal { .. } => "personal",
                Delivery::Notice { .. } => "notice",
                Delivery::Retire { .. } => "retire",
            })
            .collect()
    }

    fn personal_for(owed: &[Delivery]) -> Vec<&str> {
        owed.iter().filter_map(|d| if let Delivery::Personal { agent, .. } = d { Some(agent.as_str()) } else { None }).collect()
    }

    #[test]
    fn a_new_formation_tells_everyone_in_the_room_and_sends_no_notice_to_nobody() {
        let f = team(&["a", "b", "c"], &[("a", "b", Work, Strict)]);
        let mut seats = seated(&f);
        for seat in seats.values_mut() {
            seat.role = "peer".into();
        }
        let owed = deliveries(None, &f, &BTreeMap::new(), &seats, &[]);
        assert_eq!(personal_for(&owed), vec!["a", "b", "c"]);
        assert!(!kinds(&owed).contains(&"notice"), "{:?}", kinds(&owed));
        let roles: Vec<_> = owed.iter().filter_map(|d| if let Delivery::SetRole { member_id, role } = d { Some((member_id.as_str(), *role)) } else { None }).collect();
        assert_eq!(roles, vec![("id-m-a", MukadeRole::Lead), ("id-m-b", MukadeRole::Worker)]);
    }

    #[test]
    fn a_role_is_set_before_the_message_that_explains_it() {
        let f = team(&["a", "b"], &[("a", "b", Work, Strict)]);
        let mut seats = seated(&f);
        seats.get_mut("b").unwrap().role = "peer".into();
        let owed = deliveries(None, &f, &BTreeMap::new(), &seats, &[]);
        let k = kinds(&owed);
        assert!(k.iter().position(|x| *x == "role").unwrap() < k.iter().position(|x| *x == "personal").unwrap(), "{k:?}");
    }

    #[test]
    fn one_changed_arrow_tells_its_two_ends_and_notices_the_rest() {
        let before = team(&["a", "b", "c"], &[("a", "b", Work, Done)]);
        let mut told = BTreeMap::new();
        let seats = seated(&before);
        run(None, &before, &mut told, &seats);
        let mut now = before.clone();
        now.revision = 2;
        now.links[0].report = Steps;
        let owed = run(Some(&before), &now, &mut told, &seated(&now));
        assert_eq!(personal_for(&owed), vec!["a", "b"]);
        assert_eq!(kinds(&owed).iter().filter(|k| **k == "notice").count(), 1, "{:?}", kinds(&owed));
        let Some(Delivery::Notice { text }) = owed.iter().find(|d| matches!(d, Delivery::Notice { .. })) else { panic!() };
        assert!(text.starts_with("TEAM revision 2 changes\n"), "{text}");
    }

    #[test]
    fn moving_a_card_or_bumping_the_revision_owes_nothing() {
        let before = team(&["a", "b"], &[("a", "b", Work, Done)]);
        let mut told = BTreeMap::new();
        run(None, &before, &mut told, &seated(&before));
        let mut now = before.clone();
        now.revision = 5;
        now.agents[0].x = 300;
        assert!(run(Some(&before), &now, &mut told, &seated(&now)).is_empty());
    }

    #[test]
    fn an_agent_not_in_the_room_is_told_when_it_joins() {
        let f = team(&["a", "b"], &[]);
        let mut seats = seated(&f);
        seats.get_mut("b").unwrap().in_room = false;
        let mut told = BTreeMap::new();
        assert_eq!(personal_for(&run(None, &f, &mut told, &seats)), vec!["a"]);
        seats.get_mut("b").unwrap().in_room = true;
        assert_eq!(personal_for(&run(None, &f, &mut told, &seats)), vec!["b"]);
        assert!(run(None, &f, &mut told, &seats).is_empty());
    }

    #[test]
    fn a_new_member_of_a_told_agent_is_told_again_once() {
        let f = team(&["a"], &[]);
        let mut seats = seated(&f);
        let mut told = BTreeMap::new();
        run(None, &f, &mut told, &seats);
        // The session was resumed and Mukade gave it another member.
        seats.insert("a".into(), seat("m-new", "peer", true));
        assert_eq!(personal_for(&run(None, &f, &mut told, &seats)), vec!["a"]);
        assert!(run(None, &f, &mut told, &seats).is_empty(), "told once, not at every check");
    }

    #[test]
    fn nothing_is_owed_twice() {
        let before = team(&["a", "b", "c"], &[("a", "b", Work, Strict)]);
        let mut now = before.clone();
        now.revision = 2;
        now.links.push(Link { from: "a".into(), to: "c".into(), kind: Work, report: Done, words: "go".into() });
        let mut told = BTreeMap::new();
        run(None, &before, &mut told, &seated(&before));
        let first = run(Some(&before), &now, &mut told, &seated(&now));
        assert!(!first.is_empty());
        assert!(run(Some(&before), &now, &mut told, &seated(&now)).is_empty());
    }

    #[test]
    fn an_agent_taken_off_the_map_is_told_it_has_no_place() {
        let before = team(&["a", "b"], &[("a", "b", Work, Done)]);
        let mut told = BTreeMap::new();
        let seats = seated(&before);
        run(None, &before, &mut told, &seats);
        let mut now = team(&["a"], &[]);
        now.revision = 2;
        let owed = run(Some(&before), &now, &mut told, &seats);
        let removed = owed.iter().find_map(|d| match d {
            Delivery::Personal { agent, text, .. } if agent == "b" => Some(text.clone()),
            _ => None,
        });
        let text = removed.expect("the removed agent is told");
        assert!(text.starts_with("TEAM revision 2\n") && text.contains("no place"), "{text}");
        assert!(personal_for(&owed).contains(&"a"), "its former giver is told the arrow is gone");
        let again = run(Some(&before), &now, &mut told, &seats);
        assert!(again.is_empty(), "{again:?}");
    }

    #[test]
    fn an_agent_whose_session_left_the_thread_gets_nothing() {
        let f = team(&["a", "b"], &[("a", "b", Work, Done)]);
        let mut seats = seated(&f);
        seats.remove("b");
        let owed = deliveries(None, &f, &BTreeMap::new(), &seats, &[]);
        assert_eq!(personal_for(&owed), vec!["a"]);
        assert!(owed.iter().all(|d| !matches!(d, Delivery::SetRole { member_id, .. } if member_id.contains('b'))));
    }

    #[test]
    fn a_closed_session_gets_a_closed_notice_and_its_member_is_retired() {
        let f = team(&["a", "b"], &[("a", "b", Work, Done)]);
        let seats = seated(&f);
        let closed = vec![seat("m-gone", "peer", true)];
        let owed = deliveries(Some(&f), &f, &BTreeMap::new(), &seats, &closed);
        let k = kinds(&owed);
        assert_eq!(&k[k.len() - 2..], ["notice", "retire"], "{k:?}");
        let Some(Delivery::Notice { text }) = owed.iter().rev().find(|d| matches!(d, Delivery::Notice { .. })) else { panic!() };
        assert!(text.contains("m-gone") && text.contains("closed"), "{text}");
        assert!(owed.iter().any(|d| matches!(d, Delivery::Retire { member_id } if member_id == "id-m-gone")));
    }

    const TEMPLATE: &str = "tm_aaaaaaaaaaaa";

    fn linked(n: usize, links: &[(usize, usize, Kind, Report)]) -> Formation {
        let mut f = proper(n);
        let ids: Vec<String> = f.agents.iter().map(|a| a.id.clone()).collect();
        f.links = links
            .iter()
            .map(|(from, to, kind, report)| Link {
                from: ids[*from].clone(),
                to: ids[*to].clone(),
                kind: *kind,
                report: *report,
                words: String::new(),
            })
            .collect();
        f
    }

    #[test]
    fn a_template_keeps_no_session_and_no_revision() {
        let mut f = linked(2, &[(0, 1, Work, Strict)]);
        f.revision = 7;
        f.links[0].words = "ship it".into();
        f.agents[0].notes = "plans".into();
        f.agents[0].share = Share::Free;
        f.agents[0].model = Some(ModelRef { program: "codex".into(), connection: None, model: Some("gpt".into()) });
        (f.agents[1].x, f.agents[1].y) = (140, -20);
        let t = template_of(TEMPLATE.into(), "  Review team  ", &f).unwrap();
        assert_eq!((t.id.as_str(), t.name.as_str()), (TEMPLATE, "Review team"));
        assert!(t.agents.iter().all(|a| a.session.is_none()));
        assert_eq!(t.agents[0].notes, "plans");
        assert_eq!(t.agents[0].share, Share::Free);
        assert_eq!(t.agents[0].model, f.agents[0].model);
        assert_eq!((t.agents[1].x, t.agents[1].y), (140, -20));
        assert_eq!(t.links, f.links);
        let shown = serde_json::to_value(&t).unwrap();
        assert!(shown.get("revision").is_none(), "{shown}");
        assert!(f.agents.iter().all(|a| a.session.is_some()), "the team itself keeps its sessions");
        // What was stored reads back as it was.
        assert_eq!(serde_json::from_value::<Template>(shown).unwrap(), t);
    }

    #[test]
    fn a_team_with_agents_that_have_no_session_yet_is_saved() {
        let mut f = linked(2, &[(0, 1, Work, Done)]);
        for agent in &mut f.agents {
            agent.session = None;
        }
        let t = template_of(TEMPLATE.into(), "Empty hands", &f).unwrap();
        assert_eq!(t.agents.len(), 2);
    }

    #[test]
    fn a_formation_with_a_loop_is_not_saved() {
        let f = linked(3, &[(0, 1, Work, Done), (1, 2, Work, Done), (2, 0, Work, Done)]);
        let refused = template_of(TEMPLATE.into(), "Circle", &f).unwrap_err();
        assert!(refused.contains("circle"), "{refused}");
        // Nor what cannot be valid, nor nothing.
        let mut at = proper(1);
        at.agents[0].notes = "ask @all".into();
        assert!(template_of(TEMPLATE.into(), "At", &at).unwrap_err().contains('@'));
        assert_eq!(template_of(TEMPLATE.into(), "None", &proper(0)).unwrap_err(), "An empty team is not saved.");
        let observer = linked(3, &[(0, 1, Observes, Done), (2, 0, Work, Done)]);
        assert!(template_of(TEMPLATE.into(), "Observer", &observer).unwrap_err().contains("observes"));
    }

    #[test]
    fn a_template_has_a_name_of_one_to_sixty_characters_and_an_id_of_its_own_shape() {
        let f = proper(1);
        for bad in ["", "   ", "a\nb", "@all", &"n".repeat(61)] {
            assert!(template_of(TEMPLATE.into(), bad, &f).is_err(), "{bad:?}");
        }
        assert!(template_of(TEMPLATE.into(), &"n".repeat(60), &f).is_ok());
        for bad in ["", "tm_short", "a_aaaaaaaaaaaa", "tm_AAAAAAAAAAAA", "tm_aaaaaaaaaaa1"] {
            assert!(template_of(bad.into(), "Name", &f).is_err(), "{bad}");
        }
        let made = template_id(&[0, 1, 25, 26, 31, 32, 255, 7, 8, 9, 10, 11]);
        assert_eq!(made, "tm_abz27a7hijkl");
        assert!(template_of(made, "Name", &f).is_ok());
    }

    #[test]
    fn a_second_template_of_the_same_name_is_refused() {
        let f = proper(1);
        let mut list = Vec::new();
        add_template(&mut list, template_of(TEMPLATE.into(), "Review", &f).unwrap()).unwrap();
        for same in ["Review", " review ", "REVIEW"] {
            let again = template_of("tm_bbbbbbbbbbbb".into(), same, &f).unwrap();
            let refused = add_template(&mut list, again).unwrap_err();
            assert_eq!(refused, "A template named Review already exists.");
        }
        assert_eq!(list.len(), 1);
        add_template(&mut list, template_of("tm_bbbbbbbbbbbb".into(), "Review 2", &f).unwrap()).unwrap();
        assert_eq!(list.len(), 2);
        // The same id is refused as well, whatever the name.
        assert!(add_template(&mut list, template_of(TEMPLATE.into(), "Other", &f).unwrap()).is_err());
    }

    #[test]
    fn the_sixty_fifth_template_is_refused() {
        let mut list = Vec::new();
        let id = |i: usize| format!("tm_{}{}", "a".repeat(10), [(b'a' + (i / 26) as u8) as char, (b'a' + (i % 26) as u8) as char].iter().collect::<String>());
        for i in 0..64 {
            add_template(&mut list, template_of(id(i), &format!("Team {i}"), &proper(1)).unwrap()).unwrap();
        }
        let refused = add_template(&mut list, template_of(id(64), "One more", &proper(1)).unwrap()).unwrap_err();
        assert_eq!(refused, "A team keeps at most 64 templates. Delete one first.");
        assert_eq!(list.len(), 64);
        // Deleting makes room.
        remove_template(&mut list, &id(3));
        assert_eq!(list.len(), 63);
        add_template(&mut list, template_of(id(64), "One more", &proper(1)).unwrap()).unwrap();
        // Deleting what is not there changes nothing.
        remove_template(&mut list, "tm_zzzzzzzzzzzz");
        assert_eq!(list.len(), 64);
    }
}
