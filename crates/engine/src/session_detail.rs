//! Worker-only projection of what Mukade reads. Mukade owns how a session of any size is read
//! (ADR-0053 v2): this module only asks for what each operation needs (the overview, the
//! clipped read, the streamed read) and keeps no size bound of its own; the archive's file
//! count is the one bound, and it bounds the archive, not a session. Raw records, reasoning
//! and attachment contents never leave the default detail projection. RAW has a separate
//! confirmed request. No native export, auth read, alternate parser or command.
use crate::session_roots::{
    MUKADE_REVISION, RootSelector, SessionNamespaceGuard, SessionReadIssue, pin_reader_namespace,
    pin_session_namespace,
};
use mukade::{
    Entry, HarnessId, Kind, Session,
    model::{Meta, ResultContent},
    reader::{Lookup, SessionInfo},
    roots::StoreRoots,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shinatama_state::records::StoreRootRecord;
use std::path::{Path, PathBuf};

/// The files one pin may hold (the pin's own entry bound, `session_roots`). It was 4,096, which
/// one Claude Code workflow run exceeds by itself.
pub const MAX_INPUT_FILES: usize = 65_536;
pub const MAX_DETAIL_EVENTS: usize = 50;
pub const MAX_TEXT_BYTES: usize = 4096;
/// The length Mukade clips every string to for the view. The view cuts a text at
/// `MAX_TEXT_BYTES` and says it did; a string cut at exactly that length could not tell a text
/// that fit from one that did not, and a cut at a UTF-8 boundary loses up to 3 bytes. Four bytes
/// more (the longest UTF-8 character) keep every text over `MAX_TEXT_BYTES` over it.
pub const VIEW_CLIP_BYTES: usize = MAX_TEXT_BYTES + 4;
pub const MAX_DETAIL_REPLY_BYTES: usize = 256 * 1024;
const EVENT_PAGE_BYTES: usize = MAX_DETAIL_REPLY_BYTES - 80 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetailIssue {
    InvalidRequest,
    RootUnavailable,
    RootChanged,
    UnsafeNamespace,
    UnsupportedHarness,
    UnsupportedExportContext,
    InputTooLarge,
    SessionNotFound,
    AmbiguousSession,
    SourceChanged,
    ReadUnavailable,
    OutputLimit,
}
impl From<SessionReadIssue> for DetailIssue {
    fn from(issue: SessionReadIssue) -> Self {
        match issue {
            SessionReadIssue::RootUnavailable => Self::RootUnavailable,
            SessionReadIssue::RootChanged => Self::RootChanged,
            SessionReadIssue::SourceChanged => Self::SourceChanged,
            SessionReadIssue::UnsupportedHarness => Self::UnsupportedHarness,
            SessionReadIssue::UnsafeRoot | SessionReadIssue::UnsupportedRootShape => {
                Self::UnsafeNamespace
            }
            // The archive holds more entries than a pin accepts: said as that, never as a
            // read that failed for no stated reason.
            SessionReadIssue::MetadataTooLarge => Self::InputTooLarge,
            _ => Self::ReadUnavailable,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetailCursor {
    pub offset: usize,
    pub fingerprint: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DetailRequest {
    pub root: RootSelector,
    pub native_id: String,
    pub cursor: Option<DetailCursor>,
    pub limit: usize,
    /// Only Mukade's overview of the session, read from the file's two ends: no event, no
    /// ledger, and no bound on the session's size. Resume and thread names need no more.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub overview_only: bool,
}
/// Created only by trusted Rust from the resolved authoritative record. No path,
/// state-ready flag or fabricated root record is accepted from a UI selector.
pub struct TrustedRootSnapshot {
    record: StoreRootRecord,
    root: RootSelector,
    harness: HarnessId,
}
impl TrustedRootSnapshot {
    pub(crate) fn harness_name(&self) -> String {
        self.harness.to_string()
    }
    pub fn from_record(record: &StoreRootRecord, root: &RootSelector) -> Result<Self, DetailIssue> {
        record.validate().map_err(|_| DetailIssue::InvalidRequest)?;
        if record.id != root.store_root_id || record.config_revision != root.configuration_revision
        {
            return Err(DetailIssue::RootChanged);
        }
        if !record.available {
            return Err(DetailIssue::RootUnavailable);
        }
        let harness = record
            .harness
            .parse()
            .map_err(|_| DetailIssue::UnsupportedHarness)?;
        if matches!(harness, HarnessId::Hermes | HarnessId::DeepSeekHarness) {
            // Harnesses Mukade has gained that Shinatama registers no root for.
            return Err(DetailIssue::UnsupportedHarness);
        }
        if harness == HarnessId::OpenCode {
            // Its pinned Reader.read launches the harness exporter. The existing
            // in-worker file reader is not that separately qualified launch path.
            return Err(DetailIssue::UnsupportedExportContext);
        }
        Ok(Self {
            record: record.clone(),
            root: root.clone(),
            harness,
        })
    }
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetailEventKind {
    UserText,
    AssistantText,
    ToolCall,
    ToolResult,
    Compaction,
    Meta,
    AttachmentOmitted,
    ReasoningOmitted,
}
/// Text intentionally has no Debug implementation; diagnostics expose codes only.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DetailEvent {
    pub ordinal: usize,
    /// Index in Mukade.entries, not an invented native JSONL line/byte offset.
    pub entry_ordinal: usize,
    pub source_entry_id: Option<String>,
    pub timestamp: Option<String>,
    pub kind: DetailEventKind,
    pub text: String,
    pub truncated: bool,
    pub payload_omitted: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DetailOverview {
    pub title: Option<String>,
    pub working_directory: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub parent_native_id: Option<String>,
    pub harness_version: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DetailPage {
    pub root: RootSelector,
    pub native_id: String,
    pub harness: HarnessId,
    pub overview: DetailOverview,
    pub fingerprint: String,
    pub total_events: usize,
    pub events: Vec<DetailEvent>,
    pub next: Option<DetailCursor>,
    pub notices: Vec<String>,
    pub lineage: crate::session_lineage::SessionLineage,
    /// F20: the agent's own recorded usage for this session; claims no saving.
    pub ledger: shinatama_token_economy::SessionLedger,
    /// F06: what the session did last, verbatim from its last tool call or message.
    pub activity_title: Option<String>,
    /// F05: paths named by this session's edit/write tool calls (evidence for collisions).
    pub touched_paths: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailReply {
    pub root: RootSelector,
    pub native_id: String,
    pub page: Option<DetailPage>,
    pub issue: Option<DetailIssue>,
}
impl DetailReply {
    pub fn refuse(request: &DetailRequest, issue: DetailIssue) -> Self {
        failed(request, issue)
    }
}

fn native_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}
fn digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn optional(value: Option<&str>, cap: usize) -> Option<String> {
    value
        .filter(|value| {
            !value.is_empty() && value.len() <= cap && !value.chars().any(char::is_control)
        })
        .map(str::to_owned)
}
fn explicit_roots(snapshot: &TrustedRootSnapshot) -> StoreRoots {
    let mut roots = StoreRoots::default();
    let path = Some(PathBuf::from(&snapshot.record.resolved_path));
    match snapshot.harness {
        HarnessId::ClaudeCode => roots.claude_config_dir = path,
        HarnessId::Codex => roots.codex_home = path,
        HarnessId::Pi => roots.pi_agent_dir = path,
        HarnessId::OpenCode => {}
        HarnessId::Hermes => roots.hermes_home = path,
        HarnessId::DeepSeekHarness => roots.dsh_home = path,
    }
    roots
}
/// Invoked only inside the C09R-contained session helper, never the host/UI.
/// The view is built from Mukade's clipped read; no session is refused for its size.
pub fn read_detail(snapshot: &TrustedRootSnapshot, request: &DetailRequest) -> DetailReply {
    let result = if request.overview_only {
        read_overview(snapshot, request)
    } else {
        read(snapshot, request)
    };
    match result {
        Ok(page) => {
            let reply = DetailReply {
                root: request.root.clone(),
                native_id: request.native_id.clone(),
                page: Some(page),
                issue: None,
            };
            if serde_json::to_vec(&reply).is_ok_and(|bytes| bytes.len() <= MAX_DETAIL_REPLY_BYTES) {
                reply
            } else {
                failed(request, DetailIssue::OutputLimit)
            }
        }
        Err(issue) => failed(request, issue),
    }
}
fn failed(request: &DetailRequest, issue: DetailIssue) -> DetailReply {
    DetailReply {
        root: request.root.clone(),
        native_id: if native_id(&request.native_id) {
            request.native_id.clone()
        } else {
            String::new()
        },
        page: None,
        issue: Some(issue),
    }
}
/// Why a streamed read stopped: Mukade or the namespace refused it, or the caller's own
/// callback did (it keeps its reason itself).
pub(crate) enum StreamError {
    Detail(DetailIssue),
    Stopped,
}
/// What a streamed callback returns to stop the read; the callback records why.
#[derive(Debug)]
pub(crate) struct StreamStop;
impl std::fmt::Display for StreamStop {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("stream stopped by its consumer")
    }
}
impl std::error::Error for StreamStop {}
fn lookup_issue(error: &anyhow::Error) -> DetailIssue {
    match error.downcast_ref::<Lookup>() {
        Some(Lookup::Ambiguous(_)) => DetailIssue::AmbiguousSession,
        Some(Lookup::NotFound(_)) => DetailIssue::SessionNotFound,
        _ => DetailIssue::ReadUnavailable,
    }
}
/// The one session of the request, found by Mukade in the pinned namespace. Every read of a
/// session (view, search, export, RAW) starts here and ends in `verified`.
pub(crate) struct Opened {
    guard: SessionNamespaceGuard,
    roots: StoreRoots,
    reader: Box<dyn mukade::reader::Reader>,
    info: SessionInfo,
    bytes: u64,
}
/// Common to every operation: request shape, namespace pin, the archive's file count, and
/// Mukade's overview of the session, which names the exact whole id and the one file. The
/// file must be one the pin holds, never a path from the caller.
pub(crate) fn open(
    snapshot: &TrustedRootSnapshot,
    request: &DetailRequest,
) -> Result<Opened, DetailIssue> {
    open_under(snapshot, request, false)
}
/// [`open`] for a read of the session's own file and nothing else (its overview): the pin
/// leaves out what session folders hold, so neither their size nor a subagent writing into
/// them refuses the read. A session whose file that pin does not hold is opened under the whole
/// pin instead.
fn open_session_file(
    snapshot: &TrustedRootSnapshot,
    request: &DetailRequest,
) -> Result<Opened, DetailIssue> {
    open_under(snapshot, request, true)
}
fn open_under(
    snapshot: &TrustedRootSnapshot,
    request: &DetailRequest,
    session_files_only: bool,
) -> Result<Opened, DetailIssue> {
    if request.root != snapshot.root
        || !native_id(&request.native_id)
        || !(1..=MAX_DETAIL_EVENTS).contains(&request.limit)
        || request
            .cursor
            .as_ref()
            .is_some_and(|cursor| request.overview_only || !digest(&cursor.fingerprint))
    {
        return Err(DetailIssue::InvalidRequest);
    }
    let guard = if session_files_only {
        pin_session_namespace(&snapshot.record)?
    } else {
        pin_reader_namespace(&snapshot.record)?
    };
    // The archive's size does not bound a read; only the number of files the pin holds.
    if guard.file_count() > MAX_INPUT_FILES {
        return Err(DetailIssue::InputTooLarge);
    }
    let roots = explicit_roots(snapshot);
    let reader = mukade::reader::for_harness(snapshot.harness)
        .map_err(|_| DetailIssue::UnsupportedHarness)?;
    let info = reader
        .overview(&request.native_id, &roots)
        .map_err(|error| lookup_issue(&error))?;
    // A session Mukade knows only from other records (a Claude session that wrote no
    // transcript, named by history.jsonl or a live registration) is listed as unreadable with
    // that record as its path: there is no conversation to read.
    if info.unreadable.is_some() {
        return Err(DetailIssue::SessionNotFound);
    }
    if session_files_only && !guard.holds(Path::new(&info.path)) {
        // Mukade found the session in a file this pin left out: nothing read so far is kept.
        return open_under(snapshot, request, false);
    }
    guard.verify_session_current(Path::new(&info.path))?;
    if info.id != request.native_id {
        return Err(DetailIssue::SessionNotFound);
    }
    let bytes = guard.file_size(Path::new(&info.path))?;
    Ok(Opened { guard, roots, reader, info, bytes })
}
impl Opened {
    /// The session with every string cut to `max_string_bytes`: the entries, ids, kinds and
    /// order of the full read, whatever its size.
    fn clipped(
        &self,
        snapshot: &TrustedRootSnapshot,
        request: &DetailRequest,
        max_string_bytes: usize,
    ) -> Result<(Session, String), DetailIssue> {
        let clipped = self
            .reader
            .read_clipped(&request.native_id, &self.roots, max_string_bytes)
            .map_err(|error| lookup_issue(&error))?;
        let fingerprint = self.verified(snapshot, request, &clipped.session.meta)?;
        Ok((clipped.session, fingerprint))
    }
    /// The entries of the full read, one at a time with their full content. `on_entry` returns
    /// `StreamStop` to end the read early; the caller then knows why.
    pub(crate) fn streamed(
        &self,
        snapshot: &TrustedRootSnapshot,
        request: &DetailRequest,
        on_entry: &mut dyn FnMut(Entry) -> Result<(), StreamStop>,
    ) -> Result<(Meta, String), StreamError> {
        self.streamed_with_meta(snapshot, request, &mut |_| Ok(()), on_entry)
    }
    /// [`Opened::streamed`], handing the session's meta over before the first entry (Mukade's
    /// `read_streamed_with_meta`): what a document that opens with its meta needs.
    pub(crate) fn streamed_with_meta(
        &self,
        snapshot: &TrustedRootSnapshot,
        request: &DetailRequest,
        on_meta: &mut dyn FnMut(&Meta) -> Result<(), StreamStop>,
        on_entry: &mut dyn FnMut(Entry) -> Result<(), StreamStop>,
    ) -> Result<(Meta, String), StreamError> {
        let meta = self
            .reader
            .read_streamed_with_meta(
                &request.native_id,
                &self.roots,
                &mut |meta| on_meta(meta).map_err(anyhow::Error::from),
                &mut |entry| on_entry(entry).map_err(anyhow::Error::from),
            )
            .map_err(|error| {
                if error.downcast_ref::<StreamStop>().is_some() {
                    StreamError::Stopped
                } else {
                    StreamError::Detail(lookup_issue(&error))
                }
            })?;
        let fingerprint = self
            .verified(snapshot, request, &meta)
            .map_err(StreamError::Detail)?;
        Ok((meta, fingerprint))
    }
    /// After the read: the namespace and selected file are unchanged, it is the session and
    /// harness asked for, and the fingerprint (checked against the cursor) is the file's digest.
    fn verified(
        &self,
        snapshot: &TrustedRootSnapshot,
        request: &DetailRequest,
        meta: &Meta,
    ) -> Result<String, DetailIssue> {
        self.guard.verify_session_current(Path::new(&self.info.path))?;
        if meta.id != request.native_id || meta.harness != snapshot.harness {
            return Err(DetailIssue::SourceChanged);
        }
        session_fingerprint(snapshot, request, meta)
    }
}
/// The clipped read of one session, verified: for RAW, which clips at its own output bound.
pub(crate) fn read_clipped_verified(
    snapshot: &TrustedRootSnapshot,
    request: &DetailRequest,
    max_string_bytes: usize,
) -> Result<(Session, String), DetailIssue> {
    open(snapshot, request)?.clipped(snapshot, request, max_string_bytes)
}
fn read(
    snapshot: &TrustedRootSnapshot,
    request: &DetailRequest,
) -> Result<DetailPage, DetailIssue> {
    let opened = open(snapshot, request)?;
    // Lineage needs every row of the listing, which reads each file's two ends and never a
    // conversation: its cost follows the archive's file count, not any session's size.
    let catalog = opened
        .reader
        .list_all(&opened.roots)
        .map_err(|_| DetailIssue::ReadUnavailable)?;
    let (session, fingerprint) = opened.clipped(snapshot, request, VIEW_CLIP_BYTES)?;
    // The pinned Claude/Codex/Pi list readers do not supply session-parent
    // metadata. A complete list must never turn this into "zero children".
    let lineage = crate::session_lineage::derive_lineage(
        &catalog, &request.native_id, crate::session_lineage::ParentMetadata::Unavailable,
    )
        .map_err(|_| DetailIssue::InvalidRequest)?;
    project(snapshot, request, &session, fingerprint, lineage)
}
/// Mukade's overview of the session: what the list reads from the file's two ends. Constant
/// cost whatever the session's size; no event, ledger or activity. Its lineage is that of the
/// one row: the parent it names is unresolved here and the children are unknown, which is
/// what the row knows; resume and thread names read neither.
fn read_overview(
    snapshot: &TrustedRootSnapshot,
    request: &DetailRequest,
) -> Result<DetailPage, DetailIssue> {
    let opened = open_session_file(snapshot, request)?;
    let info = &opened.info;
    let lineage = crate::session_lineage::derive_lineage(
        std::slice::from_ref(info), &request.native_id, crate::session_lineage::ParentMetadata::Unavailable,
    )
        .map_err(|_| DetailIssue::InvalidRequest)?;
    // A change detector over what was read, never a cursor: an overview has no pages. The
    // file's size comes from the pinned namespace, since the overview carries none.
    let mut hash = Sha256::new();
    hash.update(b"overview");
    hash.update(MUKADE_REVISION.as_bytes());
    hash.update(snapshot.record.id.as_bytes());
    hash.update(snapshot.record.config_revision.to_le_bytes());
    hash.update(request.native_id.as_bytes());
    hash.update(opened.bytes.to_le_bytes());
    for field in [&info.title, &info.cwd, &info.updated, &info.parent] {
        let value = field.as_deref().unwrap_or("\u{0}");
        hash.update((value.len() as u64).to_le_bytes());
        hash.update(value.as_bytes());
    }
    Ok(DetailPage {
        root: request.root.clone(),
        native_id: request.native_id.clone(),
        harness: snapshot.harness,
        overview: DetailOverview {
            title: optional(info.title.as_deref(), 512),
            working_directory: optional(info.cwd.as_deref(), 1024),
            created_at: None,
            updated_at: optional(info.updated.as_deref(), 64),
            parent_native_id: optional(info.parent.as_deref(), 256),
            harness_version: None,
        },
        fingerprint: format!("{:x}", hash.finalize()),
        total_events: 0,
        events: Vec::new(),
        next: None,
        notices: vec!["Overview only: the conversation itself was not read.".to_owned()],
        lineage,
        ledger: Default::default(),
        activity_title: None,
        touched_paths: Vec::new(),
    })
}
/// The digest of the whole file (Mukade's `source_sha256`, the same for a clipped, a streamed
/// and a whole read) bound to the root, the pin revision and the session.
pub(crate) fn session_fingerprint(
    snapshot: &TrustedRootSnapshot,
    request: &DetailRequest,
    meta: &Meta,
) -> Result<String, DetailIssue> {
    let source = meta
        .source_sha256
        .as_deref()
        .filter(|value| digest(value))
        .ok_or(DetailIssue::ReadUnavailable)?;
    let mut hash = Sha256::new();
    hash.update(MUKADE_REVISION.as_bytes());
    hash.update(snapshot.record.id.as_bytes());
    hash.update(snapshot.record.config_revision.to_le_bytes());
    hash.update(request.native_id.as_bytes());
    hash.update(source.as_bytes());
    let fingerprint = format!("{:x}", hash.finalize());
    if request
        .cursor
        .as_ref()
        .is_some_and(|cursor| cursor.fingerprint != fingerprint)
    {
        return Err(DetailIssue::SourceChanged);
    }
    Ok(fingerprint)
}
fn project(
    snapshot: &TrustedRootSnapshot,
    request: &DetailRequest,
    session: &Session,
    fingerprint: String,
    lineage: crate::session_lineage::SessionLineage,
) -> Result<DetailPage, DetailIssue> {
    let offset = request.cursor.as_ref().map_or(0, |cursor| cursor.offset);
    let mut projection = Projection {
        offset,
        limit: request.limit,
        events: Vec::new(),
        total: 0,
        bytes: 0,
        full: false,
    };
    for (index, entry) in session.entries.iter().enumerate() {
        project_entry(&mut projection, index, entry);
    }
    if offset > projection.total {
        return Err(DetailIssue::SourceChanged);
    }
    let end = offset + projection.events.len();
    Ok(DetailPage {
        root: request.root.clone(),
        native_id: request.native_id.clone(),
        harness: snapshot.harness,
        overview: DetailOverview {
            title: optional(session.meta.title.as_deref(), 512),
            working_directory: optional(session.meta.cwd.as_deref(), 1024),
            created_at: optional(session.meta.created.as_deref(), 64),
            updated_at: optional(session.meta.updated.as_deref(), 64),
            parent_native_id: optional(session.meta.parent.as_deref(), 256),
            harness_version: optional((session.meta.harness_version != "unknown").then_some(session.meta.harness_version.as_str()), 64),
        },
        fingerprint: fingerprint.clone(),
        total_events: projection.total,
        events: projection.events,
        next: (end < projection.total).then_some(DetailCursor {
            offset: end,
            fingerprint,
        }),
        notices: [
            "Raw records, reasoning, tool arguments and attachment content are omitted.",
            "Appends remain allowed; renaming or deleting pinned files may fail briefly during the read.",
        ].into_iter().map(str::to_owned).collect(),
        lineage,
        ledger: session_ledger(session),
        activity_title: activity_title(session),
        touched_paths: touched_paths(session),
    })
}
/// Targets of edit/write tool calls, deduplicated, newest last, bounded to 200 paths / 16 KiB.
pub fn touched_paths(session: &Session) -> Vec<String> {
    const WRITERS: [&str; 9] = ["edit", "write", "multiedit", "apply_patch", "str_replace", "create", "notebookedit", "write_file", "edit_file"];
    let mut out: Vec<String> = Vec::new();
    let mut bytes = 0;
    for entry in &session.entries {
        let Kind::Step { calls, .. } = &entry.kind else { continue };
        for call in calls {
            if !WRITERS.contains(&call.source_name.to_ascii_lowercase().as_str()) {
                continue;
            }
            let Some(path) = ["file_path", "path", "notebook_path"].iter().find_map(|k| call.input.get(*k).and_then(|v| v.as_str())) else { continue };
            if path.is_empty() || path.len() > 512 || path.chars().any(char::is_control) {
                continue;
            }
            out.retain(|p| p != path);
            out.push(path.to_owned());
        }
    }
    while out.len() > 200 {
        out.remove(0);
    }
    out.retain(|p| {
        bytes += p.len();
        bytes <= 16 * 1024
    });
    out
}

/// FRD-0010 R17-R18 without a model: the last tool call (name and target) or the last
/// non-empty line of the last assistant text, redacted, at most 12 words.
pub fn activity_title(session: &Session) -> Option<String> {
    let target = |input: &serde_json::Value| {
        ["file_path", "path", "command", "cmd", "pattern", "query", "url", "description"]
            .iter()
            .find_map(|k| input.get(*k))
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Array(a) => a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(" "),
                other => other.to_string(),
            })
            .unwrap_or_default()
    };
    let raw = session.entries.iter().rev().find_map(|entry| match &entry.kind {
        Kind::Step { calls, text, .. } => calls
            .last()
            .map(|call| format!("{} {}", call.source_name, target(&call.input).lines().next().unwrap_or_default()))
            .or_else(|| text.iter().rev().flat_map(|t| t.lines().rev()).find(|l| !l.trim().is_empty()).map(str::to_owned)),
        _ => None,
    })?;
    let redacted = shinatama_export::Redactor::for_seed().redact(raw.trim());
    let words: Vec<&str> = redacted.split_whitespace().take(12).collect();
    (!words.is_empty()).then(|| words.join(" ").chars().take(160).collect())
}
/// Projects the session's native records into the token ledger (FRD-0013 R35).
pub(crate) fn session_ledger(session: &Session) -> shinatama_token_economy::SessionLedger {
    use shinatama_token_economy::LedgerEvent;
    let mut events = Vec::with_capacity(session.entries.len() * 2);
    for entry in &session.entries {
        match &entry.kind {
            Kind::User { .. } => events.push(LedgerEvent::UserTurn),
            Kind::Step { calls, .. } => {
                events.push(LedgerEvent::Step { records: &entry.raw });
                for call in calls {
                    events.push(LedgerEvent::ToolCall { name: &call.source_name, input: &call.input });
                }
            }
            Kind::Compaction { .. } => events.push(LedgerEvent::Compaction),
            Kind::Results { .. } | Kind::Meta { .. } => events.push(LedgerEvent::Other { records: &entry.raw }),
        }
    }
    shinatama_token_economy::build(events)
}
// Detail and Find share the exact event projection, including omitted markers.
// A count-only projection never evaluates text callbacks or allocates event data.
pub(crate) fn visible_event_count(entry: &Entry) -> usize {
    let mut projection = Projection {
        offset: usize::MAX, limit: 0, events: Vec::new(), total: 0, bytes: 0, full: false,
    };
    project_entry(&mut projection, 0, entry);
    projection.total
}
fn project_entry(projection: &mut Projection, index: usize, entry: &Entry) {
        match &entry.kind {
            Kind::User { text, attachments } => {
                projection.push(index, entry, DetailEventKind::UserText, false, || {
                    clip([text.as_str()])
                });
                if !attachments.is_empty() {
                    projection.push(index, entry, DetailEventKind::AttachmentOmitted, true, || clip(["Attachment references present; files and raw payloads are not opened."]));
                }
            }
            Kind::Step {
                text,
                calls,
                reasoning,
                ..
            } => {
                projection.push(index, entry, DetailEventKind::AssistantText, false, || {
                    clip(text.iter().map(String::as_str))
                });
                for call in calls {
                    projection.push(index, entry, DetailEventKind::ToolCall, true, || {
                        clip([
                            call.source_name.as_str(),
                            "Tool arguments are not included in this preview.",
                        ])
                    });
                }
                if !reasoning.is_empty() {
                    projection.push(
                        index,
                        entry,
                        DetailEventKind::ReasoningOmitted,
                        true,
                        || clip(["Reasoning and signed/encrypted payloads are not displayed."]),
                    );
                }
            }
            Kind::Results { results } => {
                for result in results {
                    match &result.content {
                    ResultContent::Text(text) => projection.push(index, entry, DetailEventKind::ToolResult, false, || clip([text.as_str()])),
                    ResultContent::Blocks(_) => projection.push(index, entry, DetailEventKind::ToolResult, true, || clip(["Structured tool result omitted; no raw or attachment content was requested."])),
                }
                }
            }
            Kind::Compaction { summary, .. } => {
                projection.push(index, entry, DetailEventKind::Compaction, false, || {
                    clip([summary.as_str()])
                })
            }
            Kind::Meta { .. } => projection.push(index, entry, DetailEventKind::Meta, true, || {
                clip(["Native metadata record; raw content is not displayed."])
            }),
        }
}
struct Projection {
    offset: usize,
    limit: usize,
    events: Vec<DetailEvent>,
    total: usize,
    bytes: usize,
    full: bool,
}
impl Projection {
    fn push(
        &mut self,
        entry_ordinal: usize,
        entry: &Entry,
        kind: DetailEventKind,
        omitted: bool,
        value: impl FnOnce() -> (String, bool),
    ) {
        let ordinal = self.total;
        self.total = self.total.saturating_add(1);
        if ordinal < self.offset || self.full || self.events.len() >= self.limit {
            return;
        }
        let (text, truncated) = value();
        let event = DetailEvent {
            ordinal,
            entry_ordinal,
            source_entry_id: optional(Some(&entry.id), 256),
            timestamp: None,
            kind,
            text,
            truncated,
            payload_omitted: omitted,
        };
        let Ok(bytes) = serde_json::to_vec(&event) else {
            self.full = true;
            return;
        };
        if self.bytes + bytes.len() + 1 > EVENT_PAGE_BYTES {
            self.full = true;
            return;
        }
        self.bytes += bytes.len() + 1;
        self.events.push(event);
    }
}
fn clip<'a>(parts: impl IntoIterator<Item = &'a str>) -> (String, bool) {
    let mut result = String::with_capacity(MAX_TEXT_BYTES);
    let mut truncated = false;
    for part in parts {
        if !result.is_empty() && !part.is_empty() {
            if result.len() == MAX_TEXT_BYTES {
                truncated = true;
                break;
            }
            result.push('\n');
        }
        let remaining = MAX_TEXT_BYTES.saturating_sub(result.len());
        if part.len() > remaining {
            let mut end = remaining;
            while !part.is_char_boundary(end) {
                end -= 1;
            }
            result.push_str(&part[..end]);
            truncated = true;
            break;
        }
        result.push_str(part);
    }
    (result, truncated)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use shinatama_state::{
        location::{LocationRequest, StorageLocation},
        records::{RootAccess, RootKind, RootOrigin, RootScope},
    };
    use std::fs;
    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            assert_eq!(
                self.0.parent(),
                Some(fs::canonicalize(std::env::temp_dir()).unwrap().as_path())
            );
            assert!(
                self.0
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("shinatama-detail-")
            );
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn unrelated_writer(harness: &str) {
        use std::io::Write;

        let fixture = Fixture(fs::canonicalize(std::env::temp_dir()).unwrap()
            .join(format!("shinatama-detail-{}", uuid::Uuid::new_v4())));
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: std::env::temp_dir(),
            home_override: Some(fixture.0.clone()),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        }).unwrap();
        location.prepare_private_directory().unwrap();
        let native_id = "01a17e07-93f7-7a73-85c7-9f0205836600";
        let (relative, body) = if harness == "codex" {
            (format!("sessions/2026/11/16/rollout-2026-11-16T12-00-00-{native_id}.jsonl"),
                format!("{}\n{}\n",
                    serde_json::json!({"type":"session_meta","payload":{"id":native_id,"cwd":"C:\\synthetic-project","timestamp":"2026-11-16T12:00:00Z","cli_version":"synthetic"}}),
                    serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}})))
        } else {
            (format!("projects/fixture-project/{native_id}.jsonl"),
                format!("{}\n", serde_json::json!({"type":"user","uuid":"entry-user","sessionId":native_id,
                    "cwd":"C:\\synthetic-project","timestamp":"2026-11-16T12:00:00Z","message":{"role":"user","content":"hello"}})))
        };
        let selected = location.directory().join(&relative);
        fs::create_dir_all(selected.parent().unwrap()).unwrap();
        fs::write(&selected, &body).unwrap();
        let other = location.directory().join(relative.replace(native_id, "01a17e07-93f7-7a73-85c7-9f0205836601"));
        fs::write(&other, body.replace(native_id, "01a17e07-93f7-7a73-85c7-9f0205836601")).unwrap();
        let index = location.directory().join("session_index.jsonl");
        if harness == "codex" {
            fs::write(&index, "").unwrap();
        }
        let record = StoreRootRecord {
            id: "concurrent-detail-root".into(), harness: harness.into(),
            kind: RootKind::Box, scope: RootScope::Primary,
            resolved_path: location.directory().to_string_lossy().trim_start_matches(r"\\?\").into(),
            origin: RootOrigin::Configured, config_revision: 1,
            access: RootAccess::ReadWrite, available: true,
        };
        let root = RootSelector { store_root_id: record.id.clone(), configuration_revision: 1 };
        let snapshot = TrustedRootSnapshot::from_record(&record, &root).unwrap();
        let request = DetailRequest { root, native_id: native_id.into(), cursor: None, limit: 1, overview_only: true };
        let opened = open(&snapshot, &request).unwrap();
        // A different running agent writes while the chosen session is being read.
        fs::OpenOptions::new().append(true).open(&other).unwrap().write_all(b"\n").unwrap();
        if harness == "codex" {
            fs::OpenOptions::new().append(true).open(&index).unwrap().write_all(
                b"{\"id\":\"01a17e07-93f7-7a73-85c7-9f0205836601\",\"thread_name\":\"Other thread\"}\n").unwrap();
        }
        assert!(opened.clipped(&snapshot, &request, 4096).is_ok(),
            "{harness}: an unrelated writer must not invalidate the chosen session");
        // Archive-wide listings still require all their inputs to remain stable.
        assert!(matches!(opened.guard.verify_current(), Err(SessionReadIssue::SourceChanged)));
        // Changes to the chosen transcript must still be refused.
        fs::OpenOptions::new().append(true).open(&selected).unwrap().write_all(b"\n").unwrap();
        assert!(matches!(opened.clipped(&snapshot, &request, 4096), Err(DetailIssue::SourceChanged)));
    }
    /// 2026-11-19: the overview of a session, which is all Resume reads, is of its own file.
    /// The files of its folder (one workflow run wrote 5,249 subagent files there) are not
    /// pinned for it, and a subagent writing one meanwhile does not refuse it.
    #[test]
    fn an_overview_is_not_refused_by_what_its_session_folder_holds() {
        let fixture = Fixture(fs::canonicalize(std::env::temp_dir()).unwrap()
            .join(format!("shinatama-detail-{}", uuid::Uuid::new_v4())));
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: std::env::temp_dir(),
            home_override: Some(fixture.0.clone()),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        }).unwrap();
        location.prepare_private_directory().unwrap();
        let native_id = "01a17e07-93f7-7a73-85c7-9f0205836600";
        let transcript = location.directory().join(format!("projects/fixture-project/{native_id}.jsonl"));
        let subagents = location.directory().join(format!("projects/fixture-project/{native_id}/subagents"));
        fs::create_dir_all(&subagents).unwrap();
        fs::write(&transcript, format!("{}\n", serde_json::json!({"type":"user","uuid":"entry-user","sessionId":native_id,
            "cwd":"C:\\synthetic-project","timestamp":"2026-11-19T12:00:00Z","message":{"role":"user","content":"hello"}}))).unwrap();
        let child = format!("{}\n", serde_json::json!({"type":"user","uuid":"entry-child","sessionId":native_id,"isSidechain":true,
            "cwd":"C:\\synthetic-project","timestamp":"2026-11-19T12:00:01Z","message":{"role":"user","content":"task"}}));
        fs::write(subagents.join("agent-a1.jsonl"), &child).unwrap();
        let record = StoreRootRecord {
            id: "session-folder-root".into(), harness: "claude_code".into(),
            kind: RootKind::Box, scope: RootScope::Primary,
            resolved_path: location.directory().to_string_lossy().trim_start_matches(r"\\?\").into(),
            origin: RootOrigin::Configured, config_revision: 1,
            access: RootAccess::ReadWrite, available: true,
        };
        let root = RootSelector { store_root_id: record.id.clone(), configuration_revision: 1 };
        let snapshot = TrustedRootSnapshot::from_record(&record, &root).unwrap();
        let request = DetailRequest { root, native_id: native_id.into(), cursor: None, limit: 1, overview_only: true };
        let opened = open_session_file(&snapshot, &request).unwrap();
        assert_eq!(opened.guard.file_count(), 1, "only the session file is pinned for an overview");
        // A subagent of a running session writes a new transcript during the read.
        fs::write(subagents.join("agent-b2.jsonl"), &child).unwrap();
        opened.guard.verify_session_current(Path::new(&opened.info.path)).unwrap();
        drop(opened);
        let reply = read_detail(&snapshot, &request);
        assert!(reply.issue.is_none());
        assert_eq!(reply.page.unwrap().overview.working_directory.as_deref(), Some("C:\\synthetic-project"));
        // A read that follows the session into its folder still pins all of it.
        assert_eq!(open(&snapshot, &request).unwrap().guard.file_count(), 3);
    }
    /// 2026-11-18: a Claude session in which only `/resume` was typed has no transcript, but
    /// `history.jsonl` names it, so Mukade's overview lists it as unreadable with the history
    /// file as its path. That path is outside the pinned `projects` tree, and the read used to
    /// fail as an unsafe namespace ("unavailable") instead of "no such conversation".
    #[test]
    fn a_claude_session_known_only_from_its_history_is_not_found() {
        let fixture = Fixture(fs::canonicalize(std::env::temp_dir()).unwrap()
            .join(format!("shinatama-detail-{}", uuid::Uuid::new_v4())));
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: std::env::temp_dir(),
            home_override: Some(fixture.0.clone()),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        }).unwrap();
        location.prepare_private_directory().unwrap();
        let native_id = "e32608a8-01a0-4a63-a51f-25c500c8a2e2";
        let said = "01a17e07-93f7-7a73-85c7-9f0205836601";
        let transcript = location.directory().join(format!("projects/fixture-project/{said}.jsonl"));
        fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        fs::write(&transcript, format!("{}\n", serde_json::json!({"type":"user","uuid":"entry-user","sessionId":said,
            "cwd":"C:\\synthetic-project","timestamp":"2026-11-18T12:00:00Z","message":{"role":"user","content":"hello"}}))).unwrap();
        fs::write(location.directory().join("history.jsonl"), format!("{}\n", serde_json::json!({"display":"/resume",
            "pastedContents":{},"timestamp":1_794_999_806_382_i64,"project":"C:\\synthetic-project","sessionId":native_id}))).unwrap();
        let record = StoreRootRecord {
            id: "history-only-root".into(), harness: "claude_code".into(),
            kind: RootKind::Box, scope: RootScope::Primary,
            resolved_path: location.directory().to_string_lossy().trim_start_matches(r"\\?\").into(),
            origin: RootOrigin::Configured, config_revision: 1,
            access: RootAccess::ReadWrite, available: true,
        };
        let root = RootSelector { store_root_id: record.id.clone(), configuration_revision: 1 };
        let snapshot = TrustedRootSnapshot::from_record(&record, &root).unwrap();
        let request = DetailRequest { root, native_id: native_id.into(), cursor: None, limit: 1, overview_only: true };
        let refused = open(&snapshot, &request).err();
        assert_eq!(refused, Some(DetailIssue::SessionNotFound));
        let reply = read_detail(&snapshot, &request);
        assert!(reply.page.is_none());
        assert!(matches!(reply.issue, Some(DetailIssue::SessionNotFound)));
        // The session that has a transcript still opens.
        let request = DetailRequest { native_id: said.into(), ..request };
        assert!(open(&snapshot, &request).is_ok());
    }
    #[test]
    fn claude_selected_session_survives_an_unrelated_writer() {
        unrelated_writer("claude_code");
    }
    #[test]
    fn codex_selected_session_survives_an_unrelated_writer() {
        unrelated_writer("codex");
    }
    #[test]
    fn real_reader_pages_utf8_without_raw_reasoning_and_reads_sessions_past_16_mib() {
        let fixture = Fixture(
            fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!("shinatama-detail-{}", uuid::Uuid::new_v4())),
        );
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: std::env::temp_dir(),
            home_override: Some(fixture.0.clone()),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        })
        .unwrap();
        location.prepare_private_directory().unwrap();
        let project = location.directory().join("projects/fixture-project");
        fs::create_dir_all(&project).unwrap();
        let path = project.join("fixture-session.jsonl");
        let rows = [
            serde_json::json!({"type":"user","uuid":"entry-user","parentUuid":null,"sessionId":"fixture-session",
            "cwd":"C:\\synthetic-project","timestamp":"2026-11-03T00:00:00Z","message":{"role":"user","content":"hello"}}),
            serde_json::json!({"type":"assistant","uuid":"entry-assistant","parentUuid":"entry-user","sessionId":"fixture-session",
            "message":{"id":"message-assistant","role":"assistant","model":"fixture-model","stop_reason":"end_turn",
                "content":[{"type":"text","text":"🦀".repeat(1400)},{"type":"thinking","thinking":"never-display","signature":"opaque-secret"}]}}),
        ];
        let bytes = rows
            .iter()
            .map(|row| serde_json::to_string(row).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(&path, &bytes).unwrap();
        let record = StoreRootRecord {
            id: "fixture-detail-root".into(),
            harness: "claude_code".into(),
            kind: RootKind::Box,
            scope: RootScope::Primary,
            resolved_path: location
                .directory()
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .into(),
            origin: RootOrigin::Configured,
            config_revision: 1,
            access: RootAccess::ReadWrite,
            available: true,
        };
        let root = RootSelector {
            store_root_id: record.id.clone(),
            configuration_revision: 1,
        };
        let snapshot = TrustedRootSnapshot::from_record(&record, &root).unwrap();
        let mut request = DetailRequest {
            root,
            native_id: "fixture-session".into(),
            cursor: None,
            limit: 1,
            overview_only: false,
        };
        let first = read_detail(&snapshot, &request);
        assert!(first.issue.is_none(), "{:?}", first.issue);
        let first_page = first.page.unwrap();
        let first_page_fingerprint = first_page.fingerprint.clone();
        assert_eq!(first_page.events.len(), 1);
        assert_eq!(first_page.events[0].text, "hello");
        request.cursor = first_page.next;
        request.limit = 50;
        let next = read_detail(&snapshot, &request);
        assert!(next.issue.is_none(), "{:?}", next.issue);
        let assistant = next
            .page
            .as_ref()
            .unwrap()
            .events
            .iter()
            .find(|event| matches!(event.kind, DetailEventKind::AssistantText))
            .unwrap();
        assert_eq!(assistant.text, "🦀".repeat(1024));
        assert!(assistant.truncated);
        let wire = String::from_utf8(serde_json::to_vec(&next).unwrap()).unwrap();
        assert!(!wire.contains("never-display"));
        assert!(!wire.contains("opaque-secret"));
        assert_eq!(fs::read(&path).unwrap(), bytes.as_bytes());
        request.cursor = None;
        request.native_id = "../fixture-session".into();
        assert_eq!(
            read_detail(&snapshot, &request).issue,
            Some(DetailIssue::InvalidRequest)
        );
        request.native_id = "fixture-session".into();
        // Other sessions in the same archive do not count: only the session read is
        // materialized, and the list reads each other file's two ends only.
        let other = project.join("other-session.jsonl");
        fs::write(&other, bytes.replace("fixture-session", "other-session")).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&other)
            .unwrap()
            .set_len(17 * 1024 * 1024)
            .unwrap();
        let beside_large = read_detail(&snapshot, &request);
        assert!(beside_large.issue.is_none(), "{:?}", beside_large.issue);
        fs::remove_file(&other).unwrap();
        // A session past 16 MiB is read like any other: Mukade clips its strings at the length the
        // view shows, so the page comes back with the same events, the long result cut and said to be.
        let mut grown = bytes.clone();
        for row in [
            serde_json::json!({"type":"assistant","uuid":"entry-call","parentUuid":"entry-assistant","sessionId":"fixture-session",
                "message":{"id":"message-call","role":"assistant","model":"fixture-model","stop_reason":"tool_use",
                    "content":[{"type":"tool_use","id":"call-1","name":"Read","input":{"file_path":"C:\\synthetic-project\\a.txt"}}]}}),
            serde_json::json!({"type":"user","uuid":"entry-result","parentUuid":"entry-call","sessionId":"fixture-session",
                "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-1","content":"é".repeat(9 * 1024 * 1024)}]}}),
        ] {
            grown.push_str(&serde_json::to_string(&row).unwrap());
            grown.push('\n');
        }
        fs::write(&path, &grown).unwrap();
        assert!(fs::metadata(&path).unwrap().len() > 17 * 1024 * 1024);
        request.cursor = None;
        request.limit = 50;
        let large = read_detail(&snapshot, &request);
        assert!(large.issue.is_none(), "{:?}", large.issue);
        let large_page = large.page.unwrap();
        let result = large_page
            .events
            .iter()
            .find(|event| matches!(event.kind, DetailEventKind::ToolResult))
            .unwrap();
        assert_eq!(result.text, "é".repeat(2048));
        assert!(result.truncated);
        assert!(large_page.notices.iter().all(|notice| !notice.contains("16 MiB")));
        assert_eq!(large_page.fingerprint.len(), 64);
        assert_ne!(large_page.fingerprint, first_page_fingerprint);
        assert_eq!(fs::read(&path).unwrap(), grown.as_bytes());
        request.cursor = Some(DetailCursor { offset: 0, fingerprint: large_page.fingerprint.clone() });
        assert!(read_detail(&snapshot, &request).issue.is_none());
        request.cursor = None;
        // An overview is Mukade's: the file's two ends, so no session is too large for it either.
        request.overview_only = true;
        let overview = read_detail(&snapshot, &request);
        assert!(overview.issue.is_none(), "{:?}", overview.issue);
        let page = overview.page.unwrap();
        assert_eq!(page.overview.working_directory.as_deref(), Some("C:\\synthetic-project"));
        assert!(page.events.is_empty() && page.next.is_none() && page.total_events == 0);
        assert_eq!(page.fingerprint.len(), 64);
        request.cursor = Some(DetailCursor { offset: 0, fingerprint: page.fingerprint });
        assert_eq!(
            read_detail(&snapshot, &request).issue,
            Some(DetailIssue::InvalidRequest)
        );
    }
}

