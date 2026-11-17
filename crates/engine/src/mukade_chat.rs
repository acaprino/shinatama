//! A thread's Mukade room (ADR-0057): the hooks that make a native session a member of
//! it and wake it when a message is addressed to it, and a bounded client the window
//! uses through the bundled Mukade program.
//!
//! The hooks are Mukade's own adapter (`mukade chat hook <harness> <event>`): it joins
//! the room at the first turn, hands what other members wrote to the model at each turn
//! and, after a turn, waits for a message addressed to the member and continues the
//! session with it. Shinatama only composes them into its native launchers, next to
//! its own tracking hook, and passes the room in the child's environment.
//! Nothing here writes Mukade's database or its adapter state: the client goes through
//! the service, and the adapter state is only read, without its credential.
use serde::Deserialize;
use serde_json::Value;
use shinatama_state::location::StorageLocation;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// How long a session waits, after each of its turns, for a message addressed to it.
/// While it waits the harness shows its stop hook running; typing or Escape ends it.
pub const LISTEN_SECONDS: u64 = 300;
/// How many times waiting may continue one session in a run of the harness.
pub const MAX_WAKES: u32 = 10;
/// The member name the person at the window writes with.
pub const USER_MEMBER: &str = "user";
/// The harness name of that member: Mukade asks every member for one.
pub const USER_HARNESS: &str = "shinatama";
/// The member Shinatama itself tells the sessions their place in the thread's team with (ADR-0058):
/// distinct from `user`, so the panel can draw its messages as notices. It is a host member too.
pub const TEAM_MEMBER: &str = "shinatama";
/// Mukade bounds a hook's stop at the listening time plus a margin.
const STOP_MARGIN_SECONDS: u64 = 30;
const CLI_TIMEOUT: Duration = Duration::from_secs(20);
/// Mukade bounds each answer of Codex's app-server at 30 s; one is enough in practice.
const TRUST_TIMEOUT: Duration = Duration::from_secs(60);
/// How long a refused approval is remembered before a launch tries again.
const TRUST_RETRY: Duration = Duration::from_secs(600);
const MAX_OUTPUT: usize = 4 * 1024 * 1024;
const MAX_ADAPTER_STATE: u64 = 16 * 1024;

/// The data folder of the service sessions use: always Shinatama's own, never another
/// installation's (an inherited MUKADE_DATA_DIR is ignored).
pub fn data_directory(base: &StorageLocation) -> PathBuf {
    crate::mukade_mcp::own_directory(base)
}

/// A Mukade room ID as the service hands it out; also safe in an environment value.
pub fn valid_room_id(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
}

/// The hooks for one launch into one room.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatHooks {
    room_id: String,
    program: PathBuf,
    data_dir: PathBuf,
}

/// One Codex hook: its event, as the `-c hooks` table names it, its command line and timeout.
/// Command and timeout are exactly Mukade's own (`harnesses::codex::chat_hooks`), because Codex
/// trusts a hook by the hash of its event and definition: the approval `mukade chat trust codex`
/// writes (`trust_codex`) is valid only for these same definitions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodexHook {
    pub table_key: &'static str,
    pub command: String,
    pub timeout: u64,
}
/// Mukade's `adapter::STOP_HOOK_TIMEOUT` (its longest listening, 3600 s, plus 30 s): the stop
/// hook's timeout is part of the identity Codex trusts, so it is Mukade's, not Shinatama's.
pub const CODEX_STOP_HOOK_TIMEOUT: u64 = 3600 + 30;

