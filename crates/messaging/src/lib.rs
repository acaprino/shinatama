//! Messages between hosted instances (FRD-0007 R01-R17, R23-R24).
//!
//! [`Messaging`] is the typed API: `instances`, `send`, `inbox`, `ack`. It enforces on the
//! server side: same-project scope by default, 16 KB bodies, expiry within one hour, secret
//! redaction before storage, a reply depth of at most six (inferred even when agents omit
//! `in_reply_to`), six messages per minute per pair and forty per hour per thread; a tripped
//! limit pauses the pair and raises an alert, nothing is dropped silently. Peer content reaches
//! a model only inside `<peer-message ... untrusted="true">`, escaped. Delivery is at least
//! once with at most five retries; `delivered` never means read. A PTY paste is used only when
//! the recipient is idle, nobody typed for ten seconds and no prompt, selection, search or
//! draft is active, so a message can never answer a permission prompt.
pub mod auth;

use serde::{Deserialize, Serialize};
use shinatama_export::Redactor;
use shinatama_state::messages::{MessageState, MessageStore, StoreError, StoredMessage};

pub const MAX_BODY_BYTES: usize = 16 * 1024;
pub const MAX_TTL_MS: i64 = 60 * 60 * 1000;
pub const MAX_DEPTH: u32 = 6;
pub const PAIR_PER_MINUTE: usize = 6;
pub const THREAD_PER_HOUR: u64 = 40;
pub const MAX_RETRIES: u32 = 5;
pub const FIRST_BACKOFF_MS: i64 = 5_000;
pub const PASTE_QUIET_MS: u64 = 10_000;
/// Without `in_reply_to`, a send soon after receiving from the same peer counts as a reply.
pub const IMPLICIT_REPLY_WINDOW_MS: i64 = 10 * 60 * 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Note,
    Ask,
    Reply,
    CatchUp,
    RoundtablePrompt,
    RoundtableAnswer,
}
impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Note => "note",
            Kind::Ask => "ask",
            Kind::Reply => "reply",
            Kind::CatchUp => "catch_up",
            Kind::RoundtablePrompt => "roundtable_prompt",
            Kind::RoundtableAnswer => "roundtable_answer",
        }
    }
}

/// A delivery channel, in the preference order of R09.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    NativeQueue,
    Hook,
    McpPull,
    Paste,
}
impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Channel::NativeQueue => "native_queue",
            Channel::Hook => "hook",
            Channel::McpPull => "mcp_pull",
            Channel::Paste => "paste",
        }
    }
    /// What "delivered" means on this channel, said honestly (R14).
    pub fn meaning(self) -> &'static str {
        match self {
            Channel::NativeQueue => "queued in the agent's own input queue",
            Channel::Hook => "injected at the agent's next hook boundary",
            Channel::McpPull => "returned to the agent by its inbox tool",
            Channel::Paste => "pasted into the agent's input line",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub agent: String,
    /// Canonical project folder.
    pub project: String,
    pub thread: Option<String>,
    pub roundtable: Option<String>,
    pub online: bool,
    /// Channels this agent supports, in preference order.
    pub channels: Vec<Channel>,
    /// For the paste channel: the instance is idle, nobody typed for this long, no prompt open.
    pub idle_quiet_ms: u64,
    pub prompt_or_draft_active: bool,
}

pub trait Directory {
    fn instances(&self) -> Vec<Instance>;
    /// Projects the user allowed to message each other (R04 setting); default none.
    fn cross_project_allowed(&self, _from: &str, _to: &str) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "to")]
