//! Read-only path-policy diagnostic. This never prepares a directory or creates a file.
use shinatama_state::location::{LocationRequest, StorageLocation};
use std::path::PathBuf;

fn main() {
    let projects = std::env::current_dir().into_iter().collect();
    let request = match LocationRequest::from_environment(projects) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    for input in std::env::args_os().skip(1) {
        let mut request = request.clone();
        request.home_override = Some(PathBuf::from(&input));
        match StorageLocation::resolve(request) {
            Ok(location) => println!(
                "ACCEPTED {} (read-only probe)",
                location.database_path().display()
            ),
            Err(error) => println!("REFUSED {:?}: {}", error.kind, error),
        }
    }
}
