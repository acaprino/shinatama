//! Read-only doctor and launch preflight. No config creation, migration or login.
pub mod check_state;
pub mod launch;
pub mod mcp;
pub mod sessions;
pub use launch::run_launch_preflight;
use shinatama_adapter::{AgentId, catalog::InstallationCatalog};
use shinatama_engine::{
    Engine,
    doctor::{CheckupStatus, DoctorReport},
};
use std::io::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    Done = 0,
    Refused = 2,
    NeedsUser = 3,
    FailedUnchanged = 4,
    RecoveryRequired = 5,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoctorOptions {
    pub json: bool,
    pub agent: Option<AgentId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Doctor(DoctorOptions),
    LaunchPreflight,
    Sessions(sessions::SessionCommand),
    Mcp,
    CheckState { json: bool },
    Help,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    MissingCommand,
    UnknownCommand,
    UnknownOption,
    MissingAgent,
    InvalidAgent,
    DuplicateOption,
    TooManyArguments,
}

pub fn parse(args: &[String]) -> Result<Command, ParseError> {
    if args.len() > 16 || args.iter().any(|argument| argument.len() > 1024) {
        return Err(ParseError::TooManyArguments);
    }
    if args.first().map(String::as_str) == Some("check-state") {
        return match &args[1..] {
            [] => Ok(Command::CheckState { json: false }),
            [flag] if flag == "--json" => Ok(Command::CheckState { json: true }),
            _ => Err(ParseError::UnknownOption),
        };
    }
    if args.len() == 1 && args[0] == "mcp" {
        return Ok(Command::Mcp);
    }
    if let Some(result) = sessions::parse(args) {
        return result
            .map(Command::Sessions)
            .map_err(|()| ParseError::UnknownOption);
    }
    if args.len() > 6 || args.iter().any(|argument| argument.len() > 128) {
        return Err(ParseError::TooManyArguments);
    }
    let Some(command) = args.first() else {
        return Err(ParseError::MissingCommand);
    };
    if matches!(command.as_str(), "--help" | "-h" | "help") && args.len() == 1 {
        return Ok(Command::Help);
    }
    if command == "launch" {
        return if args.len() == 3 && args[1] == "preflight" && args[2] == "--json" {
            Ok(Command::LaunchPreflight)
        } else {
            Err(ParseError::UnknownOption)
        };
    }
    if command != "doctor" {
        return Err(ParseError::UnknownCommand);
    }
    let mut options = DoctorOptions {
        json: false,
        agent: None,
    };
    let mut remaining = args.iter().skip(1);
    while let Some(argument) = remaining.next() {
        match argument.as_str() {
            "--json" if !options.json => options.json = true,
            "--json" => return Err(ParseError::DuplicateOption),
            "--agent" if options.agent.is_none() => {
                options.agent = Some(
                    AgentId::new(remaining.next().ok_or(ParseError::MissingAgent)?)
                        .map_err(|_| ParseError::InvalidAgent)?,
                );
            }
            "--agent" => return Err(ParseError::DuplicateOption),
            _ => return Err(ParseError::UnknownOption),
        }
    }
    Ok(Command::Doctor(options))
}

pub fn run_doctor(
    engine: &Engine,
    catalog: &dyn InstallationCatalog,
    options: &DoctorOptions,
    output: &mut dyn Write,
) -> Result<ExitCode, std::io::Error> {
    let report = DoctorReport::build(engine, catalog.inspect(), options.agent.as_ref());
    if options.json {
        serde_json::to_writer_pretty(&mut *output, &report).map_err(std::io::Error::other)?;
        writeln!(output)?;
    } else {
        write_text(&report, output)?;
    }
    Ok(if report.has_refusals() {
        ExitCode::Refused
    } else {
        ExitCode::Done
    })
}

