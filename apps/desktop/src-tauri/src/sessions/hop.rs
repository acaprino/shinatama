//! C27.4 hop preview: the contained worker exports the session as redacted STX, the hop crate
//! builds the budgeted seed and writes the private hand-off files, and the exact bytes are kept
//! here until the hop coordinator sends them. Nothing reaches the destination from this command.
use super::Controller;
use serde::{Deserialize, Serialize};
use shinatama_engine::session_detail::{DetailCursor, DetailRequest};
use shinatama_engine::session_export::ExportRequest;
use shinatama_engine::session_roots::{RootSelector, SessionReadIssue};
use shinatama_export::{Format, Mode, stx};
use shinatama_hop::preview::{self, Preview, PreviewInput};
use shinatama_hop::{HeaderFacts, git};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{State, WebviewWindow};

const DESTINATIONS: [&str; 6] = ["claude_code", "codex", "opencode", "pi", "hermes", "deepseek"];
const KEPT_PREVIEWS: usize = 8;
const INSTRUCTION_FILES: [&str; 4] = ["CLAUDE.md", "AGENTS.md", "GEMINI.md", ".github/copilot-instructions.md"];

/// A frozen preview with the source facts the hop needs; held only in this process.
#[derive(Clone)]
pub struct KeptPreview {
    pub preview: Preview,
    pub source_agent: String,
    pub source_native_id: String,
    pub source_root: RootSelector,
    pub cwd: Option<PathBuf>,
}

fn previews() -> &'static Mutex<VecDeque<KeptPreview>> {
    static PREVIEWS: OnceLock<Mutex<VecDeque<KeptPreview>>> = OnceLock::new();
    PREVIEWS.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// The frozen preview the hop must send byte for byte.
pub fn kept_preview(hop_id: &str) -> Option<KeptPreview> {
    previews().lock().ok()?.iter().find(|p| p.preview.hop_id == hop_id).cloned()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HopPreviewRequest {
    root: RootSelector,
    native_id: String,
    fingerprint: String,
    destination: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum HopPreviewOutcome {
    Ready { preview: Box<Preview> },
    Refused { issue: String, message: String },
}

fn refused(issue: &str, message: impl Into<String>) -> HopPreviewOutcome {
    HopPreviewOutcome::Refused { issue: issue.into(), message: message.into() }
}

#[tauri::command]
pub async fn hop_preview(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: HopPreviewRequest,
) -> Result<HopPreviewOutcome, SessionReadIssue> {
    super::local(&window)?;
    if request.fingerprint.len() != 64 || !request.fingerprint.bytes().all(|c| c.is_ascii_hexdigit()) || !DESTINATIONS.contains(&request.destination.as_str()) {
        return Err(SessionReadIssue::InvalidRequest);
    }
    let source_root = request.root.clone();
    let reply = match controller
        .inner()
        .export(ExportRequest {
            detail: DetailRequest {
                root: request.root,
                native_id: request.native_id,
                cursor: Some(DetailCursor { offset: 0, fingerprint: request.fingerprint }),
                limit: 1,
            },
            format: Format::Stx,
            mode: Mode::Full,
            with_attachments: false,
            thread: None,
            home: None,
        })
        .await
    {
        Ok(reply) => reply,
        Err(issue) => return Ok(refused(&format!("{issue:?}").to_lowercase(), "The session could not be read; nothing was prepared.")),
    };
    let Some(payload) = reply.payload else {
        return Ok(refused("read_unavailable", "The session could not be read; nothing was prepared."));
    };
    let data_dir: PathBuf = controller
        .services
        .load()
        .map_err(|_| SessionReadIssue::ReadUnavailable)?
        .location
        .directory()
        .to_path_buf();
    let destination = request.destination;
    tauri::async_runtime::spawn_blocking(move || {
        let document = match stx::read(payload.content.as_bytes()) {
            Ok(document) => document,
            Err(error) => return Ok(refused("read_unavailable", error.to_string())),
        };
        let items: Vec<_> = document.items().cloned().collect();
        let source = &document.header.source;
        if source.agent == destination {
            return Ok(refused("same_agent", "Resuming in the same agent is native; use Resume instead of a hop."));
        }
        let project = source.cwd.as_deref().map(Path::new).filter(|p| p.is_dir());
        let (source_agent, source_native_id) = (source.agent.clone(), source.native_session_id.clone());
        let cwd = project.map(Path::to_path_buf);
        let facts = project.map(git::facts).unwrap_or_default();
        let instruction_files = project
            .map(|p| INSTRUCTION_FILES.iter().filter(|f| p.join(f).is_file()).map(|f| (*f).to_owned()).collect())
            .unwrap_or_default();
        let header = HeaderFacts {
            thread: document.header.thread.clone(),
            source_agent: source.agent.clone(),
            source_session: source.native_session_id.clone(),
            destination_agent: destination.clone(),
            time: shinatama_export::time::now_rfc3339(),
            cwd: source.cwd.clone(),
            branch: facts.branch.or_else(|| source.git_branch.clone()),
            worktree: facts.worktree,
            uncommitted: facts.uncommitted,
            untracked: facts.untracked,
            changed_paths: facts.changed_paths,
            instruction_files,
            destination_instruction_file: None,
            full_transcript: None,
        };
        // No provider catalog is qualified yet: the unknown-window cap applies (R22).
        match preview::prepare(PreviewInput {
            items: &items,
            facts: header,
            window_tokens: None,
            export_redactions: &document.header.redactions,
            project,
            data_dir: &data_dir,
        }) {
            Ok(value) => {
                if let Ok(mut kept) = previews().lock() {
                    if kept.len() >= KEPT_PREVIEWS {
                        kept.pop_front();
                    }
                    kept.push_back(KeptPreview { preview: value.clone(), source_agent, source_native_id, source_root, cwd });
                }
                Ok(HopPreviewOutcome::Ready { preview: Box::new(value) })
            }
            Err(error) => Ok(refused("seed_refused", error.to_string())),
        }
    })
    .await
    .map_err(|_| SessionReadIssue::WorkerUnavailable)?
}
