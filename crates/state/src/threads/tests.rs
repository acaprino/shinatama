use super::*;
use crate::{
    location::{LocationRequest, StorageLocation},
    records::{PreparedStoreRoot, RootAccess, RootKind, RootOrigin, RootScope, StoreRootRecord},
};
use std::fs;

#[test]
fn resume_target_keeps_thread_root_and_owner_identity_exact() {
    let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
    let fixture = parent.join(format!("shinatama-resume-target-{}", uuid::Uuid::new_v4()));
    let location = StorageLocation::resolve(LocationRequest {
        local_data_directory: parent.clone(),
        home_override: Some(fixture.clone()),
        project_roots: vec![],
        synchronized_roots: vec![],
    })
    .unwrap();
    location.prepare_private_directory().unwrap();
    let db = Database::open(&location, None).unwrap();
    db.lock().unwrap().execute("INSERT INTO project VALUES('p_aaaaaaaaaaaa','C:\\synthetic-project','synthetic-project','Project',10,1)", []).unwrap();
    let make_thread = |id: &str| ThreadRecord {
        id: id.into(),
        project_id: "p_aaaaaaaaaaaa".into(),
        kind: ThreadKind::Work,
        parent_thread_id: None,
        companion_worker_thread_id: None,
        title: None,
        archived: false,
        revision: 1,
        created_at: 10,
        updated_at: 10,
    };
    let first = make_thread("t_aaaaaaaaaaaa");
    let second = make_thread("t_bbbbbbbbbbbb");
    db.create_thread(&first).unwrap();
    db.create_thread(&second).unwrap();
    assert_eq!(
        db.thread_resume_target("t_cccccccccccc").unwrap_err(),
        ThreadResumeError::NotFound
    );
    assert_eq!(
        db.thread_resume_target(&first.id).unwrap_err(),
        ThreadResumeError::Unbound
    );
    let archive = location
        .resolve_related_directory(&location.directory().join("native-fixture"))
        .unwrap();
    archive.prepare_private_directory().unwrap();
    let make_root = |id: &str, revision| {
        PreparedStoreRoot::resolve(StoreRootRecord {
            id: id.into(),
            harness: "claude_code".into(),
            kind: RootKind::Box,
            scope: RootScope::Primary,
            resolved_path: archive
                .directory()
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .into(),
            origin: RootOrigin::Configured,
            config_revision: revision,
            access: RootAccess::ReadWrite,
            available: true,
        })
        .unwrap()
    };
    let first_root = make_root("fixture-root", 1);
    db.register_store_root(&first_root).unwrap();
    let new_root = make_root("fixture-root-v2", 2);
    db.register_store_root_revision("fixture-root", 1, &new_root)
        .unwrap();
    let make_segment = |id: &str, thread: &str, ordinal, native: &str| SegmentRecord {
        id: id.into(),
        thread_id: thread.into(),
        ordinal,
        harness: "claude_code".into(),
        native: NativeSessionKey {
            store_root_id: "fixture-root".into(),
            store_revision: 1,
            native_id: native.into(),
        },
        archived: false,
        revision: 1,
        created_at: 20,
    };
    let older = make_segment("s_aaaaaaaaaaaa", &first.id, 1, "first-old");
    let latest = make_segment("s_bbbbbbbbbbbb", &first.id, 2, "first-latest");
    let mut unrelated = make_segment("s_cccccccccccc", &second.id, 1, "first-latest");
    unrelated.native.store_root_id = "fixture-root-v2".into();
    unrelated.native.store_revision = 2;
    db.attach_thread_segment(&older, 1).unwrap();
    db.attach_thread_segment(&latest, 2).unwrap();
    db.attach_thread_segment(&unrelated, 1).unwrap();
    let target = db.thread_resume_target(&first.id).unwrap();
    assert_eq!(target.segment, latest);
    assert_eq!(target.root, *first_root.record());
    assert_eq!(target.recorded_activation, None);
    let owner = ActivationOwner {
        instance_id: "i_aaaaaaaaaaaa".into(),
        pid: 10,
        created_at_100ns: 100,
    };
    let active = db
        .activate_thread_segment(&older.id, 1, &owner, 30)
        .unwrap();
    let target = db.thread_resume_target(&first.id).unwrap();
    assert_eq!(target.segment, latest);
    assert_eq!(target.recorded_activation, Some(active.clone()));
    assert_eq!(
        db.thread_resume_target(&second.id)
            .unwrap()
            .recorded_activation,
        None
    );
    assert_eq!(
        db.thread_activation(&owner.instance_id).unwrap(),
        Some(active)
    );
    db.end_thread_activation(&owner, 1, 40).unwrap();
    assert_eq!(
        db.thread_resume_target(&first.id)
            .unwrap()
            .recorded_activation,
        None
    );

    // A reconnect creates a distinct root. Resolving the old segment must never
    // substitute that newer root, even though both have the same physical path.
    db.mark_store_root_unavailable("fixture-root", 1).unwrap();
    assert_eq!(
        db.thread_resume_target(&first.id).unwrap_err(),
        ThreadResumeError::RootUnavailable
    );
    assert_eq!(db.segment_record(&latest.id).unwrap(), Some(latest.clone()));
    assert!(db.store_root("fixture-root-v2").unwrap().unwrap().available);

    // Simulate a stale foreign-key binding written by an external/broken writer.
    // The read must report it, not repair or reinterpret the saved native ID.
    {
        let connection = db.lock().unwrap();
        connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
        connection
            .execute(
                "UPDATE segment SET store_revision=2 WHERE id=?1",
                [&latest.id],
            )
            .unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
    }
    assert_eq!(
        db.thread_resume_target(&first.id).unwrap_err(),
        ThreadResumeError::RootChanged
    );
    assert_eq!(
        db.segment_record(&latest.id)
            .unwrap()
            .unwrap()
            .native
            .store_revision,
        2
    );
    db.lock()
        .unwrap()
        .pragma_update(None, "user_version", 5)
        .unwrap();
    assert_eq!(
        db.thread_resume_target(&first.id).unwrap_err(),
        ThreadResumeError::UnsupportedSchema
    );
    assert_eq!(
        db.lock()
            .unwrap()
            .pragma_query_value::<u32, _>(None, "user_version", |row| row.get(0))
            .unwrap(),
        5
    );
    drop(db);
    drop(archive);
    drop(location);
    assert_eq!(fixture.parent(), Some(parent.as_path()));
    assert!(
        fixture
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("shinatama-resume-target-")
    );
    fs::remove_dir_all(fixture).unwrap();
}

