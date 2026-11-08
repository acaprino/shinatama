//! C24: derived, rebuildable full-text index of session transcripts.
//!
//! It lives in its own SQLite file next to (never inside) the authoritative
//! database. Rebuilding or forgetting it can therefore never change threads,
//! projects or any other authoritative record.
use crate::location::StorageLocation;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter, types::Value};
use std::{collections::HashSet, sync::Mutex, time::Duration};

pub const FILE_NAME: &str = "session-search.sqlite";
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_BODY_BYTES: usize = 256 * 1024;
pub const MAX_TITLE_BYTES: usize = 512;
pub const MAX_QUERY_BYTES: usize = 128;
pub const MAX_QUERY_TOKENS: usize = 8;
pub const MAX_RESULTS: usize = 50;

type Result<T> = std::result::Result<T, String>;
fn sql(error: rusqlite::Error) -> String {
    format!("search index: {error}")
}

pub struct SearchIndex {
    connection: Mutex<Connection>,
}

pub struct IndexedDocument<'a> {
    pub root_id: &'a str,
    pub native_id: &'a str,
    pub harness: &'a str,
    pub title: Option<&'a str>,
    pub working_directory: Option<&'a str>,
    pub updated_at: Option<&'a str>,
    /// Cheap change detector from the metadata listing (updated time, size, turns).
    pub signature: &'a str,
    pub body: &'a str,
}

