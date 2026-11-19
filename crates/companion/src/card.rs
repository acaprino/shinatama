//! The `shinatama-card` block (FRD-0012 R51-R53).
//!
//! Every answer ends with one fenced `shinatama-card` block holding one JSON object. Only the
//! last fence counts, and only when it is the last element; earlier ones stay visible as code.
//! The block is removed from the prose before rendering. Unknown keys are ignored; an item
//! outside its limits is dropped and reported, never the block or the prose. Invalid JSON or no
//! block means no chips, entries, quiz or sources, and the kind Shinatama asked for.
use crate::text::{fold, identity, valid_name};
use serde::Serialize;
use serde_json::Value;

pub const MAX_BLOCK_BYTES: usize = 4 * 1024;
pub const RISK_MARKERS: [&str; 6] = ["worktree_only", "outside_worktree", "credentials", "costs_money", "opens_port_or_service", "hard_to_undo"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardKind {
    Learning,
    AidedDecision,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Taught {
    pub name: String,
    pub identity: String,
    pub theme: Option<String>,
    pub keywords: Vec<String>,
    pub level: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Quiz {
    pub question: String,
    pub options: Vec<String>,
    pub expected: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Source {
    pub url: String,
    pub title: String,
    pub read: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CardBlock {
    pub taught: Vec<Taught>,
    pub chips: Vec<String>,
    pub risk: Vec<String>,
    pub quiz: Option<Quiz>,
    pub sources: Vec<Source>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ParsedCard {
    /// The answer without the counted block.
    pub prose: String,
    pub kind: CardKind,
    pub block: CardBlock,
    /// Why items (or the whole block) were not used.
    pub dropped: Vec<String>,
    pub block_found: bool,
}

/// Where the last fence starts and ends, and its info string. Fences are ``` or ~~~ runs of 3+.
fn fences(text: &str) -> Vec<(usize, usize, String)> {
    let mut found = Vec::new();
    let mut offset = 0;
    let mut open: Option<(usize, String, String)> = None; // (start, marker, info)
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start().trim_end_matches(['\n', '\r']);
        let marker: String = trimmed.chars().take_while(|c| *c == '`' || *c == '~').collect();
        match &open {
            None if marker.len() >= 3 => open = Some((offset, marker.clone(), trimmed[marker.len()..].trim().to_owned())),
            Some((start, opened, info)) if marker.len() >= opened.len() && marker.chars().all(|c| opened.starts_with(c)) && trimmed[marker.len()..].trim().is_empty() => {
                found.push((*start, offset + line.len(), info.clone()));
                open = None;
            }
            _ => {}
        }
        offset += line.len();
    }
    found
}

fn take_string(value: &Value, cap: usize, dropped: &mut Vec<String>, what: &str) -> Option<String> {
    let raw = value.as_str()?;
    let folded = fold(raw);
    if valid_name(&folded, cap) {
        Some(folded)
    } else {
        dropped.push(format!("{what} outside its limits"));
        None
    }
}

fn plain(value: &Value, cap: usize) -> Option<String> {
    let text = fold(value.as_str()?);
    (!text.is_empty() && text.chars().count() <= cap && !text.chars().any(char::is_control)).then_some(text)
}

pub fn parse(answer: &str, requested: Option<CardKind>) -> ParsedCard {
    let mut dropped = Vec::new();
    let default_kind = requested.unwrap_or(CardKind::Learning);
    let all = fences(answer);
    let last = all.last().filter(|(_, end, info)| info == "shinatama-card" && answer[*end..].trim().is_empty());
    let Some((start, end, _)) = last.cloned() else {
        if all.iter().any(|(_, _, info)| info == "shinatama-card") {
            dropped.push("card block is not the last element; shown as code".into());
        }
        return ParsedCard { prose: answer.trim_end().to_owned(), kind: default_kind, block: CardBlock::default(), dropped, block_found: false };
    };
    let prose = answer[..start].trim_end().to_owned();
    let inner = &answer[start..end];
    let body: String = inner.split_inclusive('\n').skip(1).collect::<Vec<_>>().join("");
    let body = body.trim_end().trim_end_matches(['`', '~']).trim();
    let empty = |dropped: Vec<String>| ParsedCard { prose: prose.clone(), kind: default_kind, block: CardBlock::default(), dropped, block_found: true };
    if body.len() > MAX_BLOCK_BYTES {
        dropped.push("card block exceeds 4 KB".into());
        return empty(dropped);
    }
    let Ok(Value::Object(object)) = serde_json::from_str::<Value>(body) else {
        dropped.push("card block is not one JSON object".into());
        return empty(dropped);
    };
    let mut block = CardBlock::default();
    // Shinatama sets the kind it asked for; only a finished-turn card may declare it (R53).
    let kind = match (requested, object.get("kind").and_then(Value::as_str)) {
        (Some(kind), _) => kind,
        (None, Some("aided_decision")) => CardKind::AidedDecision,
        _ => CardKind::Learning,
    };
    if let Some(items) = object.get("taught").and_then(Value::as_array) {
        for item in items {
            if block.taught.len() >= 2 {
                dropped.push("more than 2 taught items".into());
                break;
            }
            let Some(name) = item.get("name").and_then(|v| take_string(v, 60, &mut dropped, "taught name")) else { continue };
            let Some(level) = item.get("level").and_then(Value::as_u64).filter(|l| (1..=4).contains(l)) else {
                dropped.push("taught level outside 1-4".into());
                continue;
            };
            let theme = item.get("theme").and_then(|v| take_string(v, 30, &mut dropped, "theme"));
            let keywords = item
                .get("keywords")
                .and_then(Value::as_array)
                .map(|k| k.iter().filter_map(|v| take_string(v, 24, &mut dropped, "keyword")).take(4).collect())
                .unwrap_or_default();
            block.taught.push(Taught { identity: identity(&name), name, theme, keywords, level: level as u8 });
        }
    }
    if kind == CardKind::AidedDecision && !block.taught.is_empty() {
        dropped.push("an Aided Decision card teaches nothing".into());
        block.taught.clear();
    }
    if let Some(chips) = object.get("chips").and_then(Value::as_array) {
        for chip in chips {
            if block.chips.len() >= 4 {
                dropped.push("more than 4 chips".into());
                break;
            }
            if let Some(name) = take_string(chip, 60, &mut dropped, "chip") {
                block.chips.push(name);
            }
        }
    }
    if let Some(risk) = object.get("risk").and_then(Value::as_array) {
        for marker in risk.iter().filter_map(Value::as_str) {
            if RISK_MARKERS.contains(&marker) && !block.risk.iter().any(|m| m == marker) {
                block.risk.push(marker.to_owned());
            } else {
                dropped.push("unknown risk marker".into());
            }
        }
    }
    if let Some(quiz) = object.get("quiz").filter(|q| !q.is_null()) {
        let question = quiz.get("question").and_then(|v| plain(v, 200));
        let options: Option<Vec<String>> = quiz.get("options").and_then(Value::as_array).and_then(|o| o.iter().map(|v| plain(v, 120)).collect());
        let expected = quiz.get("expected").and_then(Value::as_u64);
        match (question, options, expected) {
            (Some(question), Some(options), Some(expected)) if (2..=4).contains(&options.len()) && (expected as usize) < options.len() => {
                block.quiz = Some(Quiz { question, options, expected: expected as usize });
            }
            _ => dropped.push("quiz outside its limits".into()),
        }
    }
    if let Some(sources) = object.get("sources").and_then(Value::as_array) {
        for source in sources {
            if block.sources.len() >= 3 {
                dropped.push("more than 3 sources".into());
                break;
            }
            let url = source.get("url").and_then(Value::as_str).filter(|u| u.starts_with("https://") && u.len() <= 300 && !u.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, '"' | '<' | '>')));
            let title = source.get("title").and_then(|v| plain(v, 100));
            let read = source.get("read").and_then(Value::as_bool);
            match (url, title, read) {
                (Some(url), Some(title), Some(read)) => block.sources.push(Source { url: url.to_owned(), title, read }),
                _ => dropped.push("source outside its limits".into()),
            }
        }
    }
    ParsedCard { prose, kind, block, dropped, block_found: true }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(block: &str) -> String {
        format!("## What happened\n\nThe worker edited `src/lib.rs`.\n\n```shinatama-card\n{block}\n```\n")
    }

    #[test]
    fn a_valid_block_is_parsed_and_removed_from_the_prose() {
        let parsed = parse(&answer(r#"{"kind":"learning","taught":[{"name":"Borrow checker","theme":"Rust","keywords":["ownership"],"level":1}],"chips":["lifetimes"],"risk":["worktree_only"],"quiz":{"question":"Who owns x?","options":["a","b"],"expected":1},"sources":[{"url":"https://doc.rust-lang.org/book/","title":"The Book","read":true}],"future":"ignored"}"#), None);
        assert!(parsed.block_found && parsed.dropped.is_empty(), "{:?}", parsed.dropped);
        assert!(!parsed.prose.contains("shinatama-card"));
        assert_eq!(parsed.block.taught[0].identity, "borrow checker");
        assert_eq!(parsed.block.quiz.as_ref().unwrap().expected, 1);
        assert_eq!(parsed.block.sources.len(), 1);
    }

    #[test]
    fn invalid_items_are_dropped_and_the_rest_kept() {
        let parsed = parse(&answer(r#"{"taught":[{"name":"<script>","level":1},{"name":"Ok","level":9},{"name":"Traits","level":2},{"name":"Extra","level":1}],"chips":["a","b","c","d","e"],"risk":["costs_money","launch_missiles"],"quiz":{"question":"q","options":["only"],"expected":3},"sources":[{"url":"http://x","title":"t","read":true},{"url":"https://ok.example","title":"ok","read":false}]}"#), None);
        assert_eq!(parsed.block.taught.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Traits", "Extra"]);
        assert_eq!(parsed.block.chips.len(), 4);
        assert_eq!(parsed.block.risk, ["costs_money"]);
        assert!(parsed.block.quiz.is_none());
        assert_eq!(parsed.block.sources.len(), 1);
        assert!(parsed.dropped.len() >= 6, "{:?}", parsed.dropped);
        assert!(parsed.prose.contains("The worker edited"), "prose is never lost");
    }

    #[test]
    fn malformed_oversized_or_misplaced_blocks_keep_the_prose() {
        let bad = parse(&answer("{not json"), Some(CardKind::AidedDecision));
        assert!(bad.block_found && bad.block == CardBlock::default() && bad.kind == CardKind::AidedDecision);
        assert!(bad.prose.contains("The worker edited"));
        let big = parse(&answer(&format!(r#"{{"chips":["{}"]}}"#, "x".repeat(5000))), None);
        assert!(big.dropped.iter().any(|d| d.contains("4 KB")) && big.block.chips.is_empty());
        let misplaced = parse("Before\n\n```shinatama-card\n{\"chips\":[\"a\"]}\n```\n\nAfter the block.", None);
        assert!(!misplaced.block_found && misplaced.prose.contains("```shinatama-card"), "an earlier fence stays code");
        let two = parse("```shinatama-card\n{\"chips\":[\"first\"]}\n```\ntext\n```shinatama-card\n{\"chips\":[\"second\"]}\n```", None);
        assert_eq!(two.block.chips, ["second"]);
        assert!(two.prose.contains("\"first\""));
        let none = parse("Just prose.", Some(CardKind::Learning));
        assert!(!none.block_found && none.block.chips.is_empty());
    }

    #[test]
    fn requested_kind_wins_and_aided_decision_teaches_nothing() {
        let parsed = parse(&answer(r#"{"kind":"learning","taught":[{"name":"Ports","level":1}],"chips":["ports"]}"#), Some(CardKind::AidedDecision));
        assert_eq!(parsed.kind, CardKind::AidedDecision);
        assert!(parsed.block.taught.is_empty());
        assert_eq!(parsed.block.chips, ["ports"]);
        let declared = parse(&answer(r#"{"kind":"aided_decision"}"#), None);
        assert_eq!(declared.kind, CardKind::AidedDecision);
    }
}
