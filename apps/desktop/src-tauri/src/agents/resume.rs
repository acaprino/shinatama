//! Explicit resume resolves one saved thread, then invokes its registered harness service.
use super::{
    AgentStarted, Controller, latest_box,
    service::{OpenMode, SessionOperation},
};
use serde::{Deserialize, Serialize};
use shinatama_engine::{
    session_detail::{DetailIssue, DetailRequest},
    session_roots::{RootSelector, SessionReadIssue},
};
use shinatama_state::{
    database::Database,
    location::StorageLocation,
    records::{CanonicalIdKind, MAX_SAFE_REVISION, RootKind, RootScope, validate_id},
    threads::{ThreadResumeError, ThreadResumeTarget},
};
use shinatama_terminal::TerminalHost;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    thread_id: String,
    project_id: String,
    expected_revision: u64,
    rows: u16,
    cols: u16,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Error {
    pub code: &'static str,
}
type Result<T> = std::result::Result<T, Error>;
fn issue(code: &'static str) -> Error {
    Error { code }
}
fn unavailable<T>(_: T) -> Error {
    issue("unavailable")
}
fn snapshot_error(error: ThreadResumeError) -> Error {
    issue(match error {
        ThreadResumeError::Unbound => "unbound",
        ThreadResumeError::NotFound
        | ThreadResumeError::RootChanged
        | ThreadResumeError::Invalid => "source_changed",
        _ => "unavailable",
    })
}
fn read_error(error: SessionReadIssue) -> Error {
    issue(match error {
        SessionReadIssue::RootChanged | SessionReadIssue::SourceChanged => "source_changed",
        _ => "unavailable",
    })
}
fn detail_error(error: DetailIssue) -> Error {
    issue(match error {
        DetailIssue::SessionNotFound => "native_missing",
        DetailIssue::RootChanged | DetailIssue::SourceChanged | DetailIssue::AmbiguousSession => {
            "source_changed"
        }
        _ => "unavailable",
    })
}
fn target(database: &Database, request: &Request) -> Result<ThreadResumeTarget> {
    let saved = database
        .thread_resume_target(&request.thread_id)
        .map_err(snapshot_error)?;
    if saved.thread.project_id != request.project_id
        || saved.thread.revision != request.expected_revision
    {
        return Err(issue("source_changed"));
    }
    if saved.recorded_activation.is_some() {
        return Err(issue("owned_elsewhere"));
    }
    if saved.segment.harness != saved.root.harness {
        return Err(issue("unsupported_harness"));
    }
    if saved.root.kind != RootKind::Box || saved.root.scope != RootScope::Primary {
        return Err(issue("unavailable"));
    }
    Ok(saved)
}
fn canonical_project(location: &StorageLocation, value: &str) -> Result<PathBuf> {
    if value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
        return Err(issue("project_unavailable"));
    }
    let path = Path::new(value);
    location
        .validate_project(path)
        .map_err(|_| issue("project_unavailable"))?;
    let canonical = fs::canonicalize(path).map_err(|_| issue("project_unavailable"))?;
    if !fs::metadata(&canonical)
        .map_err(|_| issue("project_unavailable"))?
        .is_dir()
    {
        return Err(issue("project_unavailable"));
    }
    location
        .validate_project(&canonical)
        .map_err(|_| issue("project_unavailable"))?;
    Ok(canonical)
}
fn verify_native(
    reader: &Arc<crate::sessions::Controller>,
    location: &StorageLocation,
    saved: &ThreadResumeTarget,
    project: &Path,
) -> Result<()> {
    let selector = RootSelector {
        store_root_id: saved.root.id.clone(),
        configuration_revision: saved.root.config_revision,
    };
    let reply = tauri::async_runtime::block_on(reader.detail(DetailRequest {
        root: selector.clone(),
        native_id: saved.segment.native.native_id.clone(),
        cursor: None,
        limit: 1,
        overview_only: true,
    }))
    .map_err(read_error)?;
    if reply.root != selector || reply.native_id != saved.segment.native.native_id {
        return Err(issue("source_changed"));
    }
    if let Some(error) = reply.issue {
        return Err(detail_error(error));
    }
    let page = reply.page.ok_or_else(|| issue("native_missing"))?;
    if page.root != selector
        || page.native_id != saved.segment.native.native_id
        || page.harness.as_str() != saved.segment.harness
    {
        return Err(issue("source_changed"));
    }
    let cwd = page
        .overview
        .working_directory
        .as_deref()
        .ok_or_else(|| issue("source_changed"))?;
    if canonical_project(location, cwd)? != project {
        return Err(issue("source_changed"));
    }
    // The bounded reader projection is dropped here; no transcript reaches IPC,
    // diagnostics, persistence or the launch command.
    Ok(())
}
impl Controller {
    pub async fn resume(
        self: &Arc<Self>,
        host: Arc<TerminalHost>,
        reader: Arc<crate::sessions::Controller>,
        request: Request,
    ) -> Result<AgentStarted> {
        validate_id(&request.thread_id, CanonicalIdKind::Thread)
            .map_err(|_| issue("invalid_request"))?;
        validate_id(&request.project_id, CanonicalIdKind::Project)
            .map_err(|_| issue("invalid_request"))?;
        if !(1..=MAX_SAFE_REVISION).contains(&request.expected_revision) {
            return Err(issue("invalid_request"));
        }
        shinatama_terminal::validate_size(request.rows, request.cols)
            .map_err(|_| issue("invalid_request"))?;
        let owner = Arc::clone(self);
        tauri::async_runtime::spawn_blocking(move || {
            let state = owner.services.load().map_err(unavailable)?;
            let saved = target(&state.store, &request)?;
            let entry = owner
                .registered(&saved.segment.harness)
                .map_err(|_| issue("unsupported_harness"))?;
            if !entry.service.descriptor().native_resume {
                return Err(issue("unsupported_harness"));
            }
            let slot = entry.reserve().map_err(|_| issue("busy"))?;
            slot.run(issue("unavailable"), |service| {
                let project = canonical_project(&state.location, &saved.project_path)?;
                if latest_box(&state.store, &saved.segment.harness)
                    .map_err(unavailable)?
                    .as_ref()
                    != Some(&saved.root)
                {
                    return Err(issue("source_changed"));
                }
                verify_native(&reader, &state.location, &saved, &project)?;
                if target(&state.store, &request)? != saved {
                    return Err(issue("source_changed"));
                }
                service
                    .open(
                        &host,
                        &project,
                        request.rows,
                        request.cols,
                        SessionOperation::Resume(saved),
                        OpenMode::PrepareWhenNeeded,
                        None,
                        false,
                    )
                    .map_err(unavailable)
            })
        })
        .await
        .map_err(unavailable)?
    }
}