impl ChatHooks {
    /// `None` without a bundled Mukade program: the hooks have nothing to run.
    /// `None` also when the program's path cannot be carried by a hook command line (not a
    /// drive path, not UTF-8, control characters): the session then opens as an ordinary one.
    pub fn new(base: &StorageLocation, room_id: &str) -> Option<Self> {
        if !valid_room_id(room_id) {
            return None;
        }
        let program = crate::mukade_mcp::bundled_program()?;
        let hooks = Self::with_program(room_id, program, data_directory(base));
        hooks.program_path().is_some().then_some(hooks)
    }
    fn with_program(room_id: &str, program: PathBuf, data_dir: PathBuf) -> Self {
        Self { room_id: room_id.into(), program, data_dir }
    }
    pub fn room_id(&self) -> &str {
        &self.room_id
    }
    pub fn stop_timeout() -> u64 {
        LISTEN_SECONDS + STOP_MARGIN_SECONDS
    }
    /// After `mukade_mcp::clear_environment`: the room, the role and the listening
    /// budget the adapter reads, and the service's folder its hooks connect through.
    pub fn apply_environment(&self, environment: &mut BTreeMap<OsString, OsString>) {
        for (name, value) in [
            ("MUKADE_CHAT_ROOM_ID", OsString::from(&self.room_id)),
            ("MUKADE_CHAT_ROLE", "peer".into()),
            ("MUKADE_CHAT_LISTEN", LISTEN_SECONDS.to_string().into()),
            ("MUKADE_CHAT_MAX_WAKES", MAX_WAKES.to_string().into()),
            ("MUKADE_DATA_DIR", self.data_dir.clone().into_os_string()),
        ] {
            environment.retain(|key, _| !key.eq_ignore_ascii_case(name));
            environment.insert(name.into(), value);
        }
    }
    /// The program as a drive path with forward slashes, or `None` for anything a
    /// hook command line could not carry safely.
    fn program_path(&self) -> Option<String> {
        let path = self.program.to_str()?;
        let path = path.strip_prefix(r"\\?\").unwrap_or(path).replace('\\', "/");
        let bytes = path.as_bytes();
        (path.len() <= 4096
            && bytes.len() > 3
            && bytes[0].is_ascii_alphabetic()
            && &bytes[1..3] == b":/"
            && !path.chars().any(char::is_control))
        .then_some(path)
    }
    /// Claude Code's settings entries, in the quoting of Shinatama's own Claude hook.
    pub fn claude_hooks(&self) -> Option<Vec<(&'static str, Value)>> {
        let program = self.program_path()?.replace('\'', "'\"'\"'");
        let hook = |event: &str, timeout: u64| {
            serde_json::json!({"hooks": [{"type": "command", "timeout": timeout,
                "command": format!("'{program}' chat hook claude_code {event}")}]})
        };
        Some(vec![
            ("SessionStart", hook("session-start", 30)),
            ("UserPromptSubmit", hook("prompt", 30)),
            ("Stop", hook("stop", Self::stop_timeout())),
        ])
    }
    /// Codex's prompt and stop hooks, as Mukade writes them for Windows, where Codex runs a hook
    /// with PowerShell: `& '<program>' chat hook codex <event>` (Mukade's live record, runs 6-8).
    /// No session-start hook: that place belongs to Shinatama's own hook, and the first prompt
    /// joins the room. The two keep their place (`…:user_prompt_submit:0:0`, `…:stop:0:0`), which
    /// is the key Mukade's trust records.
    pub fn codex_hooks(&self) -> Option<Vec<CodexHook>> {
        let program = self.program_path()?.replace('\'', "''");
        let command = |event: &str| format!("& '{program}' chat hook codex {event}");
        Some(vec![
            CodexHook { table_key: "UserPromptSubmit", command: command("prompt"), timeout: 30 },
            CodexHook { table_key: "Stop", command: command("stop"), timeout: CODEX_STOP_HOOK_TIMEOUT },
        ])
    }
}

/// The sessions launched in this run of Shinatama with Mukade's hooks, and the room each was
/// launched into. The adapter's record of a session outlives the run that wrote it; only this
/// says whether the session running now has the hooks.
static LAUNCHED: std::sync::Mutex<BTreeMap<String, String>> = std::sync::Mutex::new(BTreeMap::new());
/// Called by a launcher once it composed the hooks for `session` (a stored session record id).
pub fn note_launch(session: &str, room_id: &str) {
    if let Ok(mut launched) = LAUNCHED.lock() {
        launched.insert(session.to_owned(), room_id.to_owned());
    }
}
/// The room `session` was last launched into with Mukade's hooks in this run, if any.
pub fn launched_room(session: &str) -> Option<String> {
    LAUNCHED.lock().ok()?.get(session).cloned()
}

/// What Mukade's adapter remembers about one native session: its membership, never
/// its handle, which is a credential and is not read into Shinatama.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct AdapterMember {
    pub room_id: String,
    pub member_id: String,
    pub name: String,
    pub role: String,
    #[serde(default)]
    pub wakes: u32,
}

