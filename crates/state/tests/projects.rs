#![cfg(windows)]

use shinatama_state::database::Database;
use shinatama_state::location::{LocationRequest, StorageLocation};
use shinatama_state::projects::{
    CatalogEngine, CatalogRequest, CatalogResponse, DatabaseLoader, RootInput, RootState,
    RootsSetRequest,
};
use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, UNIX_EPOCH},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "shinatama-projects-catalog-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn folder(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn database(&self) -> (Arc<Database>, StorageLocation) {
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: self.0.clone(),
            home_override: Some(self.0.join("private-state")),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        })
        .unwrap();
        (Arc::new(Database::open(&location, None).unwrap()), location)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn root(path: PathBuf) -> RootInput {
    RootInput {
        id: None,
        path: path.to_string_lossy().into_owned(),
    }
}
fn loader(database: &Arc<Database>) -> DatabaseLoader {
    let database = database.clone();
    Arc::new(move || Ok(database.clone()))
}
fn refresh(engine: &Arc<CatalogEngine>, database: &Arc<Database>) -> CatalogResponse {
    engine.read(
        loader(database),
        CatalogRequest::Refresh {
            filter: String::new(),
            offset: 0,
            limit: Some(100),
        },
    )
}
fn finished(engine: &CatalogEngine) -> CatalogResponse {
    let start = Instant::now();
    loop {
        let response = engine.snapshot(None, "", 0, 100);
        if !response.scanning {
            assert!(response.error.is_none(), "{:?}", response.error);
            return response;
        }
        assert!(start.elapsed() < Duration::from_secs(15));
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn roots_and_default_are_explicit_persistent_and_lost_updates_are_rejected() {
    let fixture = Fixture::new();
    let (database, location) = fixture.database();
    let first = fixture.folder("roots/first");
    let second = fixture.folder("roots/second");
    let initial = database.project_manager_read().unwrap();
    assert_eq!(initial.revision, 0);
    assert!(initial.roots.is_empty());
    assert!(initial.default_root_id.is_none());
    let config = database
        .project_roots_set(RootsSetRequest {
            expected_revision: 0,
            roots: vec![root(first), root(second.clone())],
            default_root_index: None,
        })
        .unwrap();
    assert!(config.default_root_id.is_none());
    assert_eq!(config.roots.len(), 2);
    assert!(
        database
            .project_roots_set(RootsSetRequest {
                expected_revision: 0,
                roots: vec![],
                default_root_index: None
            })
            .is_err()
    );
    let config = database
        .project_roots_set(RootsSetRequest {
            expected_revision: config.revision,
            roots: config
                .roots
                .iter()
                .map(|root| RootInput {
                    id: Some(root.id.clone()),
                    path: root.path.clone(),
                })
                .collect(),
            default_root_index: Some(1),
        })
        .unwrap();
    assert_eq!(
        config.default_root_id.as_deref(),
        Some(config.roots[1].id.as_str())
    );
    drop(database);
    let reopened = Database::open(&location, None).unwrap();
    assert_eq!(reopened.project_manager_read().unwrap(), config);
    fs::remove_dir(second).unwrap();
    let retained = reopened
        .project_roots_set(RootsSetRequest {
            expected_revision: config.revision,
            roots: config
                .roots
                .iter()
                .map(|root| RootInput {
                    id: Some(root.id.clone()),
                    path: root.path.clone(),
                })
                .collect(),
            default_root_index: Some(1),
        })
        .unwrap();
    assert_eq!(retained.default_root_id, config.default_root_id);
    assert_eq!(retained.roots, config.roots);
}

#[test]
fn duplicate_aliases_and_unknown_roots_do_not_partially_replace_configuration() {
    let fixture = Fixture::new();
    let (database, _) = fixture.database();
    let path = fixture.folder("root");
    let request = RootsSetRequest {
        expected_revision: 0,
        roots: vec![root(path.clone()), root(path.join("."))],
        default_root_index: Some(0),
    };
    assert!(database.project_roots_set(request).is_err());
    assert!(database.project_manager_read().unwrap().roots.is_empty());
    assert!(
        database
            .project_roots_set(RootsSetRequest {
                expected_revision: 0,
                roots: vec![root(fixture.0.join("not-created"))],
                default_root_index: Some(0)
            })
            .is_err()
    );
    assert!(!fixture.0.join("not-created").exists());
}

#[test]
fn listing_is_first_level_paged_filtered_and_never_creates_project_rows() {
    let fixture = Fixture::new();
    let (database, location) = fixture.database();
    let path = fixture.folder("root");
    for index in 0..235 {
        fs::create_dir(path.join(format!("project-{index:03}"))).unwrap();
    }
    fs::create_dir(path.join("project-000/nested-not-a-project")).unwrap();
    fs::write(path.join("not-a-folder.txt"), "synthetic").unwrap();
    database
        .project_roots_set(RootsSetRequest {
            expected_revision: 0,
            roots: vec![root(path)],
            default_root_index: Some(0),
        })
        .unwrap();
    let engine = Arc::new(CatalogEngine::new());
    let immediate = refresh(&engine, &database);
    assert!(!immediate.scan_id.is_empty());
    let response = finished(&engine);
    assert_eq!(response.total, 235);
    assert_eq!(response.projects.len(), 100);
    assert!(response.projects.iter().all(|project| project.id.is_none()));
    assert_eq!(response.roots[0].state, RootState::Ready);
    let next = engine.snapshot(Some(&response.scan_id), "", 100, 100);
    assert_eq!(next.projects.len(), 100);
    let last = engine.snapshot(None, "", 200, 1000);
    assert_eq!(last.limit, 100);
    assert_eq!(last.projects.len(), 35);
    let filtered = engine.snapshot(None, "project-229", 0, 100);
    assert_eq!(filtered.total, 1);
    assert_eq!(filtered.projects[0].name, "project-229");
    assert!(database.recent().unwrap().is_empty());
    let raw = rusqlite::Connection::open(location.database_path()).unwrap();
    assert_eq!(
        raw.query_row::<u32, _, _>("SELECT count(*) FROM project", [], |row| row.get(0))
            .unwrap(),
        0
    );
}

fn modified(path: &std::path::Path, seconds: u64) {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_WRITE_ATTRIBUTES,
    };
    fs::OpenOptions::new()
        .read(true)
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds)))
        .unwrap();
}