/// A Claude Code session of about 17.5 MiB (past Mukade's 16 MiB streaming threshold) whose one
/// tool result holds a marker far beyond the 4 KiB the view shows.
#[cfg(all(test, windows))]
pub(crate) mod large_fixture {
    use super::*;
    use shinatama_state::{
        location::{LocationRequest, StorageLocation},
        records::{RootAccess, RootKind, RootOrigin, RootScope},
    };
    use std::fs;

    pub const SESSION: &str = "large-session";
    pub const DEEP_MARKER: &str = "DEEP-MARKER-ZEBRA";

    pub struct LargeSession {
        dir: PathBuf,
        pub snapshot: TrustedRootSnapshot,
        pub root: RootSelector,
        pub path: PathBuf,
        pub bytes: u64,
        /// The archive root's path, for reading the session whole.
        pub root_path: PathBuf,
    }
    impl LargeSession {
        /// The session read whole by Mukade (what an in-memory render starts from).
        pub fn whole(&self) -> Session {
            let mut roots = StoreRoots::default();
            roots.claude_config_dir = Some(self.root_path.clone());
            mukade::reader::for_harness(HarnessId::ClaudeCode)
                .unwrap()
                .read(SESSION, &roots)
                .unwrap()
        }
    }
    impl Drop for LargeSession {
        fn drop(&mut self) {
            if self.dir.parent() == Some(fs::canonicalize(std::env::temp_dir()).unwrap().as_path())
                && self.dir.file_name().unwrap().to_string_lossy().starts_with("shinatama-large-")
            {
                let _ = fs::remove_dir_all(&self.dir);
            }
        }
    }