/// The adapter's record of `native_id` of `harness`, if its hooks joined a room.
pub fn adapter_member(base: &StorageLocation, harness: &str, native_id: &str) -> Option<AdapterMember> {
    let plain = |value: &str, limit: usize| {
        (1..=limit).contains(&value.len())
            && value.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    };
    if !plain(harness, 24) || !plain(native_id, 128) {
        return None;
    }
    let path = data_directory(base)
        .join("bus")
        .join("adapters")
        .join(harness)
        .join(format!("{native_id}.json"));
    crate::mukade_mcp::reject_reparse(&path).ok()?;
    let file = std::fs::File::open(&path).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_ADAPTER_STATE + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_ADAPTER_STATE {
        return None;
    }
    parse_adapter_member(&bytes)
}
fn parse_adapter_member(bytes: &[u8]) -> Option<AdapterMember> {
    // Unknown fields (the handle among them) are skipped, not kept.
    let member: AdapterMember = serde_json::from_slice(bytes).ok()?;
    (valid_room_id(&member.room_id) && valid_room_id(&member.member_id) && crate::session_control::member_name(&member.name))
        .then_some(member)
}

/// One `mukade chat <operation> … --json` through the bundled program, against the
/// service of `data_directory`. `text` goes through standard input (`--text-file -`),
/// never the command line. The error is Mukade's own `code: message` line.
///
/// `join` with the bare flag `("host", "")`, `set-role` and `retire` are what only the host may
/// do (ADR-0058): the program then reads the host's own credential from the service's folder, as
/// it reads the member credential for every other operation. No credential is ever an argument.
pub fn call(base: &StorageLocation, operation: &str, arguments: &[(&str, &str)], text: Option<&str>) -> Result<Value, String> {
    let argv = command_line(operation, arguments, text)?;
    let environment = [(OsString::from("MUKADE_DATA_DIR"), data_directory(base).into_os_string())];
    run(&argv, &environment, None, text, CLI_TIMEOUT, operation)
}

/// The operations Shinatama runs, and the names that are bare flags (no value follows them).
const OPERATIONS: &[&str] = &["open", "join", "rejoin", "members", "ledger", "send", "rooms", "set-role", "retire"];
const FLAGS: &[&str] = &["host"];
/// Names no argument may have: the service's and the host's credentials stay in Mukade's folder.
const CREDENTIALS: &[&str] = &["credential", "host-credential", "token", "authorization"];

fn command_line(operation: &str, arguments: &[(&str, &str)], text: Option<&str>) -> Result<Vec<String>, String> {
    if !OPERATIONS.contains(&operation) {
        return Err("This chat operation is not used by Shinatama.".into());
    }
    let mut argv: Vec<String> = vec!["chat".into(), operation.into()];
    for (name, value) in arguments {
        if !name.bytes().all(|c| c.is_ascii_lowercase() || c == b'-')
            || CREDENTIALS.contains(name)
            || value.len() > 4096
            || value.chars().any(char::is_control)
            || (FLAGS.contains(name) && !value.is_empty())
        {
            return Err("A chat argument is invalid.".into());
        }
        argv.push(format!("--{name}"));
        if !FLAGS.contains(name) {
            argv.push((*value).into());
        }
    }
    if text.is_some() {
        argv.extend(["--text-file".into(), "-".into()]);
    }
    argv.push("--json".into());
    Ok(argv)
}

