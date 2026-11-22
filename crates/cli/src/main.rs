use shinatama_adapter::catalog::PathCatalog;
use shinatama_cli::{Command, ExitCode, HELP};
use shinatama_engine::Engine;
use std::io::{IsTerminal, Write};

/// `shinatama mcp`: stdout is the protocol; refusals go to stderr and the exit code.
fn run_mcp() -> ExitCode {
    let (Ok(instance), Ok(token)) = (
        std::env::var("SHINATAMA_INSTANCE_ID"),
        std::env::var("SHINATAMA_INSTANCE_TOKEN"),
    ) else {
        let _ = writeln!(
            std::io::stderr(),
            "shinatama mcp runs only inside an instance started by Shinatama."
        );
        return ExitCode::Refused;
    };
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(_) => return ExitCode::FailedUnchanged,
    };
    let store = shinatama_state::location::LocationRequest::from_environment(vec![cwd])
        .and_then(shinatama_state::location::StorageLocation::resolve)
        .ok()
        .and_then(|location| shinatama_state::messages::MessageStore::open(&location).ok());
    let Some(store) = store else {
        let _ = writeln!(
            std::io::stderr(),
            "Shinatama's message store is unavailable."
        );
        return ExitCode::FailedUnchanged;
    };
    let server = shinatama_cli::mcp::Server {
        store,
        instance,
        token,
    };
    match shinatama_cli::mcp::run_stdio(&server) {
        Ok(()) => ExitCode::Done,
        Err(_) => ExitCode::FailedUnchanged,
    }
}

fn main() -> std::process::ExitCode {
    // Bound collection as well as parsing. Refuse non-Unicode input rather than
    // silently changing an agent ID; no untrusted argument is echoed back.
    let mut arguments = Vec::new();
    for argument in std::env::args_os().skip(1).take(17) {
        match argument.into_string() {
            Ok(argument) => arguments.push(argument),
            Err(_) => {
                let _ = writeln!(std::io::stderr(), "Invalid command arguments.");
                return std::process::ExitCode::from(ExitCode::Refused as u8);
            }
        }
    }
    let code = match shinatama_cli::parse(&arguments) {
        Ok(Command::Help) => match write!(std::io::stdout(), "{HELP}") {
            Ok(()) => ExitCode::Done,
            Err(_) => ExitCode::FailedUnchanged,
        },
        Ok(Command::Doctor(options)) => match shinatama_cli::run_doctor(
            &Engine::new(),
            &PathCatalog::from_environment(),
            &options,
            &mut std::io::stdout().lock(),
        ) {
            Ok(code) => code,
            Err(_) => {
                let _ = writeln!(
                    std::io::stderr(),
                    "Doctor output failed; no state was changed."
                );
                ExitCode::FailedUnchanged
            }
        },
        Ok(Command::Mcp) => run_mcp(),
        Ok(Command::CheckState { json }) => {
            shinatama_cli::check_state::run(json, &mut std::io::stdout().lock())
                .unwrap_or(ExitCode::FailedUnchanged)
        }
        Ok(Command::Sessions(command)) => shinatama_cli::sessions::run(
            &command,
            &mut std::io::stdout().lock(),
            &mut std::io::stderr().lock(),
        )
        .unwrap_or(ExitCode::FailedUnchanged),
        Ok(Command::LaunchPreflight) => {
            let stdin = std::io::stdin();
            let result = if stdin.is_terminal() {
                shinatama_cli::run_launch_preflight(
                    &Engine::new(),
                    &mut std::io::empty(),
                    &mut std::io::stdout().lock(),
                )
            } else {
                shinatama_cli::run_launch_preflight(
                    &Engine::new(),
                    &mut stdin.lock(),
                    &mut std::io::stdout().lock(),
                )
            };
            result.unwrap_or_else(|_| {
                let _ = writeln!(
                    std::io::stderr(),
                    "Launch preflight output failed; no process was started or state changed."
                );
                ExitCode::FailedUnchanged
            })
        }
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "Command refused: {error:?}\n{HELP}");
            ExitCode::Refused
        }
    };
    std::process::ExitCode::from(code as u8)
}
