//! C26.3 session commands (FRD-0006 R36-R37). They call the same contained session worker and
//! search index as the desktop; nothing here parses a native store in-process.
//!
//! `sessions`, `find`, `export` and `forget` run; `resume` and `hop` are refused with a reason
//! (they open an agent in the desktop). `import` does not exist (ADR-0016 v2).
use crate::ExitCode;
use std::io::Write;

pub const SESSION_HELP: &str = "\
  shinatama sessions [--agent A] [--project DIR] [--json]
  shinatama find \"text\" [--agent A] [--project DIR] [--json]
  shinatama export AGENT:SESSION [-o FILE] [--format stx|mukade|native|md]
                   [--mode full|portable] [--with-attachments] [--replace] [--json]
  shinatama forget AGENT:SESSION [--json]    (drop one session's search-index copy)
  shinatama forget THREAD_ID [--json]        (delete a saved thread, its companion, hops,
                                              hand-off files, index rows and summary setting)
  shinatama resume | hop    (open the agent in the Shinatama desktop; refused here)
";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector {
    pub agent: String,
    pub native_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionCommand {
    Sessions { agent: Option<String>, project: Option<String>, json: bool },
    Find { query: String, agent: Option<String>, project: Option<String>, json: bool },
    Export { selector: Selector, output: Option<String>, format: String, mode: String, attachments: bool, replace: bool, json: bool },
    Forget { selector: Selector, json: bool },
    ForgetThread { thread_id: String, json: bool },
    Resume,
    Hop,
}

const AGENTS: [&str; 3] = ["claude_code", "codex", "pi"];

fn agent(value: &str) -> Option<String> {
    let value = match value {
        "claude" => "claude_code",
        other => other,
    };
    AGENTS.contains(&value).then(|| value.to_owned())
}

fn selector(value: &str) -> Option<Selector> {
    let (agent_name, id) = value.split_once(':')?;
    let agent = agent(agent_name)?;
    let valid = !id.is_empty() && id.len() <= 256 && id.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'));
    valid.then(|| Selector { agent, native_id: id.to_owned() })
}

/// `None` means "not a session command"; `Some(Err(()))` a malformed one.
pub fn parse(args: &[String]) -> Option<Result<SessionCommand, ()>> {
    let command = args.first()?.as_str();
    if !matches!(command, "sessions" | "find" | "export" | "forget" | "resume" | "hop") {
        return None;
    }
    Some(parse_inner(command, &args[1..]))
}

fn parse_inner(command: &str, rest: &[String]) -> Result<SessionCommand, ()> {
    if command == "resume" {
        return Ok(SessionCommand::Resume);
    }
    if command == "hop" {
        return Ok(SessionCommand::Hop);
    }
    let mut positional = Vec::new();
    let (mut agent_filter, mut project, mut json) = (None, None, false);
    let (mut output, mut format, mut mode, mut attachments, mut replace) = (None, None, None, false, false);
    let mut iter = rest.iter();
    let once = |slot: &mut Option<String>, value: Option<&String>| -> Result<(), ()> {
        if slot.is_some() {
            return Err(());
        }
        *slot = Some(value.ok_or(())?.clone());
        Ok(())
    };
    while let Some(argument) = iter.next() {
        match argument.as_str() {
            "--json" if !json => json = true,
            "--agent" => once(&mut agent_filter, iter.next())?,
            "--project" => once(&mut project, iter.next())?,
            "-o" | "--output" => once(&mut output, iter.next())?,
            "--format" => once(&mut format, iter.next())?,
            "--mode" => once(&mut mode, iter.next())?,
            "--with-attachments" if !attachments => attachments = true,
            "--replace" if !replace => replace = true,
            value if !value.starts_with('-') => positional.push(value.to_owned()),
            _ => return Err(()),
        }
    }
    let agent_filter = match agent_filter {
        Some(value) => Some(agent(&value).ok_or(())?),
        None => None,
    };
    let export_only = output.is_some() || format.is_some() || mode.is_some() || attachments || replace;
    match command {
        "sessions" if positional.is_empty() && !export_only => Ok(SessionCommand::Sessions { agent: agent_filter, project, json }),
        "find" if positional.len() == 1 && !export_only => {
            let query = positional.remove(0);
            if query.trim().is_empty() || query.len() > 128 || query.chars().any(char::is_control) {
                return Err(());
            }
            Ok(SessionCommand::Find { query, agent: agent_filter, project, json })
        }
        "export" if positional.len() == 1 && agent_filter.is_none() && project.is_none() => {
            let selector = selector(&positional[0]).ok_or(())?;
            let format = format.unwrap_or_else(|| "stx".into());
            let mode = mode.unwrap_or_else(|| "full".into());
            if shinatama_export::Format::parse(&format).is_none() || shinatama_export::Mode::parse(&mode).is_none() {
                return Err(());
            }
            Ok(SessionCommand::Export { selector, output, format, mode, attachments, replace, json })
        }
        "forget" if positional.len() == 1 && !export_only && agent_filter.is_none() && project.is_none() => {
            let value = &positional[0];
            if value.len() == 14 && value.starts_with("t_") && value[2..].bytes().all(|c| matches!(c, b'a'..=b'z' | b'2'..=b'7')) {
                return Ok(SessionCommand::ForgetThread { thread_id: value.clone(), json });
            }
            Ok(SessionCommand::Forget { selector: selector(value).ok_or(())?, json })
        }
        _ => Err(()),
    }
}

/// A stable machine-readable refusal (R37): `{"status":"refused","code":...,"message":...}`.
fn report(out: &mut dyn Write, json: bool, code: ExitCode, reason: &str, message: &str) -> std::io::Result<ExitCode> {
    let status = match code {
        ExitCode::Done => "done",
        ExitCode::Refused => "refused",
        ExitCode::NeedsUser => "needs_user",
        ExitCode::FailedUnchanged => "failed",
        ExitCode::RecoveryRequired => "recovery_required",
    };
    if json {
        serde_json::to_writer(&mut *out, &serde_json::json!({"schema": "shinatama.cli/1", "status": status, "code": reason, "message": message}))?;
        writeln!(out)?;
    } else {
        writeln!(out, "{message}")?;
    }
    Ok(code)
}

#[cfg(not(windows))]
pub fn run(command: &SessionCommand, out: &mut dyn Write, _err: &mut dyn Write) -> std::io::Result<ExitCode> {
    let _ = command;
    report(out, false, ExitCode::Refused, "unsupported_platform", "Session commands are supported on Windows only.")
}

#[cfg(windows)]
pub fn run(command: &SessionCommand, out: &mut dyn Write, err: &mut dyn Write) -> std::io::Result<ExitCode> {
    windows::run(command, out, err)
}

#[cfg(windows)]
mod windows {
    use super::*;
    use serde_json::json;
    use shinatama_engine::{
        resources::admission::{AdmissionCoordinator, PrepareRequest},
        session_detail::DetailRequest,
        session_export::{CommitError, ExportIssue, ExportRequest, commit_export},
        session_roots::{PreparedSessionList, RootSelector, SessionListRead, SessionReadIssue, metadata_worker_budget},
    };
    use shinatama_export::{Format, Mode, output};
    use shinatama_state::{
        database::Database,
        location::{LocationRequest, StorageLocation},
        records::StoreRootRecord,
        search_index::{SearchFilters, SearchIndex},
    };
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    const MIB: u64 = 1024 * 1024;
    const MAX_LISTED: usize = 2000;

    struct Store {
        location: StorageLocation,
        database: Database,
    }

    enum Failure {
        Exit(ExitCode, &'static str, String),
    }
    type Outcome<T> = Result<T, Failure>;

    fn needs_user(code: &'static str, message: &str) -> Failure {
        Failure::Exit(ExitCode::NeedsUser, code, message.into())
    }
    fn refused(code: &'static str, message: &str) -> Failure {
        Failure::Exit(ExitCode::Refused, code, message.into())
    }
    fn failed(code: &'static str, message: &str) -> Failure {
        Failure::Exit(ExitCode::FailedUnchanged, code, message.into())
    }

    fn open_store() -> Outcome<Store> {
        let cwd = std::env::current_dir().map_err(|_| failed("storage_unavailable", "The current folder cannot be read."))?;
        let request = LocationRequest::from_environment(vec![cwd])
            .map_err(|_| failed("storage_unavailable", "Shinatama's private storage location is unavailable."))?;
        let location = StorageLocation::resolve(request)
            .map_err(|_| failed("storage_unavailable", "Shinatama's private storage location is unavailable."))?;
        if !location.database_path().try_exists().unwrap_or(false) {
            return Err(needs_user("first_run", "Open Shinatama once before using session commands; no state was created."));
        }
        let database = Database::open(&location, None)
            .map_err(|_| failed("storage_unavailable", "Shinatama's database could not be opened; nothing was changed."))?;
        Ok(Store { location, database })
    }

    fn helper() -> Outcome<PathBuf> {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("shinatama-session-helper.exe")))
            .filter(|path| path.is_file())
            .ok_or_else(|| failed("helper_missing", "shinatama-session-helper.exe is missing next to this program."))
    }

    fn issue(issue: SessionReadIssue) -> Failure {
        let code = match issue {
            SessionReadIssue::CleanupUnconfirmed => {
                return Failure::Exit(ExitCode::RecoveryRequired, "cleanup_unconfirmed", "The reader's cleanup is unconfirmed; its resources stay reserved until Shinatama recovers them.".into());
            }
            SessionReadIssue::RootUnavailable => "root_unavailable",
            SessionReadIssue::RootChanged => "root_changed",
            SessionReadIssue::SourceChanged => "source_changed",
            SessionReadIssue::TimedOut => "timed_out",
            SessionReadIssue::OutputLimit => "output_limit",
            SessionReadIssue::UnsupportedHarness => "unsupported_harness",
            _ => "read_unavailable",
        };
        refused(code, "The session read was refused; nothing was changed.")
    }

    /// One contained read, admitted through the shared reservation ledger the desktop uses.
    fn contained(store: &Store, prepared: &PreparedSessionList) -> Outcome<SessionListRead> {
        contained_with(store, prepared, None)
    }

    /// [`contained`] for an export: `sink` receives every body frame as the worker writes it.
    fn contained_with(
        store: &Store,
        prepared: &PreparedSessionList,
        sink: Option<&mut dyn FnMut(&[u8]) -> std::io::Result<()>>,
    ) -> Outcome<SessionListRead> {
        let helper = helper()?;
        let ledger = store
            .database
            .resource_reservations()
            .map_err(|_| failed("storage_unavailable", "The resource ledger could not be read."))?
            .ok_or_else(|| needs_user("first_run", "Open Shinatama once so it sets its resource policy; no state was created."))?;
        let coordinator = AdmissionCoordinator::new(1).map_err(|_| failed("worker_unavailable", "The session worker could not be prepared."))?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
        let workload = coordinator
            .prepare(&store.database, &ledger.policy.stamp, ledger.revision, PrepareRequest {
                budget: metadata_worker_budget(),
                process_commit_bytes: 768 * MIB,
                now_ms: now,
                lease_expires_at_ms: now + 15_000,
            })
            .map_err(|_| refused("resources", "Shinatama's resource budget has no room for a session reader now; retry later."))?;
        let read = match sink {
            Some(sink) => prepared.run_contained_export(&store.database, &helper, &store.location, workload, sink),
            None => prepared.run_contained(&store.database, &helper, &store.location, workload),
        };
        if read.cleanup_retained() {
            return Err(issue(SessionReadIssue::CleanupUnconfirmed));
        }
        if let Some(value) = read.issue {
            return Err(issue(value));
        }
        Ok(read)
    }

    fn output_failure(error: output::OutputError) -> Failure {
        match error {
            output::OutputError::Exists | output::OutputError::AttachmentsExist => refused("exists", "The output exists; add --replace to overwrite it. Nothing was written."),
            output::OutputError::InvalidPath | output::OutputError::ParentMissing => refused("invalid_path", "The output folder does not exist or the name is invalid. Nothing was written."),
            output::OutputError::Write(_) => failed("write_failed", "Writing failed; no partial file was left."),
        }
    }

    fn roots(store: &Store, agent: Option<&str>) -> Outcome<Vec<StoreRootRecord>> {
        let roots = store
            .database
            .list_store_roots(None, 100)
            .map_err(|_| failed("storage_unavailable", "The archive list could not be read."))?;
        Ok(roots
            .into_iter()
            .filter(|root| root.available && agent.is_none_or(|a| a == root.harness) && AGENTS.contains(&root.harness.as_str()))
            .collect())
    }

    fn selector_of(root: &StoreRootRecord) -> RootSelector {
        RootSelector { store_root_id: root.id.clone(), configuration_revision: root.config_revision }
    }

    fn list_root(store: &Store, root: &StoreRootRecord, mut visit: impl FnMut(&shinatama_engine::session_roots::SessionRow) -> bool) -> Outcome<()> {
        let mut cursor = None;
        let mut seen = 0usize;
        loop {
            let prepared = PreparedSessionList::resolve(&store.database, &store.location, &selector_of(root), cursor, 50, true).map_err(issue)?;
            let read = contained(store, &prepared)?;
            let page = read.page().ok_or_else(|| failed("read_unavailable", "The session worker returned no listing."))?;
            for row in &page.rows {
                seen += 1;
                if !visit(row) || seen >= MAX_LISTED {
                    return Ok(());
                }
            }
            match &page.next {
                Some(next) => cursor = Some(next.clone()),
                None => return Ok(()),
            }
        }
    }

    /// Finds the archive holding `selector`: the search index first, then listing each root.
    fn locate(store: &Store, selector: &Selector) -> Outcome<StoreRootRecord> {
        let candidates = roots(store, Some(&selector.agent))?;
        if let Ok(index) = SearchIndex::open(&store.location)
            && let Ok(found) = index.locate(&selector.native_id)
            && let Some(root) = candidates.iter().find(|root| found.contains(&root.id))
        {
            return Ok(root.clone());
        }
        let mut matches = Vec::new();
        for root in &candidates {
            let mut hit = false;
            list_root(store, root, |row| {
                hit |= row.native_id == selector.native_id;
                !hit
            })?;
            if hit {
                matches.push(root.clone());
            }
        }
        match matches.len() {
            0 => Err(refused("session_not_found", "No archive lists this session.")),
            1 => Ok(matches.remove(0)),
            _ => Err(refused("ambiguous_session", "More than one archive lists this session id.")),
        }
    }

    fn write_json(out: &mut dyn Write, value: serde_json::Value) -> std::io::Result<()> {
        serde_json::to_writer_pretty(&mut *out, &value)?;
        writeln!(out)
    }

    pub fn run(command: &SessionCommand, out: &mut dyn Write, err: &mut dyn Write) -> std::io::Result<ExitCode> {
        let json = match command {
            SessionCommand::Sessions { json, .. } | SessionCommand::Find { json, .. } | SessionCommand::Export { json, .. } | SessionCommand::Forget { json, .. } | SessionCommand::ForgetThread { json, .. } => *json,
            _ => false,
        };
        match execute(command, out, err) {
            Ok(code) => Ok(code),
            Err(Failure::Exit(code, reason, message)) => report(if json { out } else { err }, json, code, reason, &message),
        }
    }

    fn execute(command: &SessionCommand, out: &mut dyn Write, err: &mut dyn Write) -> Outcome<ExitCode> {
        let io = |_| failed("output_failed", "Writing the output failed.");
        match command {
            SessionCommand::Resume | SessionCommand::Hop => Err(refused(
                "desktop_only",
                "Resume and hop open the agent in a Shinatama terminal; use the desktop. Nothing was started.",
            )),
            SessionCommand::Sessions { agent, project, json } => {
                let store = open_store()?;
                let mut rows = Vec::new();
                let prefix = project.as_ref().map(|p| p.to_ascii_lowercase());
                for root in roots(&store, agent.as_deref())? {
                    list_root(&store, &root, |row| {
                        let keep = prefix.as_ref().is_none_or(|p| row.working_directory.as_ref().is_some_and(|w| w.to_ascii_lowercase().starts_with(p)));
                        if keep {
                            rows.push(json!({
                                "agent": root.harness, "session": format!("{}:{}", root.harness, row.native_id),
                                "archive": root.id, "title": row.title, "cwd": row.working_directory,
                                "updated": row.updated_at, "created": row.created_at, "parent": row.parent_native_id,
                            }));
                        }
                        rows.len() < MAX_LISTED
                    })?;
                }
                if *json {
                    write_json(out, json!({"schema": "shinatama.cli.sessions/1", "status": "done", "sessions": rows})).map_err(io)?;
                } else {
                    for row in &rows {
                        writeln!(out, "{}\t{}\t{}", row["session"].as_str().unwrap_or_default(), row["updated"].as_str().unwrap_or("-"), row["title"].as_str().unwrap_or("(untitled)")).map_err(io)?;
                    }
                }
                Ok(ExitCode::Done)
            }
            SessionCommand::Find { query, agent, project, json } => {
                let store = open_store()?;
                let index = SearchIndex::open(&store.location).map_err(|_| failed("index_unavailable", "The search index could not be opened."))?;
                if !index.enabled().unwrap_or(false) {
                    return Err(needs_user("index_disabled", "Session indexing is off; turn it on in Search in the desktop."));
                }
                let filters = SearchFilters { harness: agent.clone(), project: project.clone(), from: None, to: None };
                let hits = index.search(query, &filters, 50).map_err(|_| refused("invalid_query", "The query was refused."))?;
                if *json {
                    let hits: Vec<_> = hits.iter().map(|hit| json!({
                        "session": format!("{}:{}", hit.harness, hit.native_id), "agent": hit.harness, "archive": hit.root_id,
                        "title": hit.title, "cwd": hit.working_directory, "updated": hit.updated_at,
                        "snippet": hit.snippet, "source_deleted": hit.source_deleted,
                    })).collect();
                    write_json(out, json!({"schema": "shinatama.cli.find/1", "status": "done", "hits": hits})).map_err(io)?;
                } else {
                    for hit in &hits {
                        writeln!(out, "{}:{}\t{}\t{}", hit.harness, hit.native_id, hit.title.as_deref().unwrap_or("(untitled)"), hit.snippet.replace(['\n', '\r'], " ")).map_err(io)?;
                    }
                }
                Ok(ExitCode::Done)
            }
            SessionCommand::Forget { selector, json } => {
                let store = open_store()?;
                let index = SearchIndex::open(&store.location).map_err(|_| failed("index_unavailable", "The search index could not be opened."))?;
                let removed = index.forget_session(&selector.agent, &selector.native_id).map_err(|_| failed("index_unavailable", "The search index could not be changed."))?;
                if *json {
                    write_json(out, json!({"schema": "shinatama.cli.forget/1", "status": "done", "removed": removed})).map_err(io)?;
                } else {
                    writeln!(out, "Removed {removed} indexed cop{} of {}:{}; the agent's own session is untouched.", if removed == 1 { "y" } else { "ies" }, selector.agent, selector.native_id).map_err(io)?;
                }
                Ok(ExitCode::Done)
            }
            SessionCommand::ForgetThread { thread_id, json } => {
                use shinatama_state::forget::{ForgetError, forget_thread};
                let store = open_store()?;
                let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().and_then(|t| i64::try_from(t.as_millis()).ok()).unwrap_or(0);
                let report = forget_thread(&store.location, &store.database, thread_id, None, at).map_err(|error| {
                    let message = error.to_string();
                    match error {
                        ForgetError::NotFound => refused("not_found", &message),
                        ForgetError::Invalid => refused("invalid_thread", &message),
                        ForgetError::Changed => refused("thread_changed", &message),
                        ForgetError::Open => refused("thread_open", &message),
                        ForgetError::HopInProgress => refused("hop_in_progress", &message),
                        ForgetError::Storage(_) => failed("storage_unavailable", &message),
                        ForgetError::IndexNotUpdated => failed("index_not_updated", &message),
                    }
                })?;
                if *json {
                    write_json(out, json!({"schema": "shinatama.cli.forget_thread/1", "status": "done", "threads": report.thread_ids,
                        "hops": report.hops, "handoff_files": report.handoff_files, "index_rows": report.index_rows})).map_err(io)?;
                } else {
                    writeln!(out, "Forgot {}: {} hop(s), {} hand-off file(s), {} index row(s). The agents' own sessions are untouched.",
                        report.thread_ids.join(", "), report.hops, report.handoff_files, report.index_rows).map_err(io)?;
                }
                Ok(ExitCode::Done)
            }
            SessionCommand::Export { selector, output: target, format, mode, attachments, replace, json } => {
                let format = Format::parse(format).ok_or_else(|| refused("invalid_format", "Unknown format."))?;
                let mode = Mode::parse(mode).ok_or_else(|| refused("invalid_mode", "Unknown mode."))?;
                if format == Format::Native {
                    return Err(refused("format_unavailable", "No native export has been proven for this agent yet; use stx, mukade or md."));
                }
                if *attachments && target.is_none() {
                    return Err(refused("output_required", "--with-attachments needs -o FILE."));
                }
                let target = match target {
                    Some(path) => {
                        let path = std::path::absolute(Path::new(path)).map_err(|_| refused("invalid_path", "The output path is invalid."))?;
                        if !*replace && path.exists() {
                            return Err(refused("exists", "The output file exists; add --replace to overwrite it. Nothing was written."));
                        }
                        Some(path)
                    }
                    None => None,
                };
                let store = open_store()?;
                let root = locate(&store, selector)?;
                let home = (mode == Mode::Portable).then(|| std::env::var("USERPROFILE").ok()).flatten().filter(|h| Path::new(h).is_absolute());
                let request = ExportRequest {
                    detail: DetailRequest { root: selector_of(&root), native_id: selector.native_id.clone(), cursor: None, limit: 1, overview_only: false },
                    format, mode, with_attachments: *attachments, thread: None, home,
                };
                let prepared = PreparedSessionList::resolve_export(&store.database, &store.location, request).map_err(issue)?;
                // The body is staged as the worker writes it (a session of any size has no total
                // bound) and committed only after its final frame; dropping `staged` removes it.
                let policy = if *replace { output::Overwrite::Replace } else { output::Overwrite::Refuse };
                let mut staged = match &target {
                    Some(path) => output::stage(path, policy),
                    None => output::stage_scratch(&std::env::temp_dir()),
                }
                .map_err(output_failure)?;
                let mut write_failed = false;
                let read = {
                    let mut sink = |bytes: &[u8]| {
                        staged.write(bytes).inspect_err(|_| write_failed = true)
                    };
                    contained_with(&store, &prepared, Some(&mut sink))
                };
                if write_failed {
                    return Err(failed("write_failed", "Writing failed; no partial file was left."));
                }
                let read = read?;
                let reply = read.export().ok_or_else(|| failed("read_unavailable", "The session worker returned no export."))?;
                if let Some(value) = reply.issue {
                    let code = match value {
                        ExportIssue::FormatUnavailable => "format_unavailable",
                        ExportIssue::OutputLimit => "output_limit",
                        ExportIssue::InvalidRequest => "invalid_request",
                        ExportIssue::Detail(_) => "read_unavailable",
                    };
                    return Err(refused(code, "The export was refused; nothing was written."));
                }
                let payload = reply.payload.as_ref().ok_or_else(|| failed("read_unavailable", "The session worker returned no export."))?;
                let report_value = serde_json::to_value(&payload.report).unwrap_or_default();
                match target {
                    None => {
                        staged.copy_to(payload.prefix.as_bytes(), out).map_err(io)?;
                        if *json {
                            write_json(err, json!({"schema": "shinatama.cli.export/1", "status": "done", "report": report_value})).map_err(io)?;
                        }
                    }
                    Some(_) => {
                        let receipt = commit_export(staged, payload).map_err(|error| match error {
                            CommitError::Attachment => failed("read_unavailable", "An attachment could not be decoded; nothing was written."),
                            CommitError::Output(error) => output_failure(error),
                        })?;
                        if *json {
                            write_json(out, json!({"schema": "shinatama.cli.export/1", "status": "done", "path": receipt.path, "bytes": receipt.bytes,
                                "sha256": receipt.sha256, "attachments": receipt.attachments, "report": report_value})).map_err(io)?;
                        } else {
                            writeln!(out, "Exported {} items to {} ({} bytes); {} secret(s) redacted, reasoning not included.",
                                payload.report.items, receipt.path.display(), receipt.bytes, payload.report.redactions.values().sum::<u64>()).map_err(io)?;
                        }
                    }
                }
                Ok(ExitCode::Done)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).to_owned()).collect()
    }

    #[test]
    fn parses_the_documented_forms() {
        assert_eq!(parse(&args(&["doctor"])), None);
        assert_eq!(parse(&args(&["sessions", "--agent", "claude", "--json"])), Some(Ok(SessionCommand::Sessions { agent: Some("claude_code".into()), project: None, json: true })));
        assert!(matches!(parse(&args(&["find", "parser bug", "--agent", "codex"])), Some(Ok(SessionCommand::Find { .. }))));
        let Some(Ok(SessionCommand::Export { selector, format, mode, .. })) = parse(&args(&["export", "codex:abc-1", "-o", "x.stx.jsonl", "--mode", "portable"])) else { panic!() };
        assert_eq!(selector, Selector { agent: "codex".into(), native_id: "abc-1".into() });
        assert_eq!((format.as_str(), mode.as_str()), ("stx", "portable"));
        assert_eq!(parse(&args(&["resume", "x"])), Some(Ok(SessionCommand::Resume)));
        assert_eq!(parse(&args(&["forget", "t_abcdefgh2345", "--json"])), Some(Ok(SessionCommand::ForgetThread { thread_id: "t_abcdefgh2345".into(), json: true })));
    }

    #[test]
    fn refuses_malformed_input() {
        for bad in [
            &["export", "nocolon"][..],
            &["export", "gemini:abc"],
            &["export", "codex:../x"],
            &["export", "codex:a", "--format", "docx"],
            &["export", "codex:a", "--mode", "secret"],
            &["sessions", "extra"],
            &["sessions", "-o", "x"],
            &["find", ""],
            &["find", "a", "b"],
            &["forget", "codex:a", "--format", "md"],
            &["forget", "t_short"],
            &["forget", "t_ABCDEFGH2345"],
            &["sessions", "--json", "--json"],
        ] {
            assert_eq!(parse(&args(bad)), Some(Err(())), "{bad:?}");
        }
    }

    #[test]
    fn resume_and_hop_are_refused_with_exit_two() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        assert_eq!(run(&SessionCommand::Hop, &mut out, &mut err).unwrap(), ExitCode::Refused);
    }
}
