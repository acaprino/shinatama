//! The schema contains only state already used by the application.

use rusqlite::Connection;

pub const CURRENT_VERSION: u32 = 2;

/// Call after BEGIN IMMEDIATE: another window may have upgraded since open().
/// The write reservation prevents a newer migration racing this check/commit.
pub(crate) fn require_write_version(connection: &Connection) -> crate::database::Result<()> {
    let found: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if found > CURRENT_VERSION {
        Err(crate::database::DatabaseError::NewerSchema {
            found,
            supported: CURRENT_VERSION,
        })
    } else if found != CURRENT_VERSION {
        Err(crate::database::DatabaseError::UnsupportedSchema)
    } else {
        Ok(())
    }
}

pub(crate) const CREATE_V1: &str = "CREATE TABLE project (
    id TEXT PRIMARY KEY NOT NULL CHECK(length(id)=14 AND substr(id,1,2)='p_'),
    path TEXT NOT NULL,
    path_key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL CHECK(length(name)>0),
    last_opened INTEGER NOT NULL CHECK(last_opened>=0),
    recent_sequence INTEGER NOT NULL CHECK(recent_sequence>=0)
) STRICT;
CREATE INDEX project_recent ON project(last_opened DESC, recent_sequence DESC);
CREATE TABLE setting (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) STRICT;";

/// A version integer alone does not establish that this binary understands a schema.
/// Compare the embedded DDL, including constraints, strict types and indexes, before writing.
pub(crate) fn validate_version(connection: &Connection, version: u32) -> rusqlite::Result<()> {
    let actual = read_schema(connection)?;
    validate_definitions(actual, version)
}

/// At most one extra object is needed to reject an unknown schema. Projected
/// lengths are checked by SQLite before any DDL/name string is copied to Rust.
fn read_schema(connection: &Connection) -> rusqlite::Result<Vec<(String, String)>> {
    let mut statement = connection.prepare(
        "SELECT CASE WHEN octet_length(name)<=64 THEN name END AS bounded_name,
                CASE WHEN octet_length(sql)<=4096 THEN sql END
         FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*'
         ORDER BY bounded_name LIMIT 6",
    )?;
    statement
        .query_map([], |row| {
            let name = row
                .get::<_, Option<String>>(0)?
                .ok_or(rusqlite::Error::InvalidQuery)?;
            let sql = row
                .get::<_, Option<String>>(1)?
                .ok_or(rusqlite::Error::InvalidQuery)?;
            Ok((name, sql))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
}

fn validate_definitions(actual: Vec<(String, String)>, version: u32) -> rusqlite::Result<()> {
    if version == 0 {
        return if actual.is_empty() {
            Ok(())
        } else {
            Err(rusqlite::Error::InvalidQuery)
        };
    }
    let mut expected = ["project", "project_recent", "setting"]
        .into_iter()
        .zip(CREATE_V1.split(';').filter(|sql| !sql.trim().is_empty()))
        .collect::<Vec<_>>();
    if version == 2 {
        expected.push((
            "setting_json_insert",
            crate::migrations::SETTING_JSON_INSERT_TRIGGER,
        ));
        expected.push((
            "setting_json_update",
            crate::migrations::SETTING_JSON_UPDATE_TRIGGER,
        ));
    }
    if !matches!(version, 1 | 2)
        || actual.len() != expected.len()
        || !actual
            .iter()
            .zip(expected)
            .all(|((name, sql), (expected_name, expected_sql))| {
                name == expected_name && normalized(sql) == normalized(expected_sql)
            })
    {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(())
}

fn normalized(sql: &str) -> String {
    let mut output = String::with_capacity(sql.len());
    let mut characters = sql.trim().trim_end_matches(';').chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\'' {
            output.push(character);
            while let Some(literal_character) = characters.next() {
                output.push(literal_character);
                if literal_character == '\'' {
                    if characters.peek() == Some(&'\'') {
                        output.push(characters.next().expect("peeked escaped SQL quote"));
                    } else {
                        break;
                    }
                }
            }
        } else if !character.is_whitespace() {
            output.push(character.to_ascii_lowercase());
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_projection_refuses_oversized_names_and_sql_without_copying_them() {
        for (name, constraint) in [
            ("oversized".to_owned(), "x".repeat(8192)),
            ("x".repeat(65), String::new()),
        ] {
            let connection = Connection::open_in_memory().unwrap();
            connection
                .execute_batch(&format!(
                    "CREATE TABLE {name}(value TEXT CHECK(value!='{constraint}'))"
                ))
                .unwrap();
            assert!(matches!(
                read_schema(&connection),
                Err(rusqlite::Error::InvalidQuery)
            ));
            assert_eq!(
                connection
                    .query_row("SELECT count(*) FROM sqlite_schema", [], |row| row
                        .get::<_, i64>(0))
                    .unwrap(),
                1
            );
        }
    }

    #[test]
    fn schema_projection_never_materializes_more_than_six_objects() {
        let connection = Connection::open_in_memory().unwrap();
        for index in 0..30 {
            connection
                .execute_batch(&format!("CREATE TABLE extra_{index}(value TEXT)"))
                .unwrap();
        }
        assert_eq!(read_schema(&connection).unwrap().len(), 6);
        assert!(validate_version(&connection, CURRENT_VERSION).is_err());
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM sqlite_schema", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            30
        );
    }

    #[test]
    fn normalization_preserves_sql_literal_case_whitespace_and_escaped_quotes() {
        assert_eq!(normalized(" SELECT 'it''s A B'; "), "select'it''s A B'");
        assert_ne!(normalized("SELECT 'P_'"), normalized("SELECT 'p_'"));
        assert_ne!(normalized("SELECT 'a b'"), normalized("SELECT 'ab'"));
        assert_eq!(
            normalized("CREATE  TABLE example (value TEXT)"),
            normalized("create table example(value text)")
        );
    }

    #[test]
    #[cfg(windows)]
    fn changed_id_prefix_literals_are_refused_without_wal_backup_or_content_changes() {
        use crate::database::{Database, DatabaseError};
        use crate::location::{LocationRequest, StorageLocation};
        use std::fs;
        use std::path::PathBuf;

        for prefix in ["'P_'", "'p_ '"] {
            let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
                .join("shinatama-schema-tests")
                .join(uuid::Uuid::new_v4().to_string());
            fs::create_dir_all(&root).unwrap();
            let location = StorageLocation::resolve(LocationRequest {
                local_data_directory: root.clone(),
                home_override: Some(root.join("data")),
                project_roots: Vec::new(),
                synchronized_roots: Vec::new(),
            })
            .unwrap();
            location.prepare_private_directory().unwrap();
            let path = location.database_path();
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(&CREATE_V1.replace("'p_'", prefix))
                .unwrap();
            connection.pragma_update(None, "user_version", 1).unwrap();
            drop(connection);
            let original = fs::read(&path).unwrap();
            assert!(matches!(
                Database::open(&location, None),
                Err(DatabaseError::UnsupportedSchema)
            ));
            assert_eq!(fs::read(&path).unwrap(), original);
            let mut files = fs::read_dir(location.directory())
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            files.sort();
            assert_eq!(files, ["schema.lock", "shinatama.db"]);
            let connection = Connection::open(&path).unwrap();
            assert_eq!(
                connection
                    .pragma_query_value::<String, _>(None, "journal_mode", |row| row.get(0))
                    .unwrap(),
                "delete"
            );
            drop(connection);
            drop(location);
            fs::remove_dir_all(root).unwrap();
        }
    }
}
