//! C30.1: the ordered-pair support matrix (FRD-0005 R14-R15, R18; FRD-0006 R41-R42).
//!
//! A pair is offered only when the source has a qualified session reader and the destination
//! has a seed channel and a qualified launch in this build. Everything else is refused with
//! its reason; Gemini is never part of the matrix. Same-agent pairs are native resume, not hops.
use crate::channel::channel_for;
use serde::Serialize;

pub const AGENTS: [&str; 6] = [
    "claude_code",
    "codex",
    "opencode",
    "pi",
    "hermes",
    "deepseek",
];

/// Sources whose sessions the contained reader can project (C22/C23 evidence).
pub const QUALIFIED_READERS: [&str; 3] = ["claude_code", "codex", "pi"];
/// Destinations the desktop opens through a qualified Open path (C15-C18 evidence).
pub const QUALIFIED_LAUNCHERS: [&str; 4] = ["claude_code", "codex", "opencode", "pi"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "support")]
pub enum PairSupport {
    SeedPrompt,
    NativeResume,
    Refuse { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Pair {
    pub source: &'static str,
    pub destination: &'static str,
    pub support: PairSupport,
}

fn known(agent: &str) -> bool {
    AGENTS.contains(&agent)
}

/// `degraded` lists agents whose installed version failed its fixtures (R18).
pub fn support(source: &str, destination: &str, degraded: &[&str]) -> PairSupport {
    let refuse = |reason: String| PairSupport::Refuse { reason };
    if !known(source) || !known(destination) {
        return refuse(format!(
            "{} is not one of the supported agents.",
            if known(source) { destination } else { source }
        ));
    }
    if source == destination {
        return PairSupport::NativeResume;
    }
    if !QUALIFIED_READERS.contains(&source) {
        return refuse(match source {
            "opencode" => "OpenCode sessions need its own exporter, which is not qualified yet, so they cannot be a hop source.".into(),
            _ => format!("{source} cannot be a hop source until a verified session reader exists."),
        });
    }
    if degraded.contains(&source) {
        return refuse(format!(
            "{source}'s installed version failed its fixtures; its sessions are not read for hops."
        ));
    }
    if channel_for(destination).is_none() || !QUALIFIED_LAUNCHERS.contains(&destination) {
        return refuse(format!(
            "{destination} has no qualified launch in this build yet."
        ));
    }
    // A degraded destination still receives a seed: seeds use only its ordinary input.
    PairSupport::SeedPrompt
}

pub fn matrix(degraded: &[&str]) -> Vec<Pair> {
    let mut pairs = Vec::with_capacity(AGENTS.len() * AGENTS.len());
    for source in AGENTS {
        for destination in AGENTS {
            pairs.push(Pair {
                source,
                destination,
                support: support(source, destination, degraded),
            });
        }
    }
    pairs
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::history::tests::{item, tool};
    use crate::preview::{PreviewInput, prepare};
    use crate::seed::HeaderFacts;
    use shinatama_export::{ItemKind, Redactions};

    #[test]
    fn matrix_covers_every_ordered_pair_and_excludes_gemini() {
        let pairs = matrix(&[]);
        assert_eq!(pairs.len(), 36);
        assert!(
            pairs
                .iter()
                .all(|p| p.source != "gemini" && p.destination != "gemini")
        );
        assert!(matches!(
            support("gemini", "codex", &[]),
            PairSupport::Refuse { .. }
        ));
        let offered: Vec<_> = pairs
            .iter()
            .filter(|p| p.support == PairSupport::SeedPrompt)
            .map(|p| (p.source, p.destination))
            .collect();
        assert_eq!(
            offered,
            [
                ("claude_code", "codex"),
                ("claude_code", "opencode"),
                ("claude_code", "pi"),
                ("codex", "claude_code"),
                ("codex", "opencode"),
                ("codex", "pi"),
                ("pi", "claude_code"),
                ("pi", "codex"),
                ("pi", "opencode"),
            ]
        );
        for source in ["hermes", "deepseek", "opencode"] {
            assert!(
                pairs
                    .iter()
                    .filter(|p| p.source == source && p.destination != source)
                    .all(|p| matches!(p.support, PairSupport::Refuse { .. })),
                "{source}"
            );
        }
        assert_eq!(
            pairs
                .iter()
                .filter(|p| p.support == PairSupport::NativeResume)
                .count(),
            6
        );
    }

    #[test]
    fn a_degraded_source_is_refused_with_its_reason() {
        let PairSupport::Refuse { reason } = support("codex", "claude_code", &["codex"]) else {
            panic!()
        };
        assert!(reason.contains("failed its fixtures"));
        assert_eq!(
            support("claude_code", "codex", &["codex"]),
            PairSupport::SeedPrompt
        );
    }

    /// Synthetic history-only canary for every offered pair: the fact exists only in the source
    /// history, survives into the seed, and secrets and reasoning never do.
    #[test]
    fn every_offered_pair_carries_a_history_only_canary() {
        let base =
            std::env::temp_dir().join(format!("shinatama-pairs-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&base).unwrap();
        for pair in matrix(&[])
            .into_iter()
            .filter(|p| p.support == PairSupport::SeedPrompt)
        {
            let canary = format!("CANARY-{}-{}", pair.source, pair.destination).to_uppercase();
            let items = [
                item(
                    ItemKind::UserText,
                    &format!(
                        "remember the code word {canary}; token ghp_0123456789abcdefABCDEF0123456789abcd"
                    ),
                ),
                item(ItemKind::AssistantText, "Noted."),
                tool(
                    ItemKind::ToolCall,
                    r#"{"command":"cargo build"}"#,
                    "shell",
                    "c1",
                    false,
                ),
                tool(ItemKind::ToolResult, "ok", "shell", "c1", false),
                item(ItemKind::UserText, "now continue with the release notes"),
            ];
            let preview = prepare(PreviewInput {
                items: &items,
                facts: HeaderFacts {
                    source_agent: pair.source.into(),
                    source_session: "s".into(),
                    destination_agent: pair.destination.into(),
                    time: "t".into(),
                    ..Default::default()
                },
                window_tokens: None,
                export_redactions: &Redactions::new(),
                project: None,
                data_dir: &base,
            })
            .unwrap();
            assert!(preview.seed.contains(&canary), "{pair:?}");
            assert!(!preview.seed.contains("ghp_0123"), "{pair:?}");
            assert!(
                preview.seed.contains("[tool] shell cargo build -> ok"),
                "{pair:?}"
            );
            assert!(
                preview.lost.iter().any(|l| l.contains("hidden reasoning")),
                "never lossless: {pair:?}"
            );
        }
        std::fs::remove_dir_all(base).unwrap();
    }
}