    pub fn request(large: &LargeSession) -> DetailRequest {
        DetailRequest {
            root: large.root.clone(),
            native_id: SESSION.into(),
            cursor: None,
            limit: 50,
            overview_only: false,
        }
    }

    pub fn build() -> LargeSession {
        build_sized(1, 17 * 1024 * 1024 + 512 * 1024)
    }

    /// `results` tool results of about `result_bytes` each; the marker is in the first only.
    pub fn build_sized(results: usize, result_bytes: usize) -> LargeSession {
        let dir = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("shinatama-large-{}", uuid::Uuid::new_v4()));
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: std::env::temp_dir(),
            home_override: Some(dir.clone()),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        })
        .unwrap();
        location.prepare_private_directory().unwrap();
        let project = location.directory().join("projects/large-project");
        fs::create_dir_all(&project).unwrap();
        let path = project.join(format!("{SESSION}.jsonl"));
        let mut rows = vec![serde_json::json!({"type":"user","uuid":"u1","parentUuid":null,"sessionId":SESSION,
            "cwd":"C:\\synthetic-project","timestamp":"2026-11-13T00:00:00Z","message":{"role":"user","content":"read the big file"}})];
        let mut parent = "u1".to_owned();
        for index in 0..results {
            let big = if index == 0 {
                let head = result_bytes * 12 / 17;
                format!("{}{DEEP_MARKER}{}", "a".repeat(head), "b".repeat(result_bytes - head - DEEP_MARKER.len()))
            } else {
                format!("result {index}
{}", "c".repeat(result_bytes))
            };
            rows.push(serde_json::json!({"type":"assistant","uuid":format!("a{index}"),"parentUuid":parent,"sessionId":SESSION,
                "message":{"id":format!("m{index}"),"role":"assistant","model":"fixture-model","stop_reason":"tool_use",
                    "content":[{"type":"tool_use","id":format!("call-{index}"),"name":"Read","input":{"file_path":"C:\\synthetic-project\\big.txt"}}]}}));
            rows.push(serde_json::json!({"type":"user","uuid":format!("r{index}"),"parentUuid":format!("a{index}"),"sessionId":SESSION,
                "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":format!("call-{index}"),"content":big}]}}));
            parent = format!("r{index}");
        }
        rows.push(serde_json::json!({"type":"assistant","uuid":"a-last","parentUuid":parent,"sessionId":SESSION,
            "message":{"id":"m-last","role":"assistant","model":"fixture-model","stop_reason":"end_turn",
                "content":[{"type":"text","text":"done reading"}]}}));
        let mut text = String::new();
        for row in rows {
            text.push_str(&serde_json::to_string(&row).unwrap());
            text.push('\n');
        }
        fs::write(&path, &text).unwrap();
        let record = StoreRootRecord {
            id: "large-root".into(),
            harness: "claude_code".into(),
            kind: RootKind::Box,
            scope: RootScope::Primary,
            resolved_path: location.directory().to_string_lossy().trim_start_matches(r"\\?\").into(),
            origin: RootOrigin::Configured,
            config_revision: 1,
            access: RootAccess::ReadWrite,
            available: true,
        };
        let root = RootSelector { store_root_id: record.id.clone(), configuration_revision: 1 };
        let snapshot = TrustedRootSnapshot::from_record(&record, &root).unwrap();
        let bytes = text.len() as u64;
        assert!(bytes >= (results * result_bytes) as u64, "{bytes}");
        LargeSession { dir, snapshot, root, path, bytes, root_path: PathBuf::from(&record.resolved_path) }
    }
}