#[test]
fn v3_upgrade_preserves_saved_work_and_thread_exclusivity_survives_reopen() {
    let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
    let fixture = parent.join(format!("shinatama-threads-{}", uuid::Uuid::new_v4()));
    let location = StorageLocation::resolve(LocationRequest {
        local_data_directory: parent.clone(),
        home_override: Some(fixture.clone()),
        project_roots: vec![],
        synchronized_roots: vec![],
    })
    .unwrap();
    location.prepare_private_directory().unwrap();
    let project = "p_aaaaaaaaaaaa";
    let connection = Connection::open(location.database_path()).unwrap();
    connection.execute_batch(crate::schema::CREATE_V1).unwrap();
    connection
        .execute_batch(crate::migrations::SETTING_JSON_INSERT_TRIGGER)
        .unwrap();
    connection
        .execute_batch(crate::migrations::SETTING_JSON_UPDATE_TRIGGER)
        .unwrap();
    for ddl in crate::records::schema::MIGRATION_V3 {
        connection.execute_batch(ddl).unwrap();
    }
    connection.execute("INSERT INTO project VALUES(?1,'C:\\synthetic-saved-project','saved-project','Saved project',10,1)",[project]).unwrap();
    connection
        .execute(
            "INSERT INTO setting VALUES('saved-marker','\"keep saved work\"')",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO workspace VALUES(?1,'main',1,1,'{\"saved\":true}',NULL,NULL,NULL,10)",
            [project],
        )
        .unwrap();
    connection.pragma_update(None, "user_version", 3).unwrap();
    drop(connection);

    let db = Database::open(&location, None).unwrap();
    assert_eq!(db.diagnostics().unwrap().schema_version, 4);
    {
        let connection = db.lock().unwrap();
        assert_eq!(
            connection
                .query_row::<String, _, _>(
                    "SELECT value FROM setting WHERE key='saved-marker'",
                    [],
                    |row| row.get(0)
                )
                .unwrap(),
            "\"keep saved work\""
        );
        assert_eq!(
            connection
                .query_row::<String, _, _>(
                    "SELECT layout_json FROM workspace WHERE project_id=?1",
                    [project],
                    |row| row.get(0)
                )
                .unwrap(),
            "{\"saved\":true}"
        );
        assert_eq!(
            connection
                .query_row::<i64, _, _>(
                    "SELECT count(*) FROM pragma_foreign_key_check",
                    [],
                    |row| row.get(0)
                )
                .unwrap(),
            0
        );
    }
    assert!(fs::read_dir(location.directory()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("v3-to-v4")
    }));
    let thread = ThreadRecord {
        id: "t_aaaaaaaaaaaa".into(),
        project_id: project.into(),
        kind: ThreadKind::Work,
        parent_thread_id: None,
        companion_worker_thread_id: None,
        title: Some("Saved thread".into()),
        archived: false,
        revision: 1,
        created_at: 10,
        updated_at: 10,
    };
    db.create_thread(&thread).unwrap();
    drop(db);
    let db = Database::open(&location, None).unwrap();
    let other = Database::open(&location, None).unwrap();
    assert_eq!(db.thread_record(&thread.id).unwrap().unwrap(), thread);
    let sidebar = db.thread_sidebar_snapshot().unwrap();
    assert_eq!(sidebar.revision, 0);
    db.save_thread_sidebar(
        0,
        SidebarState {
            version: 1,
            width: 320,
            collapsed: true,
        },
    )
    .unwrap();
    assert_eq!(other.thread_sidebar_snapshot().unwrap().settings.width, 320);
    assert!(matches!(
        other.save_thread_sidebar(0, SidebarState::default()),
        Err(ThreadError::Conflict)
    ));
    db.update_thread_metadata(&thread.id, 1, Some("Renamed thread"), false, 20)
        .unwrap();
    assert!(matches!(
        other.update_thread_metadata(&thread.id, 1, Some("Stale rename"), false, 21),
        Err(ThreadError::Conflict)
    ));
    let archive = location
        .resolve_related_directory(&location.directory().join("native-fixture"))
        .unwrap();
    archive.prepare_private_directory().unwrap();
    let root = PreparedStoreRoot::resolve(StoreRootRecord {
        id: "fixture-root".into(),
        harness: "claude_code".into(),
        kind: RootKind::Box,
        scope: RootScope::Primary,
        resolved_path: archive
            .directory()
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .into(),
        origin: RootOrigin::Configured,
        config_revision: 1,
        access: RootAccess::ReadWrite,
        available: true,
    })
    .unwrap();
    db.register_store_root(&root).unwrap();
    let segment = SegmentRecord {
        id: "s_aaaaaaaaaaaa".into(),
        thread_id: thread.id.clone(),
        ordinal: 1,
        harness: "claude_code".into(),
        native: NativeSessionKey {
            store_root_id: "fixture-root".into(),
            store_revision: 1,
            native_id: "synthetic-native-id".into(),
        },
        archived: false,
        revision: 1,
        created_at: 22,
    };
    db.attach_thread_segment(&segment, 2).unwrap();
    let first = ActivationOwner {
        instance_id: "i_aaaaaaaaaaaa".into(),
        pid: 10,
        created_at_100ns: 100,
    };
    let second = ActivationOwner {
        instance_id: "i_bbbbbbbbbbbb".into(),
        pid: 10,
        created_at_100ns: 200,
    };
    db.activate_thread_segment(&segment.id, 1, &first, 30)
        .unwrap();
    assert!(matches!(
        other.activate_thread_segment(&segment.id, 2, &second, 31),
        Err(ThreadError::OwnedElsewhere)
    ));
    let reused_pid = ActivationOwner {
        created_at_100ns: 101,
        ..first.clone()
    };
    assert!(matches!(
        other.end_thread_activation(&reused_pid, 1, 32),
        Err(ThreadError::OwnedElsewhere)
    ));
    assert_eq!(
        other.list_threads(project, None, 50).unwrap().rows[0].status,
        ThreadStatus::RecordedOpen
    );
    db.end_thread_activation(&first, 1, 40).unwrap();
    other
        .activate_thread_segment(&segment.id, 3, &second, 50)
        .unwrap();
    other.end_thread_activation(&second, 1, 60).unwrap();
    db.mark_store_root_unavailable("fixture-root", 1).unwrap();
    let page = db.list_threads(project, None, 50).unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].native_available, Some(false));
    assert_eq!(page.rows[0].status, ThreadStatus::Inactive);
    let record = db.thread_record(&thread.id).unwrap().unwrap();
    db.update_thread_metadata(
        &thread.id,
        record.revision,
        record.title.as_deref(),
        true,
        70,
    )
    .unwrap();
    assert_eq!(
        other.list_threads(project, None, 50).unwrap().rows[0].status,
        ThreadStatus::Archived
    );
    drop(other);
    drop(db);
    drop(archive);
    drop(location);
    assert_eq!(fixture.parent(), Some(parent.as_path()));
    assert!(
        fixture
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("shinatama-threads-")
    );
    fs::remove_dir_all(fixture).unwrap();
}
