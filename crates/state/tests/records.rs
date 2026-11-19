#![cfg(windows)]

// External API proof against the real FS03 migration and unique private fixtures.
use rusqlite::Connection;
use shinatama_state::database::{Database, DatabaseAccess, DatabaseError};
use shinatama_state::location::{LocationRequest, StorageLocation};
use shinatama_state::records::{
    MAX_LAYOUT_BYTES, NativeSessionKey, PreparedStoreRoot, RecordsError, RootAccess, RootKind,
    RootOrigin, RootScope, StoreRootRecord, WorkspaceOwner, WorkspaceRecord, WorkspaceRestore,
};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

struct Fixture {
    root: PathBuf,
    base: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let base = fs::canonicalize(std::env::temp_dir()).unwrap();
        let root = base.join(format!("shinatama-records-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("projects")).unwrap();
        Self { root, base }
    }
    fn directory(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn location(&self) -> StorageLocation {
        StorageLocation::resolve(LocationRequest {
            local_data_directory: self.root.clone(),
            home_override: Some(self.root.join("private-state")),
            project_roots: vec![self.root.join("projects")],
            synchronized_roots: Vec::new(),
        })
        .unwrap()
    }
    fn prepared(
        &self,
        id: &str,
        revision: u64,
        directory: &str,
        kind: RootKind,
        scope: RootScope,
        access: RootAccess,
    ) -> PreparedStoreRoot {
        PreparedStoreRoot::resolve(StoreRootRecord {
            id: id.into(),
            harness: "codex".into(),
            kind,
            scope,
            resolved_path: self.directory(directory).to_string_lossy().into_owned(),
            origin: RootOrigin::Configured,
            config_revision: revision,
            access,
            available: true,
        })
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.root.is_absolute()
            && self.root.parent() == Some(self.base.as_path())
            && self
                .root
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("shinatama-records-"))
        {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
fn owner(start_time: &str) -> WorkspaceOwner {
    WorkspaceOwner {
        window_id: "synthetic-window".into(),
        pid: 101,
        start_time: start_time.into(),
    }
}
fn workspace(project: &str, json: &str) -> WorkspaceRecord {
    WorkspaceRecord {
        project_id: project.into(),
        workspace_id: "workspace-main".into(),
        revision: 1,
        schema_version: 1,
        layout_json: json.into(),
        owner: None,
        updated_at: 1_700_000_000_123,
    }
}
fn snapshot(database: &Database, project: &str) -> serde_json::Value {
    serde_json::to_value(
        database
            .workspace_record(project, "workspace-main")
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn actual_private_sqlite_and_reopen_use_the_activated_records_schema() {
    let fixture = Fixture::new();
    let location = fixture.location();
    let database = Database::open(&location, None).unwrap();
    let diagnostics = database.diagnostics().unwrap();
    assert!(
        diagnostics.schema_version >= 3,
        "FS03 must be activated through the real migration."
    );
    assert_eq!(diagnostics.journal_mode, "wal");
    assert!(diagnostics.foreign_keys);
    location.verify_private_directory().unwrap();
    location
        .verify_private_file(&location.database_path())
        .unwrap();
    for name in ["shinatama.db-wal", "shinatama.db-shm"] {
        let path = location.directory().join(name);
        if path.exists() {
            location.verify_private_file(&path).unwrap();
        }
    }
    let root = fixture.prepared(
        "source-one",
        1,
        "stores/one",
        RootKind::Box,
        RootScope::Primary,
        RootAccess::ReadWrite,
    );
    let saved = database.register_store_root(&root).unwrap();
    database.integrity_check().unwrap();
    drop(database);
    let reopened = Database::open(&location, None).unwrap();
    assert_eq!(reopened.store_root(&saved.id).unwrap(), Some(saved));
    assert_eq!(
        reopened.diagnostics().unwrap().schema_version,
        diagnostics.schema_version
    );
    reopened.integrity_check().unwrap();
}

#[test]
fn root_removal_and_reconnection_preserve_history_and_distinct_native_scopes() {
    let fixture = Fixture::new();
    let location = fixture.location();
    let database = Database::open(&location, None).unwrap();
    let project = database
        .select(&fixture.directory("projects/work"))
        .unwrap();
    let first = fixture.prepared(
        "root-a-v1",
        1,
        "stores/a",
        RootKind::Box,
        RootScope::Primary,
        RootAccess::ReadWrite,
    );
    let second = fixture.prepared(
        "root-b-v1",
        1,
        "stores/b",
        RootKind::System,
        RootScope::Additional,
        RootAccess::ReadOnly,
    );
    let first = database.register_store_root(&first).unwrap();
    let second = database.register_additional_system_root(&second).unwrap();
    let mut duplicate = first.clone();
    duplicate.id = "duplicate-primary".into();
    let duplicate = PreparedStoreRoot::resolve(duplicate).unwrap();
    assert!(matches!(
        database.register_store_root(&duplicate),
        Err(RecordsError::Conflict)
    ));
    let mut escalation = second.clone();
    escalation.id = "escalated-additional".into();
    escalation.config_revision = 2;
    escalation.scope = RootScope::Primary;
    escalation.access = RootAccess::ReadWrite;
    let escalation = PreparedStoreRoot::resolve(escalation).unwrap();
    assert!(matches!(
        database.register_store_root_revision(&second.id, 1, &escalation),
        Err(RecordsError::Conflict)
    ));
    assert_eq!(
        database.store_root(&second.id).unwrap(),
        Some(second.clone())
    );
    let sentinel = fixture.root.join("stores/a/native-history-sentinel");
    fs::write(&sentinel, b"synthetic original history remains intact").unwrap();
    let first_key = NativeSessionKey {
        store_root_id: first.id.clone(),
        store_revision: 1,
        native_id: "same-native-id".into(),
    };
    let second_key = NativeSessionKey {
        store_root_id: second.id.clone(),
        store_revision: 1,
        native_id: "same-native-id".into(),
    };
    first_key.validate().unwrap();
    second_key.validate().unwrap();
    assert_eq!(
        HashSet::from([first_key.clone(), second_key.clone()]).len(),
        2
    );
    let history = serde_json::json!({"version":1,"native_refs":[first_key,second_key]}).to_string();
    database
        .save_workspace(
            workspace(&project.id, &history),
            0,
            &owner("synthetic-start-a"),
        )
        .unwrap();
    let historical = snapshot(&database, &project.id);
    let removed = database.mark_store_root_unavailable(&first.id, 1).unwrap();
    let mut expected_old = first.clone();
    expected_old.available = false;
    assert_eq!(removed, expected_old);
    let next = fixture.prepared(
        "root-a-v2",
        2,
        "stores/a",
        RootKind::Box,
        RootScope::Primary,
        RootAccess::ReadWrite,
    );
    let next = database
        .register_store_root_revision(&first.id, 1, &next)
        .unwrap();
    let mut fork = next.clone();
    fork.id = "root-a-fork".into();
    let fork = PreparedStoreRoot::resolve(fork).unwrap();
    assert!(matches!(
        database.register_store_root_revision(&first.id, 1, &fork),
        Err(RecordsError::Conflict)
    ));
    // Additional roots have no primary-slot uniqueness constraint. Their consumed
    // predecessor must still reject a second successor after a successful CAS.
    let mut second_next = second.clone();
    second_next.id = "root-b-v2".into();
    second_next.config_revision = 2;
    let second_next = PreparedStoreRoot::resolve(second_next).unwrap();
    let second_next = database
        .register_store_root_revision(&second.id, 1, &second_next)
        .unwrap();
    let mut second_fork = second_next.clone();
    second_fork.id = "root-b-fork".into();
    let second_fork = PreparedStoreRoot::resolve(second_fork).unwrap();
    assert!(matches!(
        database.register_store_root_revision(&second.id, 1, &second_fork),
        Err(RecordsError::Conflict)
    ));
    let mut reassign = next.clone();
    reassign.id = first.id.clone();
    let reassign = PreparedStoreRoot::resolve(reassign).unwrap();
    assert!(matches!(
        database.register_store_root(&reassign),
        Err(RecordsError::Conflict)
    ));
    assert_eq!(
        database.store_root(&first.id).unwrap(),
        Some(expected_old.clone())
    );
    assert!(
        snapshot(&database, &project.id) == historical,
        "Root changes must not remap retained references."
    );
    assert_eq!(database.list_store_roots(None, 100).unwrap().len(), 4);
    assert!(database.store_root("duplicate-primary").unwrap().is_none());
    assert!(
        database
            .store_root("escalated-additional")
            .unwrap()
            .is_none()
    );
    assert!(database.store_root("root-a-fork").unwrap().is_none());
    assert!(database.store_root("root-b-fork").unwrap().is_none());
    drop(database);
    let reopened = Database::open(&location, None).unwrap();
    let old = reopened.store_root(&first.id).unwrap().unwrap();
    assert!(!old.available);
    assert_eq!(old, expected_old);
    assert!(first_key.matches_root(&old));
    assert!(!first_key.matches_root(&next));
    assert!(second_key.matches_root(&reopened.store_root(&second.id).unwrap().unwrap()));
    assert!(
        snapshot(&reopened, &project.id) == historical,
        "Reopen must preserve historical scopes."
    );
    assert_eq!(
        fs::read(&sentinel).unwrap(),
        b"synthetic original history remains intact"
    );
}

#[test]
fn two_connections_enforce_workspace_cas_fingerprint_release_and_data_only_restore() {
    let fixture = Fixture::new();
    let location = fixture.location();
    let first = Database::open(&location, None).unwrap();
    let second = Database::open(&location, None).unwrap();
    let project = first.select(&fixture.directory("projects/work")).unwrap();
    let old_owner = owner("synthetic-start-a");
    let new_owner = owner("synthetic-start-b");
    let original = workspace(
        &project.id,
        r#"{"axis":"columns","views":[{"thread_id":"t_abcdefghijkl"}]}"#,
    );
    let saved = first
        .save_workspace(original.clone(), 0, &old_owner)
        .unwrap();
    assert_eq!(saved.revision, 1);
    let before = snapshot(&first, &project.id);
    assert!(matches!(
        second.save_workspace(original.clone(), 0, &old_owner),
        Err(RecordsError::Conflict)
    ));
    assert!(matches!(
        second.save_workspace(original.clone(), 1, &new_owner),
        Err(RecordsError::OwnedElsewhere)
    ));
    assert!(matches!(
        second.release_workspace(&project.id, "workspace-main", 1, &new_owner),
        Err(RecordsError::OwnedElsewhere)
    ));
    assert!(matches!(
        first.release_workspace(&project.id, "workspace-main", 2, &old_owner),
        Err(RecordsError::Conflict)
    ));
    assert!(
        snapshot(&first, &project.id) == before,
        "Rejected owners and stale revisions must preserve saved data."
    );
    first
        .release_workspace(&project.id, "workspace-main", 1, &old_owner)
        .unwrap();
    let released = second
        .workspace_record(&project.id, "workspace-main")
        .unwrap()
        .unwrap();
    assert!(released.owner.is_none());
    assert_eq!(released.revision, 1);
    let mut replacement = original;
    replacement.revision = 999;
    replacement.layout_json = r#"{"axis":"rows","views":[{"thread_id":"t_abcdefghijkl"}]}"#.into();
    let saved = second.save_workspace(replacement, 1, &new_owner).unwrap();
    assert_eq!(saved.revision, 2);
    assert_eq!(saved.owner, Some(new_owner.clone()));
    second
        .release_workspace(&project.id, "workspace-main", 2, &new_owner)
        .unwrap();
    drop(first);
    drop(second);
    let reopened = Database::open(&location, None).unwrap();
    let record = reopened
        .workspace_record(&project.id, "workspace-main")
        .unwrap()
        .unwrap();
    assert!(record.owner.is_none());
    // Exhaustive destructuring guards the restoration boundary against silently
    // gaining a process, executable, PID, PTY handle or owner field.
    let WorkspaceRestore {
        project_id,
        workspace_id,
        revision,
        schema_version,
        layout_json,
    } = record.restore_descriptors().unwrap();
    assert_eq!(project_id, project.id);
    assert_eq!(workspace_id, "workspace-main");
    assert_eq!(revision, 2);
    assert_eq!(schema_version, 1);
    assert_eq!(layout_json, saved.layout_json);
}

#[test]
fn invalid_or_oversized_layouts_and_unknown_versions_leave_the_saved_record_unchanged() {
    let fixture = Fixture::new();
    let database = Database::open(&fixture.location(), None).unwrap();
    let project = database
        .select(&fixture.directory("projects/work"))
        .unwrap();
    let owner = owner("synthetic-start-a");
    let original = workspace(&project.id, r#"{"axis":"columns","views":[]}"#);
    database
        .save_workspace(original.clone(), 0, &owner)
        .unwrap();
    let before = snapshot(&database, &project.id);
    for document in [
        "{not-json".to_owned(),
        "[]".to_owned(),
        r#"{"nested":{"PID":123}}"#.to_owned(),
        serde_json::json!({"data":"x".repeat(MAX_LAYOUT_BYTES)}).to_string(),
    ] {
        let mut invalid = original.clone();
        invalid.layout_json = document;
        assert!(matches!(
            database.save_workspace(invalid, 1, &owner),
            Err(RecordsError::Invalid(_))
        ));
        assert!(
            snapshot(&database, &project.id) == before,
            "Invalid layout input must not alter saved data."
        );
    }
    let mut unknown = original;
    unknown.schema_version = 999;
    assert!(matches!(
        database.save_workspace(unknown, 1, &owner),
        Err(RecordsError::UnsupportedLayout)
    ));
    assert!(
        snapshot(&database, &project.id) == before,
        "Unsupported layout version must remain untouched."
    );
}

#[test]
fn an_already_open_writer_refuses_future_schema_mutations_and_reopens_read_only() {
    let fixture = Fixture::new();
    let location = fixture.location();
    let database = Database::open(&location, None).unwrap();
    let project = database
        .select(&fixture.directory("projects/work"))
        .unwrap();
    let owner = owner("synthetic-start-a");
    let root = fixture.prepared(
        "source-one",
        1,
        "stores/one",
        RootKind::Box,
        RootScope::Primary,
        RootAccess::ReadWrite,
    );
    let root = database.register_store_root(&root).unwrap();
    let layout = workspace(&project.id, r#"{"axis":"columns","views":[]}"#);
    database.save_workspace(layout.clone(), 0, &owner).unwrap();
    let before = snapshot(&database, &project.id);
    let new_root = fixture.prepared(
        "source-two",
        1,
        "stores/two",
        RootKind::System,
        RootScope::Primary,
        RootAccess::ReadOnly,
    );
    let additional = fixture.prepared(
        "source-additional",
        1,
        "stores/additional",
        RootKind::System,
        RootScope::Additional,
        RootAccess::ReadOnly,
    );
    let next = fixture.prepared(
        "source-one-v2",
        2,
        "stores/one",
        RootKind::Box,
        RootScope::Primary,
        RootAccess::ReadWrite,
    );
    let raw = Connection::open(location.database_path()).unwrap();
    raw.execute_batch("PRAGMA user_version=999").unwrap();
    let is_future = |result: Result<(), RecordsError>| {
        assert!(
            matches!(
                result,
                Err(RecordsError::Database(DatabaseError::NewerSchema {
                    found: 999,
                    ..
                }))
            ),
            "The older open connection must refuse every write entry point."
        )
    };
    is_future(database.register_store_root(&new_root).map(|_| ()));
    is_future(
        database
            .register_additional_system_root(&additional)
            .map(|_| ()),
    );
    is_future(
        database
            .register_store_root_revision(&root.id, 1, &next)
            .map(|_| ()),
    );
    is_future(
        database
            .mark_store_root_unavailable(&root.id, 1)
            .map(|_| ()),
    );
    is_future(database.save_workspace(layout, 1, &owner).map(|_| ()));
    is_future(database.release_workspace(&project.id, "workspace-main", 1, &owner));
    assert_eq!(database.store_root(&root.id).unwrap(), Some(root.clone()));
    assert!(database.store_root("source-two").unwrap().is_none());
    assert!(database.store_root("source-additional").unwrap().is_none());
    assert!(
        snapshot(&database, &project.id) == before,
        "Schema refusal must preserve workspace data and owner."
    );
    assert_eq!(
        raw.pragma_query_value::<u32, _>(None, "user_version", |row| row.get(0))
            .unwrap(),
        999
    );
    drop(raw);
    drop(database);
    let reopened = Database::open(&location, None).unwrap();
    assert!(matches!(
        reopened.access(),
        DatabaseAccess::ReadOnlyNewer { found: 999, .. }
    ));
    assert_eq!(reopened.store_root(&root.id).unwrap(), Some(root));
    assert!(
        snapshot(&reopened, &project.id) == before,
        "Read-only reopen must not recreate authoritative records."
    );
}