pub enum Address {
    Instance { id: String },
    Agent { agent: String, project: String },
    Roundtable { group: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendRequest {
    pub to: Address,
    pub kind: Kind,
    pub body: String,
    pub in_reply_to: Option<String>,
    pub dedupe_key: Option<String>,
    pub ttl_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SendReceipt {
    pub ids: Vec<String>,
    pub duplicate: bool,
    pub redactions: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InboxItem {
    pub id: String,
    pub from: String,
    pub kind: String,
    pub depth: u32,
    /// `<peer-message from=... id=... untrusted="true">escaped</peer-message>`
    pub wrapped: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MessagingError {
    UnknownCaller,
    NoRecipient,
    CrossProject,
    BodyTooLarge,
    InvalidTtl,
    DepthLimit,
    RateLimit,
    Paused,
    NotRecipient,
    Store(StoreError),
}
impl From<StoreError> for MessagingError {
    fn from(error: StoreError) -> Self {
        MessagingError::Store(error)
    }
}
impl std::fmt::Display for MessagingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            MessagingError::UnknownCaller => "The sender is not a running Shinatama instance.",
            MessagingError::NoRecipient => "No recipient matches that address in this project.",
            MessagingError::CrossProject => "Messages between projects are off by default.",
            MessagingError::BodyTooLarge => "The body exceeds 16 KB; send a file path and hash instead.",
            MessagingError::InvalidTtl => "A message expires within one hour at most.",
            MessagingError::DepthLimit => "The reply chain reached its depth limit; agent replies are paused and the user was alerted.",
            MessagingError::RateLimit => "Too many messages; agent replies are paused and the user was alerted.",
            MessagingError::Paused => "Agent-to-agent messages between these two are paused until the user resumes them.",
            MessagingError::NotRecipient => "Only the recipient can acknowledge a message.",
            MessagingError::Store(error) => return write!(f, "{error}"),
        })
    }
}
pub type Result<T> = std::result::Result<T, MessagingError>;

pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
fn attr(text: &str) -> String {
    escape(text).replace('"', "&quot;")
}

pub fn wrap(message: &StoredMessage) -> String {
    format!(
        "<peer-message from=\"{}@{}\" id=\"{}\" kind=\"{}\" untrusted=\"true\">{}</peer-message>",
        attr(&message.sender_agent),
        attr(&message.sender_instance),
        attr(&message.id),
        attr(&message.kind),
        escape(&message.body)
    )
}

pub struct Messaging<'a, D: Directory> {
    pub store: &'a MessageStore,
    pub directory: &'a D,
}

impl<'a, D: Directory> Messaging<'a, D> {
    fn caller(&self, id: &str) -> Result<Instance> {
        self.directory.instances().into_iter().find(|i| i.id == id).ok_or(MessagingError::UnknownCaller)
    }
    fn in_scope(&self, from: &Instance, to: &Instance) -> bool {
        from.project.eq_ignore_ascii_case(&to.project)
            || (from.roundtable.is_some() && from.roundtable == to.roundtable)
            || self.directory.cross_project_allowed(&from.project, &to.project)
    }

    /// Instances the caller may message.
    pub fn instances(&self, caller: &str) -> Result<Vec<Instance>> {
        let me = self.caller(caller)?;
        Ok(self.directory.instances().into_iter().filter(|i| i.id != me.id && self.in_scope(&me, i)).collect())
    }

    fn paused(&self, from: &str, to: &str, now_ms: i64) -> Result<bool> {
        let key = format!("{from}->{to}");
        Ok(self.store.alerts_since(now_ms - IMPLICIT_REPLY_WINDOW_MS)?.iter().any(|a| a.kind == "pair_paused" && a.detail == key))
    }

    fn pause(&self, from: &str, to: &str, reason: &str, now_ms: i64) -> Result<()> {
        self.store.raise_alert("pair_paused", &format!("{from}->{to}"), now_ms)?;
        self.store.raise_alert("alert", &format!("Messages from {from} to {to} paused: {reason}"), now_ms)?;
        Ok(())
    }

