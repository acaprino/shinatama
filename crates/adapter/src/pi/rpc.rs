//! Pi RPC (`--mode rpc`) control channel, read-only subset. RPC runs Pi as a
//! structured backend WITHOUT a TUI pane; Pi has no mode in which its TUI
//! attaches to a backend (PI5). Only state/command listing and a fixed refusal
//! probe can be encoded here: prompt, steer, follow-up, bash, model, session or
//! extension-UI commands are not representable, so nothing in this module can
//! start a model run or send provider traffic.
use crate::{Operation, RefusalReason};
use std::ffi::OsString;

/// Bounded JSONL framing (docs/rpc.md): split on LF only, strip one CR.
pub const MAX_RECORD_BYTES: usize = 16 * 1024;
pub const MAX_RECORDS: usize = 64;
/// A command name Pi 1.4.0 does not know; its refusal is part of the proof.
pub const REFUSAL_PROBE: &str = "shinatama_unsupported_probe";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadOnlyCommand {
    GetState,
    GetCommands,
}
impl ReadOnlyCommand {
    pub fn name(self) -> &'static str {
        match self {
            Self::GetState => "get_state",
            Self::GetCommands => "get_commands",
        }
    }
}

/// Arguments after the script token: no session file, no discovered resources.
pub fn control_arguments() -> Vec<OsString> {
    ["--mode", "rpc", "--no-session"]
        .into_iter()
        .chain(super::run::DISCOVERY_SWITCHES)
        .map(OsString::from)
        .collect()
}

/// One JSONL record. Ids are fixed ASCII tokens chosen by trusted code.
pub fn encode(id: &str, command: ReadOnlyCommand) -> Vec<u8> {
    record(id, command.name())
}
fn record(id: &str, command: &str) -> Vec<u8> {
    debug_assert!(
        id.bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    );
    let mut bytes = serde_json::to_vec(&serde_json::json!({ "id": id, "type": command }))
        .expect("fixed record");
    bytes.push(b'\n');
    bytes
}

/// The complete stdin of the contained control check: state, command list and
/// one unsupported command. Closing stdin afterwards ends Pi's RPC loop.
pub fn control_script() -> Vec<u8> {
    let mut bytes = encode("shn-state", ReadOnlyCommand::GetState);
    bytes.extend(encode("shn-commands", ReadOnlyCommand::GetCommands));
    bytes.extend(record("shn-refusal", REFUSAL_PROBE));
    bytes
}

