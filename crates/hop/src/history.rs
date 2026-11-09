//! C27.1: the seed's view of a conversation (FRD-0005 R21, R25-R27).
//!
//! Turns start at each user message. Tool calls are paired with their results as one digest
//! line. Noise is removed: injected `<system-reminder>` text, Codex injected-context messages,
//! Claude local-command records and earlier hand-off seeds with their `ready` replies, so a
//! chain A -> B -> C never nests seeds. Every text passes the seed redaction rules.
use shinatama_export::{AttachmentRef, ExportItem, ItemKind, Redactions, Redactor};

/// First line of every seed and catch-up (R26).
pub const HANDOFF_MARKER: &str = "[shinatama handoff v1]";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolStatus {
    Ok,
    Error,
    NoResult,
}
impl ToolStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolStatus::Ok => "ok",
            ToolStatus::Error => "error",
            ToolStatus::NoResult => "no result",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Assistant(String),
    Tool { name: String, target: String, status: ToolStatus, output: Option<String> },
    Compaction(String),
    Attachment(AttachmentRef),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Turn {
    /// The user's message that opened the turn; `None` for anything before the first one.
    pub user: Option<String>,
    pub lines: Vec<Line>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dropped {
    pub nested_seeds: u64,
    pub injected_context: u64,
    pub reminders: u64,
    pub local_commands: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History {
    pub turns: Vec<Turn>,
    pub attachments: Vec<AttachmentRef>,
    /// Seed-rule redactions applied here (on top of the export's own).
    pub redactions: Redactions,
    pub dropped: Dropped,
}
impl History {
    pub fn first_user(&self) -> Option<&str> {
        self.turns.iter().find_map(|turn| turn.user.as_deref())
    }
    pub fn last_user(&self) -> Option<&str> {
        self.turns.iter().rev().find_map(|turn| turn.user.as_deref())
    }
    pub fn is_empty(&self) -> bool {
        self.turns.iter().all(|turn| turn.user.is_none() && !turn.lines.iter().any(|l| matches!(l, Line::Assistant(_) | Line::Compaction(_))))
    }
}

fn strip_blocks(text: &str, tag: &str, removed: &mut u64) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(&open) {
        let Some(end) = rest[start..].find(&close) else { break };
        out.push_str(&rest[..start]);
        rest = &rest[start + end + close.len()..];
        *removed += 1;
    }
    out.push_str(rest);
    out
}

/// A message made only of injected tagged blocks (Codex environment context, user
/// instructions, permissions; Claude local command records).
fn only_tagged(text: &str, tags: &[&str]) -> bool {
    let mut rest = text.trim();
    if rest.is_empty() {
        return false;
    }
    while !rest.is_empty() {
        let Some(tag) = tags.iter().find(|tag| rest.starts_with(&format!("<{tag}>")) || rest.starts_with(&format!("<{tag} "))) else {
            return false;
        };
        let close = format!("</{tag}>");
        let Some(end) = rest.find(&close) else { return false };
        rest = rest[end + close.len()..].trim_start();
    }
    true
}

const INJECTED: &[&str] = &["environment_context", "user_instructions", "permissions instructions", "user_shell_command", "turn_aborted"];
const LOCAL_COMMAND: &[&str] = &["command-name", "command-message", "command-args", "local-command-stdout", "local-command-stderr", "local-command-caveat"];

/// One-line target of a tool call: path, command head, pattern or URL (R21).
pub fn tool_target(arguments: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(arguments).unwrap_or(serde_json::Value::Null);
    let pick = ["file_path", "path", "notebook_path", "command", "cmd", "pattern", "query", "url", "description", "prompt", "skill"]
        .iter()
        .find_map(|key| value.get(*key))
        .map(|field| match field {
            serde_json::Value::String(text) => text.clone(),
            serde_json::Value::Array(parts) => parts.iter().filter_map(|p| p.as_str()).collect::<Vec<_>>().join(" "),
            other => other.to_string(),
        })
        .unwrap_or_else(|| if value.is_null() { arguments.to_owned() } else { value.to_string() });
    let first = pick.lines().next().unwrap_or_default().trim();
    clip(first, 80)
}

/// At most `max` bytes, cut on a character boundary, with an ellipsis when cut.
pub fn clip(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut end = max.saturating_sub(3);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

/// Normalizes canonical items (already redacted by the export pass) for a seed.
pub fn normalize(items: &[ExportItem]) -> History {
    let mut redactor = Redactor::for_seed();
    let mut history = History::default();
    let mut current = Turn::default();
    let mut skip_ready = false;
    let mut open_calls: Vec<(String, usize, usize)> = Vec::new(); // (call id, turn index marker, line index)
    let mut turns: Vec<Turn> = Vec::new();
    let clean = |text: &str, history: &mut History, redactor: &mut Redactor| -> String {
        let text = strip_blocks(text, "system-reminder", &mut history.dropped.reminders);
        // Export markers use upper case; the seed shows one convention (R27).
        let text = text.replace("[REDACTED:", "[redacted:");
        redactor.redact(text.trim())
    };
    for item in items {
        match item.kind {
            ItemKind::UserText => {
                let raw = item.text.trim_start();
                if raw.starts_with(HANDOFF_MARKER) {
                    history.dropped.nested_seeds += 1;
                    skip_ready = true;
                    continue;
                }
                if only_tagged(raw, INJECTED) {
                    history.dropped.injected_context += 1;
                    continue;
                }
                if only_tagged(raw, LOCAL_COMMAND) {
                    history.dropped.local_commands += 1;
                    continue;
                }
                let text = clean(raw, &mut history, &mut redactor);
                if text.is_empty() {
                    continue;
                }
                skip_ready = false;
                if current.user.is_some() || !current.lines.is_empty() {
                    turns.push(std::mem::take(&mut current));
                    open_calls.clear();
                }
                current.user = Some(text);
            }
            ItemKind::AssistantText => {
                let text = clean(&item.text, &mut history, &mut redactor);
                if skip_ready && text.starts_with("ready ") && text.lines().count() <= 3 {
                    skip_ready = false;
                    continue;
                }
                skip_ready = false;
                if !text.is_empty() {
                    current.lines.push(Line::Assistant(text));
                }
            }
            ItemKind::ToolCall => {
                let name = item.tool.as_ref().map_or_else(|| "tool".to_owned(), |t| t.name.clone());
                let target = redactor.redact(&tool_target(&item.text).replace("[REDACTED:", "[redacted:"));
                current.lines.push(Line::Tool { name, target, status: ToolStatus::NoResult, output: None });
                if let Some(id) = item.tool.as_ref().and_then(|t| t.call_id.clone()) {
                    open_calls.push((id, turns.len(), current.lines.len() - 1));
                }
            }
            ItemKind::ToolResult => {
                let id = item.tool.as_ref().and_then(|t| t.call_id.clone());
                let error = item.tool.as_ref().is_some_and(|t| t.is_error);
                let output = clean(&item.text, &mut history, &mut redactor);
                let slot = id.and_then(|id| open_calls.iter().position(|(open, turn, _)| *open == id && *turn == turns.len()));
                match slot {
                    Some(position) => {
                        let (_, _, line) = open_calls.remove(position);
                        if let Some(Line::Tool { status, output: kept, .. }) = current.lines.get_mut(line) {
                            *status = if error { ToolStatus::Error } else { ToolStatus::Ok };
                            *kept = Some(output);
                        }
                    }
                    None => current.lines.push(Line::Tool {
                        name: item.tool.as_ref().map_or_else(|| "tool".to_owned(), |t| t.name.clone()),
                        target: String::new(),
                        status: if error { ToolStatus::Error } else { ToolStatus::Ok },
                        output: Some(output),
                    }),
                }
            }
            ItemKind::Compaction => {
                let text = clean(&item.text, &mut history, &mut redactor);
                if !text.is_empty() {
                    current.lines.push(Line::Compaction(text));
                }
            }
            ItemKind::Attachment => {
                if let Some(attachment) = &item.attachment {
                    history.attachments.push(attachment.clone());
                    current.lines.push(Line::Attachment(attachment.clone()));
                }
            }
            ItemKind::Meta => {}
        }
    }
    if current.user.is_some() || !current.lines.is_empty() {
        turns.push(current);
    }
    history.turns = turns;
    history.redactions = redactor.into_counts();
    history
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use shinatama_export::{Provenance, ToolInfo};

    pub fn item(kind: ItemKind, text: &str) -> ExportItem {
        ExportItem {
            kind,
            text: text.into(),
            tool: None,
            attachment: None,
            provenance: Provenance { agent: "claude_code".into(), native_session_id: "s".into(), entry_id: None, entry_ordinal: 0, timestamp: None },
        }
    }
    pub fn tool(kind: ItemKind, text: &str, name: &str, id: &str, error: bool) -> ExportItem {
        let mut value = item(kind, text);
        value.tool = Some(ToolInfo { name: name.into(), call_id: Some(id.into()), is_error: error });
        value
    }

    #[test]
    fn groups_turns_and_pairs_tool_results() {
        let history = normalize(&[
            item(ItemKind::UserText, "fix the parser"),
            item(ItemKind::AssistantText, "Looking."),
            tool(ItemKind::ToolCall, r#"{"file_path":"src/parser.rs"}"#, "Read", "t1", false),
            tool(ItemKind::ToolResult, "fn parse() {}", "Read", "t1", false),
            tool(ItemKind::ToolCall, r#"{"command":"cargo test --all\nmore"}"#, "Bash", "t2", false),
            tool(ItemKind::ToolResult, "1 failed", "Bash", "t2", true),
            item(ItemKind::UserText, "now the lexer"),
        ]);
        assert_eq!(history.turns.len(), 2);
        assert_eq!(history.first_user(), Some("fix the parser"));
        assert_eq!(history.last_user(), Some("now the lexer"));
        let Line::Tool { name, target, status, .. } = &history.turns[0].lines[1] else { panic!() };
        assert_eq!((name.as_str(), target.as_str(), *status), ("Read", "src/parser.rs", ToolStatus::Ok));
        let Line::Tool { target, status, .. } = &history.turns[0].lines[2] else { panic!() };
        assert_eq!((target.as_str(), *status), ("cargo test --all", ToolStatus::Error));
    }

    #[test]
    fn drops_nested_seeds_injected_context_and_reminders() {
        let history = normalize(&[
            item(ItemKind::UserText, "<environment_context>\n<cwd>C:/x</cwd>\n</environment_context>"),
            item(ItemKind::UserText, "[shinatama handoff v1] nonce=abc\nold history"),
            item(ItemKind::AssistantText, "ready abc\nContinuing."),
            item(ItemKind::UserText, "<command-name>/clear</command-name><local-command-stdout></local-command-stdout>"),
            item(ItemKind::UserText, "real request <system-reminder>secret hook text</system-reminder>"),
            item(ItemKind::AssistantText, "ready to help"),
        ]);
        assert_eq!(history.dropped, Dropped { nested_seeds: 1, injected_context: 1, reminders: 1, local_commands: 1 });
        assert_eq!(history.turns.len(), 1);
        assert_eq!(history.turns[0].user.as_deref(), Some("real request"));
        assert_eq!(history.turns[0].lines, [Line::Assistant("ready to help".into())]);
    }

    #[test]
    fn seed_rules_redact_and_unify_markers() {
        let history = normalize(&[
            item(ItemKind::UserText, "key [REDACTED:anthropic_key] and Zx9QvR2mT8kLp4WnB7yHc3JdF6sGa1Ue5VtK0rYqXoNi"),
            tool(ItemKind::ToolCall, r#"{"command":"curl -H 'Authorization: Bearer abcdefghijklmnop0123456789' x"}"#, "Bash", "t1", false),
        ]);
        let user = history.turns[0].user.as_deref().unwrap();
        assert_eq!(user, "key [redacted:anthropic_key] and [redacted:high_entropy]");
        let Line::Tool { target, .. } = &history.turns[0].lines[0] else { panic!() };
        assert!(target.contains("[redacted:bearer_token]"), "{target}");
        assert_eq!(history.redactions.get("high_entropy"), Some(&1));
    }

    #[test]
    fn clip_respects_utf8() {
        assert_eq!(clip("città città", 7), "citt...");
        assert_eq!(clip("🦀🦀🦀", 9), "🦀...");
        assert_eq!(clip("short", 80), "short");
    }
}
