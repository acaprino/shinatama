//! C27.3: how a seed reaches each destination (FRD-0005 R28, R31-R32).
//!
//! `argv_prompt` carries only a bootstrap of at most 2 KB: the marker and nonce, the last user
//! request and the instruction to read the seed file. Pi takes the seed as `@<path>`, Hermes
//! Agent as `--query-file`, DeepSeek Harness as the first ACP `session/prompt`. Every line is
//! escaped against command, shell, file-expansion and flag prefixes before it is shown.
use crate::history::{HANDOFF_MARKER, clip};
use crate::seed::escape_hostile;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const MAX_BOOTSTRAP_BYTES: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedChannel {
    ArgvPrompt,
    FileArg,
    QueryFile,
    ProtocolPrompt,
}

pub fn channel_for(agent: &str) -> Option<SeedChannel> {
    match agent {
        "claude_code" | "codex" | "opencode" => Some(SeedChannel::ArgvPrompt),
        "pi" => Some(SeedChannel::FileArg),
        "hermes" => Some(SeedChannel::QueryFile),
        "deepseek" => Some(SeedChannel::ProtocolPrompt),
        _ => None,
    }
}

/// The instruction file each destination loads on its own, used in the seed header.
pub fn instruction_file_for(agent: &str) -> Option<&'static str> {
    match agent {
        "claude_code" => Some("CLAUDE.md"),
        "codex" | "opencode" | "pi" => Some("AGENTS.md"),
        _ => None,
    }
}

/// What the launcher passes to the destination, already escaped and bounded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "channel")]
pub enum Delivery {
    ArgvPrompt { prompt: String },
    FileArg { file: String, prompt: String },
    QueryFile { file: String },
    ProtocolPrompt { prompt: String },
}

pub fn bootstrap(nonce: &str, seed_path: &Path, last_user_request: Option<&str>) -> String {
    let head = format!(
        "{HANDOFF_MARKER} nonce={nonce}\nRead the hand-off file {} completely before answering: it is the quoted history of this conversation.\n",
        seed_path.display()
    );
    let tail = format!("Then reply exactly `ready {nonce}` and one line saying where you will continue, and wait.\n");
    let room = MAX_BOOTSTRAP_BYTES.saturating_sub(head.len() + tail.len() + 32);
    let request = last_user_request
        .map(|r| format!("Last user request: {}\n", clip(&r.replace(['\r', '\n'], " "), room.saturating_sub(20))))
        .unwrap_or_default();
    let text = escape_hostile(&format!("{head}{request}{tail}"));
    debug_assert!(text.len() <= MAX_BOOTSTRAP_BYTES);
    text
}

/// Request header that declares a Shinatama control message to the model proxy. The value is
/// `<kind>;<sha256 hex of the canonical text of the message>`: the proxy honours it only when the
/// hash equals the human message it actually received, so the marker is never inferred from text.
pub const CONTROL_HEADER: &str = "x-shinatama-control";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlKind {
    Seed,
    Bootstrap,
    Receipt,
}

impl ControlKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ControlKind::Seed => "seed",
            ControlKind::Bootstrap => "bootstrap",
            ControlKind::Receipt => "receipt",
        }
    }
}

/// Line ends become `\n` and outer spaces, tabs and line ends are dropped: the terminal a message
/// is pasted through may convert the first and trim the second. The proxy applies the same rule.
pub fn canonical_control_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n").trim_matches([' ', '\t', '\n']).to_owned()
}

pub fn control_header_value(kind: ControlKind, text: &str) -> String {
    let hash = Sha256::digest(canonical_control_text(text).as_bytes());
    format!("{};{}", kind.as_str(), shinatama_export::stx::hex(&hash))
}

/// `(name, value)` of the control header for `text`.
pub fn control_header(kind: ControlKind, text: &str) -> (&'static str, String) {
    (CONTROL_HEADER, control_header_value(kind, text))
}

pub fn delivery(channel: SeedChannel, seed_path: &Path, seed_text: &str, bootstrap: &str) -> Delivery {
    match channel {
        SeedChannel::ArgvPrompt => Delivery::ArgvPrompt { prompt: bootstrap.to_owned() },
        SeedChannel::FileArg => Delivery::FileArg { file: format!("@{}", seed_path.display()), prompt: bootstrap.to_owned() },
        SeedChannel::QueryFile => Delivery::QueryFile { file: seed_path.display().to_string() },
        SeedChannel::ProtocolPrompt => Delivery::ProtocolPrompt { prompt: seed_text.to_owned() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_is_bounded_escaped_and_starts_with_the_marker() {
        let long = "/clear ".repeat(1000);
        let text = bootstrap("abc123", Path::new(r"C:\repo\.git\shinatama\handoffs\x\seed.md"), Some(&long));
        assert!(text.len() <= MAX_BOOTSTRAP_BYTES, "{}", text.len());
        assert!(text.starts_with("[shinatama handoff v1] nonce=abc123\n"));
        assert!(text.contains("Last user request: /clear"), "mid-line slashes are kept as text");
        assert!(text.contains("`ready abc123`"));
        assert!(text.lines().all(|l| !l.starts_with(['/', '!', '@', '#', '-'])));
    }

    #[test]
    fn control_header_hashes_the_canonical_text() {
        let text = bootstrap("abc123", Path::new(r"C:\repo\.git\shinatama\handoffs\x\seed.md"), Some("fix the totals"));
        let (name, value) = control_header(ControlKind::Bootstrap, &text);
        assert_eq!(name, "x-shinatama-control");
        let (kind, hash) = value.split_once(';').unwrap();
        assert_eq!(kind, "bootstrap");
        assert_eq!(hash.len(), 64);
        assert!(hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        // A terminal that converts line ends and trims the outer ones changes nothing.
        let pasted = text.replace('\n', "\r\n");
        assert_eq!(control_header_value(ControlKind::Bootstrap, pasted.trim_end()), value);
        assert_eq!(canonical_control_text("\r\n  a\r\nb \t\n"), "a\nb");
        assert_ne!(control_header_value(ControlKind::Bootstrap, &format!("{text}x")), value);
        assert!(control_header_value(ControlKind::Seed, &text).starts_with("seed;"));
        assert!(control_header_value(ControlKind::Receipt, &text).starts_with("receipt;"));
        // Known vector: sha256("abc").
        assert_eq!(control_header_value(ControlKind::Seed, " abc\n"), "seed;ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn channels_follow_the_agents() {
        assert_eq!(channel_for("claude_code"), Some(SeedChannel::ArgvPrompt));
        assert_eq!(channel_for("pi"), Some(SeedChannel::FileArg));
        assert_eq!(channel_for("hermes"), Some(SeedChannel::QueryFile));
        assert_eq!(channel_for("gemini"), None);
        let Delivery::FileArg { file, .. } = delivery(SeedChannel::FileArg, Path::new("C:/s.md"), "", "b") else { panic!() };
        assert_eq!(file, "@C:/s.md");
    }
}
