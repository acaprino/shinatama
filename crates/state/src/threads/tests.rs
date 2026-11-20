use super::*;
use crate::{
    location::{LocationRequest, StorageLocation},
    records::{PreparedStoreRoot, RootAccess, RootKind, RootOrigin, RootScope, StoreRootRecord},
};
use std::fs;

#[test]
fn authenticated_native_switches_preserve_history_and_reject_stale_or_foreign_owners() {
    let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
    let fixture = parent.join(format!("shinatama-native-switch-{}", uuid::Uuid::new_v4()));
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
    let thread = ThreadRecord {
        id: "t_aaaaaaaaaaaa".into(),
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
    db.create_thread(&thread).unwrap();
    let archive = location
        .resolve_related_directory(&location.directory().join("native-fixture"))
        .unwrap();
    archive.prepare_private_directory().unwrap();
    let root_record = StoreRootRecord {
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
    };
    db.register_store_root(&PreparedStoreRoot::resolve(root_record.clone()).unwrap())
        .unwrap();
    let key_a = NativeSessionKey {
        store_root_id: root_record.id.clone(),
        store_revision: 1,
        native_id: "12cc3a11-45db-4f1e-89c8-40d5c6be1dd3".into(),
    };
    let key_b = NativeSessionKey {
        native_id: "22cc3a11-45db-4f1e-89c8-40d5c6be1dd3".into(),
        ..key_a.clone()
    };
    let owner_a = ActivationOwner {
        instance_id: "i_aaaaaaaaaaaa".into(),
        pid: 10,
        created_at_100ns: 100,
    };
    let owner_b = ActivationOwner {
        instance_id: "i_bbbbbbbbbbbb".into(),
        pid: 11,
        created_at_100ns: 200,
    };
    let other = Database::open(&location, None).unwrap();
    let first_request = NativeSessionStart {
        thread_id: &thread.id,
        expected_thread_revision: 1,
        candidate_segment_id: "s_aaaaaaaaaaaa",
        harness: "claude_code",
        native: &key_a,
        owner: &owner_a,
        expected_activation: None,
        observed_at: 20,
    };
    let competing_request = NativeSessionStart {
        owner: &owner_b,
        candidate_segment_id: "s_dddddddddddd",
        ..first_request
    };
    let barrier = std::sync::Barrier::new(2);
    let (first, competing) = std::thread::scope(|scope| {
        let left = scope.spawn(|| {
            barrier.wait();
            db.observe_native_session_start(&first_request)
        });
        let right = scope.spawn(|| {
            barrier.wait();
            other.observe_native_session_start(&competing_request)
        });
        (left.join().unwrap(), right.join().unwrap())
    });
    let (a, owner, rejected_owner) = match (first, competing) {
        (Ok(active), Err(_)) => (active, owner_a, owner_b),
        (Err(_), Ok(active)) => (active, owner_b, owner_a),
        result => panic!("Exactly one competing writer must acquire the session: {result:?}"),
    };
    assert_eq!(a.thread.revision, 2);
    assert_eq!(a.activation.revision, 1);
    assert_eq!(a.segment.ordinal, 1);
    let repeat_request = NativeSessionStart {
        thread_id: &thread.id,
        expected_thread_revision: a.thread.revision,
        candidate_segment_id: "s_eeeeeeeeeeee",
        harness: "claude_code",
        native: &key_a,
        owner: &owner,
        expected_activation: Some(a.activation.version()),
        observed_at: 21,
    };
    let repeated = db.observe_native_session_start(&repeat_request).unwrap();
    assert!(!repeated.changed);
    assert_eq!(repeated.activation, a.activation);
    assert_eq!(repeated.thread, a.thread);
    assert!(matches!(
        other.observe_native_session_start(&NativeSessionStart {
            owner: &rejected_owner,
            expected_activation: None,
            ..repeat_request
        }),
        Err(ThreadError::OwnedElsewhere)
    ));
    let reused_pid = ActivationOwner {
        created_at_100ns: owner.created_at_100ns + 1,
        ..owner.clone()
    };
    assert!(matches!(
        db.observe_native_session_start(&NativeSessionStart {
            owner: &reused_pid,
            ..repeat_request
        }),
        Err(ThreadError::OwnedElsewhere)
    ));

    let replacement = PreparedStoreRoot::resolve(StoreRootRecord {
        id: "fixture-root-v2".into(),
        config_revision: 2,
        ..root_record.clone()
    })
    .unwrap();
    db.register_store_root_revision(&root_record.id, 1, &replacement)
        .unwrap();
    let wrong_root = NativeSessionKey {
        store_root_id: replacement.record().id.clone(),
        store_revision: 2,
        ..key_b.clone()
    };
    assert!(matches!(
        db.observe_native_session_start(&NativeSessionStart {
            native: &wrong_root,
            ..repeat_request
        }),
        Err(ThreadError::RootUnavailable)
    ));
    let b = db
        .observe_native_session_start(&NativeSessionStart {
            native: &key_b,
            candidate_segment_id: "s_bbbbbbbbbbbb",
            observed_at: 30,
            ..repeat_request
        })
        .unwrap();
    assert_eq!(
        b.previous_activation.as_ref().unwrap().sequence,
        a.activation.sequence
    );
    assert_eq!(b.previous_activation.as_ref().unwrap().ended_at, Some(30));
    assert!(b.activation.sequence > a.activation.sequence);
    assert_eq!(b.activation.revision, 3);
    assert_eq!(b.segment.ordinal, 2);
    assert!(matches!(
        other.observe_native_session_start(&repeat_request),
        Err(ThreadError::Conflict)
    ));
    assert!(matches!(
        other.end_thread_activation(&owner, a.activation.revision, 31),
        Err(ThreadError::Conflict)
    ));
    assert!(matches!(
        other.end_thread_activation_exact(
            &owner,
            ActivationVersion {
                sequence: a.activation.sequence,
                revision: b.activation.revision,
            },
            31
        ),
        Err(ThreadError::Conflict)
    ));

    let foreign_thread = ThreadRecord {
        id: "t_bbbbbbbbbbbb".into(),
        ..thread.clone()
    };
    db.create_thread(&foreign_thread).unwrap();
    let foreign_key = NativeSessionKey {
        native_id: "32cc3a11-45db-4f1e-89c8-40d5c6be1dd3".into(),
        ..key_a.clone()
    };
    db.attach_thread_segment(
        &SegmentRecord {
            id: "s_cccccccccccc".into(),
            thread_id: foreign_thread.id,
            ordinal: 1,
            harness: "claude_code".into(),
            native: foreign_key.clone(),
            archived: false,
            revision: 1,
            created_at: 20,
        },
        1,
    )
    .unwrap();
    let switch_back = NativeSessionStart {
        expected_thread_revision: b.thread.revision,
        expected_activation: Some(b.activation.version()),
        observed_at: 40,
        ..repeat_request
    };
    assert!(matches!(
        other.observe_native_session_start(&NativeSessionStart {
            native: &foreign_key,
            ..switch_back
        }),
        Err(ThreadError::OwnedElsewhere)
    ));
    assert_eq!(
        db.thread_activation(&owner.instance_id).unwrap(),
        Some(b.activation.clone())
    );
    let back = db.observe_native_session_start(&switch_back).unwrap();
    assert_eq!(back.segment.id, a.segment.id);
    assert_eq!(back.segment.ordinal, 1);
    assert_eq!(back.activation.revision, 5);
    assert!(back.activation.sequence > b.activation.sequence);
    assert_eq!(
        back.previous_activation.as_ref().unwrap().ended_at,
        Some(40)
    );
    assert_eq!(
        db.thread_resume_target(&thread.id).unwrap().segment.id,
        a.segment.id
    );
    {
        let connection = db.lock().unwrap();
        let mut query = connection.prepare("SELECT native_id,started_at,ended_at,revision FROM activation WHERE instance_id=?1 ORDER BY activation_sequence").unwrap();
        let history = query
            .query_map([&owner.instance_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            history,
            vec![
                (key_a.native_id.clone(), 20, Some(30), 2),
                (key_b.native_id.clone(), 30, Some(40), 4),
                (key_a.native_id.clone(), 40, None, 5)
            ]
        );
        assert_eq!(
            connection
                .query_row::<i64, _, _>(
                    "SELECT count(*) FROM segment WHERE thread_id=?1",
                    [&thread.id],
                    |row| row.get(0)
                )
                .unwrap(),
            2
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
    let ended = db
        .end_thread_activation_exact(&owner, back.activation.version(), 50)
        .unwrap();
    assert_eq!(ended.sequence, back.activation.sequence);
    assert_eq!(ended.ended_at, Some(50));
    let saved = db.thread_record(&thread.id).unwrap().unwrap();
    assert!(matches!(
        db.observe_native_session_start(&NativeSessionStart {
            expected_thread_revision: saved.revision,
            expected_activation: Some(ended.version()),
            observed_at: 51,
            ..switch_back
        }),
        Err(ThreadError::Conflict)
    ));
    drop(other);
    drop(db);
    let reopened = Database::open(&location, None).unwrap();
    let target = reopened.thread_resume_target(&thread.id).unwrap();
    assert_eq!(target.segment.id, a.segment.id);
    assert_eq!(target.recorded_activation, None);
    drop(reopened);
    drop(archive);
    drop(location);
    assert_eq!(fixture.parent(), Some(parent.as_path()));
    assert!(
        fixture
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("shinatama-native-switch-")
    );
    fs::remove_dir_all(fixture).unwrap();
}

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
    assert_eq!(target.segment.id, older.id);
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
                [&older.id],
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
        db.segment_record(&older.id)
            .unwrap()
            .unwrap()
            .native
            .store_revision,
        2
    );
    db.lock()
        .unwrap()
        .pragma_update(None, "user_version", crate::schema::CURRENT_VERSION + 1)
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
        crate::schema::CURRENT_VERSION + 1
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
    assert_eq!(
        db.diagnostics().unwrap().schema_version,
        crate::schema::CURRENT_VERSION
    );
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
            active_projects: vec![thread.project_id.clone()],
            hidden: true,
        },
    )
    .unwrap();
    let saved = other.thread_sidebar_snapshot().unwrap().settings;
    assert_eq!(saved.width, 320);
    assert!(saved.hidden, "a sidebar closed altogether stays closed");
    // An older save has no `hidden`: it reads as shown; a shown sidebar writes none, as before.
    let older: SidebarState = serde_json::from_str(r#"{"version":1,"width":264,"collapsed":false}"#).unwrap();
    assert!(!older.hidden);
    assert!(!serde_json::to_string(&SidebarState::default()).unwrap().contains("hidden"));
    assert_eq!(saved.active_projects, vec![thread.project_id.clone()]);
    for active_projects in [vec!["not-a-project".to_owned()], vec![thread.project_id.clone(); 2]] {
        assert!(matches!(
            db.save_thread_sidebar(1, SidebarState { active_projects, ..SidebarState::default() }),
            Err(ThreadError::Invalid)
        ));
    }
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
        other.list_threads(project, None, 50, false).unwrap().rows[0].status,
        ThreadStatus::RecordedOpen
    );
    db.end_thread_activation(&first, 1, 40).unwrap();
    other
        .activate_thread_segment(&segment.id, 3, &second, 50)
        .unwrap();
    other.end_thread_activation(&second, 1, 60).unwrap();
    db.mark_store_root_unavailable("fixture-root", 1).unwrap();
    let page = db.list_threads(project, None, 50, false).unwrap();
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
    assert!(other.list_threads(project, None, 50, false).unwrap().rows.is_empty(), "archived threads leave the default list");
    assert_eq!(
        other.list_threads(project, None, 50, true).unwrap().rows[0].status,
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

#[test]
fn forget_removes_a_thread_with_its_companion_hops_index_and_summaries_only() {
    use crate::forget::{ForgetError, forget_thread};
    use crate::hops::{HopJournal, HopMode, HopPlan, HopState, HopStrategy};
    use crate::search_index::{IndexedDocument, SearchIndex};
    let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
    let fixture = parent.join(format!("shinatama-forget-{}", uuid::Uuid::new_v4()));
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
    let thread = |id: &str, kind, parent: Option<&str>, worker: Option<&str>| ThreadRecord {
        id: id.into(),
        project_id: "p_aaaaaaaaaaaa".into(),
        kind,
        parent_thread_id: parent.map(Into::into),
        companion_worker_thread_id: worker.map(Into::into),
        title: None,
        archived: false,
        revision: 1,
        created_at: 10,
        updated_at: 10,
    };
    let (work, companion, fork, unrelated) = ("t_wwwwwwwwwwww", "t_cccccccccccc", "t_ffffffffffff", "t_uuuuuuuuuuuu");
    db.create_thread(&thread(work, ThreadKind::Work, None, None)).unwrap();
    db.create_thread(&thread(companion, ThreadKind::Companion, None, Some(work))).unwrap();
    db.create_thread(&thread(fork, ThreadKind::Work, Some(work), None)).unwrap();
    db.create_thread(&thread(unrelated, ThreadKind::Work, None, None)).unwrap();
    let archive = location.resolve_related_directory(&location.directory().join("native-fixture")).unwrap();
    archive.prepare_private_directory().unwrap();
    db.register_store_root(&PreparedStoreRoot::resolve(StoreRootRecord {
        id: "fixture-root".into(),
        harness: "claude_code".into(),
        kind: RootKind::Box,
        scope: RootScope::Primary,
        resolved_path: archive.directory().to_string_lossy().trim_start_matches(r"\?\").into(),
        origin: RootOrigin::Configured,
        config_revision: 1,
        access: RootAccess::ReadWrite,
        available: true,
    })
    .unwrap())
    .unwrap();
    let segment = |id: &str, thread: &str, native: &str| SegmentRecord {
        id: id.into(),
        thread_id: thread.into(),
        ordinal: 1,
        harness: "claude_code".into(),
        native: NativeSessionKey { store_root_id: "fixture-root".into(), store_revision: 1, native_id: native.into() },
        archived: false,
        revision: 1,
        created_at: 22,
    };
    db.attach_thread_segment(&segment("s_wwwwwwwwwwww", work, "native-w"), 1).unwrap();
    db.attach_thread_segment(&segment("s_uuuuuuuuuuuu", unrelated, "native-u"), 1).unwrap();
    let owner = ActivationOwner { instance_id: "i_aaaaaaaaaaaa".into(), pid: 10, created_at_100ns: 100 };
    db.activate_thread_segment("s_wwwwwwwwwwww", 1, &owner, 30).unwrap();
    assert!(matches!(forget_thread(&location, &db, work, None, 35), Err(ForgetError::Open)));
    db.end_thread_activation(&owner, 1, 40).unwrap();
    let shown = db.thread_record(work).unwrap().unwrap();
    let archived = db.update_thread_metadata(work, shown.revision, None, true, 37).unwrap();
    let natives = db.segment_native_ids().unwrap();
    assert!(natives.contains("native-w") && natives.contains("native-u"), "an archived thread still has its sessions");
    assert!(matches!(forget_thread(&location, &db, work, Some(shown.revision), 38), Err(ForgetError::Changed)), "a stale window cannot forget");

    let journal = HopJournal::open(&location).unwrap();
    let handoff = location.directory().join("handoffs").join("1".repeat(32));
    fs::create_dir_all(&handoff).unwrap();
    fs::write(handoff.join("seed.md"), "seed").unwrap();
    fs::write(handoff.join("full.md"), "full").unwrap();
    // The desktop keys a hop's source by session (hop_run.rs source_key), not by thread id.
    let hop = |id: char, native: &str| HopPlan {
        id: std::iter::repeat_n(id, 32).collect(),
        thread_id: format!("session:{native}"),
        destination_thread_id: None,
        mode: HopMode::Close,
        strategy: HopStrategy::SeedPrompt,
        source_agent: "claude_code".into(),
        source_native_id: native.into(),
        source_instance: None,
        destination_agent: "codex".into(),
        nonce: "abc123".into(),
        snapshot_sha256: "a".repeat(64),
        bootstrap_sha256: "b".repeat(64),
        seed_path: location.directory().join("handoffs").join(std::iter::repeat_n(id, 32).collect::<String>()).join("seed.md").display().to_string(),
    };
    let planned = journal.plan(&hop('1', "native-w"), 41).unwrap();
    journal.plan(&hop('2', "native-u"), 41).unwrap();
    // A finished fork from another thread into this one: the snapshot is the source's text.
    let into = journal.plan(&HopPlan { mode: HopMode::Fork, destination_thread_id: Some(work.into()), ..hop('3', "native-x") }, 41).unwrap();
    journal.transition(&into.plan.id, HopState::Planned, into.revision, HopState::Aborted, None, 41).unwrap();
    let index = SearchIndex::open(&location).unwrap();
    let document = |native| IndexedDocument { root_id: "fixture-root", native_id: native, harness: "claude_code", title: None, working_directory: None, updated_at: None, signature: "1", body: "secret words" };
    index.upsert(&document("native-w")).unwrap();
    index.upsert(&document("native-u")).unwrap();
    db.set_summaries_excluded(work, true).unwrap();
    db.set_summaries_excluded(unrelated, true).unwrap();
    assert!(matches!(forget_thread(&location, &db, work, Some(archived.revision), 42), Err(ForgetError::HopInProgress)));
    assert!(handoff.join("seed.md").exists(), "a refused forget changes nothing");
    assert!(db.summaries_excluded().unwrap().contains(&work.to_owned()), "a refused forget keeps the summaries pause");
    assert!(!index.locate("native-w").unwrap().is_empty(), "a refused forget keeps the index");
    journal.transition(&planned.plan.id, HopState::Planned, planned.revision, HopState::Aborted, None, 43).unwrap();

    let report = forget_thread(&location, &db, work, Some(archived.revision), 50).unwrap();
    let natives = db.segment_native_ids().unwrap();
    assert!(!natives.contains("native-w") && natives.contains("native-u"), "a forgotten thread has no session left");
    assert_eq!(report.thread_ids, [work, companion]);
    assert_eq!((report.hops, report.handoff_files, report.index_rows), (1, 2, 1));
    assert!(db.thread_record(work).unwrap().is_none() && db.thread_record(companion).unwrap().is_none());
    let forked = db.thread_record(fork).unwrap().unwrap();
    assert_eq!((forked.parent_thread_id, forked.revision), (None, 2), "a fork keeps its record and loses only the lineage pointer");
    assert!(db.thread_record(unrelated).unwrap().is_some());
    assert!(db.segment_record("s_uuuuuuuuuuuu").unwrap().is_some() && db.segment_record("s_wwwwwwwwwwww").unwrap().is_none());
    assert_eq!(db.summaries_excluded().unwrap(), [unrelated]);
    assert!(journal.hop(&planned.plan.id).unwrap().is_none() && journal.hop(&"2".repeat(32)).unwrap().is_some());
    assert!(journal.hop(&into.plan.id).unwrap().is_some(), "a hop into the forgotten thread stays with its source");
    assert!(!handoff.exists());
    assert!(index.locate("native-w").unwrap().is_empty() && !index.locate("native-u").unwrap().is_empty());
    assert!(!index.needs_index("fixture-root", "native-w", "2").unwrap(), "a forgotten session is not indexed again");
    index.upsert(&document("native-w")).unwrap();
    assert!(index.locate("native-w").unwrap().is_empty());
    assert!(matches!(forget_thread(&location, &db, work, None, 60), Err(ForgetError::NotFound)));
    drop((journal, index, db));
    let _ = fs::remove_dir_all(fixture);
}

/// 2026-11-18: ten refused New opens each left a thread that never ran ("Nothing was saved in
/// this session"). The undo removes exactly what the New created, and nothing that ran.
#[test]
fn discarding_an_unstarted_thread_removes_it_and_its_own_workset_only() {
    let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
    let fixture = parent.join(format!("shinatama-discard-{}", uuid::Uuid::new_v4()));
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
    let thread = |id: &str, parent: Option<&str>| ThreadRecord {
        id: id.into(),
        project_id: "p_aaaaaaaaaaaa".into(),
        kind: ThreadKind::Work,
        parent_thread_id: parent.map(Into::into),
        companion_worker_thread_id: None,
        title: Some("Claude Code".into()),
        archived: false,
        revision: 1,
        created_at: 10,
        updated_at: 10,
    };
    let archive = location.resolve_related_directory(&location.directory().join("native-fixture")).unwrap();
    archive.prepare_private_directory().unwrap();
    db.register_store_root(&PreparedStoreRoot::resolve(StoreRootRecord {
        id: "fixture-root".into(),
        harness: "opencode".into(),
        kind: RootKind::Box,
        scope: RootScope::Primary,
        resolved_path: archive.directory().to_string_lossy().trim_start_matches(r"\?\").into(),
        origin: RootOrigin::Configured,
        config_revision: 1,
        access: RootAccess::ReadWrite,
        available: true,
    })
    .unwrap())
    .unwrap();
    let segment = |id: &str, thread: &str, native: &str| SegmentRecord {
        id: id.into(),
        thread_id: thread.into(),
        ordinal: 1,
        harness: "opencode".into(),
        native: NativeSessionKey { store_root_id: "fixture-root".into(), store_revision: 1, native_id: native.into() },
        archived: false,
        revision: 1,
        created_at: 22,
    };
    let workset_count = |db: &Database| -> i64 { db.lock().unwrap().query_row("SELECT count(*) FROM workset", [], |row| row.get(0)).unwrap() };

    // A New without a chosen thread: its own workset goes with it.
    let alone = "t_aaaaaaaaaaaa";
    db.create_thread(&thread(alone, None)).unwrap();
    assert_eq!(db.workset_of_thread(alone).unwrap().as_deref(), Some("w_aaaaaaaaaaaa"));
    db.discard_unstarted_thread(alone).unwrap();
    assert!(db.thread_record(alone).unwrap().is_none());
    assert!(db.workset_record("w_aaaaaaaaaaaa").unwrap().is_none());
    assert_eq!(workset_count(&db), 0);

    // A New in the thread the user has open: that thread and its other session stay.
    let shared = db.create_workset("p_aaaaaaaaaaaa", None, 10).unwrap();
    let (kept, joined) = ("t_bbbbbbbbbbbb", "t_cccccccccccc");
    db.create_thread_in(&thread(kept, None), Some(&shared.id)).unwrap();
    db.create_thread_in(&thread(joined, None), Some(&shared.id)).unwrap();
    // OpenCode and Pi name their session before the start: that segment goes too.
    db.attach_thread_segment(&segment("s_cccccccccccc", joined, "native-c"), 1).unwrap();
    db.discard_unstarted_thread(joined).unwrap();
    assert!(db.thread_record(joined).unwrap().is_none());
    assert!(db.segment_record("s_cccccccccccc").unwrap().is_none());
    assert_eq!(db.workset_record(&shared.id).unwrap().unwrap(), shared);
    assert_eq!(db.workset_of_thread(kept).unwrap().as_deref(), Some(shared.id.as_str()));

    // A thread whose session ran, even once and ended, is never discarded.
    let ran = "t_dddddddddddd";
    db.create_thread(&thread(ran, None)).unwrap();
    db.attach_thread_segment(&segment("s_dddddddddddd", ran, "native-d"), 1).unwrap();
    let owner = ActivationOwner { instance_id: "i_aaaaaaaaaaaa".into(), pid: 10, created_at_100ns: 100 };
    db.activate_thread_segment("s_dddddddddddd", 1, &owner, 30).unwrap();
    assert!(matches!(db.discard_unstarted_thread(ran), Err(ThreadError::OwnedElsewhere)));
    let open = db.thread_record(ran).unwrap().unwrap();
    db.end_thread_activation(&owner, 1, 40).unwrap();
    assert!(matches!(db.discard_unstarted_thread(ran), Err(ThreadError::OwnedElsewhere)));
    assert!(db.thread_record(ran).unwrap().unwrap().revision > open.revision);
    assert!(db.segment_record("s_dddddddddddd").unwrap().is_some());

    // Nor one another thread descends from.
    db.create_thread(&thread("t_eeeeeeeeeeee", Some(kept))).unwrap();
    assert!(matches!(db.discard_unstarted_thread(kept), Err(ThreadError::OwnedElsewhere)));
    assert!(db.thread_record(kept).unwrap().is_some());
    assert!(matches!(db.discard_unstarted_thread(alone), Err(ThreadError::NotFound)));
    drop(db);
    let _ = fs::remove_dir_all(fixture);
}
