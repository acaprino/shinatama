//! `--format md`: a readable rendering of the (already redacted) transcript (R39).
use crate::model::{ItemKind, Mode, Transcript};

/// A fence longer than any backtick run inside the text, so content never closes it early.
fn fence(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in text.chars() {
        if c == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    "`".repeat((longest + 1).max(3))
}

fn block(out: &mut String, info: &str, text: &str) {
    let fence = fence(text);
    out.push_str(&fence);
    out.push_str(info);
    out.push('\n');
    out.push_str(text);
    if !text.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&fence);
    out.push_str("\n\n");
}

/// Escapes characters that would start Markdown structure inside an inline value.
fn inline(text: &str) -> String {
    text.chars()
        .flat_map(|c| match c {
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '|' => vec!['\\', c],
            '\n' | '\r' => vec![' '],
            c => vec![c],
        })
        .collect()
}

pub fn render(transcript: &Transcript, mode: Mode, created: &str) -> String {
    let source = &transcript.source;
    let mut out = String::new();
    let title = source
        .title
        .as_deref()
        .map(inline)
        .unwrap_or_else(|| format!("Session {}", inline(&source.native_session_id)));
    out.push_str(&format!("# {title}\n\n"));
    let mut facts = vec![format!(
        "- Agent: {}{}",
        inline(&source.agent),
        source
            .agent_version
            .as_deref()
            .map(|v| format!(" {}", inline(v)))
            .unwrap_or_default()
    )];
    facts.push(format!("- Session: `{}`", source.native_session_id.replace('`', "")));
    if let Some(cwd) = &source.cwd {
        facts.push(format!("- Project: {}", inline(cwd)));
    }
    if let Some(branch) = &source.git_branch {
        facts.push(format!("- Branch: {}", inline(branch)));
    }
    if let Some(thread) = &transcript.thread {
        facts.push(format!("- Thread: `{}`", thread.replace('`', "")));
    }
    facts.push(format!(
        "- Exported: {} ({} mode; secrets redacted; reasoning not included)",
        inline(created),
        mode.as_str()
    ));
    out.push_str(&facts.join("\n"));
    out.push_str("\n\n---\n\n");
    let mut last_heading: Option<&str> = None;
    for item in &transcript.items {
        let heading = match item.kind {
            ItemKind::UserText | ItemKind::Attachment => "User",
            ItemKind::AssistantText | ItemKind::ToolCall | ItemKind::ToolResult => "Assistant",
            ItemKind::Compaction => "Compaction summary",
            ItemKind::Meta => "Note",
        };
        if last_heading != Some(heading) || matches!(item.kind, ItemKind::UserText) {
            out.push_str(&format!("## {heading}\n\n"));
            last_heading = Some(heading);
        }
        match item.kind {
            ItemKind::UserText | ItemKind::AssistantText | ItemKind::Compaction | ItemKind::Meta => {
                out.push_str(item.text.trim_end());
                out.push_str("\n\n");
            }
            ItemKind::ToolCall => {
                let name = item.tool.as_ref().map_or("tool", |t| t.name.as_str());
                out.push_str(&format!("**Tool call:** {}\n\n", inline(name)));
                block(&mut out, "json", &item.text);
            }
            ItemKind::ToolResult => {
                let error = item.tool.as_ref().is_some_and(|t| t.is_error);
                out.push_str(if error { "**Tool result (error):**\n\n" } else { "**Tool result:**\n\n" });
                block(&mut out, "", &item.text);
            }
            ItemKind::Attachment => {
                let Some(attachment) = &item.attachment else { continue };
                let name = attachment.name.as_deref().map(inline).unwrap_or_else(|| inline(&attachment.kind));
                match &attachment.copied_as {
                    Some(path) => out.push_str(&format!("*Attachment:* {name} (`{}`, copied as `{}`)\n\n", attachment.sha256, path.replace('`', ""))),
                    None => out.push_str(&format!("*Attachment:* {name} (`{}`, not copied)\n\n", attachment.sha256)),
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    #[test]
    fn fences_survive_backticks_and_tool_results_are_blocks() {
        let prov = Provenance { agent: "codex".into(), native_session_id: "s1".into(), entry_id: None, entry_ordinal: 0, timestamp: None };
        let transcript = Transcript {
            source: ExportSource { agent: "codex".into(), native_session_id: "s1".into(), title: Some("# not a heading".into()), ..Default::default() },
            items: vec![
                ExportItem { kind: ItemKind::UserText, text: "hello".into(), tool: None, attachment: None, provenance: prov.clone() },
                ExportItem { kind: ItemKind::ToolResult, text: "```\ninner\n```".into(), tool: Some(ToolInfo { name: "shell".into(), call_id: None, is_error: true }), attachment: None, provenance: prov },
            ],
            ..Default::default()
        };
        let md = render(&transcript, Mode::Full, "2026-11-09T00:00:00Z");
        assert!(md.starts_with("# \\# not a heading\n"));
        assert!(md.contains("**Tool result (error):**\n\n````\n```\ninner\n```\n````\n"));
        assert!(md.contains("reasoning not included"));
    }
}
