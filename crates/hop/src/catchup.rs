//! C29.2: native cursor and catch-up (FRD-0005 R39, R45, R47, R50).
//!
//! A cursor records how much of a source conversation was carried: the number of canonical
//! items, the last native entry and a hash of everything up to it. Turns the source gains
//! after the snapshot are sent as a catch-up, rendered like a seed and confirmed with its own
//! nonce, at most twice; a source still changing after that is left for the user to decide.
use crate::history::{HANDOFF_MARKER, normalize};
use crate::seed::{self, escape_hostile, verbatim};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shinatama_export::ExportItem;

pub const MAX_CATCHUP_ROUNDS: u32 = 2;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeCursor {
    pub items: usize,
    pub last_entry_ordinal: Option<usize>,
    pub last_entry_id: Option<String>,
    pub sha256: String,
}

fn digest(items: &[ExportItem]) -> String {
    let mut hash = Sha256::new();
    for item in items {
        hash.update(item.kind.as_str().as_bytes());
        hash.update((item.text.len() as u64).to_le_bytes());
        hash.update(item.text.as_bytes());
        hash.update((item.provenance.entry_ordinal as u64).to_le_bytes());
    }
    shinatama_export::stx::hex(&hash.finalize())
}

impl NativeCursor {
    pub fn at_end(items: &[ExportItem]) -> Self {
        let last = items.last();
        Self {
            items: items.len(),
            last_entry_ordinal: last.map(|i| i.provenance.entry_ordinal),
            last_entry_id: last.and_then(|i| i.provenance.entry_id.clone()),
            sha256: digest(items),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum CursorError {
    /// The source no longer starts with what was carried (rewound or rewritten): a new
    /// preview is needed, never a guess.
    HistoryChanged,
}

/// Items the source gained after `cursor`.
pub fn items_since<'a>(
    cursor: &NativeCursor,
    items: &'a [ExportItem],
) -> Result<&'a [ExportItem], CursorError> {
    if items.len() < cursor.items || digest(&items[..cursor.items]) != cursor.sha256 {
        return Err(CursorError::HistoryChanged);
    }
    Ok(&items[cursor.items..])
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Catchup {
    pub text: String,
    pub nonce: String,
    pub turns: usize,
    pub omitted_turns: usize,
}

/// "While you were away (N turns, via <agent>): ..." limited to the missed turns and the
/// destination's seed budget, newest kept first. `None` when nothing new was said.
pub fn render(
    new_items: &[ExportItem],
    source_agent: &str,
    nonce: &str,
    window_tokens: Option<u64>,
) -> Option<Catchup> {
    let history = normalize(new_items);
    if history.is_empty() {
        return None;
    }
    let limit = seed::budget(window_tokens).seed_bytes as usize;
    let count = history.turns.len();
    let head = format!(
        "{HANDOFF_MARKER} nonce={nonce}\nWhile you were away ({count} turn{}, via {source_agent}): the conversation continued as quoted below. Continue from the last user request. Reply exactly `ready {nonce}` and one line, then wait.\n",
        if count == 1 { "" } else { "s" }
    );
    let mut ignored = 0;
    let mut kept: Vec<String> = Vec::new();
    let mut used = head.len();
    for turn in history.turns.iter().rev() {
        let text = escape_hostile(&verbatim(turn, false, &mut ignored));
        if used + text.len() + 32 > limit && !kept.is_empty() {
            break;
        }
        used += text.len();
        kept.push(text);
    }
    let omitted_turns = count - kept.len();
    let mut text = escape_hostile(&head);
    if omitted_turns > 0 {
        text.push_str(&seed::omitted(omitted_turns));
    }
    for part in kept.iter().rev() {
        text.push_str(part);
    }
    Some(Catchup {
        text,
        nonce: nonce.to_owned(),
        turns: count,
        omitted_turns,
    })
}

pub fn round_nonce(nonce: &str, round: u32) -> String {
    format!("{nonce}c{round}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::tests::item;
    use shinatama_export::ItemKind;

    fn items(n: usize) -> Vec<ExportItem> {
        (0..n)
            .map(|i| {
                let mut value = item(
                    if i % 2 == 0 {
                        ItemKind::UserText
                    } else {
                        ItemKind::AssistantText
                    },
                    &format!("message {i}"),
                );
                value.provenance.entry_ordinal = i;
                value.provenance.entry_id = Some(format!("e{i}"));
                value
            })
            .collect()
    }

    #[test]
    fn cursor_finds_new_items_and_refuses_rewritten_history() {
        let all = items(6);
        let cursor = NativeCursor::at_end(&all[..4]);
        assert_eq!(cursor.last_entry_id.as_deref(), Some("e3"));
        assert_eq!(items_since(&cursor, &all).unwrap().len(), 2);
        assert!(items_since(&cursor, &all[..4]).unwrap().is_empty());
        let mut rewritten = all.clone();
        rewritten[1].text = "edited".into();
        assert_eq!(
            items_since(&cursor, &rewritten),
            Err(CursorError::HistoryChanged)
        );
        assert_eq!(
            items_since(&cursor, &all[..3]),
            Err(CursorError::HistoryChanged)
        );
    }

    #[test]
    fn catchup_names_the_turns_and_uses_its_own_nonce() {
        let all = items(6);
        let catchup = render(&all[4..], "claude_code", &round_nonce("abc", 1), None).unwrap();
        assert!(catchup.text.starts_with(
            "[shinatama handoff v1] nonce=abcc1\nWhile you were away (1 turn, via claude_code)"
        ));
        assert!(
            catchup.text.contains("[user] message 4")
                && catchup.text.contains("[assistant] message 5")
        );
        assert!(catchup.text.contains("`ready abcc1`"));
        assert!(render(&[], "claude_code", "x", None).is_none());
    }

    #[test]
    fn long_catchups_keep_the_newest_turns_within_budget() {
        let mut many = Vec::new();
        for i in 0..400 {
            many.push(item(
                ItemKind::UserText,
                &format!("request {i} {}", "x".repeat(400)),
            ));
            many.push(item(ItemKind::AssistantText, &format!("answer {i}")));
        }
        let catchup = render(&many, "codex", "n", Some(20_000)).unwrap();
        assert!(catchup.text.len() <= seed::budget(Some(20_000)).seed_bytes as usize);
        assert!(catchup.omitted_turns > 0);
        assert!(catchup.text.contains("request 399 ") && !catchup.text.contains("request 0 "));
        assert!(
            catchup
                .text
                .contains(&format!("[{} turns omitted]", catchup.omitted_turns))
        );
    }
}
