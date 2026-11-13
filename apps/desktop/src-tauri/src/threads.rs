//! Bounded saved metadata and fixed sidebar preferences. No native launch API.
use crate::storage::Services;
use serde::{Deserialize, Serialize};
use shinatama_state::threads::{
    SidebarSnapshot, SidebarState, ThreadCursor, ThreadError, ThreadListPage,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::{Manager, State, WebviewWindow};

#[derive(Serialize)]
pub struct Error {
    code: &'static str,
}
impl From<ThreadError> for Error {
    fn from(error: ThreadError) -> Self {
        Self {
            code: match error {
                ThreadError::Invalid => "invalid",
                ThreadError::Conflict => "conflict",
                ThreadError::OwnedElsewhere => "owned_elsewhere",
                ThreadError::NotFound => "project_missing",
                ThreadError::RootUnavailable => "root_unavailable",
                ThreadError::Storage => "unavailable",
            },
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct List {
    project_id: String,
    cursor: Option<ThreadCursor>,
    limit: usize,
    #[serde(default)]
    archived: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Save {
    expected_revision: u64,
    settings: SidebarState,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Create {
    project_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rename {
    project_id: String,
    thread_id: String,
    expected_revision: u64,
    title: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Archive {
    project_id: String,
    thread_id: String,
    expected_revision: u64,
    archived: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Forget {
    project_id: String,
    thread_id: String,
    expected_revision: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Recover {
    project_id: String,
    thread_id: String,
    expected_revision: u64,
}
pub struct Controller {
    services: Arc<Services>,
    busy: AtomicBool,
}
struct Slot(Arc<Controller>);
impl Drop for Slot {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}
impl Controller {
    pub fn new(services: Arc<Services>) -> Self {
        Self {
            services,
            busy: AtomicBool::new(false),
        }
    }
    async fn work<T: Send + 'static>(
        self: &Arc<Self>,
        work: impl FnOnce(&shinatama_state::database::Database) -> Result<T, ThreadError>
        + Send
        + 'static,
    ) -> Result<T, Error> {
        self.work_result(move |database| work(database).map_err(Into::into))
            .await
    }
    async fn work_result<T: Send + 'static>(
        self: &Arc<Self>,
        work: impl FnOnce(&shinatama_state::database::Database) -> Result<T, Error> + Send + 'static,
    ) -> Result<T, Error> {
        self.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error { code: "busy" })?;
        let slot = Slot(self.clone());
        tauri::async_runtime::spawn_blocking(move || {
            let state = slot.0.services.load().map_err(|_| Error {
                code: "unavailable",
            })?;
            work(&state.store)
        })
        .await
        .map_err(|_| Error {
            code: "unavailable",
        })?
    }
}
/// Explicit metadata recovery only. This command has no process-start/stop authority.
#[tauri::command]
pub async fn thread_recover(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: Recover,
) -> Result<(), Error> {
    local(&window)?;
    controller
        .inner()
        .work_result(move |db| {
            use shinatama_engine::resources::job::{JobRecoveryObservation, probe_job_recovery};
            use shinatama_state::{
                native_session_cleanup::{
                    NativeTerminationCertificate, TerminationCertificateKind,
                    probe_native_session_cleanup,
                },
                records::{
                    CanonicalIdKind, MAX_SAFE_REVISION, WorkspaceOwner,
                    recovery::{OwnerObservation, probe_owner_identity},
                    validate_id,
                },
            };
            validate_id(&request.project_id, CanonicalIdKind::Project)
                .map_err(|_| Error { code: "invalid" })?;
            validate_id(&request.thread_id, CanonicalIdKind::Thread)
                .map_err(|_| Error { code: "invalid" })?;
            if !(1..=MAX_SAFE_REVISION).contains(&request.expected_revision) {
                return Err(Error { code: "invalid" });
            }
            let saved = db
                .thread_resume_target(&request.thread_id)
                .map_err(|_| Error {
                    code: "source_changed",
                })?;
            if saved.thread.project_id != request.project_id
                || saved.thread.revision != request.expected_revision
            {
                return Err(Error {
                    code: "source_changed",
                });
            }
            let active = saved.recorded_activation.ok_or(Error {
                code: "source_changed",
            })?;
            let receipt = db
                .native_session_cleanup(&active.owner.instance_id)
                .map_err(|_| Error {
                    code: "unavailable",
                })?;
            if let Some(receipt) = receipt {
                if receipt.owner.activation != active.owner {
                    return Err(Error {
                        code: "source_changed",
                    });
                }
                // Native observations run without a SQL transaction and never signal a
                // process. The final transaction rechecks receipt, owner and revisions.
                let probe = probe_native_session_cleanup(&receipt).map_err(|_| Error {
                    code: "unavailable",
                })?;
                if probe.shell == OwnerObservation::AliveExact {
                    return Err(Error {
                        code: "session_alive",
                    });
                }
                if probe.host == OwnerObservation::AliveExact {
                    return Err(Error {
                        code: "owner_alive",
                    });
                }
                let proof = probe.proof.ok_or(Error {
                    code: "cleanup_unproven",
                })?;
                let at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .and_then(|time| i64::try_from(time.as_millis()).ok())
                    .ok_or(Error {
                        code: "unavailable",
                    })?;
                db.recover_native_session_cleanup(
                    &proof,
                    request.expected_revision,
                    &active,
                    at.max(receipt.released_at_ms).max(active.started_at),
                )
                .map_err(|_| Error {
                    code: "source_changed",
                })?;
                return Ok(());
            }
            // ADR-0051: no cleanup receipt exists (crash before the durable
            // release). The only unlock is a qualified whole-group termination
            // certificate from the durable association's named kill-on-close job.
            let owner = db
                .native_session_owner(&active.owner.instance_id)
                .map_err(|_| Error {
                    code: "unavailable",
                })?
                .ok_or(Error {
                    code: "cleanup_unproven",
                })?;
            if owner.activation != active.owner {
                return Err(Error {
                    code: "source_changed",
                });
            }
            // Exact identity observations first: a live former host or agent
            // blocks without touching the job. PID-reuse is accepted as dead.
            let identity = |pid: u32, created: u64| WorkspaceOwner {
                window_id: owner.activation.instance_id.clone(),
                pid,
                start_time: format!("win-{created}"),
            };
            let host_probe = probe_owner_identity(&identity(
                owner.reservation.owner.pid,
                owner.reservation.owner.created_at_100ns,
            ))
            .map_err(|_| Error {
                code: "unavailable",
            })?;
            if host_probe.observation == OwnerObservation::AliveExact {
                return Err(Error {
                    code: "owner_alive",
                });
            }
            let shell_probe = probe_owner_identity(&identity(
                owner.activation.pid,
                owner.activation.created_at_100ns,
            ))
            .map_err(|_| Error {
                code: "unavailable",
            })?;
            if shell_probe.observation == OwnerObservation::AliveExact {
                return Err(Error {
                    code: "session_alive",
                });
            }
            if host_probe.proof.is_none() || shell_probe.proof.is_none() {
                return Err(Error {
                    code: "cleanup_unproven",
                });
            }
            {
                use windows_sys::Win32::System::{
                    RemoteDesktop::ProcessIdToSessionId, Threading::GetCurrentProcessId,
                };
                let mut current = 0u32;
                if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut current) } == 0
                    || current != owner.reservation.owner.windows_session_id
                {
                    return Err(Error {
                        code: "cleanup_unproven",
                    });
                }
            }
            let job_key = uuid::Uuid::parse_str(&owner.reservation.job_key)
                .map(|uuid| *uuid.as_bytes())
                .map_err(|_| Error {
                    code: "cleanup_unproven",
                })?;
            let kind = match probe_job_recovery(job_key) {
                Ok(JobRecoveryObservation::Absent) => TerminationCertificateKind::JobNameAbsent,
                Ok(JobRecoveryObservation::TerminatedByRecovery) => {
                    TerminationCertificateKind::TerminatedByRecovery
                }
                Err(_) => {
                    return Err(Error {
                        code: "cleanup_unproven",
                    });
                }
            };
            let at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|time| i64::try_from(time.as_millis()).ok())
                .ok_or(Error {
                    code: "unavailable",
                })?;
            let certificate = NativeTerminationCertificate {
                owner,
                kind,
                at,
            };
            db.certify_native_session_termination(&certificate, request.expected_revision, &active, at)
                .map_err(|_| Error {
                    code: "source_changed",
                })?;
            Ok(())
        })
        .await
}
fn local(window: &WebviewWindow) -> Result<(), Error> {
    crate::local(window).map_err(|_| Error { code: "invalid" })?;
    if window
        .state::<crate::ExitIntent>()
        .0
        .load(Ordering::Acquire)
    {
        return Err(Error { code: "closing" });
    }
    Ok(())
}
#[tauri::command]
pub async fn thread_list(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: List,
) -> Result<ThreadListPage, Error> {
    local(&window)?;
    controller
        .inner()
        .work(move |db| {
            db.list_threads(
                &request.project_id,
                request.cursor.as_ref(),
                request.limit,
                request.archived,
            )
        })
        .await
}
#[tauri::command]
pub async fn thread_sidebar_load(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
) -> Result<SidebarSnapshot, Error> {
    local(&window)?;
    controller
        .inner()
        .work(|db| db.thread_sidebar_snapshot())
        .await
}
#[tauri::command]
pub async fn thread_sidebar_save(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: Save,
) -> Result<SidebarSnapshot, Error> {
    local(&window)?;
    controller
        .inner()
        .work(move |db| db.save_thread_sidebar(request.expected_revision, request.settings))
        .await
}
#[tauri::command]
pub async fn thread_create(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: Create,
) -> Result<shinatama_state::threads::ThreadRecord, Error> {
    local(&window)?;
    controller
        .inner()
        .work(move |db| {
            db.list_threads(&request.project_id, None, 1, false)?;
            let at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|time| i64::try_from(time.as_millis()).ok())
                .ok_or(ThreadError::Storage)?;
            db.create_thread(&shinatama_state::threads::ThreadRecord {
                id: shinatama_state::records::new_id(
                    shinatama_state::records::CanonicalIdKind::Thread,
                ),
                project_id: request.project_id,
                kind: shinatama_state::records::ThreadKind::Work,
                parent_thread_id: None,
                companion_worker_thread_id: None,
                title: None,
                archived: false,
                revision: 1,
                created_at: at,
                updated_at: at,
            })
        })
        .await
}

#[tauri::command]
pub async fn thread_rename(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: Rename,
) -> Result<shinatama_state::threads::ThreadRecord, Error> {
    local(&window)?;
    controller
        .inner()
        .work(move |db| {
            // Bound the raw input before trimming. Preserve every other saved
            // field; neither archive state nor native binding comes from IPC.
            if request.title.len() > 256 || request.title.chars().any(char::is_control) {
                return Err(ThreadError::Invalid);
            }
            let title = request.title.trim();
            if title.is_empty() {
                return Err(ThreadError::Invalid);
            }
            let current = db
                .thread_record(&request.thread_id)?
                .ok_or(ThreadError::NotFound)?;
            if current.project_id != request.project_id {
                return Err(ThreadError::Invalid);
            }
            if current.revision != request.expected_revision {
                return Err(ThreadError::Conflict);
            }
            let at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|time| i64::try_from(time.as_millis()).ok())
                .ok_or(ThreadError::Storage)?;
            db.update_thread_metadata(
                &request.thread_id,
                request.expected_revision,
                Some(title),
                current.archived,
                at,
            )
        })
        .await
}

fn now_ms() -> Result<i64, ThreadError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|time| i64::try_from(time.as_millis()).ok())
        .ok_or(ThreadError::Storage)
}
/// The saved record, checked against the project and revision the window showed.
fn current(
    db: &shinatama_state::database::Database,
    project_id: &str,
    thread_id: &str,
    expected_revision: u64,
) -> Result<shinatama_state::threads::ThreadRecord, ThreadError> {
    let current = db.thread_record(thread_id)?.ok_or(ThreadError::NotFound)?;
    if current.project_id != project_id {
        return Err(ThreadError::Invalid);
    }
    if current.revision != expected_revision {
        return Err(ThreadError::Conflict);
    }
    Ok(current)
}

