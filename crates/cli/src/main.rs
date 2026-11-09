use shinatama_adapter::catalog::PathCatalog;
use shinatama_cli::{Command, ExitCode, HELP};
use shinatama_engine::Engine;
use std::io::{IsTerminal, Write};

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
