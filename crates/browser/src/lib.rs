//! Browser requests and immutable routing (FRD-0011 R01-R14).
//!
//! A request is routed before its target loads and the decision is frozen: engine, operation,
//! scope and permissions cannot change afterwards. Identity comes from the server-side
//! [`Caller`], never from fields a page or agent writes. Moli is the default for read and
//! preview; Patchright (visible Chrome) only on an explicit user request with the setting on.
//! A failure never becomes a retry on the other engine: the result can only suggest the exact
//! new command the user may choose. Nothing here starts an engine.
use serde::{Deserialize, Serialize};

pub mod cdp;
#[cfg(windows)]
pub mod host;
pub mod input;
pub mod local_server;
pub mod pages;
pub mod service;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    Moli,
    Patchright,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Operation {
    Read,
    Preview,
    OpenUi,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    User,
    Agent,
}

/// Who is asking, as established by the authenticated integration (never by the request body).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Caller {
    User,
    Agent { instance: String, thread: Option<String>, project: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub id: String,
    /// Origins (`https://example.com`) or local roots (`file:///C:/proj/`) the grant covers.
    pub targets: Vec<String>,
    pub artifacts: bool,
    /// Granted by the user for this task; an agent cannot create or widen one.
    pub granted_by_user: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub patchright_enabled: bool,
}

/// What the caller asks for. `engine` is an explicit override, not a security boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ask {
    pub operation: Operation,
    pub target: String,
    pub engine: Option<Engine>,
    pub reason: String,
    pub scope_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PermissionSnapshot {
    pub scope: Option<Scope>,
    pub artifacts: bool,
    pub visible_browser: bool,
}

/// Frozen once built: there are no setters.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Request {
    pub request_id: String,
    pub schema: u32,
    pub origin: Origin,
    pub instance: Option<String>,
    pub thread: Option<String>,
    pub project: Option<String>,
    pub operation: Operation,
    pub engine: Engine,
    pub target: String,
    pub permission_snapshot: PermissionSnapshot,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok,
    Unsupported,
    NeedsUser,
    Denied,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Refusal {
    pub outcome: Outcome,
    pub message: String,
    /// A new command the user may choose; never run automatically.
    pub suggestion: Option<String>,
}

fn refuse(outcome: Outcome, message: &str, suggestion: Option<String>) -> Refusal {
    Refusal { outcome, message: message.into(), suggestion }
}

/// The target's origin or local root, lowercased, for scope matching.
pub fn target_key(target: &str) -> Option<String> {
    let lower = target.trim().to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://")) {
        let scheme = if lower.starts_with("https") { "https" } else { "http" };
        let host = rest.split(['/', '?', '#']).next().filter(|h| !h.is_empty() && !h.contains('@'))?;
        return Some(format!("{scheme}://{host}"));
    }
    if lower.starts_with("file:///") {
        return (!lower.contains("/../") && !lower.ends_with("/..")).then_some(lower);
    }
    None
}

fn in_scope(scope: &Scope, target: &str) -> bool {
    let Some(key) = target_key(target) else { return false };
    scope.targets.iter().filter_map(|t| target_key(t)).any(|allowed| if allowed.starts_with("file:///") { key.starts_with(&allowed) } else { key == allowed })
}

