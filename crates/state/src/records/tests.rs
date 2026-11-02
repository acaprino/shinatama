use super::*;
use std::collections::HashSet;

fn workspace() -> WorkspaceRecord {
    WorkspaceRecord {
        project_id: "p_aaaaaaaaaaaa".into(),
        workspace_id: "main".into(),
        revision: 1,
        schema_version: 1,
        layout_json: r#"{"root":{"kind":"tab_group","tabs":[]}}"#.into(),
        owner: Some(WorkspaceOwner {
            window_id: "window-A".into(),
            pid: 7,
            start_time: "FILETIME:133801234567890123".into(),
        }),
        updated_at: 1_793_577_600_000,
    }
}

#[test]
fn canonical_ids_have_exact_prefix_length_and_random_base32_alphabet() {
    let mut seen = HashSet::new();
    for kind in [
        CanonicalIdKind::Project,
        CanonicalIdKind::Thread,
        CanonicalIdKind::Segment,
        CanonicalIdKind::Hop,
        CanonicalIdKind::Instance,
        CanonicalIdKind::Roundtable,
        CanonicalIdKind::Message,
        CanonicalIdKind::BrowserSession,
    ] {
        for _ in 0..32 {
            let id = new_id(kind);
            validate_id(&id, kind).unwrap();
            assert!(seen.insert(id));
        }
    }
    for invalid in [
        "t_aaaaaaaaaaa",
        "t_aaaaaaaaaaaaa",
        "t_AAAAAAAAAAAA",
        "t_aaaaaaaaaaa0",
        "s_aaaaaaaaaaaa",
    ] {
        assert!(validate_id(invalid, CanonicalIdKind::Thread).is_err());
    }
    // No prefix for root/workspace/window is fabricated by this API.
    validate_key("root supplied by catalog").unwrap();
}

#[test]
fn native_identity_is_scoped_by_binding_and_configuration_revision() {
    let first = NativeSessionKey {
        store_root_id: "configured-A".into(),
        store_revision: 1,
        native_id: "same-native-id".into(),
    };
    let mut second = first.clone();
    second.store_root_id = "configured-B".into();
    let mut next_revision = first.clone();
    next_revision.store_revision = 2;
    for key in [&first, &second, &next_revision] {
        key.validate().unwrap();
    }
    assert_eq!(HashSet::from([first, second, next_revision]).len(), 3);
}

#[test]
fn companion_worker_link_and_hop_lineage_are_distinct_and_can_coexist() {
    let mut relation = ThreadRelations {
        id: "t_aaaaaaaaaaaa".into(),
        kind: ThreadKind::Companion,
        parent_thread_id: None,
        companion_worker_thread_id: Some("t_bbbbbbbbbbbb".into()),
    };
    relation.validate().unwrap();
    relation.parent_thread_id = Some("t_cccccccccccc".into());
    relation.validate().unwrap();
    relation.companion_worker_thread_id = None;
    assert!(
        relation.validate().is_err(),
        "a hop parent never substitutes for the worker link"
    );
    relation.kind = ThreadKind::Work;
    relation.validate().unwrap();
    relation.companion_worker_thread_id = Some("t_bbbbbbbbbbbb".into());
    assert!(relation.validate().is_err());
    relation.kind = ThreadKind::Companion;
    relation.companion_worker_thread_id = Some(relation.id.clone());
    assert!(relation.validate().is_err());
}

#[test]
fn restoration_contains_only_saved_data_and_rejects_runtime_or_unknown_payloads() {
    let record = workspace();
    let restored = record.restore_descriptors().unwrap();
    let WorkspaceRestore {
        project_id,
        workspace_id,
        revision,
        schema_version,
        layout_json,
    } = restored;
    assert_eq!(
        (
            project_id.as_str(),
            workspace_id.as_str(),
            revision,
            schema_version
        ),
        ("p_aaaaaaaaaaaa", "main", 1, 1)
    );
    assert!(!layout_json.contains("FILETIME"));
    for payload in [
        r#"{"root":{"pid":12}}"#,
        r#"{"root":{"argv":["synthetic"]}}"#,
        r#"{"credentials":"SYNTHETIC_NOT_A_SECRET"}"#,
    ] {
        let mut bad = record.clone();
        bad.layout_json = payload.into();
        let error = bad.restore_descriptors().unwrap_err();
        assert!(!format!("{error:?}").contains("SYNTHETIC_NOT_A_SECRET"));
    }
    let mut future = record;
    future.schema_version = 99;
    assert!(matches!(
        future.restore_descriptors(),
        Err(RecordsError::UnsupportedLayout)
    ));
}

