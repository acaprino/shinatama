//! C26 session export, run inside the contained session worker (FRD-0006 R31-R39, ADR-0021).
//!
//! The worker reads the pinned session through the same verified path as detail, projects it
//! into `shinatama_export::Transcript` (reasoning has no representation there), redacts once and
//! renders the chosen format. The trusted parent writes the bytes atomically; the worker never
//! writes. Native export is offered only where proven: for now nowhere.
use crate::session_detail::{DetailIssue, DetailRequest, TrustedRootSnapshot, read_verified_session};
use crate::session_roots::RootSelector;
use mukade::{Kind, Session, model::ResultContent};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use shinatama_export::{
    AttachmentRef, ExportItem, ExportSource, Format, FormatDeclaration, ItemKind, Mode, Omissions,
    Provenance, Redactions, Redactor, ToolInfo, Transcript, formats, markdown, stx,
};

/// Rendered content bound. Input is already capped at 16 MiB by the detail reader.
pub const MAX_EXPORT_CONTENT_BYTES: usize = 24 * 1024 * 1024;
pub const MAX_ATTACHMENT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_ATTACHMENTS_TOTAL: usize = 12 * 1024 * 1024;
/// Worker reply bound for an export (content + base64 attachments + JSON escaping).
pub const MAX_EXPORT_REPLY_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExportRequest {
    pub detail: DetailRequest,
    pub format: Format,
    pub mode: Mode,
    pub with_attachments: bool,
    pub thread: Option<String>,
    /// The user's home folder, set by the trusted parent; used only for portable redaction.
    pub home: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportIssue {
    InvalidRequest,
    FormatUnavailable,
    OutputLimit,
    Detail(DetailIssue),
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AttachmentBlob {
    pub name: String,
    pub data_base64: String,
}

/// What was written, shown before and after the export (R31).
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExportReport {
    pub format: Format,
    pub mode: Mode,
    pub declaration: FormatDeclaration,
    pub harness: String,
    pub native_id: String,
    pub source_fingerprint: String,
    pub items: u64,
    pub redactions: Redactions,
    pub omitted: Omissions,
    pub attachments_referenced: u64,
    pub attachments_copied: u64,
    pub attachments_not_copied: u64,
    pub content_bytes: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExportPayload {
    pub report: ExportReport,
    pub suggested_name: String,
    pub content: String,
    pub attachments: Vec<AttachmentBlob>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExportReply {
    pub root: RootSelector,
    pub native_id: String,
    pub payload: Option<ExportPayload>,
    pub issue: Option<ExportIssue>,
}
impl ExportReply {
    pub fn refuse(request: &ExportRequest, issue: ExportIssue) -> Self {
        Self {
            root: request.detail.root.clone(),
            native_id: request.detail.native_id.clone(),
            payload: None,
            issue: Some(issue),
        }
    }
}

pub fn validate_request(request: &ExportRequest) -> Result<(), ExportIssue> {
    let thread_ok = request.thread.as_ref().is_none_or(|thread| {
        !thread.is_empty() && thread.len() <= 128 && thread.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    });
    let home_ok = request.home.as_ref().is_none_or(|home| {
        home.len() <= 1024 && !home.chars().any(char::is_control) && std::path::Path::new(home).is_absolute()
    });
    if !thread_ok
        || !home_ok
        || request.detail.limit != 1
        || request.detail.cursor.as_ref().is_some_and(|cursor| cursor.offset != 0)
    {
        return Err(ExportIssue::InvalidRequest);
    }
    if request.format == Format::Native {
        return Err(ExportIssue::FormatUnavailable);
    }
    Ok(())
}

pub fn read_export(snapshot: &TrustedRootSnapshot, request: &ExportRequest) -> ExportReply {
    if let Err(issue) = validate_request(request) {
        return ExportReply::refuse(request, issue);
    }
    let session = match read_verified_session(snapshot, &request.detail) {
        Ok((session, _catalog)) => session,
        Err(issue) => return ExportReply::refuse(request, ExportIssue::Detail(issue)),
    };
    let fingerprint = match crate::session_detail::session_fingerprint(snapshot, &request.detail, &session) {
        Ok(value) => value,
        Err(issue) => return ExportReply::refuse(request, ExportIssue::Detail(issue)),
    };
    match render(&session, request, fingerprint) {
        Ok(payload) => ExportReply {
            root: request.detail.root.clone(),
            native_id: request.detail.native_id.clone(),
            payload: Some(payload),
            issue: None,
        },
        Err(issue) => ExportReply::refuse(request, issue),
    }
}

/// Inline attachment content found in a record, decoded and bounded.
struct Inline {
    data: Vec<u8>,
    media_type: Option<String>,
}

fn find_inline(raw: &Value) -> Option<Inline> {
    fn walk(value: &Value, depth: usize) -> Option<Inline> {
        if depth > 6 {
            return None;
        }
        let object = value.as_object()?;
        let media = ["media_type", "mediaType", "mimeType", "mime_type"]
            .iter()
            .find_map(|key| object.get(*key).and_then(Value::as_str))
            .map(str::to_owned);
        if let Some(data) = object.get("data").and_then(Value::as_str)
            && data.len() >= 16
            && let Some(bytes) = shinatama_export::base64::decode(data, MAX_ATTACHMENT_BYTES)
        {
            return Some(Inline { data: bytes, media_type: media });
        }
        object.values().find_map(|child| walk(child, depth + 1))
    }
    walk(raw, 0)
}

fn extension(media_type: Option<&str>) -> &'static str {
    match media_type.unwrap_or_default() {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "application/pdf" => "pdf",
        "text/plain" => "txt",
        _ => "bin",
    }
}

fn timestamp(entry: &mukade::Entry) -> Option<String> {
    entry.raw.iter().find_map(|record| {
        ["timestamp", "time", "created_at"]
            .iter()
            .find_map(|key| record.get(*key))
            .and_then(Value::as_str)
            .filter(|value| value.len() <= 64 && !value.chars().any(char::is_control))
            .map(str::to_owned)
    })
}

fn bounded(value: Option<&str>, cap: usize) -> Option<String> {
    value
        .filter(|v| !v.is_empty() && v.len() <= cap && !v.chars().any(|c| c.is_control() && c != '\t'))
        .map(str::to_owned)
}

/// Projects a Mukade session. Returns the transcript and the inline attachment contents.
pub fn project(session: &Session, harness: &str, thread: Option<String>, copy: bool) -> (Transcript, Vec<(String, Vec<u8>)>, u64) {
    let meta = &session.meta;
    let mut omitted = Omissions::default();
    let mut items = Vec::new();
    let mut blobs = Vec::new();
    let mut blob_total = 0usize;
    let mut not_copied = 0u64;
    for (ordinal, entry) in session.entries.iter().enumerate() {
        let provenance = Provenance {
            agent: harness.to_owned(),
            native_session_id: meta.id.clone(),
            entry_id: bounded(Some(&entry.id), 256),
            entry_ordinal: ordinal,
            timestamp: timestamp(entry),
        };
        let push = |items: &mut Vec<ExportItem>, kind, text: String, tool: Option<ToolInfo>| {
            items.push(ExportItem { kind, text, tool, attachment: None, provenance: provenance.clone() });
        };
        match &entry.kind {
            Kind::User { text, attachments } => {
                if !text.is_empty() {
                    push(&mut items, ItemKind::UserText, text.clone(), None);
                }
                for attachment in attachments {
                    let inline = find_inline(&attachment.raw);
                    let sha256 = match &inline {
                        Some(found) => stx::hex(&Sha256::digest(&found.data)),
                        None => stx::hex(&Sha256::digest(serde_json::to_vec(&attachment.raw).unwrap_or_default())),
                    };
                    let name = attachment
                        .path
                        .as_deref()
                        .and_then(|p| std::path::Path::new(p).file_name())
                        .and_then(|n| n.to_str())
                        .and_then(|n| bounded(Some(n), 255));
                    let media_type = inline.as_ref().and_then(|i| i.media_type.clone()).and_then(|m| bounded(Some(&m), 128));
                    let mut copied_as = None;
                    let bytes = inline.as_ref().map(|i| i.data.len() as u64);
                    match inline {
                        Some(found) if copy && blob_total + found.data.len() <= MAX_ATTACHMENTS_TOTAL => {
                            let file = format!("{sha256}.{}", extension(media_type.as_deref()));
                            if !blobs.iter().any(|(n, _): &(String, Vec<u8>)| *n == file) {
                                blob_total += found.data.len();
                                blobs.push((file.clone(), found.data));
                            }
                            copied_as = Some(file);
                        }
                        _ if copy => not_copied += 1,
                        _ => {}
                    }
                    items.push(ExportItem {
                        kind: ItemKind::Attachment,
                        text: String::new(),
                        tool: None,
                        attachment: Some(AttachmentRef {
                            kind: bounded(Some(&attachment.kind), 64).unwrap_or_else(|| "attachment".into()),
                            name,
                            sha256,
                            bytes,
                            media_type,
                            content_inline: bytes.is_some(),
                            copied_as,
                        }),
                        provenance: provenance.clone(),
                    });
                }
            }
            Kind::Step { text, calls, reasoning, .. } => {
                omitted.reasoning += reasoning.len() as u64;
                let joined = text.iter().filter(|t| !t.is_empty()).cloned().collect::<Vec<_>>().join("\n\n");
                if !joined.is_empty() {
                    push(&mut items, ItemKind::AssistantText, joined, None);
                }
                for call in calls {
                    let arguments = serde_json::to_string_pretty(&call.input).unwrap_or_default();
                    push(&mut items, ItemKind::ToolCall, arguments, Some(ToolInfo {
                        name: bounded(Some(&call.source_name), 128).unwrap_or_else(|| "tool".into()),
                        call_id: bounded(Some(&call.id), 256),
                        is_error: false,
                    }));
                }
            }
            Kind::Results { results } => {
                for result in results {
                    if matches!(result.content, ResultContent::Blocks(_)) {
                        omitted.structured_results += 1;
                    }
                    let name = session
                        .entries
                        .iter()
                        .find_map(|e| match &e.kind {
                            Kind::Step { calls, .. } => calls.iter().find(|c| c.id == result.call_id).map(|c| c.source_name.clone()),
                            _ => None,
                        })
                        .and_then(|n| bounded(Some(&n), 128))
                        .unwrap_or_else(|| "tool".into());
                    push(&mut items, ItemKind::ToolResult, result.content.text(), Some(ToolInfo {
                        name,
                        call_id: bounded(Some(&result.call_id), 256),
                        is_error: result.is_error,
                    }));
                }
            }
            Kind::Compaction { summary, .. } => push(&mut items, ItemKind::Compaction, summary.clone(), None),
            Kind::Meta { .. } => omitted.meta += 1,
        }
    }
    let transcript = Transcript {
        source: ExportSource {
            agent: harness.to_owned(),
            agent_version: bounded(Some(&meta.harness_version), 64).filter(|v| v != "unknown"),
            native_session_id: meta.id.clone(),
            title: bounded(meta.title.as_deref(), 512),
            cwd: bounded(meta.cwd.as_deref(), 1024),
            git_branch: bounded(meta.git_branch.as_deref(), 256),
            created: bounded(meta.created.as_deref(), 64),
            updated: bounded(meta.updated.as_deref(), 64),
            parent: bounded(meta.parent.as_deref(), 256),
            source_sha256: bounded(meta.source_sha256.as_deref(), 64),
        },
        thread,
        items,
        omitted,
        instruction_files: Vec::new(),
    };
    (transcript, blobs, not_copied)
}

/// Removes reasoning blocks and opaque signatures from a Mukade document, then redacts it.
pub fn sanitize_mukade_document(document: &mut Value, redactor: &mut Redactor) -> u64 {
    fn strip(value: &mut Value, removed: &mut u64) {
        match value {
            Value::Array(items) => {
                let before = items.len();
                items.retain(|item| {
                    !matches!(
                        item.get("type").and_then(Value::as_str),
                        Some("thinking" | "redacted_thinking" | "reasoning")
                    )
                });
                *removed += (before - items.len()) as u64;
                items.iter_mut().for_each(|item| strip(item, removed));
            }
            Value::Object(map) => {
                for key in ["opaque", "signature", "encrypted_content", "reasoning", "thinking"] {
                    if map.remove(key).is_some() {
                        *removed += 1;
                    }
                }
                map.values_mut().for_each(|item| strip(item, removed));
            }
            _ => {}
        }
    }
    let mut removed = 0;
    strip(document, &mut removed);
    redactor.redact_json(document);
    removed
}

fn safe_file_stem(value: &str) -> String {
    let stem: String = value
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .take(64)
        .collect();
    let stem = stem.trim_matches('-').to_owned();
    if stem.is_empty() { "session".into() } else { stem }
}

pub fn render(session: &Session, request: &ExportRequest, fingerprint: String) -> Result<ExportPayload, ExportIssue> {
    let harness = session.meta.harness.to_string();
    let copy = request.with_attachments && request.format != Format::Mukade;
    let (transcript, blobs, not_copied) = project(session, &harness, request.thread.clone(), copy);
    let mut redactor = Redactor::new(request.mode, request.home.as_deref());
    let created = shinatama_export::time::now_rfc3339();
    let redacted = transcript.redacted(&mut redactor);
    let (content, omitted) = match request.format {
        Format::Stx => {
            let counts = redactor.counts().clone();
            let bytes = stx::render(&redacted, &stx::WriteOptions {
                mode: request.mode,
                created: created.clone(),
                redactions: counts,
                attachments_copied: blobs.len() as u64,
            });
            (String::from_utf8(bytes).map_err(|_| ExportIssue::OutputLimit)?, redacted.omitted)
        }
        Format::Md => (markdown::render(&redacted, request.mode, &created), redacted.omitted),
        Format::Mukade => {
            let mut document = mukade::wire::to_document(session);
            let removed = sanitize_mukade_document(&mut document, &mut redactor);
            let mut omitted = redacted.omitted;
            omitted.reasoning = omitted.reasoning.max(removed);
            (serde_json::to_string_pretty(&document).map_err(|_| ExportIssue::OutputLimit)?, omitted)
        }
        Format::Native => return Err(ExportIssue::FormatUnavailable),
    };
    if content.len() > MAX_EXPORT_CONTENT_BYTES {
        return Err(ExportIssue::OutputLimit);
    }
    let attachments_referenced = redacted.items.iter().filter(|i| i.attachment.is_some()).count() as u64;
    let report = ExportReport {
        format: request.format,
        mode: request.mode,
        declaration: formats::declaration(request.format, false),
        harness,
        native_id: session.meta.id.clone(),
        source_fingerprint: fingerprint,
        items: redacted.items.len() as u64,
        redactions: redactor.into_counts(),
        omitted,
        attachments_referenced,
        attachments_copied: blobs.len() as u64,
        attachments_not_copied: if copy { not_copied } else { attachments_referenced },
        content_bytes: content.len() as u64,
    };
    let stem = safe_file_stem(session.meta.title.as_deref().unwrap_or(&session.meta.id));
    Ok(ExportPayload {
        suggested_name: format!("{stem}{}", request.format.extension()),
        report,
        content,
        attachments: blobs
            .into_iter()
            .map(|(name, data)| AttachmentBlob { name, data_base64: shinatama_export::base64::encode(&data) })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        let raw = serde_json::json!({
            "meta": {"harness": "claude_code", "harness_version": "2.1.0", "id": "s-1", "title": "Plan: fix C:\\Users\\someone\\x", "cwd": "C:\\Users\\someone\\proj"},
            "messages": []
        });
        let mut session = mukade::wire::from_document(&raw).unwrap();
        session.entries = serde_json::from_value(serde_json::json!([
            {"id": "u1", "kind": {"user": {"text": "token ghp_0123456789abcdefABCDEF0123456789abcd", "attachments": [
                {"kind": "image", "raw": {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "iVBORw0KGgoAAAANSUhEUgAAAAE="}}}
            ]}}, "raw": [{"timestamp": "2026-11-01T10:00:00Z"}]},
            {"id": "a1", "kind": {"step": {"reasoning": [{"text": "secret thoughts"}], "text": ["Looking."], "calls": [
                {"id": "c1", "source_name": "Read", "input": {"path": "a.rs"}}
            ], "end": "tool_use"}}, "raw": []},
            {"id": "r1", "kind": {"results": {"results": [{"call_id": "c1", "content": {"text": "fn x() {}"}, "is_error": false}]}}, "raw": []},
            {"id": "m1", "kind": {"meta": {"kind": "idle"}}, "raw": []}
        ])).unwrap();
        session
    }

    fn request(format: Format, mode: Mode, with_attachments: bool) -> ExportRequest {
        ExportRequest {
            detail: DetailRequest {
                root: RootSelector { store_root_id: "r".into(), configuration_revision: 1 },
                native_id: "s-1".into(),
                cursor: None,
                limit: 1,
            },
            format,
            mode,
            with_attachments,
            thread: None,
            home: Some(r"C:\Users\someone".into()),
        }
    }

    #[test]
    fn stx_export_projects_redacts_and_omits_reasoning() {
        let session = session();
        let payload = render(&session, &request(Format::Stx, Mode::Full, false), "f".repeat(64)).unwrap();
        assert!(!payload.content.contains("secret thoughts"));
        assert!(!payload.content.contains("ghp_0123"));
        let document = stx::read(payload.content.as_bytes()).unwrap();
        let kinds: Vec<_> = document.items().map(|i| i.kind).collect();
        assert_eq!(kinds, [ItemKind::UserText, ItemKind::Attachment, ItemKind::AssistantText, ItemKind::ToolCall, ItemKind::ToolResult]);
        assert_eq!(document.header.omitted.reasoning, 1);
        assert_eq!(document.header.omitted.meta, 1);
        let first = document.items().next().unwrap();
        assert_eq!(first.provenance.timestamp.as_deref(), Some("2026-11-01T10:00:00Z"));
        let result = document.items().nth(4).unwrap();
        assert_eq!(result.tool.as_ref().unwrap().name, "Read");
        assert_eq!(payload.report.redactions.get("github_token"), Some(&1));
        assert_eq!(payload.report.attachments_not_copied, 1);
        assert!(payload.attachments.is_empty());
        assert!(payload.suggested_name.ends_with(".stx.jsonl"));
    }

    #[test]
    fn attachments_are_copied_only_on_request_and_portable_hides_home() {
        let session = session();
        let payload = render(&session, &request(Format::Stx, Mode::Portable, true), "f".repeat(64)).unwrap();
        assert_eq!(payload.attachments.len(), 1);
        assert!(payload.attachments[0].name.ends_with(".png"));
        assert_eq!(payload.report.attachments_copied, 1);
        assert!(!payload.content.contains("someone"));
        let document = stx::read(payload.content.as_bytes()).unwrap();
        let attachment = document.items().find_map(|i| i.attachment.clone()).unwrap();
        assert!(attachment.content_inline);
        assert_eq!(attachment.copied_as.as_deref(), Some(payload.attachments[0].name.as_str()));
    }

    #[test]
    fn mukade_and_markdown_never_carry_reasoning_and_native_is_refused() {
        let session = session();
        for format in [Format::Mukade, Format::Md] {
            let payload = render(&session, &request(format, Mode::Full, false), "f".repeat(64)).unwrap();
            assert!(!payload.content.contains("secret thoughts"), "{format:?}");
            assert!(!payload.content.contains("ghp_0123"), "{format:?}");
        }
        assert_eq!(validate_request(&request(Format::Native, Mode::Full, false)), Err(ExportIssue::FormatUnavailable));
    }
}
