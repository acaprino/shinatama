//! C24: persistent search over session transcripts.
//!
//! Indexing is an explicit, cancellable pass over the registered archives. It
//! reuses the contained metadata/detail readers (never the filesystem directly),
//! skips sessions whose metadata signature is unchanged, and stores only visible
//! user/assistant text in a derived index that can be forgotten or rebuilt
//! without touching authoritative threads or the archives themselves.
use super::{Controller, ListRequest, Result, SessionLibraryRoot};
use serde::{Deserialize, Serialize};
use shinatama_engine::session_detail::{DetailCursor, DetailEventKind, DetailRequest};
use shinatama_engine::session_roots::{RootSelector, SessionCursor, SessionReadIssue};
use shinatama_state::search_index::{
    IndexStatus, IndexedDocument, MAX_BODY_BYTES, MAX_RESULTS, SearchFilters, SearchIndex,
};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{State, WebviewWindow};

/// A session larger than this many detail pages keeps only its first pages.
const MAX_PAGES_PER_SESSION: usize = 12;
const BUSY_RETRIES: usize = 12;

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IndexProgress {
    /// idle | running | done | stopped | failed
    pub phase: &'static str,
    pub roots_total: u32,
    pub roots_done: u32,
    pub indexed: u32,
    pub skipped: u32,
    pub failed: u32,
    pub unsupported_archives: u32,
    pub marked_source_deleted: u32,
    pub message: Option<&'static str>,
}

