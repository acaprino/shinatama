//! `.stx.jsonl` version 1 (FRD-0006 R32-R33, ADR-0021).
//!
//! Line 1 is the header; every following line is one item with provenance. Readers keep unknown
//! fields (they are written back unchanged) and refuse a higher `version` with a message.
use crate::model::{ExportItem, ExportSource, Mode, Omissions, Transcript};
use crate::redact::Redactions;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fmt;

pub const FORMAT: &str = "shinatama.stx";
pub const VERSION: u64 = 1;
pub const EXTENSION: &str = ".stx.jsonl";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitFacts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentPolicy {
    /// `references` (hashes only) or `copied` (`--with-attachments`).
    pub policy: String,
    pub count: u64,
    pub copied: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Generator {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Header {
    pub format: String,
    pub version: u64,
    pub mode: Mode,
    pub thread: Option<String>,
    pub source: ExportSource,
    pub cwd: Option<String>,
    pub git: Option<GitFacts>,
    pub created: String,
    pub items: u64,
    pub body_sha256: String,
    pub redactions: Redactions,
    pub attachments: AttachmentPolicy,
    pub instruction_files: Vec<String>,
    pub omitted: Omissions,
    pub generator: Generator,
    /// Fields a newer writer added within version 1; kept and written back.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemLine {
    pub ordinal: u64,
    #[serde(flatten)]
    pub item: ExportItem,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// An item line this reader does not understand (for example a kind added later) is kept as is.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Item(Box<ItemLine>),
    Unknown(Value),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub header: Header,
    pub lines: Vec<Line>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReadError {
    Empty,
    NotStx,
    UnsupportedVersion { found: u64 },
    MalformedHeader,
    MalformedLine { line: usize },
    ItemCount { declared: u64, found: u64 },
    BodyMismatch,
}
impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::Empty => write!(f, "The file is empty."),
            ReadError::NotStx => write!(f, "The file is not a Shinatama STX export."),
            ReadError::UnsupportedVersion { found } => write!(
                f,
                "This file uses STX version {found}; this Shinatama reads up to version {VERSION}. Update Shinatama to read it."
            ),
            ReadError::MalformedHeader => write!(f, "The STX header line is malformed."),
            ReadError::MalformedLine { line } => write!(f, "STX line {line} is not valid JSON."),
            ReadError::ItemCount { declared, found } => write!(
                f,
                "The header declares {declared} items but the file has {found}; it may be truncated."
            ),
            ReadError::BodyMismatch => write!(
                f,
                "The item lines do not match the header checksum; the file was changed or truncated."
            ),
        }
    }
}
impl std::error::Error for ReadError {}

pub struct WriteOptions {
    pub mode: Mode,
    pub created: String,
    pub redactions: Redactions,
    pub attachments_copied: u64,
}

