//! C27.4: the immutable preview (FRD-0005 R06-R07, R27).
//!
//! The preview freezes the exact seed and bootstrap bytes with their SHA-256. Sending later
//! compares the bytes it is about to deliver with this snapshot and refuses any difference.
use crate::channel::{self, Delivery, SeedChannel};
use crate::files::{self, HandoffPaths};
use crate::history::{self, Dropped};
use crate::seed::{self, HeaderFacts, SeedRefusal, SeedReport};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shinatama_export::{ExportItem, Redactions};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preview {
    pub hop_id: String,
    pub nonce: String,
    pub destination: String,
    pub channel: SeedChannel,
    pub seed: String,
    pub seed_sha256: String,
    pub bootstrap: String,
    pub bootstrap_sha256: String,
    pub delivery: Delivery,
    pub seed_path: String,
    pub full_path: String,
    pub files_in_repository: bool,
    pub report: SeedReport,
    pub redactions: Redactions,
    pub nested_seeds_dropped: u64,
    pub injected_context_dropped: u64,
    pub carried: Vec<String>,
    pub lost: Vec<String>,
    /// What the seed carried, for catch-up (R47).
    pub source_cursor: crate::catchup::NativeCursor,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PreviewError {
    UnsupportedDestination,
    Refused(SeedRefusal),
    Files(String),
}
impl std::fmt::Display for PreviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreviewError::UnsupportedDestination => write!(f, "This destination has no seed channel."),
            PreviewError::Refused(reason) => write!(f, "{reason}"),
            PreviewError::Files(message) => write!(f, "The private hand-off files could not be written: {message}"),
        }
    }
}

pub struct PreviewInput<'a> {
    pub items: &'a [ExportItem],
    pub facts: HeaderFacts,
    pub window_tokens: Option<u64>,
    /// Counts from the export pass that produced `items`.
    pub export_redactions: &'a Redactions,
    pub project: Option<&'a Path>,
    pub data_dir: &'a Path,
}

pub fn sha256(bytes: &[u8]) -> String {
    shinatama_export::stx::hex(&Sha256::digest(bytes))
}

fn statements(report: &SeedReport, dropped: Dropped) -> (Vec<String>, Vec<String>) {
    let carried = vec![
        "The conversation text: user and assistant messages.".to_owned(),
        "Tool calls as one-line digests; outputs over 40 lines keep their first and last 15 lines.".to_owned(),
        "The same folder: files and Git state are unchanged.".to_owned(),
        "Attachments as paths and hashes.".to_owned(),
        "Project instruction files, named in the header.".to_owned(),
        "The whole transcript in a private file the destination can read on demand.".to_owned(),
    ];
    let mut lost = vec![
        "The live process and any command still running.".to_owned(),
        "Permissions granted in the source.".to_owned(),
        "Model state and hidden reasoning.".to_owned(),
    ];
    if report.omitted_turns > 0 {
        lost.push(format!("{} older turns are not in the seed (they stay in the full transcript).", report.omitted_turns));
    }
    if report.digest_turns > 0 {
        lost.push(format!("{} middle turns are reduced to one-line digests.", report.digest_turns));
    }
    if dropped.nested_seeds > 0 {
        lost.push(format!("{} earlier hand-off seeds are not repeated.", dropped.nested_seeds));
    }
    (carried, lost)
}

