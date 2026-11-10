//! Per-session token ledger (FRD-0013 R05, R35, R38).
//!
//! The ledger is built from the usage records the agent itself wrote into its session (Claude
//! `message.usage`, Codex `token_count` events, Pi message usage): input tokens per turn, cache
//! reads and writes, output, the cache hit rate, compactions and tool re-runs after a
//! compaction. It is the baseline every later technique is measured against; it states no
//! saving, and no claim may be made until ten real sessions per harness are recorded.
pub mod decision;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

pub const SESSIONS_BEFORE_CLAIMS: usize = 10;
pub const MAX_TURN_ROWS: usize = 120;

/// One model request, normalized: `input` never includes cached tokens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub output: u64,
}
impl Usage {
    pub fn add(&mut self, other: Usage) {
        self.input += other.input;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
        self.output += other.output;
    }
    pub fn prompt(&self) -> u64 {
        self.input + self.cache_read + self.cache_write
    }
    /// Cache reads over all prompt tokens, in per-mille; `None` without prompt tokens.
    pub fn hit_rate_permille(&self) -> Option<u64> {
        let prompt = self.prompt();
        (prompt > 0).then(|| self.cache_read * 1000 / prompt)
    }
}

fn number(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|k| object.get(*k).and_then(Value::as_u64))
}

/// Usage from one usage-shaped object, with the provider semantics detected from its keys.
pub fn usage_from_object(object: &serde_json::Map<String, Value>) -> Option<Usage> {
    // OpenAI / Codex: input_tokens includes cached_input_tokens.
    if let Some(cached) = number(object, &["cached_input_tokens"]) {
        let input = number(object, &["input_tokens"])?;
        return Some(Usage {
            input: input.saturating_sub(cached),
            cache_read: cached,
            cache_write: number(object, &["cache_write_input_tokens"]).unwrap_or(0),
            output: number(object, &["output_tokens"]).unwrap_or(0),
        });
    }
    // Anthropic: input_tokens excludes cache reads and writes.
    if object.contains_key("cache_read_input_tokens") || object.contains_key("cache_creation_input_tokens") || object.contains_key("input_tokens") {
        return Some(Usage {
            input: number(object, &["input_tokens"]).unwrap_or(0),
            cache_read: number(object, &["cache_read_input_tokens"]).unwrap_or(0),
            cache_write: number(object, &["cache_creation_input_tokens"]).unwrap_or(0),
            output: number(object, &["output_tokens"]).unwrap_or(0),
        });
    }
    // Pi-style camelCase.
    if object.contains_key("input") && (object.contains_key("output") || object.contains_key("cacheRead")) {
        return Some(Usage {
            input: number(object, &["input"]).unwrap_or(0),
            cache_read: number(object, &["cacheRead"]).unwrap_or(0),
            cache_write: number(object, &["cacheWrite"]).unwrap_or(0),
            output: number(object, &["output"]).unwrap_or(0),
        });
    }
    None
}

