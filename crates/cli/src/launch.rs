//! Read-only launch review. This caller cannot supply trusted launch inputs or execute a plan.
use crate::ExitCode;
use shinatama_engine::{
    Engine,
    launch::{LaunchIntent, LaunchStatus},
};
use std::io::{Read, Write};

pub const MAX_LAUNCH_INPUT_BYTES: usize = 8 * 1024;

/// Read one bounded JSON request from stdin and forward it to the same engine
/// service used by the desktop. Start and Resume differ only by the typed action.
/// Invalid input and IO failures never echo source text or parser diagnostics.
pub fn run_launch_preflight(
    engine: &Engine,
    input: &mut dyn Read,
    output: &mut dyn Write,
) -> std::io::Result<ExitCode> {
    let mut bytes = Vec::new();
    if input
        .take(MAX_LAUNCH_INPUT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return refusal(
            output,
            "launch_input_unavailable",
            ExitCode::FailedUnchanged,
        );
    }
    if bytes.len() > MAX_LAUNCH_INPUT_BYTES {
        return refusal(output, "launch_input_too_large", ExitCode::Refused);
    }
    let intent = match serde_json::from_slice::<LaunchIntent>(&bytes) {
        Ok(intent) => intent,
        Err(_) => return refusal(output, "invalid_launch_request", ExitCode::Refused),
    };
    let preflight = engine.preflight_launch(&intent, None);
    let summary = preflight.summary();
    let exit = match &summary.status {
        LaunchStatus::Available => ExitCode::Done,
        LaunchStatus::Refused => ExitCode::Refused,
        LaunchStatus::Incomplete => ExitCode::NeedsUser,
    };
    serde_json::to_writer_pretty(&mut *output, summary).map_err(std::io::Error::other)?;
    writeln!(output)?;
    Ok(exit)
}

fn refusal(
    output: &mut dyn Write,
    code: &'static str,
    exit: ExitCode,
) -> std::io::Result<ExitCode> {
    serde_json::to_writer(
        &mut *output,
        &serde_json::json!({
            "status": "refused", "error": { "code": code },
        }),
    )
    .map_err(std::io::Error::other)?;
    writeln!(output)?;
    Ok(exit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn start_and_resume_return_the_identical_engine_summary_without_launch_inputs() {
        let engine = Engine::new();
        for action in ["start", "resume"] {
            let bytes = format!(
                r#"{{"agent":"codex","action":"{action}","placement":{{"program":"system","home":"box"}}}}"#
            );
            let intent: LaunchIntent = serde_json::from_str(&bytes).unwrap();
            let expected =
                serde_json::to_value(engine.preflight_launch(&intent, None).summary()).unwrap();
            let mut output = Vec::new();
            let exit = run_launch_preflight(&engine, &mut Cursor::new(bytes), &mut output).unwrap();
            let actual: serde_json::Value = serde_json::from_slice(&output).unwrap();
            assert_eq!(exit, ExitCode::Refused);
            assert!(
                actual == expected,
                "The caller must forward the engine's summary unchanged."
            );
            assert_eq!(actual["status"], "refused");
        }
    }

    #[test]
    fn malformed_unknown_or_private_input_has_a_constant_refusal_without_reflection() {
        const PRIVATE: &str = "SYNTHETIC_LAUNCH_PRIVATE_VALUE_NEVER_ECHO";
        for bytes in [
            PRIVATE.to_owned(),
            format!(
                r#"{{"agent":"codex","action":"{PRIVATE}","placement":{{"program":"system","home":"box"}}}}"#
            ),
            format!(
                r#"{{"agent":"codex","action":"start","placement":{{"program":"system","home":"box"}},"token":"{PRIVATE}","argv":["{PRIVATE}"],"env":{{"SECRET":"{PRIVATE}"}}}}"#
            ),
            r#"{"agent":"codex","action":"resume","placement":{"program":"own","home":"system"}}"#
                .to_owned(),
        ] {
            let mut output = Vec::new();
            let exit =
                run_launch_preflight(&Engine::new(), &mut Cursor::new(bytes), &mut output).unwrap();
            assert_eq!(exit, ExitCode::Refused);
            assert!(
                !String::from_utf8_lossy(&output).contains(PRIVATE),
                "Private input must not appear in refusal output."
            );
            let actual: serde_json::Value = serde_json::from_slice(&output).unwrap();
            assert_eq!(actual["status"], "refused");
            assert_eq!(actual["error"]["code"], "invalid_launch_request");
        }
    }

    #[test]
    fn oversized_input_is_stopped_at_the_byte_limit_before_deserialization() {
        struct CountingInput {
            read: usize,
        }
        impl Read for CountingInput {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                buffer.fill(b'x');
                self.read += buffer.len();
                Ok(buffer.len())
            }
        }
        let mut input = CountingInput { read: 0 };
        let mut output = Vec::new();
        assert_eq!(
            run_launch_preflight(&Engine::new(), &mut input, &mut output).unwrap(),
            ExitCode::Refused
        );
        assert_eq!(input.read, MAX_LAUNCH_INPUT_BYTES + 1);
        let actual: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(actual["error"]["code"], "launch_input_too_large");
    }

    #[test]
    fn input_io_errors_are_failed_unchanged_without_error_payloads() {
        struct FailedInput;
        impl Read for FailedInput {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("SYNTHETIC_PRIVATE_IO_PAYLOAD"))
            }
        }
        let mut output = Vec::new();
        assert_eq!(
            run_launch_preflight(&Engine::new(), &mut FailedInput, &mut output).unwrap(),
            ExitCode::FailedUnchanged
        );
        assert!(
            !String::from_utf8_lossy(&output).contains("SYNTHETIC_PRIVATE_IO_PAYLOAD"),
            "IO diagnostics must not expose source payloads."
        );
    }
}