/// Renders an already redacted transcript. The caller redacts first (see
/// [`Transcript::redacted`]) so every format shares one redaction pass and one count.
pub fn render(transcript: &Transcript, options: &WriteOptions) -> Vec<u8> {
    let mut body = Vec::new();
    for (ordinal, item) in transcript.items.iter().enumerate() {
        let line = ItemLine {
            ordinal: ordinal as u64,
            item: item.clone(),
            extra: Map::new(),
        };
        serde_json::to_writer(&mut body, &line).expect("item lines serialize");
        body.push(b'\n');
    }
    let attachments = transcript
        .items
        .iter()
        .filter(|item| item.attachment.is_some())
        .count() as u64;
    let header = Header {
        format: FORMAT.into(),
        version: VERSION,
        mode: options.mode,
        thread: transcript.thread.clone(),
        source: transcript.source.clone(),
        cwd: transcript.source.cwd.clone(),
        git: transcript.source.git_branch.as_ref().map(|branch| GitFacts {
            branch: Some(branch.clone()),
        }),
        created: options.created.clone(),
        items: transcript.items.len() as u64,
        body_sha256: hex(&Sha256::digest(&body)),
        redactions: options.redactions.clone(),
        attachments: AttachmentPolicy {
            policy: if options.attachments_copied > 0 { "copied" } else { "references" }.into(),
            count: attachments,
            copied: options.attachments_copied,
        },
        instruction_files: transcript.instruction_files.clone(),
        omitted: transcript.omitted,
        generator: Generator {
            name: "shinatama".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
        extra: Map::new(),
    };
    let mut out = serde_json::to_vec(&header).expect("header serializes");
    out.push(b'\n');
    out.extend_from_slice(&body);
    out
}

pub fn read(bytes: &[u8]) -> Result<Document, ReadError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ReadError::NotStx)?;
    let mut lines = text.split('\n');
    let first = lines.next().filter(|l| !l.trim().is_empty()).ok_or(ReadError::Empty)?;
    let raw: Value = serde_json::from_str(first.trim_end_matches('\r')).map_err(|_| ReadError::NotStx)?;
    if raw.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(ReadError::NotStx);
    }
    // Version first: a newer header may not have this version's fields at all.
    let version = raw
        .get("version")
        .and_then(Value::as_u64)
        .ok_or(ReadError::MalformedHeader)?;
    if version > VERSION {
        return Err(ReadError::UnsupportedVersion { found: version });
    }
    if version == 0 {
        return Err(ReadError::MalformedHeader);
    }
    let header: Header = serde_json::from_value(raw).map_err(|_| ReadError::MalformedHeader)?;
    let body_start = first.len() + 1;
    let body = bytes.get(body_start..).unwrap_or_default();
    if hex(&Sha256::digest(body)) != header.body_sha256 {
        return Err(ReadError::BodyMismatch);
    }
    let mut parsed = Vec::new();
    for (index, line) in lines.enumerate() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line)
            .map_err(|_| ReadError::MalformedLine { line: index + 2 })?;
        parsed.push(match serde_json::from_value::<ItemLine>(value.clone()) {
            Ok(item) => Line::Item(Box::new(item)),
            Err(_) => Line::Unknown(value),
        });
    }
    if parsed.len() as u64 != header.items {
        return Err(ReadError::ItemCount {
            declared: header.items,
            found: parsed.len() as u64,
        });
    }
    Ok(Document {
        header,
        lines: parsed,
    })
}

