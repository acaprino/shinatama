//! The schema contains only state already used by the application.

use rusqlite::{Connection, TransactionBehavior};

pub const CURRENT_VERSION: u32 = 1;

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
    let mut statement = connection.prepare(
        "SELECT name,sql FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*' ORDER BY name",
    )?;
    let actual = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if version == 0 {
        return if actual.is_empty() {
            Ok(())
        } else {
            Err(rusqlite::Error::InvalidQuery)
        };
    }
    let expected = ["project", "project_recent", "setting"]
        .into_iter()
        .zip(CREATE_V1.split(';').filter(|sql| !sql.trim().is_empty()));
    if version != CURRENT_VERSION
        || actual.len() != 3
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

pub(crate) fn initialize(connection: &mut Connection) -> rusqlite::Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version: u32 = transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
    validate_version(&transaction, version)?;
    if version == 0 {
        transaction.execute_batch(CREATE_V1)?;
        transaction.pragma_update(None, "user_version", CURRENT_VERSION)?;
    }
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

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