/// ADR-0011 operations are not routed over this channel. Attach does not exist
/// for Pi; message/headless/event operations need a model run and stay
/// unqualified until a provider-backed fixture exists.
pub fn operation_over_rpc(operation: Operation) -> Result<(), RefusalReason> {
    Err(match operation {
        Operation::AttachBackend => RefusalReason::NotImplemented,
        Operation::QueueMessage | Operation::AskHeadless | Operation::StructuredEvents => {
            RefusalReason::NotQualified
        }
        _ => RefusalReason::NotImplemented,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    UnknownCommand,
    Other,
}

/// Closed projection of one response. Model, provider, cost and error text are
/// never retained; only the fields a control check needs.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ResponseSummary {
    pub id: Option<String>,
    pub command: String,
    pub success: bool,
    pub refusal: Option<Refusal>,
    pub is_streaming: Option<bool>,
    pub message_count: Option<u64>,
    pub command_count: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ControlSummary {
    pub responses: Vec<ResponseSummary>,
    pub events: usize,
    pub invalid_records: usize,
    pub truncated: bool,
}
impl ControlSummary {
    fn response(&self, id: &str) -> Option<&ResponseSummary> {
        self.responses
            .iter()
            .find(|response| response.id.as_deref() == Some(id))
    }
    /// State read while idle with no messages, command listing answered, and
    /// the unsupported command refused as unknown. No event means no run.
    pub fn proven(&self) -> bool {
        let state = self.response("shn-state");
        let commands = self.response("shn-commands");
        let refusal = self.response("shn-refusal");
        self.invalid_records == 0
            && !self.truncated
            && self.events == 0
            && self.responses.len() == 3
            && state.is_some_and(|state| {
                state.command == "get_state"
                    && state.success
                    && state.is_streaming == Some(false)
                    && state.message_count == Some(0)
            })
            && commands.is_some_and(|commands| {
                commands.command == "get_commands"
                    && commands.success
                    && commands.command_count.is_some()
            })
            && refusal.is_some_and(|refusal| {
                refusal.command == REFUSAL_PROBE
                    && !refusal.success
                    && refusal.refusal == Some(Refusal::UnknownCommand)
            })
    }
}

pub fn summarize(stdout: &[u8]) -> ControlSummary {
    let mut summary = ControlSummary::default();
    let mut parts = stdout.split(|byte| *byte == b'\n').peekable();
    let mut count = 0;
    while let Some(line) = parts.next() {
        // A final empty slice after the last LF is not a record.
        if line.is_empty() && parts.peek().is_none() {
            break;
        }
        count += 1;
        if count > MAX_RECORDS {
            summary.truncated = true;
            break;
        }
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        match parse(line) {
            Some(Record::Response(response)) => summary.responses.push(response),
            Some(Record::Event) => summary.events += 1,
            None => summary.invalid_records += 1,
        }
    }
    summary
}

enum Record {
    Response(ResponseSummary),
    Event,
}
fn parse(line: &[u8]) -> Option<Record> {
    if line.len() > MAX_RECORD_BYTES {
        return None;
    }
    let serde_json::Value::Object(record) = serde_json::from_slice(line).ok()? else {
        return None;
    };
    let kind = record.get("type")?.as_str()?;
    if kind != "response" {
        return Some(Record::Event);
    }
    let text = |key: &str| record.get(key).and_then(serde_json::Value::as_str);
    let id = match record.get("id") {
        None => None,
        Some(value) => Some(value.as_str()?.chars().take(64).collect()),
    };
    let command = text("command")?.chars().take(64).collect::<String>();
    let success = record.get("success")?.as_bool()?;
    let data = record.get("data");
    let field = |key: &str| data.and_then(|data| data.get(key));
    Some(Record::Response(ResponseSummary {
        refusal: (!success).then(|| match text("error") {
            Some(error) if error.starts_with("Unknown command:") => Refusal::UnknownCommand,
            _ => Refusal::Other,
        }),
        is_streaming: field("isStreaming").and_then(serde_json::Value::as_bool),
        message_count: field("messageCount").and_then(serde_json::Value::as_u64),
        command_count: field("commands")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len),
        id,
        command,
        success,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Captured from pi 1.4.0 (`--mode rpc --no-session` with discovery off,
    // PI_OFFLINE=1, empty PI_CODING_AGENT_DIR); ids replaced by the script's.
    const STATE: &str = r#"{"id":"shn-state","type":"response","command":"get_state","success":true,"data":{"model":{"type":"chat","id":"moonshotai/kimi-k2.6","name":"MoonshotAI: Kimi K2.6","api":"openai-completions","baseUrl":"https://openrouter.ai/api/v1","provider":"openrouter","reasoning":true,"input":["text","image"],"cost":{"input":0.65,"output":3.41,"cacheRead":0.15,"cacheWrite":0},"contextWindow":262144,"maxTokens":235929},"thinkingLevel":"medium","isStreaming":false,"isCompacting":false,"steeringMode":"one-at-a-time","followUpMode":"one-at-a-time","sessionId":"01a1c49b-b9e0-7051-93f6-3cf6f18e994f","autoCompactionEnabled":true,"messageCount":0,"pendingMessageCount":0}}"#;
    const COMMANDS: &str = r#"{"id":"shn-commands","type":"response","command":"get_commands","success":true,"data":{"commands":[]}}"#;
    const REFUSED: &str = r#"{"id":"shn-refusal","type":"response","command":"shinatama_unsupported_probe","success":false,"error":"Unknown command: shinatama_unsupported_probe"}"#;

    fn transcript(lines: &[&str]) -> Vec<u8> {
        lines
            .iter()
            .flat_map(|line| line.bytes().chain(*b"\n"))
            .collect()
    }

    #[test]
    fn only_read_only_commands_and_the_fixed_refusal_probe_are_encodable() {
        let script = String::from_utf8(control_script()).unwrap();
        let lines: Vec<_> = script.lines().collect();
        assert_eq!(
            lines,
            [
                r#"{"id":"shn-state","type":"get_state"}"#,
                r#"{"id":"shn-commands","type":"get_commands"}"#,
                r#"{"id":"shn-refusal","type":"shinatama_unsupported_probe"}"#,
            ]
        );
        for forbidden in [
            "prompt",
            "steer",
            "follow_up",
            "bash",
            "set_model",
            "message",
        ] {
            assert!(!script.contains(forbidden), "{forbidden}");
        }
        let arguments = control_arguments();
        assert_eq!(
            &arguments[..3],
            ["--mode", "rpc", "--no-session"].map(OsString::from)
        );
        assert!(arguments.contains(&OsString::from("--no-extensions")));
        assert_eq!(
            operation_over_rpc(Operation::AttachBackend),
            Err(RefusalReason::NotImplemented)
        );
        for operation in Operation::ALL {
            assert!(operation_over_rpc(operation).is_err(), "{operation:?}");
        }
    }

    #[test]
    fn captured_control_transcript_is_proven_and_drops_model_details() {
        let summary = summarize(&transcript(&[STATE, COMMANDS, REFUSED]));
        assert!(summary.proven(), "{summary:?}");
        let projected = serde_json::to_string(&summary).unwrap();
        for private in [
            "kimi",
            "openrouter",
            "baseUrl",
            "01a1c49b",
            "Unknown command",
        ] {
            assert!(!projected.contains(private), "{private}");
        }
        // CRLF framing is accepted; a run event or a missing refusal is not.
        let crlf = format!("{STATE}\r\n{COMMANDS}\r\n{REFUSED}\r\n");
        assert!(summarize(crlf.as_bytes()).proven());
        let event = r#"{"type":"agent_start"}"#;
        assert!(!summarize(&transcript(&[STATE, COMMANDS, REFUSED, event])).proven());
        assert!(!summarize(&transcript(&[STATE, COMMANDS])).proven());
        let accepted = REFUSED.replace(r#""success":false"#, r#""success":true"#);
        assert!(!summarize(&transcript(&[STATE, COMMANDS, &accepted])).proven());
        let streaming = STATE.replace(r#""isStreaming":false"#, r#""isStreaming":true"#);
        assert!(!summarize(&transcript(&[&streaming, COMMANDS, REFUSED])).proven());
        assert!(!summarize(&transcript(&[STATE, "not json", COMMANDS, REFUSED])).proven());
    }

    #[test]
    fn unicode_line_separators_stay_inside_one_record() {
        let state = STATE.replace("MoonshotAI: Kimi K2.6", "a\u{2028}b\u{2029}c");
        let summary = summarize(&transcript(&[&state, COMMANDS, REFUSED]));
        assert_eq!(summary.responses.len(), 3);
        assert!(summary.proven());
    }
}
