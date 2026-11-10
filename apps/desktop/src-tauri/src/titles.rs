//! F06: verbatim activity titles (FRD-0010 R17-R22). The only backend in this build is the
//! model-free one: the last tool call or message, read through the contained session worker.
//! Nothing leaves the machine and nothing costs tokens. A thread with summaries paused is never
//! read. Titles refresh on a state change or after two minutes, at most eight reads per call.
#![cfg(windows)]
use crate::{Services, sessions};
use serde::{Deserialize, Serialize};
use shinatama_engine::session_detail::DetailRequest;
use shinatama_engine::session_roots::RootSelector;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{State, WebviewWindow};

const REFRESH: Duration = Duration::from_secs(120);
const MAX_READS_PER_CALL: usize = 8;

type Cache = Mutex<HashMap<String, (Instant, String, Option<String>)>>;
fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TitleRequest {
    instance_id: String,
    thread_id: Option<String>,
    state: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleRow {
    instance_id: String,
    thread_id: Option<String>,
    title: Option<String>,
    source: &'static str,
    reason: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Titles {
    enabled: bool,
    excluded: Vec<String>,
    rows: Vec<TitleRow>,
}

#[tauri::command]
pub async fn instance_titles(window: WebviewWindow, services: State<'_, Services>, controller: State<'_, Arc<sessions::Controller>>, requests: Vec<TitleRequest>) -> Result<Titles, String> {
    crate::local(&window)?;
    if requests.len() > 32 {
        return Err("Too many instances.".into());
    }
    let services = services.inner().clone();
    let (enabled, excluded) = {
        let services = services.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let store = &services.load()?.store;
            Ok::<_, String>((store.titles_enabled().map_err(|e| e.to_string())?, store.summaries_excluded().map_err(|e| e.to_string())?))
        })
        .await
        .map_err(|_| "The title worker stopped.".to_owned())??
    };
    let mut rows = Vec::with_capacity(requests.len());
    let mut reads = 0;
    for request in requests {
        let mut row = TitleRow { instance_id: request.instance_id, thread_id: request.thread_id.clone(), title: None, source: "verbatim", reason: None };
        let Some(thread) = request.thread_id else {
            row.reason = Some("no thread");
            rows.push(row);
            continue;
        };
        if !enabled {
            row.reason = Some("titles off");
        } else if excluded.contains(&thread) {
            row.reason = Some("summaries paused");
        } else {
            let cached = cache().lock().ok().and_then(|c| c.get(&thread).cloned());
            match cached {
                Some((at, state, title)) if state == request.state && at.elapsed() < REFRESH => row.title = title,
                _ if reads >= MAX_READS_PER_CALL => row.reason = Some("refresh deferred"),
                _ => {
                    reads += 1;
                    let target = {
                        let services = services.clone();
                        let thread = thread.clone();
                        tauri::async_runtime::spawn_blocking(move || services.load().ok().and_then(|s| s.store.thread_resume_target(&thread).ok()))
                            .await
                            .ok()
                            .flatten()
                    };
                    match target {
                        None => row.reason = Some("no session yet"),
                        Some(target) => {
                            let detail = controller
                                .inner()
                                .detail(DetailRequest {
                                    root: RootSelector { store_root_id: target.segment.native.store_root_id.clone(), configuration_revision: target.segment.native.store_revision },
                                    native_id: target.segment.native.native_id.clone(),
                                    cursor: None,
                                    limit: 1,
                                })
                                .await;
                            match detail.ok().and_then(|reply| reply.page) {
                                Some(page) => {
                                    row.title = page.activity_title.clone();
                                    if let Ok(mut c) = cache().lock() {
                                        c.insert(thread.clone(), (Instant::now(), request.state.clone(), page.activity_title));
                                    }
                                }
                                None => row.reason = Some("session not readable now"),
                            }
                        }
                    }
                }
            }
        }
        rows.push(row);
    }
    Ok(Titles { enabled, excluded, rows })
}

#[tauri::command]
pub async fn titles_set_enabled(window: WebviewWindow, services: State<'_, Services>, enabled: bool) -> Result<(), String> {
    crate::local(&window)?;
    let services = services.inner().clone();
    tauri::async_runtime::spawn_blocking(move || services.load()?.store.set_titles_enabled(enabled).map_err(|e| e.to_string()))
        .await
        .map_err(|_| "The title worker stopped.".to_owned())?
}

/// R14's "pause summaries" action: the thread is never read for titles while paused.
#[tauri::command]
pub async fn thread_summaries_pause(window: WebviewWindow, services: State<'_, Services>, thread_id: String, paused: bool) -> Result<Vec<String>, String> {
    crate::local(&window)?;
    let services = services.inner().clone();
    if paused && let Ok(mut c) = cache().lock() {
        c.remove(&thread_id);
    }
    tauri::async_runtime::spawn_blocking(move || services.load()?.store.set_summaries_excluded(&thread_id, paused).map_err(|e| e.to_string()))
        .await
        .map_err(|_| "The title worker stopped.".to_owned())?
}
