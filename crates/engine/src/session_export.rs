//! C26 session export, run inside the contained session worker (FRD-0006 R31-R39, ADR-0021).
//!
//! The worker streams the pinned session through Mukade (the same verified path as detail, whatever
//! its size), projects it one entry at a time (reasoning has no representation in the export
//! model), redacts each item once and renders the chosen format as the items arrive. The
//! rendered body leaves the worker in bounded frames (`export_frames`), with no total bound; the
//! trusted parent stages the frames and commits the file atomically, and the worker never
//! writes. Native export is offered only where proven: for now nowhere.
use crate::export_frames::FrameWriter;
use crate::session_detail::{
    DetailIssue, DetailRequest, StreamError, StreamStop, TrustedRootSnapshot, open,
};
use crate::session_roots::{MUKADE_REVISION, RootSelector};
use mukade::{Entry, Kind, Session, model::{Meta, ResultContent}};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use shinatama_export::{
    AttachmentRef, ExportItem, ExportSource, Format, FormatDeclaration, ItemKind, Mode, Omissions,
    Provenance, Redactions, Redactor, ToolInfo, formats, markdown, output, stx,
};
use std::io::{self, Write};
use std::ops::Deref;

/// The bound of the `mukade` format's document only, until its writer streams too: that format
/// is built from the whole session and is refused when its output does not fit. Markdown and
/// STX have no content bound.
pub const MAX_EXPORT_CONTENT_BYTES: usize = 24 * 1024 * 1024;
/// One attachment, and all of one export's attachments: they bound what the final frame carries,
/// not the session (its text streams in frames of at most 1 MiB with no total).
pub const MAX_ATTACHMENT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_ATTACHMENTS_TOTAL: usize = 12 * 1024 * 1024;

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
    /// The bytes that go in front of the streamed body: Markdown's title and facts, STX's header
    /// line (its checksum and counts exist only after the last item). Empty for `mukade`.
    pub prefix: String,
    pub attachments: Vec<AttachmentBlob>,
}

/// The final frame of an export: the protocol and the pinned Mukade revision bind it to the
/// request as they bind every other worker reply.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExportFinal {
    pub protocol: u8,
    pub revision: String,
    pub export: ExportReply,
}

