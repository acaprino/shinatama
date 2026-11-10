//! C36: project overview rows (FRD-0004 R07-R08, R16). Pins live in Shinatama's database;
//! git facts come from a bounded parallel scan with a short cache; the last thread is the
//! project's most recent Shinatama thread. A worktree is created only by an explicit request
//! from the new-instance flow, never by the overview itself.
use crate::Services;
use serde::Serialize;
use shinatama_projects::git::{self, GitCache, GitFacts};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::{State, WebviewWindow};

fn cache() -> &'static GitCache {
    static CACHE: OnceLock<GitCache> = OnceLock::new();
    CACHE.get_or_init(GitCache::default)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastThread {
    id: String,
    title: Option<String>,
    harness: Option<String>,
    revision: u64,
    updated_at: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRow {
    id: String,
    path: String,
    name: String,
    available: bool,
    pinned: bool,
    last_opened: i64,
    git: GitFacts,
    last_thread: Option<LastThread>,
}

#[tauri::command]
pub async fn project_overview(window: WebviewWindow, services: State<'_, Services>) -> Result<Vec<ProjectRow>, String> {
    crate::local(&window)?;
    let services = services.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = services.load()?;
        let projects = state.store.recent().map_err(|e| format!("{e:#}"))?;
        let pins = state.store.pinned_projects().map_err(|e| format!("{e:#}"))?;
        let paths: Vec<PathBuf> = projects.iter().map(|p| PathBuf::from(&p.path)).collect();
        let facts = cache().scan(&paths, git::DEFAULT_TIMEOUT);
        let mut rows: Vec<ProjectRow> = projects
            .into_iter()
            .zip(facts)
            .map(|(project, git)| {
                let last_thread = state
                    .store
                    .list_threads(&project.id, None, 1)
                    .ok()
                    .and_then(|page| page.rows.into_iter().next())
                    .map(|t| LastThread { id: t.id, title: t.title, harness: t.harness, revision: t.revision, updated_at: t.updated_at });
                ProjectRow {
                    pinned: pins.contains(&project.id),
                    git: if project.available { git } else { GitFacts::Unknown { reason: "folder unavailable".into() } },
                    id: project.id,
                    path: project.path,
                    name: project.name,
                    available: project.available,
                    last_opened: project.last_opened,
                    last_thread,
                }
            })
            .collect();
        let rank = |id: &str| pins.iter().position(|p| p == id).unwrap_or(usize::MAX);
        rows.sort_by(|a, b| rank(&a.id).cmp(&rank(&b.id)).then(b.last_opened.cmp(&a.last_opened)));
        Ok(rows)
    })
    .await
    .map_err(|_| "The project worker stopped.".to_owned())?
}

#[tauri::command]
pub async fn project_pin(window: WebviewWindow, services: State<'_, Services>, project_id: String, pinned: bool) -> Result<Vec<String>, String> {
    crate::local(&window)?;
    let services = services.inner().clone();
    tauri::async_runtime::spawn_blocking(move || services.load()?.store.set_project_pinned(&project_id, pinned).map_err(|e| e.to_string()))
        .await
        .map_err(|_| "The project worker stopped.".to_owned())?
}

fn recorded(services: &Services, path: &str) -> Result<PathBuf, String> {
    let state = services.load()?;
    let norm = |p: &str| p.trim_start_matches(r"\\?\").trim_end_matches(['\\', '/']).to_lowercase();
    state
        .store
        .recent()
        .map_err(|e| format!("{e:#}"))?
        .into_iter()
        .find(|p| p.available && norm(&p.path) == norm(path))
        .map(|p| PathBuf::from(p.path))
        .ok_or_else(|| "This folder is not one of your projects.".to_owned())
}

#[tauri::command]
pub async fn project_worktree_plan(window: WebviewWindow, services: State<'_, Services>, project: String, name: String) -> Result<git::WorktreePlan, String> {
    crate::local(&window)?;
    let services = services.inner().clone();
    tauri::async_runtime::spawn_blocking(move || git::plan_worktree(&recorded(&services, &project)?, &name).map_err(|e| e.to_string()))
        .await
        .map_err(|_| "The project worker stopped.".to_owned())?
}

/// Creates exactly the worktree the user confirmed (same name, same derived path).
#[tauri::command]
pub async fn project_worktree_create(window: WebviewWindow, services: State<'_, Services>, project: String, name: String, confirmed_path: String) -> Result<String, String> {
    crate::local(&window)?;
    let services = services.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = recorded(&services, &project)?;
        let plan = git::plan_worktree(&root, &name).map_err(|e| e.to_string())?;
        if !same(&plan.path, Path::new(&confirmed_path)) {
            return Err("The worktree location changed after you confirmed it; nothing was created.".into());
        }
        let path = git::create_worktree(&plan).map_err(|e| e.to_string())?;
        cache().invalidate(&root);
        Ok(path.display().to_string())
    })
    .await
    .map_err(|_| "The project worker stopped.".to_owned())?
}

fn same(a: &Path, b: &Path) -> bool {
    let n = |p: &Path| p.to_string_lossy().trim_start_matches(r"\\?\").trim_end_matches(['\\', '/']).to_lowercase();
    n(a) == n(b)
}