impl Document {
    /// Writes the document back, keeping unknown fields and lines; the checksum is recomputed.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut body = Vec::new();
        for line in &self.lines {
            match line {
                Line::Item(item) => serde_json::to_writer(&mut body, item),
                Line::Unknown(value) => serde_json::to_writer(&mut body, value),
            }
            .expect("lines serialize");
            body.push(b'\n');
        }
        let mut header = self.header.clone();
        header.items = self.lines.len() as u64;
        header.body_sha256 = hex(&Sha256::digest(&body));
        let mut out = serde_json::to_vec(&header).expect("header serializes");
        out.push(b'\n');
        out.extend_from_slice(&body);
        out
    }
    pub fn items(&self) -> impl Iterator<Item = &ExportItem> {
        self.lines.iter().filter_map(|line| match line {
            Line::Item(item) => Some(&item.item),
            Line::Unknown(_) => None,
        })
    }
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ItemKind, Provenance, ToolInfo};
    use crate::redact::Redactor;

    pub(crate) fn sample() -> Transcript {
        let prov = |ordinal| Provenance {
            agent: "claude_code".into(),
            native_session_id: "abc-123".into(),
            entry_id: Some(format!("e{ordinal}")),
            entry_ordinal: ordinal,
            timestamp: Some("2026-11-01T10:00:00Z".into()),
        };
        Transcript {
            source: ExportSource {
                agent: "claude_code".into(),
                agent_version: Some("2.1.0".into()),
                native_session_id: "abc-123".into(),
                title: Some("Fix the parser".into()),
                cwd: Some(r"C:\Users\someone\proj".into()),
                git_branch: Some("main".into()),
                ..ExportSource::default()
            },
            thread: Some("thread-1".into()),
            items: vec![
                ExportItem { kind: ItemKind::UserText, text: "my key is sk-ant-api03-AbCdEf0123456789AbCdEf0123456789xyz".into(), tool: None, attachment: None, provenance: prov(0) },
                ExportItem { kind: ItemKind::AssistantText, text: "Reading the file.".into(), tool: None, attachment: None, provenance: prov(1) },
                ExportItem { kind: ItemKind::ToolCall, text: "{\"path\":\"src/lib.rs\"}".into(), tool: Some(ToolInfo { name: "Read".into(), call_id: Some("t1".into()), is_error: false }), attachment: None, provenance: prov(1) },
                ExportItem { kind: ItemKind::ToolResult, text: "fn main() {}".into(), tool: Some(ToolInfo { name: "Read".into(), call_id: Some("t1".into()), is_error: false }), attachment: None, provenance: prov(2) },
                ExportItem { kind: ItemKind::Compaction, text: "Summary so far.".into(), tool: None, attachment: None, provenance: prov(3) },
            ],
            omitted: Omissions { reasoning: 2, meta: 1, structured_results: 0 },
            instruction_files: vec!["CLAUDE.md".into()],
        }
    }

    fn rendered(mode: Mode) -> Vec<u8> {
        let mut redactor = Redactor::new(mode, Some(r"C:\Users\someone"));
        let transcript = sample().redacted(&mut redactor);
        render(&transcript, &WriteOptions {
            mode,
            created: "2026-11-09T00:00:00Z".into(),
            redactions: redactor.into_counts(),
            attachments_copied: 0,
        })
    }

    #[test]
    fn write_read_round_trip_keeps_items_in_order() {
        let bytes = rendered(Mode::Full);
        let document = read(&bytes).unwrap();
        assert_eq!(document.header.version, 1);
        assert_eq!(document.header.items, 5);
        let kinds: Vec<_> = document.items().map(|item| item.kind).collect();
        assert_eq!(kinds, [ItemKind::UserText, ItemKind::AssistantText, ItemKind::ToolCall, ItemKind::ToolResult, ItemKind::Compaction]);
        assert_eq!(document.to_bytes(), bytes, "re-serialization is byte stable");
        assert_eq!(document.header.omitted.reasoning, 2);
    }

    #[test]
    fn both_modes_redact_secrets_and_portable_hides_home() {
        for mode in [Mode::Full, Mode::Portable] {
            let text = String::from_utf8(rendered(mode)).unwrap();
            assert!(!text.contains("sk-ant-api03"), "{mode:?} leaked a key");
            assert!(text.contains("[REDACTED:anthropic_key]"));
            assert!(text.contains("\"anthropic_key\":1"));
            assert_eq!(text.contains("someone"), mode == Mode::Full);
        }
    }

    #[test]
    fn reasoning_is_never_written() {
        let text = String::from_utf8(rendered(Mode::Full)).unwrap();
        assert!(!text.contains("\"reasoning\":\""));
        assert!(!text.contains("thinking"));
    }

    #[test]
    fn unknown_fields_and_lines_are_kept() {
        let bytes = rendered(Mode::Full);
        let text = String::from_utf8(bytes).unwrap();
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        let mut header: Value = serde_json::from_str(&lines[0]).unwrap();
        header["future_field"] = serde_json::json!({"x": 1});
        let mut item: Value = serde_json::from_str(&lines[1]).unwrap();
        item["future_item_field"] = Value::Bool(true);
        lines[1] = item.to_string();
        lines.push(serde_json::json!({"ordinal": 5, "kind": "hologram", "text": "?"}).to_string());
        let body: String = lines[1..].iter().map(|l| format!("{l}\n")).collect();
        header["items"] = 6.into();
        header["body_sha256"] = hex(&Sha256::digest(body.as_bytes())).into();
        let edited = format!("{header}\n{body}");
        let document = read(edited.as_bytes()).unwrap();
        assert_eq!(document.header.extra["future_field"]["x"], 1);
        assert!(matches!(&document.lines[5], Line::Unknown(v) if v["kind"] == "hologram"));
        let Line::Item(first) = &document.lines[0] else { panic!() };
        assert_eq!(first.extra["future_item_field"], true);
        let again = String::from_utf8(document.to_bytes()).unwrap();
        assert!(again.contains("future_field") && again.contains("future_item_field") && again.contains("hologram"));
    }

    #[test]
    fn higher_version_is_refused_with_a_message() {
        let header = serde_json::json!({"format": FORMAT, "version": 2, "something": "new"});
        let error = read(format!("{header}\n").as_bytes()).unwrap_err();
        assert_eq!(error, ReadError::UnsupportedVersion { found: 2 });
        assert!(error.to_string().contains("version 2"));
    }

    #[test]
    fn truncation_and_tampering_are_detected() {
        let bytes = rendered(Mode::Full);
        let cut = &bytes[..bytes.len() - 10];
        assert_eq!(read(cut).unwrap_err(), ReadError::BodyMismatch);
        assert_eq!(read(b"{\"format\":\"other\"}\n").unwrap_err(), ReadError::NotStx);
        assert_eq!(read(b"").unwrap_err(), ReadError::Empty);
    }
}