#[test]
fn opened_projects_precede_new_projects_which_use_folder_modification_time() {
    let fixture = Fixture::new();
    let (database, _) = fixture.database();
    let path = fixture.folder("root");
    let opened = fixture.folder("root/opened");
    let newer = fixture.folder("root/newer");
    let older = fixture.folder("root/older");
    modified(&opened, 1000);
    modified(&newer, 3000);
    modified(&older, 2000);
    let project = database.project_manager_select(&opened).unwrap();
    let config = database.project_manager_read().unwrap();
    database
        .project_roots_set(RootsSetRequest {
            expected_revision: config.revision,
            roots: vec![root(path)],
            default_root_index: Some(0),
        })
        .unwrap();
    let engine = Arc::new(CatalogEngine::new());
    refresh(&engine, &database);
    let response = finished(&engine);
    assert_eq!(
        response
            .projects
            .iter()
            .map(|project| project.name.as_str())
            .collect::<Vec<_>>(),
        ["opened", "newer", "older"]
    );
    assert_eq!(response.projects[0].id.as_ref(), Some(&project.id));
    assert!(
        database
            .project_manager_read()
            .unwrap()
            .non_standard_history
            .is_empty()
    );
}

#[test]
fn root_parent_may_contain_state_but_state_candidates_are_excluded() {
    let fixture = Fixture::new();
    let (database, _) = fixture.database();
    fixture.folder("allowed-project");
    database
        .project_roots_set(RootsSetRequest {
            expected_revision: 0,
            roots: vec![root(fixture.0.clone())],
            default_root_index: Some(0),
        })
        .unwrap();
    let engine = Arc::new(CatalogEngine::new());
    refresh(&engine, &database);
    let response = finished(&engine);
    assert_eq!(response.total, 1);
    assert_eq!(response.projects[0].name, "allowed-project");
}

#[test]
fn missing_default_is_reported_and_never_replaced_by_a_healthy_root() {
    let fixture = Fixture::new();
    let (database, _) = fixture.database();
    let missing = fixture.folder("removed");
    let healthy = fixture.folder("healthy");
    fixture.folder("healthy/project");
    let config = database
        .project_roots_set(RootsSetRequest {
            expected_revision: 0,
            roots: vec![root(missing.clone()), root(healthy)],
            default_root_index: Some(0),
        })
        .unwrap();
    fs::remove_dir(missing).unwrap();
    let engine = Arc::new(CatalogEngine::new());
    refresh(&engine, &database);
    let response = finished(&engine);
    assert_eq!(response.roots[0].state, RootState::Missing);
    assert_eq!(response.roots[1].state, RootState::Ready);
    assert_eq!(
        response.roots_config.default_root_id,
        config.default_root_id
    );
    assert_eq!(response.total, 1);
}

