//! F21: decision compaction with rules only (FRD-0013 R13-R15, R19).
//!
//! Every `tool_use` is paired with its `tool_result`. Calls in the first message and in the
//! newest `preserve_recent` messages are pinned. For every other pair, deterministic features
//! decide where they are conclusive: the result was cited later, the file was edited after
//! being read, the same call was repeated later, an error was later resolved, and a per-tool
//! prior. Pairs the rules cannot decide are left to a scorer (julia1, out of this crate) and
//! default to Keep here. A result is never left without its call, user and assistant text is
//! never a candidate, and text that states a constraint always survives. When the plan removes
//! less than `min_reduction` of the candidate bytes, it reports that the structured summary
//! fallback is needed instead.
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    User { text: String },
    Assistant { text: String, calls: Vec<Call> },
    Results { results: Vec<CallResult> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Call {
    pub id: String,
    pub tool: String,
    /// The call's main target: a path, a command, a pattern.
    pub target: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallResult {
    pub call_id: String,
    pub text: String,
    pub is_error: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Keep,
    Truncate,
    Remove,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Pinned,
    Rule,
    /// Left for the scorer; Keep until it answers.
    Undecided,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Decision {
    pub call_id: String,
    pub outcome: Outcome,
    pub stage: Stage,
    pub reason: &'static str,
    pub result_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub preserve_recent: usize,
    pub truncate_head_bytes: usize,
    pub min_reduction: f64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            preserve_recent: 6,
            truncate_head_bytes: 400,
            min_reduction: 0.25,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Plan {
    pub decisions: Vec<Decision>,
    pub candidate_bytes: usize,
    pub removed_bytes: usize,
    /// Below `min_reduction`: use the structured summary fallback instead.
    pub needs_summary_fallback: bool,
}

const CONSTRAINT_WORDS: [&str; 10] = [
    "must",
    "never",
    "always",
    "do not",
    "don't",
    "required",
    "constraint",
    "only use",
    "make sure",
    "non ",
];

/// A result that states a constraint or holds the exact error the next turn needs.
fn protected(text: &str) -> bool {
    let lower = text.to_lowercase();
    CONSTRAINT_WORDS.iter().any(|w| lower.contains(w)) && text.len() < 4_000
}

fn reads(tool: &str) -> bool {
    matches!(
        tool.to_ascii_lowercase().as_str(),
        "read" | "view" | "cat" | "read_file" | "open"
    )
}
fn edits(tool: &str) -> bool {
    matches!(
        tool.to_ascii_lowercase().as_str(),
        "edit" | "write" | "multiedit" | "apply_patch" | "str_replace" | "create"
    )
}
/// Tools whose output is usually throwaway once acted on.
fn low_prior(tool: &str) -> bool {
    matches!(
        tool.to_ascii_lowercase().as_str(),
        "ls" | "glob" | "list" | "find" | "tree"
    )
}

pub fn plan(messages: &[Message], settings: &Settings) -> Plan {
    let mut calls: Vec<(usize, &Call)> = Vec::new();
    let mut results: HashMap<&str, &CallResult> = HashMap::new();
    for (index, message) in messages.iter().enumerate() {
        match message {
            Message::Assistant { calls: c, .. } => calls.extend(c.iter().map(|call| (index, call))),
            Message::Results { results: r } => {
                for result in r {
                    results.insert(&result.call_id, result);
                }
            }
            Message::User { .. } => {}
        }
    }
    let first_assistant = messages
        .iter()
        .position(|m| matches!(m, Message::Assistant { .. }));
    let pinned_from = messages.len().saturating_sub(settings.preserve_recent);
    let later_text = |after: usize| -> String {
        messages
            .iter()
            .skip(after + 1)
            .filter_map(|m| match m {
                Message::User { text } | Message::Assistant { text, .. } => Some(text.as_str()),
                Message::Results { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut decisions = Vec::new();
    let mut candidate_bytes = 0;
    let mut removed_bytes = 0;
    let mut seen_later: HashSet<(String, String)> = HashSet::new();
    // Walk newest first so "repeated later" is known when an older copy is reached.
    for (index, call) in calls.iter().rev() {
        let Some(result) = results.get(call.id.as_str()) else {
            continue;
        }; // unpaired: never touched
        let bytes = result.text.len();
        let key = (call.tool.to_ascii_lowercase(), call.target.clone());
        let repeated_later = !seen_later.insert(key);
        let (outcome, stage, reason) = if Some(*index) == first_assistant {
            (Outcome::Keep, Stage::Pinned, "first message")
        } else if *index >= pinned_from {
            (Outcome::Keep, Stage::Pinned, "recent messages")
        } else if protected(&result.text) {
            (Outcome::Keep, Stage::Rule, "states a constraint")
        } else {
            let later = later_text(*index);
            let cited = !call.target.is_empty()
                && call.target.len() >= 3
                && later.contains(call.target.as_str());
            let edited_after = reads(&call.tool)
                && calls.iter().any(|(i, other)| {
                    i > index && edits(&other.tool) && other.target == call.target
                });
            let error_resolved = result.is_error
                && calls.iter().any(|(i, other)| {
                    i > index
                        && other.tool == call.tool
                        && other.target == call.target
                        && results.get(other.id.as_str()).is_some_and(|r| !r.is_error)
                });
            if repeated_later {
                (Outcome::Remove, Stage::Rule, "same call repeated later")
            } else if edited_after {
                (Outcome::Remove, Stage::Rule, "file edited after this read")
            } else if error_resolved {
                (Outcome::Truncate, Stage::Rule, "error resolved later")
            } else if cited {
                (Outcome::Keep, Stage::Rule, "cited later")
            } else if low_prior(&call.tool) {
                (Outcome::Truncate, Stage::Rule, "listing tool prior")
            } else {
                (Outcome::Keep, Stage::Undecided, "left for the scorer")
            }
        };
        if stage != Stage::Pinned {
            candidate_bytes += bytes;
        }
        removed_bytes += match outcome {
            Outcome::Remove => bytes,
            Outcome::Truncate => bytes.saturating_sub(settings.truncate_head_bytes),
            Outcome::Keep => 0,
        };
        decisions.push(Decision {
            call_id: call.id.clone(),
            outcome,
            stage,
            reason,
            result_bytes: bytes,
        });
    }
    decisions.reverse();
    let needs_summary_fallback = candidate_bytes == 0
        || (removed_bytes as f64) < settings.min_reduction * candidate_bytes as f64;
    Plan {
        decisions,
        candidate_bytes,
        removed_bytes,
        needs_summary_fallback,
    }
}

/// Applies a plan. Removed pairs lose call and result together; truncated results keep their
/// head and a note naming the dropped bytes. User and assistant text is never changed.
pub fn apply(messages: &[Message], plan: &Plan, settings: &Settings) -> Vec<Message> {
    let by_id: HashMap<&str, Outcome> = plan
        .decisions
        .iter()
        .map(|d| (d.call_id.as_str(), d.outcome))
        .collect();
    let removed = |id: &str| by_id.get(id) == Some(&Outcome::Remove);
    messages
        .iter()
        .filter_map(|message| match message {
            Message::User { .. } => Some(message.clone()),
            Message::Assistant { text, calls } => Some(Message::Assistant {
                text: text.clone(),
                calls: calls.iter().filter(|c| !removed(&c.id)).cloned().collect(),
            }),
            Message::Results { results } => {
                let kept: Vec<CallResult> = results
                    .iter()
                    .filter(|r| !removed(&r.call_id))
                    .map(|r| match by_id.get(r.call_id.as_str()) {
                        Some(Outcome::Truncate) if r.text.len() > settings.truncate_head_bytes => {
                            let mut end = settings.truncate_head_bytes;
                            while !r.text.is_char_boundary(end) {
                                end -= 1;
                            }
                            CallResult {
                                text: format!(
                                    "{}\n[shinatama: {} bytes dropped; re-run the tool if needed]",
                                    &r.text[..end],
                                    r.text.len() - end
                                ),
                                ..r.clone()
                            }
                        }
                        _ => r.clone(),
                    })
                    .collect();
                (!kept.is_empty()).then_some(Message::Results { results: kept })
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(id: &str, tool: &str, target: &str) -> Call {
        Call {
            id: id.into(),
            tool: tool.into(),
            target: target.into(),
        }
    }
    fn result(id: &str, text: &str, error: bool) -> CallResult {
        CallResult {
            call_id: id.into(),
            text: text.into(),
            is_error: error,
        }
    }
    fn step(text: &str, calls: Vec<Call>) -> Message {
        Message::Assistant {
            text: text.into(),
            calls,
        }
    }
    fn res(results: Vec<CallResult>) -> Message {
        Message::Results { results }
    }

    fn conversation() -> Vec<Message> {
        let big = "x".repeat(5_000);
        vec![
            Message::User {
                text: "fix the bug, you must not touch the public API".into(),
            },
            step("start", vec![call("c0", "Read", "src/api.rs")]),
            res(vec![result("c0", &big, false)]),
            step("", vec![call("c1", "Read", "src/lib.rs")]),
            res(vec![result("c1", &big, false)]),
            step("", vec![call("c2", "Bash", "cargo test")]),
            res(vec![result("c2", &format!("error[E0308] {big}"), true)]),
            step("", vec![call("c3", "Edit", "src/lib.rs")]),
            res(vec![result("c3", "ok", false)]),
            step("", vec![call("c4", "Glob", "**/*.rs")]),
            res(vec![result("c4", &big, false)]),
            step("", vec![call("c5", "Grep", "fn parse")]),
            res(vec![result("c5", &big, false)]),
            step("", vec![call("c6", "Read", "Cargo.toml")]),
            res(vec![result(
                "c6",
                "You must keep edition 2024. Never change it.",
                false,
            )]),
            step("", vec![call("c7", "Bash", "cargo test")]),
            res(vec![result("c7", "test result: ok", false)]),
            step("", vec![call("c8", "Read", "src/main.rs")]),
            res(vec![result("c8", &big, false)]),
            Message::User {
                text: "and the parser in fn parse?".into(),
            },
            step("Looking at fn parse", vec![]),
            Message::User { text: "ok".into() },
            step("done", vec![]),
        ]
    }

    #[test]
    fn rules_decide_with_recorded_stages() {
        let plan = plan(&conversation(), &Settings::default());
        let get = |id: &str| {
            plan.decisions
                .iter()
                .find(|d| d.call_id == id)
                .unwrap()
                .clone()
        };
        assert_eq!(
            (get("c0").outcome, get("c0").stage),
            (Outcome::Keep, Stage::Pinned)
        );
        assert_eq!(
            (get("c1").outcome, get("c1").reason),
            (Outcome::Remove, "file edited after this read")
        );
        assert_eq!(
            (get("c2").outcome, get("c2").reason),
            (Outcome::Remove, "same call repeated later")
        );
        assert_eq!(
            (get("c4").outcome, get("c4").reason),
            (Outcome::Truncate, "listing tool prior")
        );
        assert_eq!(
            (get("c5").outcome, get("c5").reason),
            (Outcome::Keep, "cited later")
        );
        assert_eq!(
            (get("c6").outcome, get("c6").reason),
            (Outcome::Keep, "states a constraint")
        );
        assert_eq!(
            get("c8").stage,
            Stage::Pinned,
            "inside the newest six messages"
        );
        assert!(!plan.needs_summary_fallback);
    }

    #[test]
    fn applying_keeps_pairs_together_and_text_untouched() {
        let messages = conversation();
        let settings = Settings::default();
        let plan = plan(&messages, &settings);
        let out = apply(&messages, &plan, &settings);
        let call_ids: HashSet<String> = out
            .iter()
            .flat_map(|m| match m {
                Message::Assistant { calls, .. } => calls.iter().map(|c| c.id.clone()).collect(),
                _ => vec![],
            })
            .collect();
        let result_ids: HashSet<String> = out
            .iter()
            .flat_map(|m| match m {
                Message::Results { results } => results.iter().map(|r| r.call_id.clone()).collect(),
                _ => vec![],
            })
            .collect();
        assert_eq!(
            call_ids, result_ids,
            "no result without its call and no call without its result"
        );
        assert!(!call_ids.contains("c1") && !call_ids.contains("c2"));
        let texts = |ms: &[Message]| {
            ms.iter()
                .filter_map(|m| match m {
                    Message::User { text } | Message::Assistant { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            texts(&out),
            texts(&messages),
            "user and assistant text is never a candidate"
        );
        let glob = out
            .iter()
            .find_map(|m| match m {
                Message::Results { results } => results.iter().find(|r| r.call_id == "c4").cloned(),
                _ => None,
            })
            .unwrap();
        assert!(
            glob.text
                .contains("bytes dropped; re-run the tool if needed")
                && glob.text.len() < 600
        );
    }

    #[test]
    fn small_reductions_ask_for_the_structured_fallback() {
        let messages = vec![
            Message::User { text: "go".into() },
            step("", vec![call("a", "Read", "x.rs")]),
            res(vec![result("a", "short", false)]),
            step("", vec![call("b", "Bash", "make")]),
            res(vec![result("b", &"y".repeat(3_000), false)]),
            Message::User { text: "1".into() },
            Message::User { text: "2".into() },
            Message::User { text: "3".into() },
            Message::User { text: "4".into() },
            Message::User { text: "5".into() },
            Message::User { text: "6".into() },
        ];
        let plan = plan(&messages, &Settings::default());
        assert_eq!(
            plan.decisions
                .iter()
                .find(|d| d.call_id == "b")
                .unwrap()
                .stage,
            Stage::Undecided
        );
        assert!(
            plan.needs_summary_fallback,
            "nothing decisive: summary fallback"
        );
        let unpaired = vec![
            Message::User { text: "x".into() },
            step("", vec![call("z", "Read", "a")]),
        ];
        assert!(
            super::plan(&unpaired, &Settings::default())
                .decisions
                .is_empty(),
            "an unanswered call is never touched"
        );
    }
}