/// An export rendered whole in memory: the payload and the body that follows its prefix. The
/// worker never builds this; it is what the streamed file is compared with.
pub struct Rendered {
    pub payload: ExportPayload,
    pub body: Vec<u8>,
}
impl Deref for Rendered {
    type Target = ExportPayload;
    fn deref(&self) -> &ExportPayload {
        &self.payload
    }
}
impl Rendered {
    /// The whole file: prefix then body.
    pub fn content(&self) -> Vec<u8> {
        let mut out = self.payload.prefix.as_bytes().to_vec();
        out.extend_from_slice(&self.body);
        out
    }
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

/// What the `mukade` format may hold before it stops: it collects the entries it streams, and a
/// session whose text is twice its content bound cannot render within it once redacted.
const MAX_STREAMED_BYTES: usize = 2 * MAX_EXPORT_CONTENT_BYTES;

enum Stop {
    Issue(ExportIssue),
    /// The pipe to the parent failed; there is nobody to tell.
    Io(io::Error),
}

/// A session's size is no reason to refuse an export (ADR-0053 v2): its entries are read one at
/// a time through Mukade's streamed read, projected, redacted and rendered as they arrive, and the
/// body goes to the parent in bounded frames. Nothing but one entry and one frame is held. Only
/// the `mukade` format needs the whole `Session` at once (`wire::to_document` writes the
/// interchange document from it), so only that format collects the entries it streams.
pub fn stream_export<W: Write>(
    snapshot: Result<&TrustedRootSnapshot, DetailIssue>,
    request: &ExportRequest,
    frames: &mut FrameWriter<W>,
) -> io::Result<()> {
    let export = match (validate_request(request), snapshot) {
        (Err(issue), _) => ExportReply::refuse(request, issue),
        (Ok(()), Err(issue)) => ExportReply::refuse(request, ExportIssue::Detail(issue)),
        (Ok(()), Ok(snapshot)) => match run(snapshot, request, frames) {
            Ok(payload) => ExportReply {
                root: request.detail.root.clone(),
                native_id: request.detail.native_id.clone(),
                payload: Some(payload),
                issue: None,
            },
            Err(Stop::Issue(issue)) => ExportReply::refuse(request, issue),
            Err(Stop::Io(error)) => return Err(error),
        },
    };
    let last = ExportFinal { protocol: 1, revision: MUKADE_REVISION.into(), export };
    frames.finish(&serde_json::to_vec(&last).map_err(io::Error::other)?)
}

/// Bytes of an entry's JSON, counted without building it.
fn json_len(entry: &Entry) -> usize {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    let _ = serde_json::to_writer(&mut count, entry);
    count.0
}

fn run<W: Write>(
    snapshot: &TrustedRootSnapshot,
    request: &ExportRequest,
    frames: &mut FrameWriter<W>,
) -> Result<ExportPayload, Stop> {
    let opened = open(snapshot, &request.detail)
        .map_err(|issue| Stop::Issue(ExportIssue::Detail(issue)))?;
    let copy = request.with_attachments && request.format != Format::Mukade;
    let whole = request.format == Format::Mukade;
    let mut projector = Projector::new(
        snapshot.harness_name(),
        request.detail.native_id.clone(),
        copy,
    );
    let mut emitter = Emitter::new(request);
    let mut entries = Vec::new();
    let mut collected = 0usize;
    let mut failure = None;
    let mut read = 0u64;
    let streamed = opened.streamed(snapshot, &request.detail, &mut |entry| {
        projector.push(&entry);
        for item in projector.take_items() {
            if let Err(error) = emitter.item(item, &mut |bytes| frames.write_body(bytes)) {
                failure = Some(Stop::Io(error));
                return Err(StreamStop);
            }
        }
        read += 1;
        if let Err(error) = frames.progress(read, 0) {
            failure = Some(Stop::Io(error));
            return Err(StreamStop);
        }
        if whole {
            collected = collected.saturating_add(json_len(&entry));
            entries.push(entry);
            if projector.text_bytes().saturating_add(collected) > MAX_STREAMED_BYTES {
                failure = Some(Stop::Issue(ExportIssue::OutputLimit));
                return Err(StreamStop);
            }
        }
        Ok(())
    });
    let (meta, fingerprint) = match streamed {
        Ok(done) => done,
        Err(StreamError::Detail(issue)) => return Err(Stop::Issue(ExportIssue::Detail(issue))),
        Err(StreamError::Stopped) => {
            return Err(failure.unwrap_or(Stop::Issue(ExportIssue::OutputLimit)));
        }
    };
    let session = whole.then(|| Session { meta: meta.clone(), entries, branches: Vec::new() });
    let body_bytes = frames.body_bytes();
    let done = finalize(
        &meta,
        request,
        fingerprint,
        projector,
        emitter,
        session.as_ref(),
        body_bytes,
        shinatama_export::time::now_rfc3339(),
    )
    .map_err(Stop::Issue)?;
    if let Some(document) = &done.document {
        frames.write_body(document.as_bytes()).map_err(Stop::Io)?;
    }
    Ok(done.payload)
}

/// Redacts and renders the items one at a time. The same code renders the streamed export and
/// the in-memory one, so a file written from frames is the in-memory render, byte for byte.
struct Emitter {
    format: Format,
    mode: Mode,
    thread: Option<String>,
    redactor: Redactor,
    md: markdown::Body,
    stx: stx::BodyWriter,
    items: u64,
    attachments: u64,
    text: String,
    line: Vec<u8>,
}

/// A buffer that held one huge item is not kept for the next.
const KEEP_BUFFER_BYTES: usize = 4 * 1024 * 1024;

impl Emitter {
    fn new(request: &ExportRequest) -> Self {
        Self {
            format: request.format,
            mode: request.mode,
            thread: request.thread.clone(),
            redactor: Redactor::new(request.mode, request.home.as_deref()),
            md: markdown::Body::new(),
            stx: stx::BodyWriter::new(),
            items: 0,
            attachments: 0,
            text: String::new(),
            line: Vec::new(),
        }
    }

