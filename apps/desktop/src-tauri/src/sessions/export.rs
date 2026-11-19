//! C26 export from the session detail: the native Save dialog names the file first (its overwrite
//! prompt is the explicit consent), the contained worker renders and streams the body in frames
//! that are staged beside the destination as they arrive, and the atomic writer commits after the
//! final frame. A session of any size exports; a failure or a cancel removes the staged file.
use super::{Controller, StagedExport};
use serde::{Deserialize, Serialize};
use shinatama_engine::session_detail::{DetailCursor, DetailRequest};
use shinatama_engine::session_export::{ExportIssue, ExportReport, ExportRequest};
use shinatama_engine::session_roots::{RootSelector, SessionReadIssue};
use shinatama_export::{Format, Mode, output};
use std::sync::Arc;
use tauri::{State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportUiRequest {
    root: RootSelector,
    native_id: String,
    fingerprint: String,
    format: Format,
    mode: Mode,
    with_attachments: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum ExportOutcome {
    Saved {
        path: String,
        bytes: u64,
        sha256: String,
        attachments: u64,
        report: Box<ExportReport>,
    },
    Cancelled,
    Refused {
        issue: String,
    },
}

fn issue_code(issue: ExportIssue) -> String {
    match serde_json::to_value(issue).unwrap_or_default() {
        serde_json::Value::String(code) => code,
        serde_json::Value::Object(map) => map
            .get("detail")
            .and_then(|v| v.as_str())
            .map_or_else(|| "read_unavailable".into(), str::to_owned),
        _ => "read_unavailable".into(),
    }
}

fn read_issue_code(issue: SessionReadIssue) -> String {
    serde_json::to_value(issue)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "read_unavailable".into())
}

#[tauri::command]
pub async fn session_export(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: ExportUiRequest,
    request_id: Option<String>,
) -> Result<ExportOutcome, SessionReadIssue> {
    super::local(&window)?;
    if request.fingerprint.len() != 64 || !request.fingerprint.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(SessionReadIssue::InvalidRequest);
    }
    let home = (request.mode == Mode::Portable)
        .then(|| std::env::var("USERPROFILE").ok())
        .flatten()
        .filter(|home| std::path::Path::new(home).is_absolute());
    let format = request.format;
    let worker_request = ExportRequest {
        detail: DetailRequest {
            root: request.root,
            native_id: request.native_id,
            cursor: Some(DetailCursor {
                offset: 0,
                fingerprint: request.fingerprint,
            }),
            limit: 1,
            overview_only: false,
        },
        format,
        mode: request.mode,
        with_attachments: request.with_attachments,
        thread: None,
        home,
    };
    // The dialog comes first: the staged file lives beside the destination, so the destination
    // must be known before the worker starts. The name is the session's id (a title is read by
    // the worker, after the dialog); the dialog's overwrite prompt is the consent.
    let extension = format.extension().trim_start_matches('.').to_owned();
    let (sender, mut receiver) = tauri::async_runtime::channel(1);
    window
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("Export session")
        .set_file_name(format!("{}{}", worker_request.detail.native_id, format.extension()))
        .add_filter(
            shinatama_export::formats::declaration(format, false).label,
            &[extension.as_str()],
        )
        .save_file(move |path| {
            let _ = sender.try_send(path);
        });
    let Some(Some(path)) = receiver.recv().await else {
        return Ok(ExportOutcome::Cancelled);
    };
    let path = path
        .into_path()
        .map_err(|_| SessionReadIssue::InvalidRequest)?;
    let staging = path.clone();
    let staged = match tauri::async_runtime::spawn_blocking(move || {
        output::stage(&staging, output::Overwrite::Replace)
    })
    .await
    .map_err(|_| SessionReadIssue::WorkerUnavailable)?
    {
        Ok(staged) => staged,
        Err(error) => {
            return Ok(ExportOutcome::Refused {
                issue: format!("write_failed:{error}"),
            });
        }
    };
    // Watched from the moment the worker starts: its progress is sent to the window and a cancel
    // stops it. The id is registered only now, so a cancel before the dialog closes finds nothing.
    let (watch, _guard) = controller.inner().watch(&window, "export", request_id)?;
    let (reply, staged) = match controller.inner().export_staged(worker_request, staged, watch).await {
        Ok(StagedExport::Done { reply, staged }) => (reply, staged),
        Ok(StagedExport::WriteFailed) => {
            return Ok(ExportOutcome::Refused {
                issue: "write_failed:the file could not be written".into(),
            });
        }
        // Stopped by the person: the staged file was removed with the dropped stage.
        Err(SessionReadIssue::Cancelled) => return Ok(ExportOutcome::Cancelled),
        Err(issue) => {
            return Ok(ExportOutcome::Refused {
                issue: read_issue_code(issue),
            });
        }
    };
    let payload = match (reply.payload, reply.issue) {
        (Some(payload), None) => payload,
        (_, Some(issue)) => {
            return Ok(ExportOutcome::Refused {
                issue: issue_code(issue),
            });
        }
        _ => {
            return Ok(ExportOutcome::Refused {
                issue: "read_unavailable".into(),
            });
        }
    };
    tauri::async_runtime::spawn_blocking(move || {
        // The native Save dialog already asked before replacing an existing file.
        Ok(match shinatama_engine::session_export::commit_export(staged, &payload) {
            Ok(receipt) => ExportOutcome::Saved {
                path: receipt.path.display().to_string(),
                bytes: receipt.bytes,
                sha256: receipt.sha256,
                attachments: receipt.attachments,
                report: Box::new(payload.report),
            },
            Err(error) => ExportOutcome::Refused {
                issue: format!("write_failed:{error:?}"),
            },
        })
    })
    .await
    .map_err(|_| SessionReadIssue::WorkerUnavailable)?
}