/// Codex's own approval of Mukade's hooks, written by `mukade chat trust codex` into the
/// `config.toml` of the Codex home Shinatama launches with (its private home), as Codex's
/// `/hooks` would: Codex's app-server lists the hooks with their keys and hashes, and the
/// approval is recorded for exactly those. Nothing outside that home is written, and no
/// approval is skipped or bypassed. `Ok` only when Codex then reports every one of Mukade's
/// hooks trusted. Already trusted hooks are left as they are, so a repeat is harmless; each
/// (home, Codex program) pair is checked once per run of Shinatama, a refusal again after a while.
pub fn trust_codex(base: &StorageLocation, codex_home: &Path, private_root: &Path, codex_program: &Path, project: &Path) -> Result<(), String> {
    // A success holds for the run; a refusal is remembered for a while, so a Codex build that
    // does not take Mukade's hooks does not delay every launch with another attempt.
    type Outcomes = BTreeMap<(PathBuf, PathBuf), (Result<(), String>, Instant)>;
    static OUTCOMES: std::sync::Mutex<Outcomes> = std::sync::Mutex::new(BTreeMap::new());
    let pair = (codex_home.to_owned(), codex_program.to_owned());
    if let Some((outcome, at)) = OUTCOMES.lock().ok().and_then(|outcomes| outcomes.get(&pair).cloned())
        && (outcome.is_ok() || at.elapsed() < TRUST_RETRY)
    {
        return outcome;
    }
    let outcome = trust_codex_now(base, codex_home, private_root, codex_program, project);
    if let Ok(mut outcomes) = OUTCOMES.lock() {
        outcomes.insert(pair, (outcome.clone(), Instant::now()));
    }
    outcome
}
fn trust_codex_now(base: &StorageLocation, codex_home: &Path, private_root: &Path, codex_program: &Path, project: &Path) -> Result<(), String> {
    // Codex is started as Shinatama starts it: its home, profile and application folders
    // are the private ones, so the approval lands in the config.toml that session reads.
    let environment = [
        (OsString::from("CODEX_HOME"), codex_home.as_os_str().to_owned()),
        (OsString::from("HOME"), codex_home.as_os_str().to_owned()),
        (OsString::from("USERPROFILE"), codex_home.as_os_str().to_owned()),
        (OsString::from("APPDATA"), private_root.join("roaming").into_os_string()),
        (OsString::from("LOCALAPPDATA"), private_root.join("local").into_os_string()),
        (OsString::from("MUKADE_CODEX_EXE"), codex_program.as_os_str().to_owned()),
        (OsString::from("MUKADE_DATA_DIR"), data_directory(base).into_os_string()),
    ];
    let argv = ["chat".to_owned(), "trust".to_owned(), "codex".to_owned()];
    let report = run(&argv, &environment, Some(project), None, TRUST_TIMEOUT, "trust")?;
    if report["status"] != "trusted" {
        let hooks: Vec<String> = report["hooks"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|hook| format!("{} {}", hook["event"].as_str().unwrap_or("?"), hook["after"].as_str().unwrap_or("?")))
            .collect();
        return Err(format!("Codex did not confirm Mukade's hooks as trusted ({}).", hooks.join(", ")));
    }
    Ok(())
}

/// One bounded run of the bundled program, without a console. Both pipes drain on their own
/// threads: a long ledger page fills a pipe buffer long before the program ends.
fn run(
    argv: &[String],
    extra: &[(OsString, OsString)],
    cwd: Option<&Path>,
    text: Option<&str>,
    timeout: Duration,
    operation: &str,
) -> Result<Value, String> {
    let program = crate::mukade_mcp::bundled_program()
        .ok_or_else(|| "This Shinatama has no Mukade program in its mukade folder.".to_owned())?;
    let mut environment: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
    crate::mukade_mcp::isolate_environment(&mut environment);
    for (name, value) in extra {
        environment.retain(|key, _| !key.eq_ignore_ascii_case(name));
        environment.insert(name.clone(), value.clone());
    }
    let mut command = std::process::Command::new(&program);
    command
        .args(argv)
        .env_clear()
        .envs(environment)
        .stdin(if text.is_some() { std::process::Stdio::piped() } else { std::process::Stdio::null() })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = command.spawn().map_err(|_| "The bundled Mukade program could not be run.".to_owned())?;
    if let (Some(text), Some(mut stdin)) = (text, child.stdin.take()) {
        let written = stdin.write_all(text.as_bytes());
        drop(stdin);
        if written.is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("The message could not be handed to Mukade.".into());
        }
    }
    let drain = |stream: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(stream) = stream {
                let _ = stream.take(MAX_OUTPUT as u64 + 1).read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let stdout = drain(child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let stderr = drain(child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Mukade chat {operation} did not finish in time."));
            }
        }
    };
    let output = stdout.join().unwrap_or_default();
    let errors = stderr.join().unwrap_or_default();
    if !status.success() {
        let line: String = String::from_utf8_lossy(&errors)
            .lines()
            .find(|line| !line.starts_with("request_id:"))
            .unwrap_or_default()
            .chars()
            .filter(|c| !c.is_control())
            .take(400)
            .collect();
        return Err(if line.is_empty() { format!("Mukade chat {operation} failed.") } else { line });
    }
    if output.len() > MAX_OUTPUT {
        return Err(format!("Mukade chat {operation} answered more than Shinatama reads."));
    }
    serde_json::from_slice(&output).map_err(|_| format!("Mukade chat {operation} gave no readable answer."))
}