#[cfg(test)]
mod ledger_tests {
    #[test]
    fn ledger_reads_the_agents_own_usage_records() {
        let mut session = mukade::wire::from_document(&serde_json::json!({
            "meta": {"harness": "claude_code", "harness_version": "2.1.0", "id": "s"}, "messages": []
        }))
        .unwrap();
        session.entries = serde_json::from_value(serde_json::json!([
            {"id": "u1", "kind": {"user": {"text": "go", "attachments": []}}, "raw": []},
            {"id": "a1", "kind": {"step": {"reasoning": [], "text": ["ok"], "calls": [{"id": "c1", "source_name": "Read", "input": {"path": "a"}}], "end": "tool_use"}},
             "raw": [{"message": {"id": "m1", "usage": {"input_tokens": 4, "cache_read_input_tokens": 96, "output_tokens": 3}}}]},
            {"id": "k1", "kind": {"compaction": {"summary": "s", "trigger": "auto"}}, "raw": []},
            {"id": "u2", "kind": {"user": {"text": "again", "attachments": []}}, "raw": []},
            {"id": "a2", "kind": {"step": {"reasoning": [], "text": [], "calls": [{"id": "c2", "source_name": "Read", "input": {"path": "a"}}], "end": "tool_use"}},
             "raw": [{"message": {"id": "m2", "usage": {"input_tokens": 50, "cache_creation_input_tokens": 50, "output_tokens": 1}}}]}
        ]))
        .unwrap();
        let ledger = super::session_ledger(&session);
        assert_eq!(super::activity_title(&session).as_deref(), Some("Read a"), "the last tool call wins");
        assert!(super::touched_paths(&session).is_empty(), "reads are not writes");
        assert_eq!(ledger.turns, 2);
        assert_eq!(ledger.requests, 2);
        assert_eq!(ledger.total.cache_read, 96);
        assert_eq!(ledger.hit_rate_permille, Some(480));
        assert_eq!((ledger.compactions, ledger.reruns_after_compaction), (1, 1));
    }
}
