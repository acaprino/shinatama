// Standalone metadata reader. The parent verifies atomic job membership before
// resuming this process; never launch it directly as an unprotected fallback.
#[cfg(windows)]
fn main() {
    shinatama_engine::session_roots::helper_main();
}
#[cfg(not(windows))]
fn main() {
    std::process::exit(2);
}
