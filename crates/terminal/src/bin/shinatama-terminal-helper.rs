#[cfg(windows)]
fn main() {
    if std::env::args_os().nth(1).as_deref() != Some(std::ffi::OsStr::new("--ipc")) {
        std::process::exit(2);
    }
    std::process::exit(if shinatama_terminal::windows_helper::run().is_ok() {
        0
    } else {
        2
    });
}
#[cfg(not(windows))]
fn main() {
    std::process::exit(2);
}
