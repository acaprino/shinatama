//! Worker-only projection of the reviewed Mukade whole-session reader. Input is
//! capped BEFORE list/read; raw records, reasoning and attachment contents never
//! leave the default detail projection. RAW has a separate confirmed request.
//! No native export, auth read, alternate parser or command.
use crate::session_roots::{MUKADE_REVISION, RootSelector, SessionReadIssue, pin_reader_namespace};
use mukade::{
    Entry, HarnessId, Kind, Session, model::ResultContent, reader::SessionInfo, roots::StoreRoots,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shinatama_state::records::StoreRootRecord;
use std::path::{Path, PathBuf};

pub const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_INPUT_FILES: usize = 4096;
pub const MAX_DETAIL_EVENTS: usize = 50;
pub const MAX_TEXT_BYTES: usize = 4096;
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
}
/// Created only by trusted Rust from the resolved authoritative record. No path,
/// state-ready flag or fabricated root record is accepted from a UI selector.
pub struct TrustedRootSnapshot {
    record: StoreRootRecord,
    root: RootSelector,
    harness: HarnessId,
}
impl TrustedRootSnapshot {
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
    }
    roots
}
/// Invoked only inside the C09R-contained session helper, never the host/UI.
/// Input pagination is NOT claimed: the pin still constructs its full Session.
pub fn read_detail(snapshot: &TrustedRootSnapshot, request: &DetailRequest) -> DetailReply {
    let result = read(snapshot, request);
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
fn read(
    snapshot: &TrustedRootSnapshot,
    request: &DetailRequest,
) -> Result<DetailPage, DetailIssue> {
    let (session, catalog) = read_verified_session(snapshot, request)?;
    // The pinned Claude/Codex/Pi list readers do not supply session-parent
    // metadata. A complete list must never turn this into "zero children".
    let lineage = crate::session_lineage::derive_lineage(
        &catalog, &request.native_id, crate::session_lineage::ParentMetadata::Unavailable,
    )
        .map_err(|_| DetailIssue::InvalidRequest)?;
    project(snapshot, request, &session, lineage)
}
/// Shared ONLY by confirmed in-worker projections. Returns actual Mukade data,
/// never a host/default payload. All root, size, candidate, identity and source
/// checks remain common to normal detail and the explicitly requested RAW entry.
pub(crate) fn read_verified_session(
    snapshot: &TrustedRootSnapshot,
    request: &DetailRequest,
) -> Result<(Session, Vec<SessionInfo>), DetailIssue> {
    if request.root != snapshot.root
        || !native_id(&request.native_id)
        || !(1..=MAX_DETAIL_EVENTS).contains(&request.limit)
        || request
            .cursor
            .as_ref()
            .is_some_and(|cursor| !digest(&cursor.fingerprint))
    {
        return Err(DetailIssue::InvalidRequest);
    }
    let guard = pin_reader_namespace(&snapshot.record)?;
    // Conservative bound over the ENTIRE effective namespace, including Codex's
    // optional index/pages. No native parsing or content allocation precedes it.
    if guard.total_input_bytes() > MAX_INPUT_BYTES || guard.file_count() > MAX_INPUT_FILES {
        return Err(DetailIssue::InputTooLarge);
    }
    let roots = explicit_roots(snapshot);
    let reader = mukade::reader::for_harness(snapshot.harness)
        .map_err(|_| DetailIssue::UnsupportedHarness)?;
    let catalog = reader
        .list_all(&roots)
        .map_err(|_| DetailIssue::ReadUnavailable)?;
    guard.verify_current()?;
    let mut exact = catalog.iter().filter(|info| info.id == request.native_id);
    let candidate = exact.next().ok_or(DetailIssue::SessionNotFound)?;
    if exact.next().is_some() {
        return Err(DetailIssue::AmbiguousSession);
    }
    // The path came from this root's actual catalog, not the caller. Prefixes are
    // never accepted as session IDs; only an already pinned exact file may read.
    if guard.file_size(Path::new(&candidate.path))? > MAX_INPUT_BYTES {
        return Err(DetailIssue::InputTooLarge);
    }
    let session = reader.read(&request.native_id, &roots).map_err(|error| {
        match error.downcast_ref::<mukade::reader::Lookup>() {
            Some(mukade::reader::Lookup::Ambiguous(_)) => DetailIssue::AmbiguousSession,
            Some(mukade::reader::Lookup::NotFound(_)) => DetailIssue::SessionNotFound,
            _ => DetailIssue::ReadUnavailable,
        }
    })?;
    guard.verify_current()?;
    if session.meta.id != request.native_id || session.meta.harness != snapshot.harness {
        return Err(DetailIssue::SourceChanged);
    }
    session_fingerprint(snapshot, request, &session)?;
    Ok((session, catalog))
}
pub(crate) fn session_fingerprint(
    snapshot: &TrustedRootSnapshot,
    request: &DetailRequest,
    session: &Session,
) -> Result<String, DetailIssue> {
    let source = session
        .meta
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
    lineage: crate::session_lineage::SessionLineage,
) -> Result<DetailPage, DetailIssue> {
    let fingerprint = session_fingerprint(snapshot, request, session)?;
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
            "Detail reads are limited to archives up to 16 MiB.",
            "Raw records, reasoning, tool arguments and attachment content are omitted.",
            "Appends remain allowed; renaming or deleting pinned files may fail briefly during the read.",
        ].into_iter().map(str::to_owned).collect(),
        lineage,
        ledger: session_ledger(session),
    })
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
    #[test]
    fn real_reader_pages_utf8_without_raw_reasoning_and_refuses_large_input_before_parsing() {
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
        };
        let first = read_detail(&snapshot, &request);
        assert!(first.issue.is_none(), "{:?}", first.issue);
        let first_page = first.page.unwrap();
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
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(30 * 1024 * 1024)
            .unwrap();
        // Sparse invalid JSON proves refusal precedes even the native header read.
        assert_eq!(
            read_detail(&snapshot, &request).issue,
            Some(DetailIssue::InputTooLarge)
        );
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
        assert_eq!(ledger.turns, 2);
        assert_eq!(ledger.requests, 2);
        assert_eq!(ledger.total.cache_read, 96);
        assert_eq!(ledger.hit_rate_permille, Some(480));
        assert_eq!((ledger.compactions, ledger.reruns_after_compaction), (1, 1));
    }
}