pub const HELP: &str = "shinatama doctor [--agent AGENT_ID] [--json]\nRead-only Windows installation/capability check. No login, update, config creation or agent launch.\n\nshinatama launch preflight --json\nRead one JSON request (maximum 8192 bytes) from stdin: {\"agent\":\"codex\",\"action\":\"start\",\"placement\":{\"program\":\"system\",\"home\":\"box\"}}\nUse action \"resume\" for Resume through the same engine service. This is a read-only review: it never starts an agent or creates state. Pipe/redirect the JSON and close stdin; interactive stdin is refused.\nLaunch status/exit: available=0; refused=2; incomplete=3. Input/output IO failure=4. No argv, environment values or tokens are emitted.\n\nSession commands (same contained reader and search index as the desktop):\n  shinatama sessions [--agent A] [--project DIR] [--json]\n  shinatama find \"text\" [--agent A] [--project DIR] [--json]\n  shinatama export AGENT:SESSION [-o FILE] [--format stx|mukade|native|md] [--mode full|portable] [--with-attachments] [--replace] [--json]\n  shinatama forget AGENT:SESSION [--json]   drop one session's search-index copy\n  shinatama forget THREAD_ID [--json]       delete a saved thread with its companion, hops, hand-off files, index rows and summary setting\n  resume and hop open the agent in the desktop and are refused here. There is no import.\nAgents: claude (claude_code), codex, pi. Secrets are redacted in every export; reasoning is never exported.\n\nExit codes: 0 done; 2 refused; 3 user action; 4 failed unchanged; 5 recovery required.\n";

fn write_text(report: &DoctorReport, output: &mut dyn Write) -> std::io::Result<()> {
    writeln!(
        output,
        "Shinatama doctor — read-only, schema {}",
        report.schema_version
    )?;
    if report.agents.is_empty() {
        writeln!(
            output,
            "No registered or known agent matches the requested ID."
        )?;
    }
    for agent in &report.agents {
        let status = match agent.status {
            CheckupStatus::Missing => "missing",
            CheckupStatus::Degraded => "degraded",
            CheckupStatus::Ready => "ready",
            CheckupStatus::Synthetic => "synthetic fixture",
        };
        writeln!(output, "{}: {status}", agent.display_name)?;
        for installation in &agent.installations {
            writeln!(
                output,
                "  {} ({:?}; {})",
                installation.path.display(),
                installation.kind,
                installation.argument_status()
            )?;
        }
        for reason in &agent.blockers {
            writeln!(output, "  Refused: {reason:?}")?;
        }
    }
    for issue in &report.discovery_issues {
        writeln!(output, "Discovery: {issue:?}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use shinatama_adapter::catalog::CatalogSnapshot;
    use std::cell::Cell;

    struct Catalog {
        calls: Cell<usize>,
    }
    impl InstallationCatalog for Catalog {
        fn inspect(&self) -> CatalogSnapshot {
            self.calls.set(self.calls.get() + 1);
            CatalogSnapshot::default()
        }
    }

    #[test]
    fn parser_refuses_unbounded_or_unsupported_actions() {
        let args = |values: &[&str]| {
            values
                .iter()
                .map(|value| (*value).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            parse(&args(&["doctor", "--json", "--agent", "codex"])).unwrap(),
            Command::Doctor(DoctorOptions {
                json: true,
                agent: Some(AgentId::new("codex").unwrap())
            })
        );
        assert_eq!(
            parse(&args(&["doctor", "--login"])),
            Err(ParseError::UnknownOption)
        );
        assert_eq!(
            parse(&args(&["doctor", "--agent"])),
            Err(ParseError::MissingAgent)
        );
        assert_eq!(
            parse(&args(&["doctor", "--json", "--json"])),
            Err(ParseError::DuplicateOption)
        );
        assert_eq!(
            parse(&args(&["run", "codex"])),
            Err(ParseError::UnknownCommand)
        );
        assert_eq!(
            parse(&args(&["launch", "preflight", "--json"])),
            Ok(Command::LaunchPreflight)
        );
        assert_eq!(
            parse(&args(&["launch", "preflight", "--run"])),
            Err(ParseError::UnknownOption)
        );
    }

    #[test]
    fn json_doctor_has_stable_schema_explicit_refusals_and_no_side_effect_interface() {
        let catalog = Catalog {
            calls: Cell::new(0),
        };
        let mut output = Vec::new();
        let exit = run_doctor(
            &Engine::new(),
            &catalog,
            &DoctorOptions {
                json: true,
                agent: None,
            },
            &mut output,
        )
        .unwrap();
        assert_eq!(exit, ExitCode::Refused);
        assert_eq!(catalog.calls.get(), 1);
        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["read_only"], true);
        assert_eq!(value["agents"].as_array().unwrap().len(), 6);
        assert_eq!(value["agents"][0]["status"], "missing");
        assert_eq!(
            value["agents"][0]["capabilities"]["start_new"]["state"],
            "unsupported"
        );
    }
}