/// Mukade's launch kit for OpenCode or Pi (`mukade shinatama-kit`, built on Mukade's own
/// `chat_launch`), written into `folder`: the arguments it adds and, for OpenCode, the inline
/// configuration merged into `opencode_config`. Starts no harness; holds no credential.
pub struct PluginKit {
    pub arguments: Vec<OsString>,
    pub opencode_config: Option<String>,
}
pub fn plugin_kit(base: &StorageLocation, harness: &str, folder: &Path, connection: &crate::mukade_mcp::Connection, opencode_config: Option<&str>) -> Result<PluginKit, String> {
    if !matches!(harness, "opencode" | "pi") {
        return Err("Only OpenCode and Pi take Mukade's plugin kit.".into());
    }
    let folder = folder.to_str().ok_or("The kit folder cannot be named to Mukade.")?;
    let argv = [
        "shinatama-kit".to_owned(),
        harness.to_owned(),
        folder.strip_prefix(r"\\?\").unwrap_or(folder).to_owned(),
        connection.endpoint().to_owned(),
        connection.service_id().to_owned(),
    ];
    let mut extra = vec![(OsString::from("MUKADE_DATA_DIR"), data_directory(base).into_os_string())];
    if let Some(config) = opencode_config {
        extra.push((OsString::from("OPENCODE_CONFIG_CONTENT"), OsString::from(config)));
    }
    let value = run(&argv, &extra, None, None, CLI_TIMEOUT, "kit")?;
    let arguments = value["arguments"]
        .as_array()
        .ok_or("Mukade gave no kit arguments.")?
        .iter()
        .map(|argument| argument.as_str().filter(|text| !text.chars().any(char::is_control)).map(OsString::from))
        .collect::<Option<Vec<_>>>()
        .ok_or("Mukade gave an invalid kit argument.")?;
    let opencode_config = value["opencode_config"].as_str().map(str::to_owned);
    if harness == "opencode" && opencode_config.is_none() {
        return Err("Mukade gave no OpenCode configuration.".into());
    }
    Ok(PluginKit { arguments, opencode_config })
}

/// The room of a thread, opened (or found: Mukade returns the existing room for a
/// known name) by its name. A fresh request id is right here: `open` is idempotent by
/// name, not only by request.
pub fn open_room(base: &StorageLocation, room_name: &str) -> Result<String, String> {
    let request = uuid::Uuid::new_v4().to_string();
    let answer = call(base, "open", &[("request-id", &request), ("name", room_name)], None)?;
    answer["room_id"]
        .as_str()
        .filter(|id| valid_room_id(id))
        .map(str::to_owned)
        .ok_or_else(|| "Mukade did not return the room.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hooks() -> ChatHooks {
        ChatHooks::with_program("0123456789abcdef0123456789abcdef", PathBuf::from(r"C:\Shinatama\mukade\mukade.exe"), PathBuf::from(r"C:\Shinatama\data\mukade"))
    }

    #[test]
    fn environment_names_the_room_and_replaces_inherited_membership() {
        let mut environment = BTreeMap::from([
            (OsString::from("mukade_chat_room_id"), OsString::from("parent")),
            (OsString::from("KEEP"), OsString::from("yes")),
        ]);
        hooks().apply_environment(&mut environment);
        assert_eq!(environment.get(&OsString::from("MUKADE_CHAT_ROOM_ID")).unwrap(), "0123456789abcdef0123456789abcdef");
        assert!(!environment.contains_key(&OsString::from("mukade_chat_room_id")));
        assert_eq!(environment.get(&OsString::from("MUKADE_CHAT_ROLE")).unwrap(), "peer");
        assert_eq!(environment.get(&OsString::from("MUKADE_CHAT_LISTEN")).unwrap(), "300");
        assert_eq!(environment.get(&OsString::from("MUKADE_DATA_DIR")).unwrap(), r"C:\Shinatama\data\mukade");
        assert_eq!(environment.get(&OsString::from("KEEP")).unwrap(), "yes");
    }

    #[test]
    fn hook_commands_run_mukade_s_adapter_for_each_turn_boundary() {
        let claude = hooks().claude_hooks().unwrap();
        let events: Vec<_> = claude.iter().map(|(event, _)| *event).collect();
        assert_eq!(events, ["SessionStart", "UserPromptSubmit", "Stop"]);
        assert_eq!(claude[2].1["hooks"][0]["command"], "'C:/Shinatama/mukade/mukade.exe' chat hook claude_code stop");
        assert_eq!(claude[2].1["hooks"][0]["timeout"], 330);
        let codex = hooks().codex_hooks().unwrap();
        // Exactly Mukade's Windows hook lines and timeouts, so Mukade's trust applies to them.
        assert_eq!(codex[0].command, "& 'C:/Shinatama/mukade/mukade.exe' chat hook codex prompt");
        assert_eq!((codex[0].table_key, codex[0].timeout), ("UserPromptSubmit", 30));
        assert_eq!(codex[1].command, "& 'C:/Shinatama/mukade/mukade.exe' chat hook codex stop");
        assert_eq!((codex[1].table_key, codex[1].timeout), ("Stop", 3630));
        let odd = ChatHooks::with_program("room", PathBuf::from("relative/mukade.exe"), PathBuf::new());
        assert!(odd.claude_hooks().is_none() && odd.codex_hooks().is_none());
        let quoted = ChatHooks::with_program("room", PathBuf::from(r"C:\O'Brien\mukade.exe"), PathBuf::new());
        assert!(quoted.codex_hooks().unwrap()[0].command.contains("C:/O''Brien/mukade.exe"));
    }

    #[test]
    fn the_host_operations_are_run_with_a_bare_flag_and_no_credential_on_the_command_line() {
        let line = |operation: &str, arguments: &[(&str, &str)], text: Option<&str>| command_line(operation, arguments, text);
        let join = line("join", &[("request-id", "r"), ("room-id", "room"), ("name", "shinatama"), ("host", "")], None).unwrap();
        assert_eq!(join, ["chat", "join", "--request-id", "r", "--room-id", "room", "--name", "shinatama", "--host", "--json"]);
        let role = line("set-role", &[("room-id", "room"), ("member-handle", "h"), ("member-id", "m"), ("role", "lead")], None).unwrap();
        assert_eq!(&role[..2], ["chat", "set-role"]);
        assert!(line("retire", &[("member-id", "m")], None).is_ok());
        // The text goes through standard input, never the command line.
        assert_eq!(line("send", &[("room-id", "room")], Some("hello")).unwrap()[4..], ["--text-file", "-", "--json"]);
        // `host` is a flag, not a field: it takes no value, and nothing else is a bare flag.
        assert!(line("join", &[("host", "true")], None).is_err());
        assert!(line("join", &[("name", "")], None).is_ok());
        // The host's credential is read by Mukade's command from its own folder; none is ever an argument.
        for name in ["host-credential", "credential", "token", "authorization"] {
            assert!(line("set-role", &[(name, "x")], None).is_err(), "{name}");
        }
        // Operations Shinatama does not use stay refused.
        for operation in ["delete", "close", "read", "wait", "ack", "observe"] {
            assert!(line(operation, &[], None).is_err(), "{operation}");
        }
    }

    #[test]
    fn launches_are_remembered_per_session_for_this_run() {
        assert_eq!(launched_room("t_registrytest1"), None);
        note_launch("t_registrytest1", "room-a");
        note_launch("t_registrytest1", "room-b");
        assert_eq!(launched_room("t_registrytest1").as_deref(), Some("room-b"));
    }

    #[test]
    fn adapter_state_is_read_without_its_handle_and_validated() {
        let state = br#"{"room_id":"abc123","member_id":"def456","member_handle":"0f0e0d0c-0000-4000-8000-000000000000","name":"claude-1","role":"peer","delivered":null,"wakes":2,"bound":false}"#;
        let member = parse_adapter_member(state).unwrap();
        assert_eq!(member.name, "claude-1");
        assert_eq!(member.wakes, 2);
        assert!(!format!("{member:?}").contains("0f0e0d0c"));
        assert!(parse_adapter_member(br#"{"room_id":"a b","member_id":"x","name":"n","role":"peer"}"#).is_none());
        assert!(parse_adapter_member(br#"{"room_id":"a","member_id":"x","name":"All","role":"peer"}"#).is_none());
        assert!(valid_room_id("0123456789abcdef0123456789abcdef") && !valid_room_id("") && !valid_room_id("a\nb"));
    }
}
