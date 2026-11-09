//! C26 export from the session detail: the contained worker renders, the native Save dialog
//! names the file (its overwrite prompt is the explicit consent), and the atomic writer commits.
use super::Controller;
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
        },
        format,
        mode: request.mode,
        with_attachments: request.with_attachments,
        thread: None,
        home,
    };
    let reply = match controller.inner().export(worker_request).await {
        Ok(reply) => reply,
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
    let extension = format.extension().trim_start_matches('.').to_owned();
    let (sender, mut receiver) = tauri::async_runtime::channel(1);
    window
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("Export session")
        .set_file_name(&payload.suggested_name)
        .add_filter(payload.report.declaration.label.clone(), &[extension.as_str()])
        .save_file(move |path| {
            let _ = sender.try_send(path);
        });
    let Some(Some(path)) = receiver.recv().await else {
        return Ok(ExportOutcome::Cancelled);
    };
    let path = path
        .into_path()
        .map_err(|_| SessionReadIssue::InvalidRequest)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut attachments = Vec::with_capacity(payload.attachments.len());
        for blob in &payload.attachments {
            let data = shinatama_export::base64::decode(
                &blob.data_base64,
                shinatama_engine::session_export::MAX_ATTACHMENT_BYTES,
            )
            .ok_or(SessionReadIssue::WorkerUnavailable)?;
            attachments.push(output::AttachmentFile {
                name: blob.name.clone(),
                data,
            });
        }
        // The native Save dialog already asked before replacing an existing file.
        Ok(
            match output::write_export(
                &path,
                output::Overwrite::Replace,
                payload.content.as_bytes(),
                &attachments,
            ) {
                Ok(receipt) => ExportOutcome::Saved {
                    path: receipt.path.display().to_string(),
                    bytes: receipt.bytes,
                    sha256: receipt.sha256,
                    attachments: receipt.attachments,
                    report: Box::new(payload.report),
                },
                Err(error) => ExportOutcome::Refused {
                    issue: format!("write_failed:{error}"),
                },
            },
        )
    })
    .await
    .map_err(|_| SessionReadIssue::WorkerUnavailable)?
}