/// Usage carried by one native record, with a key to drop duplicates (Claude repeats one
/// message's usage on every content-block record of that message).
pub fn usage_in_record(record: &Value) -> Option<(Option<String>, Usage)> {
    // Codex: {"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{...}}}}
    if let Some(info) = record.get("payload").filter(|p| p.get("type").and_then(Value::as_str) == Some("token_count")).and_then(|p| p.get("info")) {
        let last = info.get("last_token_usage")?.as_object()?;
        let total = info.get("total_token_usage").map(|t| t.to_string());
        return usage_from_object(last).map(|u| (total, u));
    }
    let message = record.get("message").unwrap_or(record);
    let usage = message.get("usage")?.as_object()?;
    let id = message.get("id").and_then(Value::as_str).map(str::to_owned);
    usage_from_object(usage).map(|u| (id, u))
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnUsage {
    pub turn: usize,
    pub requests: u64,
    pub usage: Usage,
    pub tool_calls: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLedger {
    pub turns: usize,
    pub requests: u64,
    pub total: Usage,
    pub hit_rate_permille: Option<u64>,
    pub compactions: u64,
    pub tool_calls: u64,
    /// Tool calls after a compaction that repeat a call made before it (same tool, same input).
    pub reruns_after_compaction: u64,
    /// Newest turns first are kept when there are more than [`MAX_TURN_ROWS`].
    pub per_turn: Vec<TurnUsage>,
    pub per_turn_truncated: bool,
    /// Records whose usage could not be read: counted, never guessed.
    pub records_without_usage: u64,
}

/// Events the session reader projects into, in native order.
#[derive(Clone, Debug, PartialEq)]
pub enum LedgerEvent<'a> {
    UserTurn,
    /// A model step with its native records.
    Step { records: &'a [Value] },
    ToolCall { name: &'a str, input: &'a Value },
    Compaction,
    /// Any other record (Codex token_count events arrive as metadata).
    Other { records: &'a [Value] },
}

pub fn build<'a>(events: impl IntoIterator<Item = LedgerEvent<'a>>) -> SessionLedger {
    let mut ledger = SessionLedger::default();
    let mut turns: Vec<TurnUsage> = Vec::new();
    let mut seen_usage: HashSet<String> = HashSet::new();
    let mut before: HashSet<String> = HashSet::new();
    let mut after: Vec<String> = Vec::new();
    let mut compacted = false;
    let mut user_turns = 0usize;
    let current = |turns: &mut Vec<TurnUsage>| -> usize {
        if turns.is_empty() {
            turns.push(TurnUsage { turn: 0, ..TurnUsage::default() });
        }
        turns.len() - 1
    };
    for event in events {
        match event {
            LedgerEvent::UserTurn => {
                user_turns += 1;
                turns.push(TurnUsage { turn: user_turns, ..TurnUsage::default() });
            }
            LedgerEvent::Step { records } | LedgerEvent::Other { records } => {
                let step = matches!(event, LedgerEvent::Step { .. });
                let mut found = false;
                for record in records {
                    if let Some((key, usage)) = usage_in_record(record) {
                        found = true;
                        if let Some(key) = key
                            && !seen_usage.insert(key)
                        {
                            continue;
                        }
                        let index = current(&mut turns);
                        turns[index].requests += 1;
                        turns[index].usage.add(usage);
                    }
                }
                if step && !found {
                    ledger.records_without_usage += 1;
                }
            }
            LedgerEvent::ToolCall { name, input } => {
                let index = current(&mut turns);
                turns[index].tool_calls += 1;
                ledger.tool_calls += 1;
                let key = format!("{name}\u{0}{input}");
                if compacted {
                    if before.contains(&key) {
                        ledger.reruns_after_compaction += 1;
                    }
                    after.push(key);
                } else {
                    before.insert(key);
                }
            }
            LedgerEvent::Compaction => {
                ledger.compactions += 1;
                compacted = true;
                before.extend(after.drain(..));
            }
        }
    }
    for turn in &turns {
        ledger.requests += turn.requests;
        ledger.total.add(turn.usage);
    }
    ledger.turns = user_turns;
    ledger.hit_rate_permille = ledger.total.hit_rate_permille();
    if turns.len() > MAX_TURN_ROWS {
        ledger.per_turn_truncated = true;
        turns.drain(..turns.len() - MAX_TURN_ROWS);
    }
    ledger.per_turn = turns;
    ledger
}

/// R05/R38: whether any saving may be stated for a harness, given its measured sessions.
pub fn may_claim(measured_sessions: usize) -> bool {
    measured_sessions >= SESSIONS_BEFORE_CLAIMS
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_anthropic_openai_and_pi_usage() {
        let claude = json!({"message":{"id":"m1","usage":{"input_tokens":2,"cache_creation_input_tokens":100,"cache_read_input_tokens":300,"output_tokens":7}}});
        assert_eq!(usage_in_record(&claude).unwrap(), (Some("m1".into()), Usage { input: 2, cache_read: 300, cache_write: 100, output: 7 }));
        let codex = json!({"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1},"last_token_usage":{"input_tokens":36429,"cached_input_tokens":13440,"cache_write_input_tokens":0,"output_tokens":206}}}});
        assert_eq!(usage_in_record(&codex).unwrap().1, Usage { input: 22989, cache_read: 13440, cache_write: 0, output: 206 });
        let pi = json!({"message":{"usage":{"input":10,"output":3,"cacheRead":5,"cacheWrite":1}}});
        assert_eq!(usage_in_record(&pi).unwrap().1, Usage { input: 10, cache_read: 5, cache_write: 1, output: 3 });
        assert!(usage_in_record(&json!({"message":{"role":"user"}})).is_none());
        assert!(usage_in_record(&json!({"payload":{"type":"token_count","info":null}})).is_none());
    }

    #[test]
    fn builds_per_turn_totals_with_duplicates_dropped() {
        let a = [json!({"message":{"id":"m1","usage":{"input_tokens":10,"cache_read_input_tokens":90,"output_tokens":5}}}), json!({"message":{"id":"m1","usage":{"input_tokens":10,"cache_read_input_tokens":90,"output_tokens":5}}})];
        let b = [json!({"message":{"id":"m2","usage":{"input_tokens":20,"cache_read_input_tokens":80,"output_tokens":1}}})];
        let none = [json!({"message":{"role":"assistant"}})];
        let ledger = build([LedgerEvent::UserTurn, LedgerEvent::Step { records: &a }, LedgerEvent::UserTurn, LedgerEvent::Step { records: &b }, LedgerEvent::Step { records: &none }]);
        assert_eq!(ledger.turns, 2);
        assert_eq!(ledger.requests, 2, "the repeated message id counts once");
        assert_eq!(ledger.total, Usage { input: 30, cache_read: 170, cache_write: 0, output: 6 });
        assert_eq!(ledger.hit_rate_permille, Some(850));
        assert_eq!((ledger.per_turn[0].turn, ledger.per_turn[0].usage.input), (1, 10));
        assert_eq!((ledger.per_turn[1].turn, ledger.per_turn[1].usage.input), (2, 20));
        assert_eq!(ledger.records_without_usage, 1);
    }

    #[test]
    fn counts_compactions_and_tool_reruns_after_them() {
        let read = json!({"path":"a.rs"});
        let other = json!({"path":"b.rs"});
        let ledger = build([
            LedgerEvent::UserTurn,
            LedgerEvent::ToolCall { name: "Read", input: &read },
            LedgerEvent::Compaction,
            LedgerEvent::ToolCall { name: "Read", input: &read },
            LedgerEvent::ToolCall { name: "Read", input: &other },
            LedgerEvent::Compaction,
            LedgerEvent::ToolCall { name: "Read", input: &other },
        ]);
        assert_eq!(ledger.compactions, 2);
        assert_eq!(ledger.tool_calls, 4);
        assert_eq!(ledger.reruns_after_compaction, 2);
    }

    #[test]
    fn no_claim_before_ten_sessions_and_rows_are_bounded() {
        assert!(!may_claim(9) && may_claim(10));
        let events: Vec<_> = (0..200).map(|_| LedgerEvent::UserTurn).collect();
        let ledger = build(events);
        assert_eq!(ledger.per_turn.len(), MAX_TURN_ROWS);
        assert!(ledger.per_turn_truncated);
        assert_eq!(ledger.per_turn.last().unwrap().turn, 200);
    }
}
