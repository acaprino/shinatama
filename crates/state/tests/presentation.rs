#![cfg(windows)]
use shinatama_state::{
    database::{Database, DatabaseError},
    location::{LocationRequest, StorageLocation},
};
use std::{fs, path::PathBuf};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        Self(
            fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!("shinatama-presentation-{}", uuid::Uuid::new_v4())),
        )
    }
    fn location(&self) -> StorageLocation {
        StorageLocation::resolve(LocationRequest {
            local_data_directory: std::env::temp_dir(),
            home_override: Some(self.0.clone()),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        })
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let base = fs::canonicalize(std::env::temp_dir()).unwrap();
        assert_eq!(self.0.parent(), Some(base.as_path()));
        assert!(
            self.0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("shinatama-presentation-")
        );
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn json(revision: u64, size: u8) -> String {
    format!(
        r#"{{"revision":{revision},"settings":{{"version":1,"skin":"shinatama","followSkin":false,"fontFamily":"profile","fontSize":{size},"lineHeight":115}}}}"#
    )
}
#[test]
fn presentation_compare_and_save_is_atomic_across_connections_and_bounded() {
    let fixture = Fixture::new();
    let location = fixture.location();
    let first = Database::open(&location, None).unwrap();
    let second = Database::open(&location, None).unwrap();
    assert_eq!(first.presentation_json().unwrap(), None);
    first.set_presentation_json(0, &json(1, 14)).unwrap();
    assert!(matches!(
        second.set_presentation_json(0, &json(1, 16)),
        Err(DatabaseError::Busy(_))
    ));
    assert_eq!(second.presentation_json().unwrap(), Some(json(1, 14)));
    assert!(first.set_presentation_json(1, &" ".repeat(8193)).is_err());
    second.set_presentation_json(1, &json(2, 16)).unwrap();
    assert_eq!(first.presentation_json().unwrap(), Some(json(2, 16)));
}
#[test]
fn unsupported_presentation_and_a_newer_database_are_preserved() {
    let fixture = Fixture::new();
    let location = fixture.location();
    let database = Database::open(&location, None).unwrap();
    database.set_presentation_json(0, &json(1, 14)).unwrap();
    let raw = rusqlite::Connection::open(location.database_path()).unwrap();
    let future = json(1, 14).replace("\"version\":1", "\"version\":9");
    raw.execute(
        "UPDATE setting SET value=?1 WHERE key='ui.presentation.v1'",
        [&future],
    )
    .unwrap();
    assert!(database.set_presentation_json(1, &json(2, 16)).is_err());
    assert_eq!(database.presentation_json().unwrap(), Some(future.clone()));
    raw.pragma_update(None, "user_version", 999).unwrap();
    assert!(matches!(
        database.set_presentation_json(1, &json(2, 16)),
        Err(DatabaseError::NewerSchema { found: 999, .. })
    ));
    assert_eq!(database.presentation_json().unwrap(), Some(future));
}
