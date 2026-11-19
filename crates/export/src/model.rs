//! The canonical transcript as export sees it (ADR-0021: kinds and provenance).
use serde::{Deserialize, Serialize};

/// `full` is the user's own backup; `portable` is redacted further for sharing (R33).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Full,
    Portable,
}
impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Full => "full",
            Mode::Portable => "portable",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "full" => Some(Mode::Full),
            "portable" => Some(Mode::Portable),
            _ => None,
        }
    }
}

/// Exportable item kinds. There is deliberately no `reasoning` variant (INV-001).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    UserText,
    AssistantText,
    ToolCall,
    ToolResult,
    Attachment,
    Compaction,
    Meta,
}
impl ItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::UserText => "user_text",
            ItemKind::AssistantText => "assistant_text",
            ItemKind::ToolCall => "tool_call",
            ItemKind::ToolResult => "tool_result",
            ItemKind::Attachment => "attachment",
            ItemKind::Compaction => "compaction",
            ItemKind::Meta => "meta",
        }
    }
}

/// Where an item came from (INV-002).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub agent: String,
    pub native_session_id: String,
    /// The native entry id, where the source record has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_id: Option<String>,
    /// Index of the source entry in native order.
    pub entry_ordinal: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
}

/// A hashed reference; content is copied only with `--with-attachments` (R33).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentRef {
    /// The source's own kind for it (Claude `file`, an image block, ...).
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// SHA-256 of the attachment content when the record carries it inline, else of the
    /// reference record itself; `content_inline` tells which.
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    pub content_inline: bool,
    /// Relative path of the copied file, set only when the content was copied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copied_as: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportItem {
    pub kind: ItemKind,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment: Option<AttachmentRef>,
    pub provenance: Provenance,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportSource {
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_version: Option<String>,
    pub native_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_sha256: Option<String>,
}

/// What the projection left out on purpose, so the report can say so.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Omissions {
    pub reasoning: u64,
    pub meta: u64,
    pub structured_results: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Transcript {
    pub source: ExportSource,
    pub thread: Option<String>,
    pub items: Vec<ExportItem>,
    pub omitted: Omissions,
    pub instruction_files: Vec<String>,
}

impl Transcript {
    /// One redaction pass over every free-text field. Identifiers (session ids, entry ids,
    /// hashes, timestamps) and tool names are structural and kept.
    pub fn redacted(&self, redactor: &mut crate::redact::Redactor) -> Transcript {
        let mut copy = self.clone();
        let opt = |redactor: &mut crate::redact::Redactor, value: &mut Option<String>| {
            if let Some(text) = value.as_mut() {
                *text = redactor.redact(text);
            }
        };
        opt(redactor, &mut copy.source.title);
        opt(redactor, &mut copy.source.cwd);
        for item in &mut copy.items {
            item.text = redactor.redact(&item.text);
            if let Some(attachment) = item.attachment.as_mut() {
                opt(redactor, &mut attachment.name);
            }
        }
        for file in &mut copy.instruction_files {
            *file = redactor.redact(file);
        }
        copy
    }
}