#[derive(Clone, Debug, Default)]
pub struct SearchFilters {
    pub harness: Option<String>,
    /// Working-directory prefix, compared case-insensitively.
    pub project: Option<String>,
    /// Inclusive ISO-8601 bounds, compared lexicographically with `updated_at`.
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchHit {
    pub root_id: String,
    pub native_id: String,
    pub harness: String,
    pub title: Option<String>,
    pub working_directory: Option<String>,
    pub updated_at: Option<String>,
    pub snippet: String,
    /// The native source is gone: still searchable, never natively resumable.
    pub source_deleted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexStatus {
    pub enabled: bool,
    pub documents: u64,
    pub source_deleted: u64,
    pub body_bytes: u64,
}

fn truncate(value: &str, cap: usize) -> &str {
    if value.len() <= cap {
        return value;
    }
    let mut end = cap;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

fn query_tokens(raw: &str) -> Vec<String> {
    raw.split_whitespace()
        .map(|token| token.chars().filter(|c| !c.is_control()).collect::<String>())
        .filter(|token| !token.is_empty())
        .take(MAX_QUERY_TOKENS)
        .collect()
}

/// Builds a safe MATCH expression: every token is a quoted literal, so user input
/// can never reach FTS operators (OR, NEAR, column filters, prefix syntax).
pub fn match_expression(raw: &str) -> Option<String> {
    if raw.len() > MAX_QUERY_BYTES {
        return None;
    }
    let tokens: Vec<String> = query_tokens(raw)
        .into_iter()
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect();
    let (last, earlier) = tokens.split_last()?;
    let mut expression = earlier.join(" ");
    if !expression.is_empty() {
        expression.push(' ');
    }
    // Prefix match only on the word still being typed.
    expression.push_str(last);
    expression.push('*');
    Some(expression)
}

const SNIPPET_BEFORE: usize = 80;
const SNIPPET_AFTER: usize = 140;

fn floor_boundary(text: &str, mut at: usize) -> usize {
    at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}
fn ceil_boundary(text: &str, mut at: usize) -> usize {
    at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at += 1;
    }
    at
}

/// Linear-time, bounded excerpt around the first match (FTS5's snippet() is
/// quadratic on long bodies that repeat a term). ASCII-case-insensitive; when no
/// token is located literally, the start of the text is shown without markers.
fn excerpt(body: &str, tokens: &[String]) -> String {
    let folded = body.to_ascii_lowercase();
    let found = tokens
        .iter()
        .filter_map(|token| {
            folded
                .find(&token.to_ascii_lowercase())
                .map(|at| (at, token.len()))
        })
        .min_by_key(|(at, _)| *at);
    let (start, end, mark) = match found {
        Some((at, len)) => (
            floor_boundary(body, at.saturating_sub(SNIPPET_BEFORE)),
            ceil_boundary(body, at + len + SNIPPET_AFTER),
            Some((at, len)),
        ),
        None => (0, ceil_boundary(body, SNIPPET_BEFORE + SNIPPET_AFTER), None),
    };
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    let mut cursor = start;
    if let Some((at, len)) = mark {
        out.push_str(&body[start..at]);
        out.push('«');
        out.push_str(&body[at..at + len]);
        out.push('»');
        cursor = at + len;
    }
    out.push_str(&body[cursor..end]);
    if end < body.len() {
        out.push('…');
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn like_prefix(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 1);
    for c in value.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

impl SearchIndex {
    pub fn open(location: &StorageLocation) -> Result<Self> {
        let path = location.directory().join(FILE_NAME);
        let connection = Connection::open(&path).map_err(sql)?;
        location
            .secure_private_file(&path)
            .map_err(|error| format!("search index file: {error}"))?;
        Self::prepare(connection)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::prepare(Connection::open_in_memory().map_err(sql)?)
    }

    fn prepare(connection: Connection) -> Result<Self> {
        connection
            .busy_timeout(Duration::from_millis(5000))
            .map_err(sql)?;
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(sql)?;
        if version > SCHEMA_VERSION {
            return Err("search index was written by a newer Shinatama".into());
        }
        if version < SCHEMA_VERSION {
            connection
                .execute_batch(
                    "BEGIN;
                     CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                     CREATE TABLE IF NOT EXISTS doc(
                       id INTEGER PRIMARY KEY,
                       root_id TEXT NOT NULL,
                       native_id TEXT NOT NULL,
                       harness TEXT NOT NULL,
                       title TEXT,
                       working_directory TEXT,
                       updated_at TEXT,
                       signature TEXT NOT NULL,
                       source_deleted INTEGER NOT NULL DEFAULT 0,
                       body_bytes INTEGER NOT NULL,
                       indexed_at TEXT NOT NULL,
                       UNIQUE(root_id, native_id));
                     CREATE INDEX IF NOT EXISTS doc_harness ON doc(harness);
                     CREATE VIRTUAL TABLE IF NOT EXISTS doc_fts USING fts5(
                       title, body, tokenize='unicode61 remove_diacritics 2');
                     PRAGMA user_version = 1;
                     COMMIT;",
                )
                .map_err(sql)?;
        }
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| "search index lock poisoned".to_owned())
    }

    pub fn enabled(&self) -> Result<bool> {
        let connection = self.lock()?;
        let value: Option<String> = connection
            .query_row("SELECT value FROM meta WHERE key='enabled'", [], |row| {
                row.get(0)
            })
            .optional()
            .map_err(sql)?;
        Ok(value.as_deref() != Some("0"))
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<()> {
        self.lock()?
            .execute(
                "INSERT INTO meta(key,value) VALUES('enabled',?1)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                [if enabled { "1" } else { "0" }],
            )
            .map_err(sql)?;
        Ok(())
    }

    /// True when the session was never indexed, changed since, or lost its source
    /// and came back. Unchanged sessions are never read again.
    pub fn needs_index(&self, root_id: &str, native_id: &str, signature: &str) -> Result<bool> {
        let stored: Option<(String, i64)> = self
            .lock()?
            .query_row(
                "SELECT signature,source_deleted FROM doc WHERE root_id=?1 AND native_id=?2",
                params![root_id, native_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql)?;
        Ok(match stored {
            None => true,
            Some((stored, deleted)) => deleted != 0 || stored != signature,
        })
    }

    pub fn upsert(&self, document: &IndexedDocument<'_>) -> Result<()> {
        if !self.enabled()? {
            return Err("indexing is turned off".into());
        }
        let body = truncate(document.body, MAX_BODY_BYTES);
        let title = document.title.map(|t| truncate(t, MAX_TITLE_BYTES));
        let mut connection = self.lock()?;
        let transaction = connection.transaction().map_err(sql)?;
        let existing: Option<i64> = transaction
            .query_row(
                "SELECT id FROM doc WHERE root_id=?1 AND native_id=?2",
                params![document.root_id, document.native_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql)?;
        let now = chrono::Utc::now().to_rfc3339();
        let id = if let Some(id) = existing {
            transaction
                .execute(
                    "UPDATE doc SET harness=?2,title=?3,working_directory=?4,updated_at=?5,
                     signature=?6,source_deleted=0,body_bytes=?7,indexed_at=?8 WHERE id=?1",
                    params![
                        id,
                        document.harness,
                        title,
                        document.working_directory,
                        document.updated_at,
                        document.signature,
                        body.len() as i64,
                        now
                    ],
                )
                .map_err(sql)?;
            transaction
                .execute("DELETE FROM doc_fts WHERE rowid=?1", [id])
                .map_err(sql)?;
            id
        } else {
            transaction
                .execute(
                    "INSERT INTO doc(root_id,native_id,harness,title,working_directory,updated_at,
                     signature,body_bytes,indexed_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                    params![
                        document.root_id,
                        document.native_id,
                        document.harness,
                        title,
                        document.working_directory,
                        document.updated_at,
                        document.signature,
                        body.len() as i64,
                        now
                    ],
                )
                .map_err(sql)?;
            transaction.last_insert_rowid()
        };
        transaction
            .execute(
                "INSERT INTO doc_fts(rowid,title,body) VALUES(?1,?2,?3)",
                params![id, title.unwrap_or(""), body],
            )
            .map_err(sql)?;
        transaction.commit().map_err(sql)
    }

    /// After a COMPLETE listing of a root: sessions no longer present stay
    /// searchable but are flagged as source-deleted (not natively resumable).
    pub fn mark_missing(&self, root_id: &str, present: &HashSet<String>) -> Result<u64> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction().map_err(sql)?;
        let known: Vec<(i64, String)> = {
            let mut statement = transaction
                .prepare("SELECT id,native_id FROM doc WHERE root_id=?1 AND source_deleted=0")
                .map_err(sql)?;
            statement
                .query_map([root_id], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(sql)?
                .collect::<std::result::Result<_, _>>()
                .map_err(sql)?
        };
        let mut marked = 0;
        for (id, native_id) in known {
            if !present.contains(&native_id) {
                transaction
                    .execute("UPDATE doc SET source_deleted=1 WHERE id=?1", [id])
                    .map_err(sql)?;
                marked += 1;
            }
        }
        transaction.commit().map_err(sql)?;
        Ok(marked)
    }

    pub fn search(
        &self,
        query: &str,
        filters: &SearchFilters,
        limit: usize,
    ) -> Result<Vec<SearchHit>> {
        let Some(expression) = match_expression(query) else {
            return Err("empty or oversized query".into());
        };
        let mut conditions = String::new();
        let mut values: Vec<Value> = vec![Value::Text(expression)];
        if let Some(harness) = &filters.harness {
            values.push(Value::Text(harness.clone()));
            conditions.push_str(&format!(" AND d.harness=?{}", values.len()));
        }
        if let Some(project) = &filters.project {
            values.push(Value::Text(like_prefix(project)));
            conditions.push_str(&format!(
                " AND d.working_directory LIKE ?{} ESCAPE '\\'",
                values.len()
            ));
        }
        if let Some(from) = &filters.from {
            values.push(Value::Text(from.clone()));
            conditions.push_str(&format!(" AND d.updated_at>=?{}", values.len()));
        }
        if let Some(to) = &filters.to {
            values.push(Value::Text(to.clone()));
            conditions.push_str(&format!(" AND d.updated_at<=?{}", values.len()));
        }
        values.push(Value::Integer(limit.clamp(1, MAX_RESULTS) as i64));
        let statement = format!(
            "SELECT d.root_id,d.native_id,d.harness,d.title,d.working_directory,d.updated_at,
             d.source_deleted,d.id
             FROM doc_fts JOIN doc d ON d.id=doc_fts.rowid
             WHERE doc_fts MATCH ?1{conditions} ORDER BY doc_fts.rank LIMIT ?{}",
            values.len()
        );
        let connection = self.lock()?;
        let mut prepared = connection.prepare(&statement).map_err(sql)?;
        let rows: Vec<(SearchHit, i64)> = prepared
            .query_map(params_from_iter(values), |row| {
                Ok((
                    SearchHit {
                        root_id: row.get(0)?,
                        native_id: row.get(1)?,
                        harness: row.get(2)?,
                        title: row.get(3)?,
                        working_directory: row.get(4)?,
                        updated_at: row.get(5)?,
                        source_deleted: row.get::<_, i64>(6)? != 0,
                        snippet: String::new(),
                    },
                    row.get(7)?,
                ))
            })
            .map_err(sql)?
            .collect::<std::result::Result<_, _>>()
            .map_err(sql)?;
        let tokens = query_tokens(query);
        let mut hits = Vec::with_capacity(rows.len());
        for (mut hit, id) in rows {
            let body: String = connection
                .query_row("SELECT body FROM doc_fts WHERE rowid=?1", [id], |row| {
                    row.get(0)
                })
                .map_err(sql)?;
            hit.snippet = excerpt(&body, &tokens);
            hits.push(hit);
        }
        Ok(hits)
    }

    /// Removes indexed text for one archive or for all of them. Source files and
    /// authoritative threads are never touched.
    pub fn forget(&self, root_id: Option<&str>) -> Result<u64> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction().map_err(sql)?;
        let removed = match root_id {
            Some(root) => {
                transaction
                    .execute(
                        "DELETE FROM doc_fts WHERE rowid IN (SELECT id FROM doc WHERE root_id=?1)",
                        [root],
                    )
                    .map_err(sql)?;
                transaction
                    .execute("DELETE FROM doc WHERE root_id=?1", [root])
                    .map_err(sql)?
            }
            None => {
                transaction.execute("DELETE FROM doc_fts", []).map_err(sql)?;
                transaction.execute("DELETE FROM doc", []).map_err(sql)?
            }
        };
        transaction.commit().map_err(sql)?;
        Ok(removed as u64)
    }

    pub fn status(&self) -> Result<IndexStatus> {
        let enabled = self.enabled()?;
        let (documents, source_deleted, body_bytes): (i64, i64, i64) = self
            .lock()?
            .query_row(
                "SELECT count(*),coalesce(sum(source_deleted),0),coalesce(sum(body_bytes),0) FROM doc",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(sql)?;
        Ok(IndexStatus {
            enabled,
            documents: documents as u64,
            source_deleted: source_deleted as u64,
            body_bytes: body_bytes as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc<'a>(
        root: &'a str,
        id: &'a str,
        harness: &'a str,
        cwd: &'a str,
        updated: &'a str,
        signature: &'a str,
        body: &'a str,
    ) -> IndexedDocument<'a> {
        IndexedDocument {
            root_id: root,
            native_id: id,
            harness,
            title: Some(id),
            working_directory: Some(cwd),
            updated_at: Some(updated),
            signature,
            body,
        }
    }

    fn corpus() -> SearchIndex {
        let index = SearchIndex::open_in_memory().unwrap();
        index
            .upsert(&doc("r1", "a", "claude_code", "D:\\work\\alpha", "2026-11-01T10:00:00Z", "s1", "fix the parser crash in tokenizer"))
            .unwrap();
        index
            .upsert(&doc("r1", "b", "codex", "D:\\work\\beta", "2026-11-03T10:00:00Z", "s1", "refactor the tokenizer and add tests"))
            .unwrap();
        index
            .upsert(&doc("r2", "c", "claude_code", "D:\\work\\alpha_2", "2026-10-01T10:00:00Z", "s1", "unrelated gardening notes"))
            .unwrap();
        index
    }

    #[test]
    fn matches_filters_by_agent_project_and_date() {
        let index = corpus();
        let all = index.search("tokenizer", &SearchFilters::default(), 50).unwrap();
        assert_eq!(all.len(), 2);
        assert!(all[0].snippet.contains('«'));
        let by_agent = index
            .search("tokenizer", &SearchFilters { harness: Some("codex".into()), ..Default::default() }, 50)
            .unwrap();
        assert_eq!(by_agent.iter().map(|h| h.native_id.as_str()).collect::<Vec<_>>(), ["b"]);
        let by_project = index
            .search("tokenizer", &SearchFilters { project: Some("D:\\work\\alpha".into()), ..Default::default() }, 50)
            .unwrap();
        assert_eq!(by_project.len(), 1);
        assert_eq!(by_project[0].native_id, "a");
        let by_date = index
            .search("tokenizer", &SearchFilters { from: Some("2026-11-02".into()), ..Default::default() }, 50)
            .unwrap();
        assert_eq!(by_date.iter().map(|h| h.native_id.as_str()).collect::<Vec<_>>(), ["b"]);
        let none = index
            .search("tokenizer", &SearchFilters { to: Some("2026-10-15".into()), ..Default::default() }, 50)
            .unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn prefix_works_on_the_last_word_and_wildcards_in_paths_are_literal() {
        let index = corpus();
        assert_eq!(index.search("tokeniz", &SearchFilters::default(), 50).unwrap().len(), 2);
        let literal = index
            .search("tokenizer", &SearchFilters { project: Some("D:\\work\\%".into()), ..Default::default() }, 50)
            .unwrap();
        assert!(literal.is_empty(), "% in a project filter must not act as a wildcard");
    }

    #[test]
    fn hostile_queries_never_reach_fts_operators() {
        let index = corpus();
        for query in [
            "\"", "tokenizer OR gardening", "NEAR(a b)", "title:tokenizer", "tokenizer*", "a\"b\"c", "-tokenizer",
            "(", "'; DROP TABLE doc; --", "\u{0}\u{1}",
        ] {
            // Must not error from FTS syntax; literal tokens simply match or not.
            let result = index.search(query, &SearchFilters::default(), 50);
            assert!(result.is_ok() || query.chars().all(char::is_control), "{query:?}");
        }
        assert!(index.search("tokenizer OR gardening", &SearchFilters::default(), 50).unwrap().is_empty());
        assert_eq!(index.status().unwrap().documents, 3);
        assert!(index.search("   ", &SearchFilters::default(), 50).is_err());
        assert!(index.search(&"x".repeat(MAX_QUERY_BYTES + 1), &SearchFilters::default(), 50).is_err());
    }

    #[test]
    fn results_and_bodies_stay_bounded() {
        let index = SearchIndex::open_in_memory().unwrap();
        let huge = "word ".repeat(MAX_BODY_BYTES);
        for n in 0..(MAX_RESULTS + 10) {
            index
                .upsert(&doc("r", &format!("s{n}"), "codex", "D:\\w", "2026-11-01", "x", &huge))
                .unwrap();
        }
        let hits = index.search("word", &SearchFilters::default(), 10_000).unwrap();
        assert_eq!(hits.len(), MAX_RESULTS);
        assert!(hits.iter().all(|hit| hit.snippet.len() < 400));
        assert!(index.status().unwrap().body_bytes <= (MAX_RESULTS as u64 + 10) * MAX_BODY_BYTES as u64);
    }

    #[test]
    fn excerpts_are_linear_bounded_and_utf8_safe() {
        let tokens = vec!["parser".to_owned()];
        let text = format!("{}Parser crash here{}", "é".repeat(500), " tail".repeat(500));
        let out = excerpt(&text, &tokens);
        assert!(out.contains("«Parser»"), "{out}");
        assert!(out.starts_with('…') && out.ends_with('…'));
        assert!(out.len() < 400, "{}", out.len());
        // No literal hit: the start of the text, unmarked.
        let plain = excerpt("alpha beta gamma", &["zzz".to_owned()]);
        assert_eq!(plain, "alpha beta gamma");
        // A million repeats must not be quadratic.
        let started = std::time::Instant::now();
        let huge = "word ".repeat(200_000);
        assert!(excerpt(&huge, &["word".to_owned()]).contains("«word»"));
        assert!(started.elapsed().as_secs() < 2);
    }

    #[test]
    fn unchanged_sessions_are_skipped_and_changed_ones_replace_their_text() {
        let index = corpus();
        assert!(!index.needs_index("r1", "a", "s1").unwrap());
        assert!(index.needs_index("r1", "a", "s2").unwrap());
        assert!(index.needs_index("r1", "new", "s1").unwrap());
        index
            .upsert(&doc("r1", "a", "claude_code", "D:\\work\\alpha", "2026-11-05T10:00:00Z", "s2", "now about databases"))
            .unwrap();
        assert!(index.search("parser", &SearchFilters::default(), 50).unwrap().is_empty());
        assert_eq!(index.search("databases", &SearchFilters::default(), 50).unwrap().len(), 1);
        assert_eq!(index.status().unwrap().documents, 3);
    }

    #[test]
    fn deleted_sources_stay_searchable_but_flagged_and_return_when_present_again() {
        let index = corpus();
        let present: HashSet<String> = ["a".to_owned()].into_iter().collect();
        assert_eq!(index.mark_missing("r1", &present).unwrap(), 1);
        let hits = index.search("tokenizer", &SearchFilters::default(), 50).unwrap();
        let b = hits.iter().find(|hit| hit.native_id == "b").unwrap();
        assert!(b.source_deleted);
        assert!(!hits.iter().find(|hit| hit.native_id == "a").unwrap().source_deleted);
        // Another root is untouched.
        assert!(!index.search("gardening", &SearchFilters::default(), 50).unwrap()[0].source_deleted);
        assert_eq!(index.status().unwrap().source_deleted, 1);
        // A reappearing source is re-read once, then unflagged.
        assert!(index.needs_index("r1", "b", "s1").unwrap());
        index
            .upsert(&doc("r1", "b", "codex", "D:\\work\\beta", "2026-11-03T10:00:00Z", "s1", "refactor the tokenizer and add tests"))
            .unwrap();
        assert!(!index.search("tokenizer", &SearchFilters::default(), 50).unwrap().iter().any(|hit| hit.source_deleted));
    }

    #[test]
    fn forget_one_archive_or_all_and_disabling_stops_indexing() {
        let index = corpus();
        assert_eq!(index.forget(Some("r1")).unwrap(), 2);
        assert!(index.search("tokenizer", &SearchFilters::default(), 50).unwrap().is_empty());
        assert_eq!(index.search("gardening", &SearchFilters::default(), 50).unwrap().len(), 1);
        index.set_enabled(false).unwrap();
        assert!(index.upsert(&doc("r1", "z", "codex", "D:\\w", "2026-11-01", "x", "text")).is_err());
        assert!(!index.status().unwrap().enabled);
        index.set_enabled(true).unwrap();
        assert_eq!(index.forget(None).unwrap(), 1);
        assert_eq!(index.status().unwrap().documents, 0);
    }

    #[cfg(windows)]
    #[test]
    fn index_file_is_separate_from_the_authoritative_database() {
        use crate::{database::Database, location::LocationRequest};
        let root = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
            .join("shinatama-state-tests")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(root.join("projects")).unwrap();
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: root.clone(),
            home_override: Some(root.join("private-state")),
            project_roots: vec![root.join("projects")],
            synchronized_roots: Vec::new(),
        })
        .unwrap();
        let database = Database::open(&location, None).unwrap();
        let index = SearchIndex::open(&location).unwrap();
        index.upsert(&doc("r", "a", "codex", "D:\\w", "2026-11-01", "s", "hello world")).unwrap();
        assert_ne!(location.database_path(), location.directory().join(FILE_NAME));
        assert!(location.directory().join(FILE_NAME).exists());
        let before = database.diagnostics().unwrap().schema_version;
        index.forget(None).unwrap();
        drop(index);
        assert_eq!(database.diagnostics().unwrap().schema_version, before);
        // Reopen: schema and enabled flag persist.
        let reopened = SearchIndex::open(&location).unwrap();
        assert_eq!(reopened.status().unwrap().documents, 0);
        drop(database);
        let _ = std::fs::remove_dir_all(root);
    }
}
