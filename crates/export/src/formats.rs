//! The four export choices and what each one declares (FRD-0006 R31, R39).
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// `.stx.jsonl`, Shinatama's own export.
    Stx,
    /// Mukade's portable interchange JSON; never a lossless backup.
    Mukade,
    /// The agent's own export command, only where it is proven.
    Native,
    /// The readable rendering.
    Md,
}
impl Format {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "stx" => Some(Format::Stx),
            "mukade" => Some(Format::Mukade),
            "native" => Some(Format::Native),
            "md" | "markdown" => Some(Format::Md),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Stx => "stx",
            Format::Mukade => "mukade",
            Format::Native => "native",
            Format::Md => "md",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Format::Stx => crate::stx::EXTENSION,
            Format::Mukade => ".mukade.json",
            Format::Native => ".native",
            Format::Md => ".md",
        }
    }
    pub const ALL: [Format; 4] = [Format::Stx, Format::Mukade, Format::Native, Format::Md];
}

/// What one export choice promises, shown before writing and repeated in the report (R31).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormatDeclaration {
    pub format: Format,
    pub label: String,
    pub format_version: String,
    pub raw_records: String,
    pub unknown_fields: String,
    pub attachments: String,
    pub redaction: String,
    pub losses: Vec<String>,
    pub overwrite: String,
    pub atomic_output: String,
    pub lossless_backup: bool,
    /// `None` when the format can be written; otherwise why it cannot, for this agent.
    pub unavailable: Option<String>,
}

const OVERWRITE: &str = "An existing file is never replaced unless replacing is chosen explicitly.";
const ATOMIC: &str = "Written to a temporary file in the same folder and renamed when complete; a failure leaves no partial file.";
const REDACTION: &str = "Secrets are redacted in every mode; portable also replaces the home folder with ~ and removes e-mail addresses.";

pub fn declaration(format: Format, native_supported: bool) -> FormatDeclaration {
    let common_losses = vec![
        "Reasoning is never exported.".to_owned(),
        "Native metadata records are dropped and counted.".to_owned(),
    ];
    match format {
        Format::Stx => FormatDeclaration {
            format,
            label: "Shinatama STX (.stx.jsonl)".into(),
            format_version: format!("{} {}", crate::stx::FORMAT, crate::stx::VERSION),
            raw_records: "Native raw records are not included; each item keeps its provenance (agent, session, entry id, ordinal, timestamp).".into(),
            unknown_fields: "Readers keep unknown fields and write them back; a higher version is refused.".into(),
            attachments: "Hashed references; content is copied beside the file only with attachments enabled.".into(),
            redaction: REDACTION.into(),
            losses: [common_losses.clone(), vec!["Structured tool results are kept as their text form.".into()]].concat(),
            overwrite: OVERWRITE.into(),
            atomic_output: ATOMIC.into(),
            lossless_backup: false,
            unavailable: None,
        },
        Format::Mukade => FormatDeclaration {
            format,
            label: "Portable Mukade JSON".into(),
            format_version: format!("mukade document ({})", "pinned library revision"),
            raw_records: "Mukade documents never carry source records.".into(),
            unknown_fields: "Mukade's own document shape; Shinatama adds no fields to it.".into(),
            attachments: "Attachment references as Mukade records them; content is not copied.".into(),
            redaction: REDACTION.into(),
            losses: [common_losses, vec!["This is a portable interchange document, not a lossless backup.".into(), "Thinking blocks and opaque signatures are removed.".into()]].concat(),
            overwrite: OVERWRITE.into(),
            atomic_output: ATOMIC.into(),
            lossless_backup: false,
            unavailable: None,
        },
        Format::Native => FormatDeclaration {
            format,
            label: "The agent's own export".into(),
            format_version: "Defined by the agent.".into(),
            raw_records: "Defined by the agent.".into(),
            unknown_fields: "Defined by the agent.".into(),
            attachments: "Defined by the agent.".into(),
            redaction: "Not applied by Shinatama.".into(),
            losses: vec![],
            overwrite: OVERWRITE.into(),
            atomic_output: ATOMIC.into(),
            lossless_backup: false,
            unavailable: (!native_supported).then(|| {
                "No native export has been proven for this agent yet; choose STX, Mukade JSON or Markdown.".into()
            }),
        },
        Format::Md => FormatDeclaration {
            format,
            label: "Markdown".into(),
            format_version: "Readable rendering, not machine-readable.".into(),
            raw_records: "Not included.".into(),
            unknown_fields: "Not applicable.".into(),
            attachments: "Listed by name and hash; content is copied only with attachments enabled.".into(),
            redaction: REDACTION.into(),
            losses: [common_losses, vec!["Provenance per item is not shown.".into()]].concat(),
            overwrite: OVERWRITE.into(),
            atomic_output: ATOMIC.into(),
            lossless_backup: false,
            unavailable: None,
        },
    }
}
