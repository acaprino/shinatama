//! F03P: `shinatama check-state [--json]`. Read-only: integrity of every Shinatama store and a
//! credential scan that reports counts only. Exit 0 clean, 2 credential-like text found,
//! 5 a store failed its integrity check, 3 before the first run.
use crate::ExitCode;
use shinatama_export::{Mode, Redactor};
use shinatama_state::location::{LocationRequest, StorageLocation};
use shinatama_state::privacy::{PrivacyReport, scan};
use std::io::Write;
use std::path::Path;

/// One kind per credential-like occurrence, with the export redaction rules.
pub fn detect(text: &str) -> Vec<String> {
    let mut redactor = Redactor::new(Mode::Full, None);
    let _ = redactor.redact(text);
    redactor
        .into_counts()
        .into_iter()
        .flat_map(|(kind, count)| std::iter::repeat_n(kind, count as usize))
        .collect()
}

pub fn report_for(directory: &Path) -> PrivacyReport {
    scan(directory, &detect)
}

pub fn exit_for(report: &PrivacyReport) -> ExitCode {
    if report.stores.iter().any(|s| s.integrity_ok == Some(false)) {
        ExitCode::RecoveryRequired
    } else if !report.clean {
        ExitCode::Refused
    } else {
        ExitCode::Done
    }
}

pub fn run(json: bool, out: &mut dyn Write) -> std::io::Result<ExitCode> {
    let location = std::env::current_dir()
        .ok()
        .and_then(|cwd| LocationRequest::from_environment(vec![cwd]).ok())
        .and_then(|request| StorageLocation::resolve(request).ok());
    let Some(location) = location else {
        writeln!(out, "Shinatama's private storage location is unavailable.")?;
        return Ok(ExitCode::FailedUnchanged);
    };
    if !location.database_path().is_file() {
        writeln!(
            out,
            "Open Shinatama once first; nothing was checked or created."
        )?;
        return Ok(ExitCode::NeedsUser);
    }
    let report = report_for(location.directory());
    let code = exit_for(&report);
    if json {
        serde_json::to_writer_pretty(
            &mut *out,
            &serde_json::json!({"schema": "shinatama.cli.check-state/1", "report": report}),
        )?;
        writeln!(out)?;
        return Ok(code);
    }
    for store in &report.stores {
        let state = match (store.present, store.integrity_ok) {
            (false, _) => "absent".to_owned(),
            (true, Some(true)) => "integrity ok".to_owned(),
            (true, _) => "INTEGRITY FAILED".to_owned(),
        };
        let role = if store.authoritative {
            "authoritative"
        } else {
            "operational, rebuildable"
        };
        writeln!(out, "{}: {state} ({role})", store.file)?;
        for (place, kinds) in &store.findings {
            let list: Vec<String> = kinds.iter().map(|(k, n)| format!("{k} {n}")).collect();
            writeln!(
                out,
                "  credential-like text in {place}: {}",
                list.join(", ")
            )?;
        }
    }
    for (file, kinds) in &report.text_files {
        let list: Vec<String> = kinds.iter().map(|(k, n)| format!("{k} {n}")).collect();
        writeln!(out, "credential-like text in {file}: {}", list.join(", "))?;
    }
    writeln!(
        out,
        "{}",
        if report.clean {
            "No credential-like text found. Values are never printed."
        } else {
            "Values are never printed; the counts say where to look."
        }
    )?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detector_counts_kinds_and_exit_codes_follow_the_report() {
        let kinds = detect(
            "a ghp_0123456789abcdefABCDEF0123456789abcd and sk-ant-api03-AbCdEf0123456789AbCdEf0123456789xyz",
        );
        assert_eq!(kinds.len(), 2);
        assert!(
            detect("[REDACTED:github_token] and a uuid 550e8400e29b41d4a716446655440000")
                .is_empty(),
            "already redacted text and ids are not findings"
        );
        let dir =
            std::env::temp_dir().join(format!("shinatama-check-state-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = rusqlite::Connection::open(dir.join("shinatama.db")).unwrap();
        db.execute_batch("CREATE TABLE setting(key TEXT, value TEXT); INSERT INTO setting VALUES('ui','{\"theme\":\"dark\"}');").unwrap();
        drop(db);
        let clean = report_for(&dir);
        assert!(clean.clean);
        assert_eq!(exit_for(&clean), ExitCode::Done);
        let db = rusqlite::Connection::open(dir.join("shinatama.db")).unwrap();
        db.execute(
            "INSERT INTO setting VALUES('leak','token ghp_0123456789abcdefABCDEF0123456789abcd')",
            [],
        )
        .unwrap();
        drop(db);
        let dirty = report_for(&dir);
        assert_eq!(exit_for(&dirty), ExitCode::Refused);
        assert!(!serde_json::to_string(&dirty).unwrap().contains("ghp_0123"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
