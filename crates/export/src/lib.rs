//! Shinatama's session export formats (FRD-0006 R31-R39, ADR-0021).
//!
//! The crate never reads a native store and never parses Mukade's interchange JSON into STX or
//! the other way round (ADR-0021 INV-006). Callers project a session into [`model::Transcript`];
//! `reasoning` has no representation in that model, so it cannot be exported (INV-001).
pub mod base64;
pub mod formats;
pub mod markdown;
pub mod model;
pub mod output;
pub mod redact;
pub mod stx;
pub mod time;

pub use formats::{Format, FormatDeclaration};
pub use model::{
    AttachmentRef, ExportItem, ExportSource, ItemKind, Mode, Omissions, Provenance, ToolInfo,
    Transcript,
};
pub use redact::{Redactions, Redactor};
