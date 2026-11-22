//! The Mukade revision is named in four places: the workspace dependency this crate builds the
//! program from, the reviewed pin manifest, the commit of the release whose terminal browser
//! `build-release.bat` downloads (the manifest's `release`), and `MUKADE_REVISION`, which the
//! session reader's worker protocol carries. A pin advance must move all four together.
use std::path::Path;

fn read(relative: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(relative),
    )
    .unwrap()
}

fn quoted_after<'a>(text: &'a str, marker: &str) -> &'a str {
    let rest = &text[text
        .find(marker)
        .unwrap_or_else(|| panic!("{marker} not found"))
        + marker.len()..];
    let rest = &rest[rest.find('"').unwrap() + 1..];
    &rest[..rest.find('"').unwrap()]
}

#[test]
fn the_workspace_the_manifest_and_the_reader_name_one_mukade_revision() {
    let cargo = read("Cargo.toml");
    let line = cargo
        .lines()
        .find(|line| line.starts_with("mukade = "))
        .expect("the mukade workspace dependency");
    let dependency = quoted_after(line, "rev =");
    assert_eq!(dependency.len(), 40, "a full immutable revision: {line}");
    assert!(!line.contains("tag") && !line.contains("branch"), "{line}");
    let manifest: serde_json::Value =
        serde_json::from_str(&read("scripts/mukade-pin.json")).unwrap();
    assert_eq!(manifest["revision"], dependency);
    assert_eq!(
        manifest["release"]["commit"], dependency,
        "the release of the terminal browser"
    );
    assert_eq!(
        quoted_after(
            &read("crates/engine/src/session_roots.rs"),
            "pub const MUKADE_REVISION: &str ="
        ),
        dependency
    );
    assert!(read("scripts/mukade-pin.ps1").contains(dependency));
}