#[test]
fn timestamps_are_integer_unix_milliseconds_and_layouts_have_finite_budgets() {
    let mut record = workspace();
    record.updated_at = -1;
    assert!(record.validate().is_err());
    record.updated_at = 0;
    record.validate().unwrap();
    let mut value = serde_json::to_value(&record).unwrap();
    value["updated_at"] = serde_json::json!(0.5);
    assert!(serde_json::from_value::<WorkspaceRecord>(value).is_err());
    record.layout_json = format!("{{\"title\":\"{}\"}}", "x".repeat(MAX_LAYOUT_BYTES));
    assert!(record.validate().is_err());
    record.layout_json = format!("{{\"root\":{}}}", "[".repeat(40) + "0" + &"]".repeat(40));
    assert!(record.validate().is_err());
}

#[test]
fn operational_classification_never_grants_cleanup_authority_over_saved_history() {
    for name in [
        "project",
        "store_root",
        "workspace",
        "thread",
        "snapshot",
        "setting",
        "learning_memory",
    ] {
        assert_eq!(table_authority(name), TableAuthority::Authoritative);
    }
    for name in [
        "instance",
        "event",
        "summary_log",
        "session_index",
        "session_text",
    ] {
        assert_eq!(table_authority(name), TableAuthority::Operational);
    }
    assert_eq!(
        table_authority("thread_task"),
        TableAuthority::DerivedDeferred
    );
    assert_eq!(table_authority("future_queue"), TableAuthority::Unknown);
    assert_eq!(schema::OBJECTS.len(), 6);
    assert!(
        schema::OBJECTS
            .iter()
            .all(|(name, ddl)| name.len() <= 64 && ddl.len() <= 4096)
    );
    assert!(
        schema::OBJECTS
            .iter()
            .all(|(name, _)| !schema::OPERATIONAL_NAMES.contains(name))
    );
}

#[test]
fn proposed_sql_preserves_root_bindings_and_requires_a_complete_workspace_owner() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE project(id TEXT PRIMARY KEY) STRICT; INSERT INTO project(id) VALUES('p_aaaaaaaaaaaa');").unwrap();
    for statement in schema::MIGRATION_V3 {
        connection.execute_batch(statement).unwrap();
    }
    connection
        .execute(
            "INSERT INTO store_root VALUES(?1,'codex','box','primary',?2,'configured',1,'read_write',1,NULL)",
            params![
                "root-A",
                if cfg!(windows) {
                    r"C:\private\home"
                } else {
                    "/private/home"
                }
            ],
        )
        .unwrap();
    assert!(connection.execute("INSERT INTO store_root VALUES('duplicate-primary','codex','box','primary','other','configured',1,'read_write',1,NULL)",[]).is_err());
    assert!(connection.execute("INSERT INTO store_root VALUES('additional-rw','codex','system','additional','other','configured',1,'read_write',1,NULL)",[]).is_err());
    assert!(
        connection
            .execute(
                "UPDATE store_root SET resolved_path='different' WHERE id='root-A'",
                []
            )
            .is_err()
    );
    connection
        .execute("UPDATE store_root SET available=0 WHERE id='root-A'", [])
        .unwrap();
    assert!(
        connection
            .execute("UPDATE store_root SET available=1 WHERE id='root-A'", [])
            .is_err()
    );
    assert_eq!(
        connection
            .query_row::<i64, _, _>(
                "SELECT count(*) FROM store_root WHERE id='root-A' AND available=0",
                [],
                |row| row.get(0)
            )
            .unwrap(),
        1
    );
    // Reconnect preserves the unavailable predecessor, while the UNIQUE parent
    // link consumes its CAS even for an attempted differently numbered successor.
    connection.execute("INSERT INTO store_root VALUES('root-next','codex','box','primary','next','configured',2,'read_write',1,'root-A')",[]).unwrap();
    assert!(connection.execute("INSERT INTO store_root VALUES('second-successor','codex','box','primary','other','configured',3,'read_write',1,'root-A')",[]).is_err());
    assert!(
        connection
            .execute(
                "UPDATE store_root SET scope='additional' WHERE id='root-next'",
                []
            )
            .is_err()
    );
    assert!(connection.execute("INSERT INTO workspace VALUES('p_aaaaaaaaaaaa','main',1,1,'{}','window-A',NULL,'FILETIME:1',0)",[]).is_err());
    connection.execute("INSERT INTO workspace VALUES('p_aaaaaaaaaaaa','main',1,1,'{}','window-A',7,'FILETIME:1',0)",[]).unwrap();
    assert!(
        connection
            .execute("UPDATE workspace SET owner_window_id='window-B'", [])
            .is_err()
    );
    connection
        .execute(
            "UPDATE workspace SET owner_window_id=NULL,owner_pid=NULL,owner_start_time=NULL",
            [],
        )
        .unwrap();
    connection.execute("UPDATE workspace SET owner_window_id='window-B',owner_pid=8,owner_start_time='FILETIME:2'",[]).unwrap();
}