    /// Redacts `item` (its text and its attachment's name: identifiers, hashes, timestamps and
    /// tool names are structural), renders it and hands the bytes to `sink`. The `mukade`
    /// format renders its document at the end; its items are only redacted, so the counts match.
    fn item(
        &mut self,
        mut item: ExportItem,
        sink: &mut dyn FnMut(&[u8]) -> io::Result<()>,
    ) -> io::Result<()> {
        item.text = self.redactor.redact(&item.text);
        if let Some(attachment) = item.attachment.as_mut()
            && let Some(name) = attachment.name.as_mut()
        {
            *name = self.redactor.redact(name);
        }
        self.items += 1;
        self.attachments += u64::from(item.attachment.is_some());
        match self.format {
            Format::Md => {
                self.text.clear();
                self.md.item(&mut self.text, &item);
                let result = sink(self.text.as_bytes());
                if self.text.capacity() > KEEP_BUFFER_BYTES {
                    self.text = String::new();
                }
                result
            }
            Format::Stx => {
                self.line.clear();
                self.stx.item(&item, &mut self.line);
                let result = sink(&self.line);
                if self.line.capacity() > KEEP_BUFFER_BYTES {
                    self.line = Vec::new();
                }
                result
            }
            _ => Ok(()),
        }
    }
}

struct Done {
    payload: ExportPayload,
    /// The `mukade` format's whole document, written as the body.
    document: Option<String>,
}

/// After the last entry: the header that goes in front of the body, and the report.
fn finalize(
    meta: &Meta,
    request: &ExportRequest,
    fingerprint: String,
    projector: Projector,
    mut emitter: Emitter,
    whole: Option<&Session>,
    body_bytes: u64,
    created: String,
) -> Result<Done, ExportIssue> {
    let harness = meta.harness.to_string();
    let copy = request.with_attachments && request.format != Format::Mukade;
    let parts = projector.finish(meta);
    let mut source = parts.source;
    // The header's free text is redacted as the items were (the same counts).
    for value in [&mut source.title, &mut source.cwd] {
        if let Some(text) = value.as_mut() {
            *text = emitter.redactor.redact(text);
        }
    }
    let mut omitted = parts.omitted;
    let (prefix, document) = match request.format {
        Format::Stx => {
            let writer = std::mem::take(&mut emitter.stx);
            let header = writer.header(
                &source,
                emitter.thread.as_deref(),
                &[],
                omitted,
                &stx::WriteOptions {
                    mode: emitter.mode,
                    created,
                    redactions: emitter.redactor.counts().clone(),
                    attachments_copied: parts.blobs.len() as u64,
                },
            );
            (String::from_utf8(header).map_err(|_| ExportIssue::OutputLimit)?, None)
        }
        Format::Md => (
            markdown::header(&source, emitter.thread.as_deref(), emitter.mode, &created),
            None,
        ),
        Format::Mukade => {
            let session = whole.ok_or(ExportIssue::InvalidRequest)?;
            let mut document = mukade::wire::to_document(session);
            let removed = sanitize_mukade_document(&mut document, &mut emitter.redactor);
            omitted.reasoning = omitted.reasoning.max(removed);
            let text = serde_json::to_string_pretty(&document).map_err(|_| ExportIssue::OutputLimit)?;
            if text.len() > MAX_EXPORT_CONTENT_BYTES {
                return Err(ExportIssue::OutputLimit);
            }
            (String::new(), Some(text))
        }
        Format::Native => return Err(ExportIssue::FormatUnavailable),
    };
    let content_bytes = match &document {
        Some(text) => text.len() as u64,
        None => prefix.len() as u64 + body_bytes,
    };
    let attachments_referenced = emitter.attachments;
    let report = ExportReport {
        format: request.format,
        mode: request.mode,
        declaration: formats::declaration(request.format, false),
        harness,
        native_id: meta.id.clone(),
        source_fingerprint: fingerprint,
        items: emitter.items,
        redactions: emitter.redactor.into_counts(),
        omitted,
        attachments_referenced,
        attachments_copied: parts.blobs.len() as u64,
        attachments_not_copied: if copy { parts.not_copied } else { attachments_referenced },
        content_bytes,
    };
    let stem = safe_file_stem(meta.title.as_deref().unwrap_or(&meta.id));
    Ok(Done {
        payload: ExportPayload {
            suggested_name: format!("{stem}{}", request.format.extension()),
            report,
            prefix,
            attachments: parts
                .blobs
                .into_iter()
                .map(|(name, data)| AttachmentBlob { name, data_base64: shinatama_export::base64::encode(&data) })
                .collect(),
        },
        document,
    })
}

/// Why a staged export could not be committed.
#[derive(Debug, PartialEq, Eq)]
pub enum CommitError {
    /// An attachment came back undecodable or over its bound; nothing was written.
    Attachment,
    Output(output::OutputError),
}

/// The parent's last step: the staged body, the final frame's prefix and the decoded
/// attachments become the file, atomically (see `output::Staged::commit`).
pub fn commit_export(
    staged: output::Staged,
    payload: &ExportPayload,
) -> Result<output::Receipt, CommitError> {
    let mut files = Vec::with_capacity(payload.attachments.len());
    for blob in &payload.attachments {
        let data = shinatama_export::base64::decode(&blob.data_base64, MAX_ATTACHMENT_BYTES)
            .ok_or(CommitError::Attachment)?;
        files.push(output::AttachmentFile { name: blob.name.clone(), data });
    }
    staged.commit(payload.prefix.as_bytes(), &files).map_err(CommitError::Output)
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

/// Projects a session into the transcript one entry at a time, so a stream and a whole session
/// give the same items. The name of a result's tool is found among the calls already seen (a
/// result follows its call).
pub struct Projector {
    harness: String,
    native_id: String,
    copy: bool,
    omitted: Omissions,
    items: Vec<ExportItem>,
    blobs: Vec<(String, Vec<u8>)>,
    blob_total: usize,
    not_copied: u64,
    call_names: std::collections::HashMap<String, String>,
    ordinal: usize,
    text_bytes: usize,
}

impl Projector {
    pub fn new(harness: String, native_id: String, copy: bool) -> Self {
        Self {
            harness,
            native_id,
            copy,
            omitted: Omissions::default(),
            items: Vec::new(),
            blobs: Vec::new(),
            blob_total: 0,
            not_copied: 0,
            call_names: std::collections::HashMap::new(),
            ordinal: 0,
            text_bytes: 0,
        }
    }

    /// The text held so far, the measure of what a stream may still take.
    pub fn text_bytes(&self) -> usize {
        self.text_bytes
    }

    /// The items the last entries produced, handed on so a stream holds one entry's worth.
    pub fn take_items(&mut self) -> Vec<ExportItem> {
        std::mem::take(&mut self.items)
    }

    pub fn push(&mut self, entry: &Entry) {
        let ordinal = self.ordinal;
        self.ordinal += 1;
        let provenance = Provenance {
            agent: self.harness.clone(),
            native_session_id: self.native_id.clone(),
            entry_id: bounded(Some(&entry.id), 256),
            entry_ordinal: ordinal,
            timestamp: timestamp(entry),
        };
        let copy = self.copy;
        let add = |items: &mut Vec<ExportItem>, held: &mut usize, kind, text: String, tool: Option<ToolInfo>| {
            *held += text.len();
            items.push(ExportItem { kind, text, tool, attachment: None, provenance: provenance.clone() });
        };
        match &entry.kind {
            Kind::User { text, attachments } => {
                if !text.is_empty() {
                    add(&mut self.items, &mut self.text_bytes, ItemKind::UserText, text.clone(), None);
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
                        Some(found) if copy && self.blob_total + found.data.len() <= MAX_ATTACHMENTS_TOTAL => {
                            let file = format!("{sha256}.{}", extension(media_type.as_deref()));
                            if !self.blobs.iter().any(|(n, _): &(String, Vec<u8>)| *n == file) {
                                self.blob_total += found.data.len();
                                self.blobs.push((file.clone(), found.data));
                            }
                            copied_as = Some(file);
                        }
                        _ if copy => self.not_copied += 1,
                        _ => {}
                    }
                    self.items.push(ExportItem {
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
                self.omitted.reasoning += reasoning.len() as u64;
                let joined = text.iter().filter(|t| !t.is_empty()).cloned().collect::<Vec<_>>().join("\n\n");
                if !joined.is_empty() {
                    add(&mut self.items, &mut self.text_bytes, ItemKind::AssistantText, joined, None);
                }
                for call in calls {
                    self.call_names.entry(call.id.clone()).or_insert_with(|| call.source_name.clone());
                    let arguments = serde_json::to_string_pretty(&call.input).unwrap_or_default();
                    add(&mut self.items, &mut self.text_bytes, ItemKind::ToolCall, arguments, Some(ToolInfo {
                        name: bounded(Some(&call.source_name), 128).unwrap_or_else(|| "tool".into()),
                        call_id: bounded(Some(&call.id), 256),
                        is_error: false,
                    }));
                }
            }
            Kind::Results { results } => {
                for result in results {
                    if matches!(result.content, ResultContent::Blocks(_)) {
                        self.omitted.structured_results += 1;
                    }
                    let name = self
                        .call_names
                        .get(&result.call_id)
                        .and_then(|n| bounded(Some(n), 128))
                        .unwrap_or_else(|| "tool".into());
                    add(&mut self.items, &mut self.text_bytes, ItemKind::ToolResult, result.content.text(), Some(ToolInfo {
                        name,
                        call_id: bounded(Some(&result.call_id), 256),
                        is_error: result.is_error,
                    }));
                }
            }
            Kind::Compaction { summary, .. } => {
                add(&mut self.items, &mut self.text_bytes, ItemKind::Compaction, summary.clone(), None)
            }
            Kind::Meta { .. } => self.omitted.meta += 1,
        }
    }

    /// The header's facts, what was omitted, the inline attachment contents and how many were
    /// not copied.
    pub fn finish(self, meta: &Meta) -> Parts {
        Parts {
            source: ExportSource {
                agent: self.harness,
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
            omitted: self.omitted,
            blobs: self.blobs,
            not_copied: self.not_copied,
        }
    }
}

pub struct Parts {
    pub source: ExportSource,
    pub omitted: Omissions,
    pub blobs: Vec<(String, Vec<u8>)>,
    pub not_copied: u64,
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

/// Renders a session held whole in memory: the same items, redaction and writers as the
/// streamed export, collected. It is what a streamed file is compared with.
pub fn render(session: &Session, request: &ExportRequest, fingerprint: String) -> Result<Rendered, ExportIssue> {
    render_at(session, request, fingerprint, shinatama_export::time::now_rfc3339())
}

/// [`render`] with the export's own timestamp given, so two renders can be compared.
pub fn render_at(
    session: &Session,
    request: &ExportRequest,
    fingerprint: String,
    created: String,
) -> Result<Rendered, ExportIssue> {
    let copy = request.with_attachments && request.format != Format::Mukade;
    let mut projector = Projector::new(
        session.meta.harness.to_string(),
        session.meta.id.clone(),
        copy,
    );
    let mut emitter = Emitter::new(request);
    let mut body = Vec::new();
    for entry in &session.entries {
        projector.push(entry);
        for item in projector.take_items() {
            emitter
                .item(item, &mut |bytes| {
                    body.extend_from_slice(bytes);
                    Ok(())
                })
                .map_err(|_| ExportIssue::OutputLimit)?;
        }
    }
    let body_bytes = body.len() as u64;
    let done = finalize(&session.meta, request, fingerprint, projector, emitter, Some(session), body_bytes, created)?;
    Ok(Rendered {
        body: done.document.map(String::into_bytes).unwrap_or(body),
        payload: done.payload,
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

    fn text(rendered: &Rendered) -> String {
        String::from_utf8(rendered.content()).unwrap()
    }

    fn request(format: Format, mode: Mode, with_attachments: bool) -> ExportRequest {
        ExportRequest {
            detail: DetailRequest {
                root: RootSelector { store_root_id: "r".into(), configuration_revision: 1 },
                native_id: "s-1".into(),
                cursor: None,
                limit: 1,
                overview_only: false,
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
        assert!(!text(&payload).contains("secret thoughts"));
        assert!(!text(&payload).contains("ghp_0123"));
        let document = stx::read(&payload.content()).unwrap();
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
        assert!(!text(&payload).contains("someone"));
        let document = stx::read(&payload.content()).unwrap();
        let attachment = document.items().find_map(|i| i.attachment.clone()).unwrap();
        assert!(attachment.content_inline);
        assert_eq!(attachment.copied_as.as_deref(), Some(payload.attachments[0].name.as_str()));
    }

    #[test]
    fn mukade_and_markdown_never_carry_reasoning_and_native_is_refused() {
        let session = session();
        for format in [Format::Mukade, Format::Md] {
            let payload = render(&session, &request(format, Mode::Full, false), "f".repeat(64)).unwrap();
            assert!(!text(&payload).contains("secret thoughts"), "{format:?}");
            assert!(!text(&payload).contains("ghp_0123"), "{format:?}");
        }
        assert_eq!(validate_request(&request(Format::Native, Mode::Full, false)), Err(ExportIssue::FormatUnavailable));
    }

    /// The parent's side of the pipe, in process: frames in, body frames staged, final frame kept.
    #[cfg(windows)]
    struct ParentSide {
        parser: crate::export_frames::FrameParser,
        buffer: Vec<u8>,
        staged: Option<output::Staged>,
        fail_after: Option<u64>,
        fed: u64,
    }
    #[cfg(windows)]
    impl Write for ParentSide {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.fed += bytes.len() as u64;
            if self.fail_after.is_some_and(|limit| self.fed > limit) {
                return Err(io::Error::other("the parent went away"));
            }
            self.buffer.extend_from_slice(bytes);
            let staged = self.staged.as_mut().unwrap();
            self.parser
                .take(&mut self.buffer, &mut |event| match event {
                    crate::export_frames::Event::Body(frame) => staged.write(frame),
                    crate::export_frames::Event::Progress(_) => Ok(()),
                })
                .map_err(|_| io::Error::other("bad frame"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// One export as the parent does it: stage, stream, commit only on a payload.
    #[cfg(windows)]
    fn export_to_file(
        large: &crate::session_detail::large_fixture::LargeSession,
        request: &ExportRequest,
        path: &std::path::Path,
        fail_after: Option<u64>,
    ) -> Result<(output::Receipt, ExportPayload), String> {
        let staged = output::stage(path, output::Overwrite::Replace).map_err(|e| e.to_string())?;
        let mut side = ParentSide {
            parser: crate::export_frames::FrameParser::new(),
            buffer: Vec::new(),
            staged: Some(staged),
            fail_after,
            fed: 0,
        };
        {
            let mut frames = FrameWriter::new(&mut side);
            stream_export(Ok(&large.snapshot), request, &mut frames).map_err(|e| e.to_string())?;
        }
        let last: ExportFinal = serde_json::from_slice(side.parser.final_frame().ok_or("no final frame")?)
            .map_err(|e| e.to_string())?;
        let body = side.parser.body_bytes();
        let payload = last.export.payload.ok_or_else(|| format!("refused: {:?}", last.export.issue))?;
        assert_eq!(payload.report.content_bytes, payload.prefix.len() as u64 + body);
        let receipt = commit_export(side.staged.take().unwrap(), &payload).map_err(|e| format!("{e:?}"))?;
        Ok((receipt, payload))
    }

    /// The timestamp the streamed file carries, so the in-memory render can use the same one.
    #[cfg(windows)]
    fn created_of(file: &[u8], format: Format) -> String {
        let head = String::from_utf8_lossy(&file[..file.len().min(4096)]).into_owned();
        match format {
            Format::Md => {
                let from = head.find("- Exported: ").unwrap() + "- Exported: ".len();
                let to = head[from..].find(" (").unwrap();
                head[from..from + to].replace("\\", "")
            }
            _ => {
                let line = head.lines().next().unwrap();
                serde_json::from_str::<Value>(line).unwrap()["created"].as_str().unwrap().to_owned()
            }
        }
    }

    #[cfg(windows)]
    fn request_for(
        large: &crate::session_detail::large_fixture::LargeSession,
        format: Format,
    ) -> ExportRequest {
        let mut export = request(format, Mode::Full, false);
        export.detail = crate::session_detail::large_fixture::request(large);
        export.detail.limit = 1;
        export.home = None;
        export
    }

    #[cfg(windows)]
    #[test]
    fn a_session_past_16_mib_is_exported_in_full_and_refused_only_when_its_output_does_not_fit() {
        use crate::session_detail::large_fixture;
        let large = large_fixture::build();
        assert!(large.bytes > 17 * 1024 * 1024);
        let folder = large.root_path.join("out");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("big.md");
        let (receipt, payload) = export_to_file(&large, &request_for(&large, Format::Md), &path, None).unwrap();
        let file = std::fs::read(&path).unwrap();
        let content = String::from_utf8(file).unwrap();
        // Every word, not the 4 KiB the view shows: the marker sits 12 MiB into the result.
        assert!(content.contains(large_fixture::DEEP_MARKER));
        assert!(content.contains("done reading"));
        assert_eq!(payload.report.native_id, large_fixture::SESSION);
        assert_eq!(payload.report.source_fingerprint.len(), 64);
        assert_eq!(payload.report.items, 4);
        assert!(content.len() > 17 * 1024 * 1024);
        assert_eq!(receipt.bytes, content.len() as u64);
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1, "no staged file is left");
        // The interchange document holds each record twice (entries and source), so its output,
        // not the session's size, is what does not fit: it stays bounded until its writer streams.
        let other = folder.join("big.json");
        let refused = export_to_file(&large, &request_for(&large, Format::Mukade), &other, None);
        assert!(refused.err().unwrap().contains("OutputLimit"));
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1, "a refusal leaves nothing");
    }

    #[cfg(windows)]
    #[test]
    fn md_and_stx_of_a_session_past_64_mib_stream_in_full_and_match_the_in_memory_render() {
        use crate::session_detail::large_fixture;
        // Six results of 11 MiB: 66 MiB of text, past what a single reply or 24 MiB could hold.
        let large = large_fixture::build_sized(6, 11 * 1024 * 1024);
        assert!(large.bytes > 64 * 1024 * 1024);
        let folder = large.root_path.join("out");
        std::fs::create_dir_all(&folder).unwrap();
        let whole = large.whole();
        for (format, name) in [(Format::Md, "huge.md"), (Format::Stx, "huge.stx.jsonl")] {
            let request = request_for(&large, format);
            let path = folder.join(name);
            let (receipt, payload) = export_to_file(&large, &request, &path, None).unwrap();
            let file = std::fs::read(&path).unwrap();
            assert!(file.len() > 64 * 1024 * 1024, "{format:?}: {}", file.len());
            assert_eq!(receipt.bytes, file.len() as u64);
            assert_eq!(payload.report.content_bytes, file.len() as u64);
            assert_eq!(payload.report.items, 14);
            let memory = render_at(&whole, &request, payload.report.source_fingerprint.clone(), created_of(&file, format)).unwrap();
            assert_eq!(memory.content().len(), file.len(), "{format:?}");
            assert!(memory.content() == file, "{format:?}: the streamed file is not the in-memory render");
            if format == Format::Stx {
                // The header's checksum covers the body the frames carried.
                let document = stx::read(&file).unwrap();
                assert_eq!(document.header.items, 14);
            }
        }
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 2);
    }

    #[cfg(windows)]
    #[test]
    fn a_failed_or_refused_stream_leaves_no_staged_file_and_keeps_the_old_one() {
        use crate::session_detail::large_fixture;
        let large = large_fixture::build();
        let folder = large.root_path.join("out");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("keep.md");
        std::fs::write(&path, b"previous").unwrap();
        // The pipe fails in the middle of the body.
        let broken = export_to_file(&large, &request_for(&large, Format::Md), &path, Some(3 * 1024 * 1024));
        assert!(broken.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"previous");
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
        // The final frame refuses (the file changed since the page was read), after the body.
        let mut stale = request_for(&large, Format::Md);
        stale.detail.cursor = Some(crate::session_detail::DetailCursor { offset: 0, fingerprint: "0".repeat(64) });
        let refused = export_to_file(&large, &stale, &path, None);
        assert!(refused.err().unwrap().contains("SourceChanged"));
        assert_eq!(std::fs::read(&path).unwrap(), b"previous");
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
    }
}
