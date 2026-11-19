//! Companion card protocol (FRD-0012).
//!
//! - [`wrap`]: every operand reaches the companion as escaped, untrusted data (R12-R13, R36).
//! - [`presets`]: fixed English templates, IT/EN labels and the level rules (R33-R34, R38).
//! - [`card`]: the trailing `shinatama-card` block, parsed with item-level limits (R51-R53).
//! - [`render`]: card prose to inert HTML through the shared Markdown renderer (R41-R42).
//!
//! Renderer decision (resolves the legacy "native, no JavaScript runtime" wording for the
//! Tauri shell): cards are rendered by Shinatama's own Rust Markdown renderer into a fixed tag
//! subset with every character escaped; the WebView displays that HTML but no script, remote
//! request or image is ever produced. `mermaid` blocks stay visible source: no diagram
//! renderer is qualified yet, so none is claimed.
pub mod card;
pub mod presets;
pub mod render;
pub mod text;
pub mod wrap;
