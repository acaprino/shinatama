//! The Mukade program, as Mukade's own `src/bin/mukade.rs` builds it: `mukade::cli::run` with the
//! process arguments, its timing report, and its exit codes. Nothing of Mukade's commands is added
//! or changed, so a hook line or a `chat` command behaves exactly as Mukade documents it.
//!
//! One command is Shinatama's own, under a name Mukade does not use: `shinatama-kit` (see `kit`).

mod kit;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some(kit::COMMAND) {
        std::process::exit(kit::run(&args[1..]));
    }
    let started = std::time::Instant::now();
    let result = mukade::cli::run(&args);
    let phases = mukade::timing::report();
    if !phases.is_empty() {
        eprint!("{phases}");
        eprintln!(
            "{:<28} {:>9.1} ms",
            "total",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
    if let Err(error) = result {
        eprintln!("{error}");
        // Mukade 5.5.2: 2 refused with a reason, 4 failed and nothing changed, 5 failed and recovery needed.
        let code = error
            .downcast_ref::<mukade::HopError>()
            .map(mukade::HopError::exit_code)
            .unwrap_or(1);
        std::process::exit(code);
    }
}