#[tauri::command]
pub async fn thread_archive(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: Archive,
) -> Result<shinatama_state::threads::ThreadRecord, Error> {
    local(&window)?;
    controller
        .inner()
        .work(move |db| {
            let current = current(db, &request.project_id, &request.thread_id, request.expected_revision)?;
            db.update_thread_metadata(
                &request.thread_id,
                request.expected_revision,
                current.title.as_deref(),
                request.archived,
                now_ms()?,
            )
        })
        .await
}

/// FRD-0014 R27. The sidebar asks for a second click before calling this.
#[tauri::command]
pub async fn thread_forget(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: Forget,
) -> Result<shinatama_state::forget::ForgetReport, Error> {
    use shinatama_state::forget::{ForgetError, forget_thread};
    local(&window)?;
    let services = controller.services.clone();
    let report = controller
        .inner()
        .work_result(move |db| {
            // Archive state and revision are checked again inside the delete transaction.
            let thread = db.thread_record(&request.thread_id)?.ok_or(Error { code: "not_found" })?;
            if thread.project_id != request.project_id {
                return Err(Error { code: "invalid" });
            }
            let location = services.load().map_err(|_| Error { code: "unavailable" })?.location.clone();
            forget_thread(&location, db, &request.thread_id, Some(request.expected_revision), now_ms()?).map_err(|error| Error {
                code: match error {
                    ForgetError::NotFound => "not_found",
                    ForgetError::Invalid => "invalid",
                    ForgetError::Changed => "conflict",
                    ForgetError::IndexNotUpdated => "index_not_updated",
                    ForgetError::Open => "owned_elsewhere",
                    ForgetError::HopInProgress => "hop_in_progress",
                    ForgetError::Storage(_) => "unavailable",
                },
            })
        })
        .await?;
    for id in &report.thread_ids {
        crate::titles::forget_cached(id);
    }
    Ok(report)
}