/// Routes and freezes a request (R07-R12).
pub fn route(caller: &Caller, ask: &Ask, scopes: &[Scope], settings: &Settings) -> Result<Request, Refusal> {
    if target_key(&ask.target).is_none() {
        return Err(refuse(Outcome::Denied, "Only http(s) URLs and file:/// paths inside a granted root are browsed.", None));
    }
    let scope = ask.scope_id.as_ref().and_then(|id| scopes.iter().find(|s| &s.id == id)).cloned();
    let (origin, instance, thread, project) = match caller {
        Caller::User => (Origin::User, None, None, None),
        Caller::Agent { instance, thread, project } => (Origin::Agent, Some(instance.clone()), thread.clone(), Some(project.clone())),
    };
    let engine = match (ask.operation, ask.engine) {
        (Operation::OpenUi, _) | (_, Some(Engine::Patchright)) => Engine::Patchright,
        _ => Engine::Moli,
    };
    if origin == Origin::Agent {
        if engine == Engine::Patchright {
            return Err(refuse(Outcome::NeedsUser, "An agent cannot open a visible browser; ask the user to run the command.", Some(format!("/browse open-ui {}", ask.target))));
        }
        match &scope {
            Some(s) if s.granted_by_user && in_scope(s, &ask.target) => {}
            Some(_) => return Err(refuse(Outcome::Denied, "The target is outside the task's granted scope.", None)),
            None => return Err(refuse(Outcome::NeedsUser, "An agent may browse only inside a scope the user granted for this task.", None)),
        }
    }
    if engine == Engine::Patchright && !settings.patchright_enabled {
        return Err(refuse(Outcome::Unsupported, "The visible browser path is off in settings.", None));
    }
    if ask.operation == Operation::Preview && !scope.as_ref().is_some_and(|s| s.artifacts) && origin == Origin::Agent {
        return Err(refuse(Outcome::Denied, "A preview needs artifact permission in the scope.", None));
    }
    debug_assert!(engine == Engine::Moli || ask.operation == Operation::OpenUi || ask.engine == Some(Engine::Patchright), "Patchright only by open-ui or an explicit override");
    let artifacts = scope.as_ref().is_some_and(|s| s.artifacts) || (origin == Origin::User && ask.operation == Operation::Preview);
    Ok(Request {
        request_id: uuid::Uuid::new_v4().simple().to_string(),
        schema: SCHEMA_VERSION,
        origin,
        instance,
        thread,
        project,
        operation: ask.operation,
        engine,
        target: ask.target.trim().to_owned(),
        permission_snapshot: PermissionSnapshot { scope, artifacts, visible_browser: engine == Engine::Patchright },
        reason: ask.reason.chars().filter(|c| !c.is_control()).take(300).collect(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BrowserResult {
    pub request_id: String,
    pub engine: Engine,
    pub outcome: Outcome,
    pub produced_visual: bool,
    pub suggestion: Option<String>,
    pub error: Option<String>,
}

/// Builds the result honestly (R13): a visual request with only text is not `ok`, and a failure
/// carries at most a suggested new command, never an automatic retry on the other engine.
pub fn finish(request: &Request, engine_outcome: Outcome, produced_visual: bool, error: Option<String>) -> BrowserResult {
    let visual_missing = request.operation == Operation::Preview && engine_outcome == Outcome::Ok && !produced_visual;
    let outcome = if visual_missing { Outcome::Failed } else { engine_outcome };
    let suggestion = (outcome == Outcome::Failed && request.engine == Engine::Moli && request.origin == Origin::User)
        .then(|| format!("/browse open-ui {}", request.target));
    BrowserResult {
        request_id: request.request_id.clone(),
        engine: request.engine,
        outcome,
        produced_visual,
        suggestion,
        error: if visual_missing { Some("Only text was produced; the preview was not made.".into()) } else { error },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(targets: &[&str], artifacts: bool) -> Scope {
        Scope { id: "s1".into(), targets: targets.iter().map(|t| (*t).to_owned()).collect(), artifacts, granted_by_user: true }
    }
    fn ask(operation: Operation, target: &str, scope: Option<&str>) -> Ask {
        Ask { operation, target: target.into(), engine: None, reason: "docs".into(), scope_id: scope.map(str::to_owned) }
    }
    fn agent() -> Caller {
        Caller::Agent { instance: "i_a".into(), thread: Some("t_a".into()), project: "C:/p".into() }
    }
    const ON: Settings = Settings { patchright_enabled: true };

    #[test]
    fn moli_is_the_default_and_open_ui_is_patchright_for_the_user_only() {
        let read = route(&Caller::User, &ask(Operation::Read, "https://docs.rs/serde", None), &[], &ON).unwrap();
        assert_eq!((read.engine, read.origin), (Engine::Moli, Origin::User));
        let ui = route(&Caller::User, &ask(Operation::OpenUi, "https://login.example.com", None), &[], &ON).unwrap();
        assert_eq!(ui.engine, Engine::Patchright);
        assert!(ui.permission_snapshot.visible_browser);
        let off = route(&Caller::User, &ask(Operation::OpenUi, "https://x.example", None), &[], &Settings { patchright_enabled: false }).unwrap_err();
        assert_eq!(off.outcome, Outcome::Unsupported);
        let agent_ui = route(&agent(), &ask(Operation::OpenUi, "https://x.example", Some("s1")), &[scope(&["https://x.example"], false)], &ON).unwrap_err();
        assert_eq!(agent_ui.outcome, Outcome::NeedsUser);
        assert_eq!(agent_ui.suggestion.as_deref(), Some("/browse open-ui https://x.example"));
    }

    #[test]
    fn agents_stay_inside_a_user_granted_scope() {
        let scopes = [scope(&["https://docs.rs", "file:///C:/proj/"], false)];
        assert!(route(&agent(), &ask(Operation::Read, "https://docs.rs/tokio/latest", Some("s1")), &scopes, &ON).is_ok());
        assert!(route(&agent(), &ask(Operation::Read, "file:///C:/proj/README.md", Some("s1")), &scopes, &ON).is_ok());
        assert_eq!(route(&agent(), &ask(Operation::Read, "https://evil.example", Some("s1")), &scopes, &ON).unwrap_err().outcome, Outcome::Denied);
        assert_eq!(route(&agent(), &ask(Operation::Read, "file:///C:/proj/../secret", Some("s1")), &scopes, &ON).unwrap_err().outcome, Outcome::Denied);
        assert_eq!(route(&agent(), &ask(Operation::Read, "https://docs.rs", None), &scopes, &ON).unwrap_err().outcome, Outcome::NeedsUser);
        let self_granted = [Scope { granted_by_user: false, ..scope(&["https://evil.example"], true) }];
        assert_eq!(route(&agent(), &ask(Operation::Read, "https://evil.example", Some("s1")), &self_granted, &ON).unwrap_err().outcome, Outcome::Denied);
        assert_eq!(route(&agent(), &ask(Operation::Preview, "https://docs.rs", Some("s1")), &scopes, &ON).unwrap_err().outcome, Outcome::Denied, "preview needs artifact permission");
        assert_eq!(route(&agent(), &ask(Operation::Read, "https://user:pw@docs.rs", Some("s1")), &scopes, &ON).unwrap_err().outcome, Outcome::Denied);
        assert_eq!(route(&agent(), &ask(Operation::Read, "javascript:alert(1)", Some("s1")), &scopes, &ON).unwrap_err().outcome, Outcome::Denied);
    }

    #[test]
    fn identity_comes_from_the_caller_and_failures_never_switch_engine() {
        let request = route(&agent(), &ask(Operation::Read, "https://docs.rs", Some("s1")), &[scope(&["https://docs.rs"], false)], &ON).unwrap();
        assert_eq!((request.instance.as_deref(), request.origin), (Some("i_a"), Origin::Agent));
        let failed = finish(&request, Outcome::Failed, false, Some("timeout".into()));
        assert_eq!((failed.engine, failed.outcome), (Engine::Moli, Outcome::Failed));
        assert!(failed.suggestion.is_none(), "an agent's failure suggests nothing it could act on");
        let user = route(&Caller::User, &ask(Operation::Preview, "https://docs.rs", None), &[], &ON).unwrap();
        let text_only = finish(&user, Outcome::Ok, false, None);
        assert_eq!(text_only.outcome, Outcome::Failed, "a visual request with only text is not satisfied");
        assert_eq!(text_only.suggestion.as_deref(), Some("/browse open-ui https://docs.rs"), "shown to the user, never run");
        assert_eq!(finish(&user, Outcome::Ok, true, None).outcome, Outcome::Ok);
    }
}
