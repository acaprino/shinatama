//! Explicit layout ownership recovery. Native observations run off the UI and
//! outside SQL transactions; they never terminate or signal a process. Load
//! the saved record internally; never accept a purported record/proof from IPC.
use super::{MAX_SAFE_REVISION, RecordsError, Result, WorkspaceOwner, WorkspaceRecord};
use serde::Serialize;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownReason {
    InvalidFingerprint,
    UnsupportedPlatform,
    AccessDenied,
    OsError(u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerObservation {
    AliveExact,
    DeadExact,
    PidMissing,
    PidReused,
    Unknown(UnknownReason),
}

pub struct OwnerProbe {
    pub observation: OwnerObservation,
    pub proof: Option<DeadWorkspaceOwnerProof>,
}
impl fmt::Debug for OwnerProbe {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("OwnerProbe")
            .field("observation", &self.observation)
            .field("has_proof", &self.proof.is_some())
            .finish()
    }
}
/// Native producer only. Not Clone, Serialize or Deserialize. The exact former
/// owner, scope and revision remain inseparable from this historical-death proof.
pub struct DeadWorkspaceOwnerProof {
    pub(super) project_id: String,
    pub(super) workspace_id: String,
    revision: u64,
    owner: WorkspaceOwner,
    observation: OwnerObservation,
    #[cfg(windows)]
    _process: Option<std::os::windows::io::OwnedHandle>,
}
impl fmt::Debug for DeadWorkspaceOwnerProof {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("DeadWorkspaceOwnerProof")
            .field("observation", &self.observation)
            .finish_non_exhaustive()
    }
}
impl DeadWorkspaceOwnerProof {
    pub(super) fn matches(&self, record: &WorkspaceRecord) -> bool {
        record.project_id == self.project_id
            && record.workspace_id == self.workspace_id
            && record.revision == self.revision
            && record.owner.as_ref() == Some(&self.owner)
            && matches!(
                self.observation,
                OwnerObservation::DeadExact
                    | OwnerObservation::PidMissing
                    | OwnerObservation::PidReused
            )
    }
}

