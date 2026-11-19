//! H27 shim named `opencode.exe`, put first on PATH in the OpenCode scratch env.
//! txcript spawns `opencode session import <file> [--directory D]` with no `--standalone`;
//! OpenCode 2.1.4 then hands the work to its background service, which runs with the real
//! home. The shim forwards to the real binary ($H27_OPENCODE_EXE) and appends `--standalone`
//! to `session …` commands, so the import lands in the scratch OPENCODE_DB. Each call is
//! logged to $H27_SHIM_LOG.
use std::io::Write;
use std::process::{Command, exit};

fn main() {
    let real = std::env::var("H27_OPENCODE_EXE").expect("H27_OPENCODE_EXE not set");
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("session") && !args.iter().any(|a| a == "--standalone") {
        args.push("--standalone".into());
    }
    if let Ok(log) = std::env::var("H27_SHIM_LOG") {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log) {
            let _ = writeln!(f, "{}", serde_like(&args));
        }
    }
    let status = Command::new(real).args(&args).status().expect("spawn real opencode");
    exit(status.code().unwrap_or(1));
}

fn serde_like(args: &[String]) -> String {
    let quoted: Vec<String> = args.iter().map(|a| format!("{a:?}")).collect();
    format!("[{}]", quoted.join(","))
}
