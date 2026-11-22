//! C41.2/C42: project files in the desktop. The root must be a project recorded in Shinatama's
//! own database; every operation re-checks it. Nothing opened here is executed.
use crate::Services;
use serde::{Deserialize, Serialize};
use shinatama_files::buffers::{Overwrite, SaveError, save_text};
use shinatama_files::markdown::{self, Rendered};
use shinatama_files::navigator::{Listing, NavError, Root, SearchResult, TextFile};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{State, WebviewWindow};

fn same(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .trim_start_matches(r"\\?\")
            .trim_end_matches(['\\', '/'])
            .to_lowercase()
    };
    norm(a) == norm(b)
}

/// Only a project Shinatama recorded may be browsed.
fn root(services: &Services, project: &str) -> Result<Root, String> {
    let state = services.load()?;
    let known = state.store.recent().map_err(|e| format!("{e:#}"))?;
    let path = PathBuf::from(project);
    if !known
        .iter()
        .any(|p| p.available && same(Path::new(&p.path), &path))
    {
        return Err("This folder is not one of your projects.".into());
    }
    Root::open(&path).map_err(|e| e.to_string())
}

fn cancels() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static CANCELS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    CANCELS.get_or_init(Default::default)
}

fn token(request_id: &str) -> Result<Arc<AtomicBool>, String> {
    if request_id.is_empty()
        || request_id.len() > 64
        || !request_id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err("Invalid request id.".into());
    }
    let flag = Arc::new(AtomicBool::new(false));
    let mut map = cancels()
        .lock()
        .map_err(|_| "Cancellation is unavailable.")?;
    if map.len() > 64 {
        map.clear();
    }
    map.insert(request_id.to_owned(), flag.clone());
    Ok(flag)
}

fn done(request_id: &str) {
    if let Ok(mut map) = cancels().lock() {
        map.remove(request_id);
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| "The file worker stopped.".to_owned())?
}

#[tauri::command]
pub async fn files_list(
    window: WebviewWindow,
    services: State<'_, Services>,
    project: String,
    path: String,
    request_id: String,
) -> Result<Listing, String> {
    crate::local(&window)?;
    let services = services.inner().clone();
    let flag = token(&request_id)?;
    let result = blocking(move || {
        root(&services, &project)?
            .list(&path, &flag)
            .map_err(|e: NavError| e.to_string())
    })
    .await;
    done(&request_id);
    result
}

#[tauri::command]
pub async fn files_search(
    window: WebviewWindow,
    services: State<'_, Services>,
    project: String,
    query: String,
    request_id: String,
) -> Result<SearchResult, String> {
    crate::local(&window)?;
    let services = services.inner().clone();
    let flag = token(&request_id)?;
    let result = blocking(move || {
        root(&services, &project)?
            .search(&query, &flag)
            .map_err(|e| e.to_string())
    })
    .await;
    done(&request_id);
    result
}

#[tauri::command(async)]
pub fn files_cancel(window: WebviewWindow, request_id: String) -> Result<(), String> {
    crate::local(&window)?;
    if let Some(flag) = cancels()
        .lock()
        .map_err(|_| "Cancellation is unavailable.")?
        .get(&request_id)
    {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub async fn files_read(
    window: WebviewWindow,
    services: State<'_, Services>,
    project: String,
    path: String,
) -> Result<TextFile, String> {
    crate::local(&window)?;
    let services = services.inner().clone();
    blocking(move || {
        root(&services, &project)?
            .read(&path)
            .map_err(|e| e.to_string())
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveRequest {
    project: String,
    path: String,
    text: String,
    base_sha256: String,
    overwrite: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum SaveOutcome {
    Saved { sha256: String },
    Conflict { disk_sha256: String },
    Failed { message: String },
}

#[tauri::command]
pub async fn files_save(
    window: WebviewWindow,
    services: State<'_, Services>,
    attention: State<'_, crate::attention::Controller>,
    request: SaveRequest,
) -> Result<SaveOutcome, String> {
    crate::local(&window)?;
    let services = services.inner().clone();
    let outcome = blocking(move || {
        let root = root(&services, &request.project)?;
        let overwrite = if request.overwrite {
            Overwrite::Always
        } else {
            Overwrite::IfUnchanged
        };
        Ok(
            match save_text(
                &root,
                &request.path,
                &request.text,
                &request.base_sha256,
                overwrite,
            ) {
                Ok(sha256) => SaveOutcome::Saved { sha256 },
                Err(SaveError::Conflict { disk_sha256 }) => SaveOutcome::Conflict { disk_sha256 },
                Err(error) => SaveOutcome::Failed {
                    message: error.to_string(),
                },
            },
        )
    })
    .await;
    if matches!(outcome, Ok(SaveOutcome::Saved { .. })) {
        attention.saved();
    }
    outcome
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum ImageOutcome {
    Ready {
        image: shinatama_files::preview::ImagePreview,
    },
    Refused {
        message: String,
    },
}

#[tauri::command]
pub async fn files_image(
    window: WebviewWindow,
    services: State<'_, Services>,
    project: String,
    path: String,
) -> Result<ImageOutcome, String> {
    crate::local(&window)?;
    let services = services.inner().clone();
    blocking(move || {
        let root = root(&services, &project)?;
        Ok(match shinatama_files::preview::image(&root, &path) {
            Ok(image) => ImageOutcome::Ready { image },
            Err(error) => ImageOutcome::Refused {
                message: error.to_string(),
            },
        })
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownView {
    html: String,
    truncated: bool,
}

#[tauri::command]
pub async fn files_visual_parse(
    window: WebviewWindow,
    text: String,
) -> Result<shinatama_files::visual::VisualDocument, String> {
    crate::local(&window)?;
    if text.len() > shinatama_files::buffers::MAX_BUFFER_BYTES {
        return Err("The document is too large for visual editing.".into());
    }
    blocking(move || Ok(shinatama_files::visual::parse(&text))).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VisualEdit {
    index: usize,
    text: String,
}

#[tauri::command]
pub async fn files_visual_apply(
    window: WebviewWindow,
    text: String,
    edits: Vec<VisualEdit>,
) -> Result<String, String> {
    crate::local(&window)?;
    if text.len() > shinatama_files::buffers::MAX_BUFFER_BYTES || edits.len() > 64 {
        return Err("The edit is too large.".into());
    }
    blocking(move || {
        let edits: Vec<(usize, String)> = edits.into_iter().map(|e| (e.index, e.text)).collect();
        shinatama_files::visual::apply(&text, &edits).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn files_markdown(window: WebviewWindow, text: String) -> Result<MarkdownView, String> {
    crate::local(&window)?;
    blocking(move || {
        let Rendered { html, truncated } = markdown::render(&text);
        Ok(MarkdownView { html, truncated })
    })
    .await
}
