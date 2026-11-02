#![cfg(windows)]

use std::os::windows::process::CommandExt;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path())
            && self
                .0
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("shinatama-doctor-"))
        {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn real_doctor_refuses_unqualified_programs_without_executing_wrappers_or_creating_state() {
    let fixture = Fixture(std::env::temp_dir().join(format!(
        "shinatama-doctor-{}-{}", std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    )));
    fs::create_dir(&fixture.0).unwrap();
    let programs = fixture.0.join("programs with spaces");
    fs::create_dir(&programs).unwrap();
    let native = programs.join("claude.exe");
    let wrapper = programs.join("codex.cmd");
    let sentinel = b"SYNTHETIC_PRIVATE_WRAPPER_CONTENT_MUST_NOT_BE_READ";
    fs::write(&native, b"synthetic non-executable installation fixture").unwrap();
    fs::write(&wrapper, sentinel).unwrap();
    let uncreated = fixture.0.join("must-not-be-created");
    let output = Command::new(env!("CARGO_BIN_EXE_shinatama"))
        .args(["doctor", "--json"])
        .current_dir(&fixture.0)
        .env_clear()
        .env("SystemRoot", std::env::var_os("SystemRoot").unwrap())
        .env("PATH", &programs)
        .env("HOME", &uncreated)
        .env("USERPROFILE", &uncreated)
        .env("LOCALAPPDATA", &uncreated)
        .env("APPDATA", &uncreated)
        .env("SHINATAMA_HOME", &uncreated)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW; only the test-owned CLI.
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    assert!(output.stdout.len() < 1024 * 1024);
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains(std::str::from_utf8(sentinel).unwrap())
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["read_only"], true);
    let agents = report["agents"].as_array().unwrap();
    for id in ["claude_code", "codex"] {
        let agent = agents.iter().find(|agent| agent["agent"] == id).unwrap();
        assert_eq!(agent["status"], "degraded");
        assert_eq!(agent["installations"].as_array().unwrap().len(), 1);
    }
    assert!(!uncreated.exists());
    assert_eq!(fs::read(&wrapper).unwrap(), sentinel);
    assert_eq!(
        fs::read(&native).unwrap(),
        b"synthetic non-executable installation fixture"
    );
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&programs).unwrap().count(), 2);
}