/// Builds the preview and writes the private hand-off files. Nothing is sent.
pub fn prepare(input: PreviewInput<'_>) -> Result<Preview, PreviewError> {
    let destination = input.facts.destination_agent.clone();
    let channel = channel::channel_for(&destination).ok_or(PreviewError::UnsupportedDestination)?;
    let hop_id = uuid::Uuid::new_v4().simple().to_string();
    let nonce = uuid::Uuid::new_v4().simple().to_string()[..12].to_owned();
    let paths: HandoffPaths = files::handoff_paths(input.project, input.data_dir, &hop_id);
    let mut facts = input.facts;
    facts.full_transcript = Some(paths.full.display().to_string());
    if facts.destination_instruction_file.is_none() {
        facts.destination_instruction_file = channel::instruction_file_for(&destination).map(str::to_owned);
    }
    let history = history::normalize(input.items);
    let seed = seed::render(&history, &facts, &nonce, input.window_tokens).map_err(PreviewError::Refused)?;
    let full = seed::render_full(&history, &facts, &nonce);
    let bootstrap = channel::bootstrap(&nonce, &paths.seed, history.last_user());
    files::write_handoff(&paths, &seed.text, &full).map_err(|e| PreviewError::Files(e.to_string()))?;
    let mut redactions = input.export_redactions.clone();
    for (kind, count) in &history.redactions {
        *redactions.entry(kind.clone()).or_insert(0) += count;
    }
    let (carried, lost) = statements(&seed.report, history.dropped);
    Ok(Preview {
        delivery: channel::delivery(channel, &paths.seed, &seed.text, &bootstrap),
        seed_sha256: sha256(seed.text.as_bytes()),
        bootstrap_sha256: sha256(bootstrap.as_bytes()),
        hop_id,
        nonce,
        destination,
        channel,
        seed: seed.text,
        bootstrap,
        seed_path: paths.seed.display().to_string(),
        full_path: paths.full.display().to_string(),
        files_in_repository: paths.in_repository,
        report: seed.report,
        redactions,
        nested_seeds_dropped: history.dropped.nested_seeds,
        injected_context_dropped: history.dropped.injected_context,
        carried,
        lost,
        source_cursor: crate::catchup::NativeCursor::at_end(input.items),
    })
}

/// The bytes about to be delivered must be exactly the previewed ones, and the seed file on disk
/// must still hold the previewed seed.
pub fn verify_send(preview: &Preview, bootstrap: &[u8]) -> bool {
    sha256(bootstrap) == preview.bootstrap_sha256
        && std::fs::read(&preview.seed_path).is_ok_and(|bytes| sha256(&bytes) == preview.seed_sha256)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::history::tests::item;
    use shinatama_export::ItemKind;

    #[test]
    fn preview_bytes_match_the_files_and_tampering_is_detected() {
        let base = std::env::temp_dir().join(format!("shinatama-preview-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(base.join("data")).unwrap();
        let items = [
            item(ItemKind::UserText, "first task with key ghp_0123456789abcdefABCDEF0123456789abcd"),
            item(ItemKind::AssistantText, "Done."),
            item(ItemKind::UserText, "next: /compact then continue"),
        ];
        let mut export = Redactions::new();
        export.insert("api_key".into(), 2);
        let preview = prepare(PreviewInput {
            items: &items,
            facts: HeaderFacts { source_agent: "claude_code".into(), source_session: "s1".into(), destination_agent: "codex".into(), time: "t".into(), ..Default::default() },
            window_tokens: None,
            export_redactions: &export,
            project: None,
            data_dir: &base.join("data"),
        })
        .unwrap();
        assert_eq!(preview.channel, SeedChannel::ArgvPrompt);
        assert_eq!(std::fs::read_to_string(&preview.seed_path).unwrap(), preview.seed);
        assert!(preview.seed.contains(&preview.full_path));
        assert!(!preview.seed.contains("ghp_0123"));
        assert_eq!(preview.redactions.get("github_token"), Some(&1));
        assert_eq!(preview.redactions.get("api_key"), Some(&2));
        assert!(verify_send(&preview, preview.bootstrap.as_bytes()));
        assert!(!verify_send(&preview, b"something else"));
        std::fs::OpenOptions::new().append(true).open(&preview.seed_path).map(|mut f| std::io::Write::write_all(&mut f, b"x")).unwrap().unwrap();
        assert!(!verify_send(&preview, preview.bootstrap.as_bytes()), "a changed seed file is refused");
        assert!(prepare(PreviewInput {
            items: &items,
            facts: HeaderFacts { destination_agent: "gemini".into(), ..Default::default() },
            window_tokens: None,
            export_redactions: &export,
            project: None,
            data_dir: &base.join("data"),
        })
        .is_err());
        std::fs::remove_dir_all(base).unwrap();
    }
}