/// One OpenProcess, creation-time read and zero-time wait, no retry. Missing PID
/// or a different creation time proves this saved identity is gone; a reused live
/// PID is never killed or mistaken for the old owner. Death is irreversible, so
/// freshness is enforced by record/owner/revision CAS instead of a wall clock.
pub fn probe_workspace_owner(record: &WorkspaceRecord) -> Result<OwnerProbe> {
    record.validate()?;
    if record.revision >= MAX_SAFE_REVISION {
        return Err(RecordsError::Invalid("revisione recupero"));
    }
    let owner = record
        .owner
        .as_ref()
        .ok_or(RecordsError::Invalid("workspace senza proprietario"))?;
    let Some(expected) = windows_creation_time(&owner.start_time) else {
        return Ok(OwnerProbe {
            observation: OwnerObservation::Unknown(UnknownReason::InvalidFingerprint),
            proof: None,
        });
    };
    #[cfg(windows)]
    {
        Ok(native::probe(record, owner, expected))
    }
    #[cfg(not(windows))]
    {
        let _ = expected;
        Ok(OwnerProbe {
            observation: OwnerObservation::Unknown(UnknownReason::UnsupportedPlatform),
            proof: None,
        })
    }
}
fn windows_creation_time(value: &str) -> Option<u64> {
    let decimal = value.strip_prefix("win-")?;
    if decimal.is_empty()
        || decimal.len() > 20
        || !decimal.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let time = decimal.parse::<u64>().ok()?;
    (time != 0 && time.to_string() == decimal).then_some(time)
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::{
        ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, FILETIME, GetLastError, WAIT_OBJECT_0,
        WAIT_TIMEOUT,
    };
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        WaitForSingleObject,
    };

    fn ticks(time: FILETIME) -> u64 {
        (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
    }
    fn open_failure(code: u32) -> OwnerObservation {
        match code {
            ERROR_INVALID_PARAMETER => OwnerObservation::PidMissing,
            ERROR_ACCESS_DENIED => OwnerObservation::Unknown(UnknownReason::AccessDenied),
            other => OwnerObservation::Unknown(UnknownReason::OsError(other)),
        }
    }
    pub(super) fn probe(
        record: &WorkspaceRecord,
        owner: &WorkspaceOwner,
        expected: u64,
    ) -> OwnerProbe {
        let raw = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                owner.pid,
            )
        };
        let (observation, process) = if raw.is_null() {
            (open_failure(unsafe { GetLastError() }), None)
        } else {
            let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
            let (mut creation, mut exit, mut kernel, mut user) = (
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
            );
            let observation = if unsafe {
                GetProcessTimes(raw, &mut creation, &mut exit, &mut kernel, &mut user)
            } == 0
            {
                OwnerObservation::Unknown(UnknownReason::OsError(unsafe { GetLastError() }))
            } else if ticks(creation) == 0 {
                OwnerObservation::Unknown(UnknownReason::InvalidFingerprint)
            } else if ticks(creation) != expected {
                OwnerObservation::PidReused
            } else {
                match unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) } {
                    WAIT_OBJECT_0 => OwnerObservation::DeadExact,
                    WAIT_TIMEOUT => OwnerObservation::AliveExact,
                    _ => {
                        OwnerObservation::Unknown(UnknownReason::OsError(unsafe { GetLastError() }))
                    }
                }
            };
            (observation, Some(handle))
        };
        let proof = if matches!(
            observation,
            OwnerObservation::DeadExact
                | OwnerObservation::PidMissing
                | OwnerObservation::PidReused
        ) {
            Some(DeadWorkspaceOwnerProof {
                project_id: record.project_id.clone(),
                workspace_id: record.workspace_id.clone(),
                revision: record.revision,
                owner: owner.clone(),
                observation,
                _process: process,
            })
        } else {
            None
        };
        OwnerProbe { observation, proof }
    }

    #[cfg(test)]
    #[test]
    fn denied_and_unknown_errors_never_produce_a_missing_pid_observation() {
        assert_eq!(
            open_failure(ERROR_INVALID_PARAMETER),
            OwnerObservation::PidMissing
        );
        assert_eq!(
            open_failure(ERROR_ACCESS_DENIED),
            OwnerObservation::Unknown(UnknownReason::AccessDenied)
        );
        assert_eq!(
            open_failure(87_654),
            OwnerObservation::Unknown(UnknownReason::OsError(87_654))
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_opaque_zero_and_noncanonical_fingerprints_cannot_authorize_recovery() {
        for invalid in [
            "synthetic-start-a",
            "123",
            "win-0",
            "win-01",
            "win-+1",
            "win-18446744073709551616",
            "win-1\0",
        ] {
            assert_eq!(windows_creation_time(invalid), None);
        }
        assert_eq!(
            windows_creation_time("win-18446744073709551615"),
            Some(u64::MAX)
        );
    }
    #[cfg(windows)]
    fn owner(pid: u32, handle: std::os::windows::io::RawHandle) -> WorkspaceOwner {
        use windows_sys::Win32::Foundation::FILETIME;
        use windows_sys::Win32::System::Threading::GetProcessTimes;
        let (mut created, mut exit, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        assert_ne!(
            unsafe { GetProcessTimes(handle, &mut created, &mut exit, &mut kernel, &mut user) },
            0
        );
        WorkspaceOwner {
            window_id: "recovery-fixture-window".into(),
            pid,
            start_time: format!(
                "win-{}",
                (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime)
            ),
        }
    }
    fn record(project: &str, workspace: &str, owner: WorkspaceOwner) -> WorkspaceRecord {
        WorkspaceRecord {
            project_id: project.into(),
            workspace_id: workspace.into(),
            revision: 1,
            schema_version: 1,
            layout_json: r#"{"axis":"rows","views":[]}"#.into(),
            owner: Some(owner),
            updated_at: 1_700_000_000_123,
        }
    }
    #[cfg(windows)]
    #[test]
    fn live_identity_and_reused_pid_are_distinct_and_no_current_process_is_signalled() {
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        let owner = owner(std::process::id(), unsafe { GetCurrentProcess() });
        let mut record = record("p_abcdefghijkl", "recovery", owner);
        let live = probe_workspace_owner(&record).unwrap();
        assert_eq!(live.observation, OwnerObservation::AliveExact);
        assert!(live.proof.is_none());
        let old = windows_creation_time(&record.owner.as_ref().unwrap().start_time).unwrap();
        record.owner.as_mut().unwrap().start_time = format!("win-{}", old - 1);
        let reused = probe_workspace_owner(&record).unwrap();
        assert_eq!(reused.observation, OwnerObservation::PidReused);
        assert!(reused.proof.is_some());
        assert_eq!(std::process::id(), record.owner.as_ref().unwrap().pid);
    }
    #[cfg(windows)]
    #[test]
    fn controlled_dead_owner_fixture() {
        if std::env::var_os("SHINATAMA_WORKSPACE_RECOVERY_FIXTURE").is_some() {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    #[cfg(windows)]
    #[test]
    fn real_dead_owner_recovery_is_cas_bound_preserves_layout_and_refuses_future_schema() {
        use crate::database::{Database, DatabaseError};
        use crate::location::{LocationRequest, StorageLocation};
        use std::{
            fs,
            os::windows::io::AsRawHandle,
            process::{Command, Stdio},
            time::{Duration, Instant},
        };
        let base = fs::canonicalize(std::env::temp_dir()).unwrap();
        let root = base.join(format!(
            "shinatama-workspace-recovery-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&root).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "records::recovery::tests::controlled_dead_owner_fixture",
            ])
            .env_clear()
            .env("SHINATAMA_WORKSPACE_RECOVERY_FIXTURE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let previous = owner(child.id(), child.as_raw_handle());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let location = StorageLocation::resolve(LocationRequest {
                local_data_directory: root.join("platform"),
                home_override: Some(root.join("private")),
                project_roots: Vec::new(),
                synchronized_roots: Vec::new(),
            })
            .unwrap();
            let database = Database::open(&location, None).unwrap();
            let project_path = root.join("project");
            fs::create_dir(&project_path).unwrap();
            let project = database.select(&project_path).unwrap();
            let original = database
                .save_workspace(record(&project.id, "first", previous.clone()), 0, &previous)
                .unwrap();
            database
                .save_workspace(
                    record(&project.id, "second", previous.clone()),
                    0,
                    &previous,
                )
                .unwrap();
            database
                .save_workspace(record(&project.id, "clear", previous.clone()), 0, &previous)
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            while child.try_wait().unwrap().is_none() {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
            let probe = probe_workspace_owner(&original).unwrap();
            assert_eq!(probe.observation, OwnerObservation::DeadExact);
            let proof = probe.proof.unwrap();
            let current = owner(std::process::id(), unsafe {
                windows_sys::Win32::System::Threading::GetCurrentProcess()
            });
            let recovered = database.recover_workspace(&proof, Some(&current)).unwrap();
            assert_eq!(recovered.revision, 2);
            assert_eq!(recovered.owner, Some(current));
            assert_eq!(recovered.layout_json, original.layout_json);
            assert_eq!(recovered.updated_at, original.updated_at);
            assert_eq!(recovered.schema_version, original.schema_version);
            assert!(matches!(
                database.recover_workspace(&proof, None),
                Err(RecordsError::Conflict)
            ));
            let second = database
                .workspace_record(&project.id, "second")
                .unwrap()
                .unwrap();
            assert_eq!(second.owner, original.owner);
            assert_eq!(second.revision, 1);
            let clear = database
                .workspace_record(&project.id, "clear")
                .unwrap()
                .unwrap();
            let clear_proof = probe_workspace_owner(&clear).unwrap().proof.unwrap();
            let cleared = database.recover_workspace(&clear_proof, None).unwrap();
            assert!(cleared.owner.is_none());
            assert_eq!(cleared.revision, 2);
            assert_eq!(cleared.layout_json, clear.layout_json);
            assert_eq!(cleared.updated_at, clear.updated_at);
            let stale_owner_proof = probe_workspace_owner(&second).unwrap().proof.unwrap();
            database
                .release_workspace(&project.id, "second", 1, &previous)
                .unwrap();
            assert!(matches!(
                database.recover_workspace(&stale_owner_proof, None),
                Err(RecordsError::Conflict)
            ));
            // Re-seed only this synthetic record to exercise a valid, exact
            // native proof through the live future-schema admission guard.
            let second = database.save_workspace(second, 1, &previous).unwrap();
            let proof = probe_workspace_owner(&second).unwrap().proof.unwrap();
            let before = serde_json::to_value(&second).unwrap();
            let raw = rusqlite::Connection::open(location.database_path()).unwrap();
            raw.pragma_update(None, "user_version", 999).unwrap();
            assert!(matches!(
                database.recover_workspace(&proof, None),
                Err(RecordsError::Database(DatabaseError::NewerSchema {
                    found: 999,
                    ..
                }))
            ));
            assert_eq!(
                serde_json::to_value(
                    database
                        .workspace_record(&project.id, "second")
                        .unwrap()
                        .unwrap()
                )
                .unwrap(),
                before
            );
        }));
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(root.parent(), Some(base.as_path()));
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("shinatama-workspace-recovery-")
        );
        let _ = fs::remove_dir_all(&root);
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }
}
