#![cfg(windows)]

use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct OwnedCli(Child);
impl Drop for OwnedCli {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_some() {
            return;
        }
        let _ = self.0.kill();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if self.0.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn real_cli_launch_review_uses_only_stdin_selectors_and_has_bounded_safe_output() {
    for action in ["start", "resume"] {
        let request = format!(
            r#"{{"agent":"codex","action":"{action}","placement":{{"program":"system","home":"box"}}}}"#
        );
        let mut child = OwnedCli(
            Command::new(env!("CARGO_BIN_EXE_shinatama"))
                .args(["launch", "preflight", "--json"])
                .env_clear()
                .env("HOME", r"C:\fictional-shinatama-cli-home-never-read")
                .env(
                    "SHINATAMA_HOME",
                    r"C:\fictional-shinatama-cli-state-never-created",
                )
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .creation_flags(0x0800_0000)
                .spawn()
                .unwrap(),
        );
        child
            .0
            .stdin
            .take()
            .unwrap()
            .write_all(request.as_bytes())
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "The owned CLI exceeded its preflight deadline."
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(status.code(), Some(2));
        // This known CLI emits one small summary and has no descendants.
        let mut output = Vec::new();
        child
            .0
            .stdout
            .take()
            .unwrap()
            .take(8193)
            .read_to_end(&mut output)
            .unwrap();
        assert!(
            output.len() <= 8192,
            "CLI summary exceeded its output budget."
        );
        let mut diagnostics = Vec::new();
        child
            .0
            .stderr
            .take()
            .unwrap()
            .take(1025)
            .read_to_end(&mut diagnostics)
            .unwrap();
        assert!(
            diagnostics.is_empty(),
            "A successful read-only refusal needs no raw diagnostics."
        );
        let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(report["status"], "refused");
        assert_eq!(report["action"], action);
        assert_eq!(report["agent"], "codex");
        assert!(
            report.get("argv").is_none()
                && report.get("env").is_none()
                && report.get("token").is_none(),
            "Only the safe engine summary may be emitted."
        );
    }
}
