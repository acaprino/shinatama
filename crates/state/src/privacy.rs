//! F03P: integrity and credential checks over Shinatama's own stores (FRD-0014 R27-R32).
//!
//! Every SQLite file Shinatama writes in its data folder is opened read-only, checked with
//! `PRAGMA integrity_check`, and every text cell is passed to a caller-supplied detector (the
//! export redaction rules). Findings are reported as counts per store, table, column and kind:
//! a value is never returned, logged or printed. Log and text files in the folder are scanned
//! the same way, bounded in size.
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

pub const STORES: [(&str, bool); 4] = [
    ("shinatama.db", true),
    ("session-search.sqlite", false),
    ("hops.sqlite", false),
    ("messages.sqlite", false),
];
pub const MAX_CELL_BYTES: usize = 1024 * 1024;
pub const MAX_ROWS_PER_TABLE: usize = 200_000;
pub const MAX_TEXT_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreReport {
    pub file: String,
    /// The authoritative database; the others are operational and can be rebuilt or dropped.
    pub authoritative: bool,
    pub present: bool,
    pub integrity_ok: Option<bool>,
    /// "table.column" -> kind -> count.
    pub findings: BTreeMap<String, BTreeMap<String, u64>>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyReport {
    pub stores: Vec<StoreReport>,
    /// file name -> kind -> count.
    pub text_files: BTreeMap<String, BTreeMap<String, u64>>,
    pub clean: bool,
}

fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn scan_store(
    path: &Path,
    authoritative: bool,
    detect: &dyn Fn(&str) -> Vec<String>,
) -> StoreReport {
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut report = StoreReport {
        file,
        authoritative,
        present: path.is_file(),
        integrity_ok: None,
        findings: BTreeMap::new(),
        truncated: false,
    };
    if !report.present {
        return report;
    }
    let Ok(connection) = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        report.integrity_ok = Some(false);
        return report;
    };
    let _ = connection.busy_timeout(std::time::Duration::from_secs(2));
    report.integrity_ok = Some(
        connection
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .is_ok_and(|v| v == "ok"),
    );
    let tables: Vec<String> = connection
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND sql NOT LIKE 'CREATE VIRTUAL TABLE%'")
        .and_then(|mut s| s.query_map([], |r| r.get::<_, String>(0)).and_then(|rows| rows.collect()))
        .unwrap_or_default();
    for table in tables {
        let columns: Vec<String> = connection
            .prepare(&format!("PRAGMA table_info({})", quote(&table)))
            .and_then(|mut s| {
                s.query_map([], |r| r.get::<_, String>(1))
                    .and_then(|rows| rows.collect())
            })
            .unwrap_or_default();
        for column in columns {
            let sql = format!(
                "SELECT {c} FROM {t} WHERE typeof({c})='text' AND length(CAST({c} AS BLOB))<={max} LIMIT {limit}",
                c = quote(&column),
                t = quote(&table),
                max = MAX_CELL_BYTES,
                limit = MAX_ROWS_PER_TABLE + 1
            );
            let Ok(mut statement) = connection.prepare(&sql) else {
                continue;
            };
            let Ok(rows) = statement.query_map([], |r| r.get::<_, String>(0)) else {
                continue;
            };
            for (count, value) in rows.flatten().enumerate() {
                if count >= MAX_ROWS_PER_TABLE {
                    report.truncated = true;
                    break;
                }
                for kind in detect(&value) {
                    *report
                        .findings
                        .entry(format!("{table}.{column}"))
                        .or_default()
                        .entry(kind)
                        .or_insert(0) += 1;
                }
            }
        }
    }
    report
}

/// Checks the stores and text/log files directly inside `directory`.
pub fn scan(directory: &Path, detect: &dyn Fn(&str) -> Vec<String>) -> PrivacyReport {
    let mut report = PrivacyReport {
        stores: STORES
            .iter()
            .map(|(name, auth)| scan_store(&directory.join(name), *auth, detect))
            .collect(),
        ..PrivacyReport::default()
    };
    if let Ok(entries) = std::fs::read_dir(directory) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let text_like = ["log", "txt", "json", "jsonl"]
                .iter()
                .any(|ext| name.to_ascii_lowercase().ends_with(&format!(".{ext}")));
            let Ok(meta) = entry.metadata() else { continue };
            if !text_like || !meta.is_file() || meta.len() > MAX_TEXT_FILE_BYTES {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                for kind in detect(&text) {
                    *report
                        .text_files
                        .entry(name.clone())
                        .or_default()
                        .entry(kind)
                        .or_insert(0) += 1;
                }
            }
        }
    }
    report.clean = report
        .stores
        .iter()
        .all(|s| s.integrity_ok != Some(false) && s.findings.is_empty())
        && report.text_files.is_empty();
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(text: &str) -> Vec<String> {
        text.match_indices("ghp_")
            .map(|_| "github_token".to_owned())
            .collect()
    }

    #[test]
    fn reports_counts_never_values_and_checks_integrity() {
        let dir = std::env::temp_dir().join(format!("shinatama-privacy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = Connection::open(dir.join("hops.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE hop_event(detail TEXT); INSERT INTO hop_event VALUES('ok'),('leak ghp_SECRETVALUE'),(NULL);").unwrap();
        drop(db);
        std::fs::write(dir.join("app.log"), "line ghp_SECRET2").unwrap();
        std::fs::write(dir.join("image.png"), "ghp_notscanned").unwrap();
        let report = scan(&dir, &detect);
        let hops = report
            .stores
            .iter()
            .find(|s| s.file == "hops.sqlite")
            .unwrap();
        assert_eq!(hops.integrity_ok, Some(true));
        assert_eq!(hops.findings["hop_event.detail"]["github_token"], 1);
        assert!(
            !report
                .stores
                .iter()
                .find(|s| s.file == "shinatama.db")
                .unwrap()
                .present
        );
        assert_eq!(report.text_files["app.log"]["github_token"], 1);
        assert!(!report.text_files.contains_key("image.png"));
        assert!(!report.clean);
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            !json.contains("SECRET"),
            "values never appear in the report"
        );
        std::fs::write(
            dir.join("messages.sqlite"),
            b"not a database at all, just bytes",
        )
        .unwrap();
        let broken = scan(&dir, &detect);
        assert_eq!(
            broken
                .stores
                .iter()
                .find(|s| s.file == "messages.sqlite")
                .unwrap()
                .integrity_ok,
            Some(false)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