#[derive(Default)]
pub struct RunState {
    running: AtomicBool,
    cancel: AtomicBool,
    progress: Mutex<IndexProgress>,
}
impl RunState {
    fn update(&self, change: impl FnOnce(&mut IndexProgress)) {
        if let Ok(mut progress) = self.progress.lock() {
            change(&mut progress);
        }
    }
    fn snapshot(&self) -> IndexProgress {
        self.progress
            .lock()
            .map(|progress| progress.clone())
            .unwrap_or_default()
    }
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexOverview {
    enabled: bool,
    documents: u64,
    source_deleted: u64,
    body_bytes: u64,
}
impl From<IndexStatus> for IndexOverview {
    fn from(status: IndexStatus) -> Self {
        Self {
            enabled: status.enabled,
            documents: status.documents,
            source_deleted: status.source_deleted,
            body_bytes: status.body_bytes,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexState {
    progress: IndexProgress,
    index: IndexOverview,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchRequest {
    query: String,
    harness: Option<String>,
    project: Option<String>,
    /// YYYY-MM-DD, inclusive.
    from: Option<String>,
    to: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHitView {
    store_root_id: String,
    /// 0 when the archive was removed since indexing: the hit is not openable.
    configuration_revision: u64,
    native_id: String,
    harness: String,
    title: Option<String>,
    working_directory: Option<String>,
    updated_at: Option<String>,
    snippet: String,
    source_deleted: bool,
}

fn date(value: &Option<String>) -> Result<Option<String>> {
    match value.as_deref() {
        None | Some("") => Ok(None),
        Some(text)
            if text.len() == 10
                && text.bytes().enumerate().all(|(i, b)| match i {
                    4 | 7 => b == b'-',
                    _ => b.is_ascii_digit(),
                }) =>
        {
            Ok(Some(text.to_owned()))
        }
        _ => Err(SessionReadIssue::InvalidRequest),
    }
}
fn text_filter(value: &Option<String>, cap: usize) -> Result<Option<String>> {
    match value.as_deref() {
        None | Some("") => Ok(None),
        Some(text) if text.len() <= cap && !text.chars().any(char::is_control) => {
            Ok(Some(text.to_owned()))
        }
        _ => Err(SessionReadIssue::InvalidRequest),
    }
}

impl Controller {
    fn search_index(&self) -> Result<Arc<SearchIndex>> {
        let mut slot = self
            .search
            .lock()
            .map_err(|_| SessionReadIssue::ReadUnavailable)?;
        if let Some(index) = slot.as_ref() {
            return Ok(index.clone());
        }
        let state = self
            .services
            .load()
            .map_err(|_| SessionReadIssue::ReadUnavailable)?;
        let index = Arc::new(
            SearchIndex::open(&state.location).map_err(|_| SessionReadIssue::ReadUnavailable)?,
        );
        *slot = Some(index.clone());
        Ok(index)
    }

    /// Retry only the transient "another reader is running" refusal.
    fn call<T>(
        self: &Arc<Self>,
        run: &RunState,
        mut attempt: impl FnMut(&Arc<Self>) -> Result<T>,
    ) -> Result<T> {
        for _ in 0..BUSY_RETRIES {
            if run.cancelled() {
                return Err(SessionReadIssue::TimedOut);
            }
            match attempt(self) {
                Err(SessionReadIssue::ReadUnavailable) => {
                    std::thread::sleep(Duration::from_millis(250))
                }
                other => return other,
            }
        }
        Err(SessionReadIssue::ReadUnavailable)
    }

    fn read_body(
        self: &Arc<Self>,
        run: &RunState,
        root: &RootSelector,
        native_id: &str,
    ) -> Result<String> {
        let mut body = String::new();
        let mut cursor: Option<DetailCursor> = None;
        for _ in 0..MAX_PAGES_PER_SESSION {
            let request = DetailRequest {
                root: root.clone(),
                native_id: native_id.to_owned(),
                cursor: cursor.clone(),
                limit: 50,
            };
            let reply = self.call(run, |controller| {
                tauri::async_runtime::block_on(controller.detail(request.clone()))
            })?;
            let page = reply
                .page
                .ok_or(SessionReadIssue::ReadUnavailable)?;
            for event in &page.events {
                if matches!(
                    event.kind,
                    DetailEventKind::UserText | DetailEventKind::AssistantText
                ) && !event.text.is_empty()
                {
                    body.push_str(&event.text);
                    body.push('\n');
                }
            }
            if body.len() >= MAX_BODY_BYTES {
                break;
            }
            match page.next {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        Ok(body)
    }

    /// One archive: list every page, read only new/changed sessions, then flag the
    /// ones that vanished. A listing that fails midway never flags anything.
    fn index_root(
        self: &Arc<Self>,
        run: &RunState,
        index: &SearchIndex,
        root: &SessionLibraryRoot,
    ) -> Result<()> {
        let selector = RootSelector {
            store_root_id: root.store_root_id.clone(),
            configuration_revision: root.configuration_revision,
        };
        let mut present = HashSet::new();
        let mut cursor: Option<SessionCursor> = None;
        loop {
            if run.cancelled() {
                return Err(SessionReadIssue::TimedOut);
            }
            let request = ListRequest {
                root: selector.clone(),
                cursor: cursor.clone(),
                limit: 50,
                include_children: true,
            };
            let page = self.call(run, |controller| {
                tauri::async_runtime::block_on(controller.list(ListRequest {
                    root: request.root.clone(),
                    cursor: request.cursor.clone(),
                    limit: request.limit,
                    include_children: request.include_children,
                }))
            })?;
            for row in &page.rows {
                present.insert(row.native_id.clone());
                let signature = format!(
                    "{}|{}|{}",
                    row.updated_at.as_deref().unwrap_or(""),
                    row.bytes.unwrap_or(0),
                    row.turns.unwrap_or(0)
                );
                match index.needs_index(&root.store_root_id, &row.native_id, &signature) {
                    Ok(false) => {
                        run.update(|p| p.skipped += 1);
                        continue;
                    }
                    Ok(true) => {}
                    Err(_) => return Err(SessionReadIssue::ReadUnavailable),
                }
                match self.read_body(run, &selector, &row.native_id) {
                    Ok(body) => {
                        let stored = index.upsert(&IndexedDocument {
                            root_id: &root.store_root_id,
                            native_id: &row.native_id,
                            harness: &root.harness,
                            title: row.title.as_deref(),
                            working_directory: row.working_directory.as_deref(),
                            updated_at: row.updated_at.as_deref(),
                            signature: &signature,
                            body: &body,
                        });
                        if stored.is_ok() {
                            run.update(|p| p.indexed += 1);
                        } else {
                            return Err(SessionReadIssue::ReadUnavailable);
                        }
                    }
                    // Cancel/timeouts and resource pressure stop the pass; a single
                    // unreadable session (oversized, changed, unsupported) is counted.
                    Err(SessionReadIssue::TimedOut | SessionReadIssue::WorkerUnavailable) => {
                        return Err(SessionReadIssue::WorkerUnavailable);
                    }
                    Err(_) => run.update(|p| p.failed += 1),
                }
            }
            match page.next {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        let marked = index
            .mark_missing(&root.store_root_id, &present)
            .map_err(|_| SessionReadIssue::ReadUnavailable)?;
        run.update(|p| p.marked_source_deleted += marked as u32);
        Ok(())
    }

    fn index_pass(self: &Arc<Self>, run: &RunState, index: &SearchIndex) -> Result<()> {
        let roots = tauri::async_runtime::block_on(self.roots())?;
        run.update(|p| p.roots_total = roots.len() as u32);
        for root in roots {
            if run.cancelled() {
                return Err(SessionReadIssue::TimedOut);
            }
            // OpenCode readers need a separately qualified exporter launch.
            if !root.available || root.harness == "opencode" {
                run.update(|p| {
                    p.unsupported_archives += 1;
                    p.roots_done += 1;
                });
                continue;
            }
            match self.index_root(run, index, &root) {
                Ok(()) => {}
                // A root the reader refuses (namespace, shape, size) does not stop the others.
                Err(
                    SessionReadIssue::UnsafeRoot
                    | SessionReadIssue::UnsupportedHarness
                    | SessionReadIssue::UnsupportedRootShape
                    | SessionReadIssue::RootUnavailable
                    | SessionReadIssue::RootChanged
                    | SessionReadIssue::MetadataTooLarge,
                ) => run.update(|p| p.unsupported_archives += 1),
                Err(other) => return Err(other),
            }
            run.update(|p| p.roots_done += 1);
        }
        Ok(())
    }

    pub fn index_start(self: &Arc<Self>) -> Result<IndexProgress> {
        let index = self.search_index()?;
        if !index.enabled().map_err(|_| SessionReadIssue::ReadUnavailable)? {
            return Err(SessionReadIssue::InvalidRequest);
        }
        let run = self.run.clone();
        if run
            .running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(SessionReadIssue::ReadUnavailable);
        }
        run.cancel.store(false, Ordering::Release);
        run.update(|p| {
            *p = IndexProgress {
                phase: "running",
                ..IndexProgress::default()
            }
        });
        let controller = Arc::clone(self);
        tauri::async_runtime::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                controller.index_pass(&run, &index)
            }));
            run.update(|p| match &result {
                Ok(Ok(())) => {
                    p.phase = "done";
                    p.message = None;
                }
                Ok(Err(SessionReadIssue::TimedOut)) => {
                    p.phase = "stopped";
                    p.message = Some("Indexing was stopped. Progress so far is kept.");
                }
                Ok(Err(SessionReadIssue::WorkerUnavailable)) => {
                    p.phase = "stopped";
                    p.message = Some("Windows is low on resources. Try again later; progress so far is kept.");
                }
                _ => {
                    p.phase = "failed";
                    p.message = Some("Indexing failed. Progress so far is kept.");
                }
            });
            run.running.store(false, Ordering::Release);
        });
        Ok(self.run.snapshot())
    }
}

#[tauri::command]
pub async fn session_index_start(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
) -> Result<IndexProgress> {
    super::local(&window)?;
    controller.inner().index_start()
}
#[tauri::command]
pub async fn session_index_cancel(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
) -> Result<()> {
    super::local(&window)?;
    controller.run.cancel.store(true, Ordering::Release);
    Ok(())
}
#[tauri::command]
pub async fn session_index_status(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
) -> Result<IndexState> {
    super::local(&window)?;
    let controller = controller.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let status = controller
            .search_index()?
            .status()
            .map_err(|_| SessionReadIssue::ReadUnavailable)?;
        Ok(IndexState {
            progress: controller.run.snapshot(),
            index: status.into(),
        })
    })
    .await
    .map_err(|_| SessionReadIssue::CleanupUnconfirmed)?
}
#[tauri::command]
pub async fn session_index_enable(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    enabled: bool,
) -> Result<()> {
    super::local(&window)?;
    if !enabled {
        controller.run.cancel.store(true, Ordering::Release);
    }
    let controller = controller.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        controller
            .search_index()?
            .set_enabled(enabled)
            .map_err(|_| SessionReadIssue::ReadUnavailable)
    })
    .await
    .map_err(|_| SessionReadIssue::CleanupUnconfirmed)?
}
/// Forgets indexed text only; archives and authoritative threads are untouched.
#[tauri::command]
pub async fn session_index_forget(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    store_root_id: Option<String>,
) -> Result<u64> {
    super::local(&window)?;
    if controller.run.running.load(Ordering::Acquire) {
        return Err(SessionReadIssue::ReadUnavailable);
    }
    let controller = controller.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        controller
            .search_index()?
            .forget(store_root_id.as_deref())
            .map_err(|_| SessionReadIssue::ReadUnavailable)
    })
    .await
    .map_err(|_| SessionReadIssue::CleanupUnconfirmed)?
}
#[tauri::command]
pub async fn session_search(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: SearchRequest,
) -> Result<Vec<SearchHitView>> {
    super::local(&window)?;
    let filters = SearchFilters {
        harness: text_filter(&request.harness, 64)?,
        project: text_filter(&request.project, 1024)?,
        from: date(&request.from)?,
        // Inclusive end of day.
        to: date(&request.to)?.map(|day| format!("{day}T23:59:59.999999999Z")),
    };
    let query = request.query;
    let controller = controller.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let hits = controller
            .search_index()?
            .search(&query, &filters, MAX_RESULTS)
            .map_err(|_| SessionReadIssue::InvalidRequest)?;
        let state = controller
            .services
            .load()
            .map_err(|_| SessionReadIssue::ReadUnavailable)?;
        let revisions: HashMap<String, u64> = state
            .store
            .list_store_roots(None, super::ROOT_LIMIT)
            .map_err(|_| SessionReadIssue::ReadUnavailable)?
            .into_iter()
            .map(|root| (root.id, root.config_revision))
            .collect();
        Ok(hits
            .into_iter()
            .map(|hit| SearchHitView {
                configuration_revision: revisions.get(&hit.root_id).copied().unwrap_or(0),
                store_root_id: hit.root_id,
                native_id: hit.native_id,
                harness: hit.harness,
                title: hit.title,
                working_directory: hit.working_directory,
                updated_at: hit.updated_at,
                snippet: hit.snippet,
                source_deleted: hit.source_deleted,
            })
            .collect())
    })
    .await
    .map_err(|_| SessionReadIssue::CleanupUnconfirmed)?
}