// ADR-0055: a workset is what the interface calls a thread, a set of sessions. These commands
// read and change saved metadata only; none starts, resumes or stops a process.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorksetList {
    project_id: String,
    #[serde(default)]
    archived: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorksetPage {
    rows: Vec<shinatama_state::worksets::WorksetSummary>,
    more: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorksetCreate {
    project_id: String,
    title: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorksetRef {
    workset_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorksetUpdate {
    workset_id: String,
    expected_revision: u64,
    /// Absent leaves the title; an empty string clears it.
    title: Option<String>,
    archived: Option<bool>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorksetDelete {
    workset_id: String,
    expected_revision: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorksetAdopt {
    workset_id: String,
    thread_id: String,
}
fn workset_title(raw: &str) -> Result<Option<&str>, ThreadError> {
    if raw.len() > 256 || raw.chars().any(char::is_control) {
        return Err(ThreadError::Invalid);
    }
    let title = raw.trim();
    Ok((!title.is_empty()).then_some(title))
}
#[tauri::command]
pub async fn workset_list(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: WorksetList,
) -> Result<WorksetPage, Error> {
    local(&window)?;
    controller
        .inner()
        .work(move |db| {
            let (rows, more) = db.list_worksets(&request.project_id, request.archived)?;
            Ok(WorksetPage { rows, more })
        })
        .await
}
#[tauri::command]
pub async fn workset_create(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: WorksetCreate,
) -> Result<shinatama_state::worksets::WorksetRecord, Error> {
    local(&window)?;
    controller
        .inner()
        .work(move |db| {
            let title = match request.title.as_deref() {
                Some(raw) => workset_title(raw)?,
                None => None,
            };
            db.create_workset(&request.project_id, title, now_ms()?)
        })
        .await
}
#[tauri::command]
pub async fn workset_threads(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: WorksetRef,
) -> Result<Vec<shinatama_state::threads::ThreadSummary>, Error> {
    local(&window)?;
    controller
        .inner()
        .work(move |db| db.workset_threads(&request.workset_id))
        .await
}
#[tauri::command]
pub async fn workset_update(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: WorksetUpdate,
) -> Result<shinatama_state::worksets::WorksetRecord, Error> {
    local(&window)?;
    controller
        .inner()
        .work(move |db| {
            let title = match request.title.as_deref() {
                Some(raw) => Some(workset_title(raw)?),
                None => None,
            };
            db.update_workset(
                &request.workset_id,
                request.expected_revision,
                title,
                request.archived,
                now_ms()?,
            )
        })
        .await
}
/// Deletes a thread that holds no session; sessions are forgotten one by one first.
#[tauri::command]
pub async fn workset_delete(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: WorksetDelete,
) -> Result<(), Error> {
    local(&window)?;
    controller
        .inner()
        .work(move |db| db.delete_empty_workset(&request.workset_id, request.expected_revision))
        .await
}
/// Puts a session in the thread the user has open (or moved it to).
#[tauri::command]
pub async fn workset_adopt(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: WorksetAdopt,
) -> Result<(), Error> {
    local(&window)?;
    controller
        .inner()
        .work(move |db| db.move_thread_to_workset(&request.thread_id, &request.workset_id, now_ms()?))
        .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorksetOf {
    thread_id: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorksetScope {
    id: String,
    project_id: String,
}
/// The thread a session belongs to; the window uses it to place a session it did not start itself.
#[tauri::command]
pub async fn workset_of(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: WorksetOf,
) -> Result<Option<WorksetScope>, Error> {
    local(&window)?;
    controller
        .inner()
        .work(move |db| {
            let Some(id) = db.workset_of_thread(&request.thread_id)? else {
                return Ok(None);
            };
            Ok(db.workset_record(&id)?.map(|record| WorksetScope {
                id: record.id,
                project_id: record.project_id,
            }))
        })
        .await
}