#[test]
fn only_ten_nonstandard_projects_are_kept_and_alias_reopen_moves_one_entry() {
    let fixture = Fixture::new();
    let (database, _) = fixture.database();
    let root_path = fixture.folder("root");
    fixture.folder("root/standard");
    database
        .project_roots_set(RootsSetRequest {
            expected_revision: 0,
            roots: vec![root(root_path)],
            default_root_index: Some(0),
        })
        .unwrap();
    let mut selected = Vec::new();
    for index in 0..11 {
        selected.push(
            database
                .project_manager_select(&fixture.folder(&format!("outside-{index:02}")))
                .unwrap(),
        );
    }
    let config = database.project_manager_read().unwrap();
    assert_eq!(config.non_standard_history.len(), 10);
    assert!(!config.non_standard_history.contains(&selected[0].id));
    let same = database
        .project_manager_select(&fixture.0.join("outside-01/."))
        .unwrap();
    assert_eq!(same.id, selected[1].id);
    let config = database.project_manager_read().unwrap();
    assert_eq!(config.non_standard_history.len(), 10);
    assert_eq!(config.non_standard_history[0], same.id);
    database
        .project_manager_select(&fixture.0.join("root/standard"))
        .unwrap();
    let config = database.project_manager_read().unwrap();
    assert_eq!(config.non_standard_history.len(), 10);
    assert_eq!(config.non_standard_history[0], same.id);
}

#[test]
fn candidate_and_root_counts_are_bounded() {
    let fixture = Fixture::new();
    let (database, _) = fixture.database();
    let path = fixture.folder("many");
    for index in 0..1005 {
        fs::create_dir(path.join(format!("p-{index:04}"))).unwrap();
    }
    assert!(
        database
            .project_roots_set(RootsSetRequest {
                expected_revision: 0,
                roots: (0..17)
                    .map(|index| root(fixture.0.join(format!("uncreated-{index}"))))
                    .collect(),
                default_root_index: None
            })
            .is_err()
    );
    database
        .project_roots_set(RootsSetRequest {
            expected_revision: 0,
            roots: vec![root(path)],
            default_root_index: Some(0),
        })
        .unwrap();
    let engine = Arc::new(CatalogEngine::new());
    refresh(&engine, &database);
    let response = finished(&engine);
    assert_eq!(response.total, 1000);
    assert_eq!(response.roots[0].state, RootState::Truncated);
    assert_eq!(response.projects.len(), 100);
}

#[test]
fn cancellation_prevents_network_lookups_and_leaves_roots_and_selection_unchanged() {
    use shinatama_state::cancellation::MutationToken;
    use shinatama_state::database::DatabaseError;
    let fixture = Fixture::new();
    let (database, _) = fixture.database();
    let token = MutationToken::new();
    assert!(token.cancel());
    assert!(matches!(
        database.project_manager_select_if_not_cancelled(
            std::path::Path::new(r"\\offline-server\share\project"),
            &token
        ),
        Err(DatabaseError::Cancelled)
    ));
    assert!(matches!(
        database.project_roots_set_if_not_cancelled(
            RootsSetRequest {
                expected_revision: 0,
                roots: vec![root(PathBuf::from(r"\\offline-server\share\root"))],
                default_root_index: Some(0)
            },
            &token
        ),
        Err(DatabaseError::Cancelled)
    ));
    assert!(database.recent().unwrap().is_empty());
    assert_eq!(database.project_manager_read().unwrap().revision, 0);
}

#[test]
fn history_failure_rolls_back_the_project_record_in_the_same_transaction() {
    let fixture = Fixture::new();
    let (database, location) = fixture.database();
    let project = fixture.folder("project");
    let raw = rusqlite::Connection::open(location.database_path()).unwrap();
    let config = serde_json::json!({ "version":1,"revision":9_007_199_254_740_991u64,"roots":[],"defaultRootId":null,"nonStandardHistory":[] });
    let original = config.to_string();
    raw.execute(
        "INSERT INTO setting VALUES('projects.manager.v1',?1)",
        [&original],
    )
    .unwrap();
    assert!(database.project_manager_select(&project).is_err());
    assert_eq!(
        raw.query_row::<u32, _, _>("SELECT count(*) FROM project", [], |row| row.get(0))
            .unwrap(),
        0
    );
    assert_eq!(
        raw.query_row::<String, _, _>(
            "SELECT value FROM setting WHERE key='projects.manager.v1'",
            [],
            |row| row.get(0)
        )
        .unwrap(),
        original
    );
}