    pub fn send(&self, caller: &str, request: SendRequest, now_ms: i64) -> Result<SendReceipt> {
        let me = self.caller(caller)?;
        if request.body.len() > MAX_BODY_BYTES {
            return Err(MessagingError::BodyTooLarge);
        }
        let ttl = request.ttl_ms.unwrap_or(MAX_TTL_MS);
        if ttl <= 0 || ttl > MAX_TTL_MS {
            return Err(MessagingError::InvalidTtl);
        }
        let everyone = self.directory.instances();
        let matching: Vec<&Instance> = everyone
            .iter()
            .filter(|i| i.id != me.id)
            .filter(|i| match &request.to {
                Address::Instance { id } => &i.id == id,
                Address::Agent { agent, project } => &i.agent == agent && i.project.eq_ignore_ascii_case(project),
                Address::Roundtable { group } => i.roundtable.as_deref() == Some(group.as_str()),
            })
            .collect();
        if matching.is_empty() {
            return Err(MessagingError::NoRecipient);
        }
        let recipients: Vec<&Instance> = matching.iter().copied().filter(|i| self.in_scope(&me, i)).collect();
        if recipients.is_empty() {
            return Err(MessagingError::CrossProject);
        }
        let mut redactor = Redactor::for_seed();
        let body = redactor.redact(&request.body);
        let redactions = redactor.counts().values().sum();
        let mut ids = Vec::new();
        let mut duplicate = false;
        for recipient in recipients {
            if self.paused(&me.id, &recipient.id, now_ms)? {
                return Err(MessagingError::Paused);
            }
            // Depth: explicit parent, else the latest message received from this peer recently.
            let parent = match &request.in_reply_to {
                Some(id) => self.store.get(id)?,
                None => self.store.between(&recipient.id, &me.id, now_ms - IMPLICIT_REPLY_WINDOW_MS)?.pop(),
            };
            let depth = parent.as_ref().map_or(0, |p| p.depth + 1);
            if depth > MAX_DEPTH {
                self.pause(&me.id, &recipient.id, "reply depth limit", now_ms)?;
                return Err(MessagingError::DepthLimit);
            }
            let recent = self.store.between(&me.id, &recipient.id, now_ms - 60_000)?;
            let thread_count = match &me.thread {
                Some(thread) => self.store.count_thread_since(thread, now_ms - 3_600_000)?,
                None => 0,
            };
            if recent.len() >= PAIR_PER_MINUTE || thread_count >= THREAD_PER_HOUR {
                self.pause(&me.id, &recipient.id, "rate limit", now_ms)?;
                return Err(MessagingError::RateLimit);
            }
            let id = uuid::Uuid::new_v4().simple().to_string();
            let dedupe_key = match &request.dedupe_key {
                Some(key) => format!("{}:{}:{}", me.id, recipient.id, key),
                None => id.clone(),
            };
            let message = StoredMessage {
                id: id.clone(),
                kind: request.kind.as_str().into(),
                sender_instance: me.id.clone(),
                sender_agent: me.agent.clone(),
                sender_project: me.project.clone(),
                sender_thread: me.thread.clone(),
                recipient_instance: recipient.id.clone(),
                recipient_project: recipient.project.clone(),
                group_id: match &request.to {
                    Address::Roundtable { group } => Some(group.clone()),
                    _ => None,
                },
                body: body.clone(),
                depth,
                in_reply_to: parent.map(|p| p.id),
                dedupe_key,
                created_at_ms: now_ms,
                expires_at_ms: now_ms + ttl,
                state: MessageState::Queued,
                attempts: 0,
                next_attempt_at_ms: now_ms,
                channel: None,
                failure: None,
            };
            match self.store.insert(&message) {
                Ok(()) => ids.push(id),
                Err(StoreError::Duplicate { existing_id }) => {
                    duplicate = true;
                    ids.push(existing_id);
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(SendReceipt { ids, duplicate, redactions })
    }

    /// MCP pull: the caller's unexpired messages, wrapped as untrusted data, marked delivered.
    pub fn inbox(&self, caller: &str, now_ms: i64) -> Result<Vec<InboxItem>> {
        let me = self.caller(caller)?;
        self.store.expire(now_ms)?;
        let mut items = Vec::new();
        for message in self.store.for_recipient(&me.id, now_ms, 50)? {
            if message.state == MessageState::Queued {
                self.store.update_state(&message.id, MessageState::Queued, MessageState::Delivered, message.attempts, message.next_attempt_at_ms, Some(Channel::McpPull.as_str()), None)?;
            }
            items.push(InboxItem {
                wrapped: wrap(&message),
                from: format!("{}@{}", message.sender_agent, message.sender_instance),
                id: message.id,
                kind: message.kind,
                depth: message.depth,
            });
        }
        Ok(items)
    }

    pub fn ack(&self, caller: &str, id: &str) -> Result<()> {
        let message = self.store.get(id)?.ok_or(MessagingError::Store(StoreError::NotFound))?;
        if message.recipient_instance != caller {
            return Err(MessagingError::NotRecipient);
        }
        for from in [MessageState::Delivered, MessageState::Queued] {
            if self.store.update_state(id, from, MessageState::Acked, message.attempts, message.next_attempt_at_ms, None, None)? {
                return Ok(());
            }
        }
        Ok(())
    }
}

/// Pushes messages through a recipient's channels (R09-R13).
pub trait Transport {
    fn push(&mut self, recipient: &Instance, channel: Channel, wrapped: &str) -> std::result::Result<(), String>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeliveryReport {
    pub delivered: u64,
    pub retried: u64,
    pub failed: u64,
    pub expired: u64,
    pub waiting_offline: u64,
}

/// One delivery pass: expired messages expire, offline recipients wait, push channels are
/// tried in order (MCP pull is passive: the message stays queued for the inbox tool).
pub fn deliver_due<D: Directory>(store: &MessageStore, directory: &D, transport: &mut dyn Transport, now_ms: i64) -> Result<DeliveryReport> {
    let mut report = DeliveryReport { expired: store.expire(now_ms)?, ..DeliveryReport::default() };
    let instances = directory.instances();
    for message in store.due(now_ms, 200)? {
        let Some(recipient) = instances.iter().find(|i| i.id == message.recipient_instance && i.online) else {
            report.waiting_offline += 1;
            continue;
        };
        if !store.update_state(&message.id, MessageState::Queued, MessageState::Delivering, message.attempts, message.next_attempt_at_ms, None, None)? {
            continue; // another pass took it
        }
        let wrapped = wrap(&message);
        let mut outcome = Err("no push channel".to_owned());
        let mut used = None;
        for channel in recipient.channels.iter().copied().filter(|c| *c != Channel::McpPull) {
            if channel == Channel::Paste && (recipient.idle_quiet_ms < PASTE_QUIET_MS || recipient.prompt_or_draft_active) {
                outcome = Err("paste not safe now".into());
                continue;
            }
            outcome = transport.push(recipient, channel, &wrapped);
            if outcome.is_ok() {
                used = Some(channel);
                break;
            }
        }
        match (outcome, used) {
            (Ok(()), Some(channel)) => {
                store.update_state(&message.id, MessageState::Delivering, MessageState::Delivered, message.attempts, now_ms, Some(channel.as_str()), None)?;
                report.delivered += 1;
            }
            (result, _) => {
                let failure = result.err();
                let pull_only = recipient.channels.contains(&Channel::McpPull);
                let attempts = message.attempts + 1;
                if attempts > MAX_RETRIES && !pull_only {
                    store.update_state(&message.id, MessageState::Delivering, MessageState::Failed, attempts.min(6), now_ms, None, failure.as_deref())?;
                    store.raise_alert("alert", &format!("Message {} to {} failed after {MAX_RETRIES} retries", message.id, message.recipient_instance), now_ms)?;
                    report.failed += 1;
                } else {
                    let backoff = FIRST_BACKOFF_MS * (1i64 << (attempts.min(MAX_RETRIES) - 1));
                    store.update_state(&message.id, MessageState::Delivering, MessageState::Queued, attempts.min(6), now_ms + backoff, None, failure.as_deref())?;
                    report.retried += 1;
                }
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests;
