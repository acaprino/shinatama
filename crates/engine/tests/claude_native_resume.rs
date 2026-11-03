#![cfg(windows)]
//! Explicit local headless hook/persistence proof, not a TUI/IPC qualification.
//! Every native process belongs to the shared bounded C09 fixture.
#[path = "support/claude_native.rs"]
mod claude_native;

use claude_native::{Fixture, PrivateCase, Result, Scenario, checked, require, write_evidence};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use shinatama_engine::{
    claude_session_events::{ClaudeSessionEventReceiver, SessionEventKind},
    launch::identity::LaunchIdentity,
};
use shinatama_state::location::StorageLocation;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

const THREAD: &str = "t_abcdefghijkl";
const START_PROMPT: &str = "SHINATAMA_SYNTHETIC_START_PROMPT_A";
const RESUME_PROMPT: &str = "SHINATAMA_SYNTHETIC_RESUME_PROMPT_B";
const START_MARKER: &str = "SHINATAMA_SYNTHETIC_START_REPLY_A";
const RESUME_MARKER: &str = "SHINATAMA_SYNTHETIC_RESUME_REPLY_B";
const BASH: &str = "C:/Program Files/Git/usr/bin/bash.exe";
const BASH_SHA: &str = "acf4ecb52e601f7b4a37db51b07650b5d0315eafd010590e98079fa026da4b7b";
const TRANSCRIPT_CAP: usize = 1024 * 1024;

struct PinnedProgram {
    path: PathBuf,
    hash: String,
    _files: Vec<File>,
}
impl PinnedProgram {
    fn open(path: PathBuf) -> Result<Self> {
        checked(StorageLocation::preflight(&path), "local-proof-program")?;
        let ancestors: Vec<_> = path
            .parent()
            .ok_or("program-parent")?
            .ancestors()
            .take(65)
            .collect();
        require(ancestors.len() <= 64, "bounded-program-ancestors")?;
        let mut pins = Vec::new();
        for ancestor in ancestors.into_iter().rev() {
            let file = checked(
                OpenOptions::new()
                    .read(true)
                    .access_mode(FILE_READ_ATTRIBUTES)
                    .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                    .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                    .open(ancestor),
                "pin-proof-program-parent",
            )?;
            let flags =
                checked(file.metadata(), "proof-program-parent-metadata")?.file_attributes();
            require(
                flags & FILE_ATTRIBUTE_DIRECTORY != 0 && flags & FILE_ATTRIBUTE_REPARSE_POINT == 0,
                "ordinary-proof-program-parent",
            )?;
            pins.push(file);
        }
        let mut file = checked(
            OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&path),
            "pin-proof-program",
        )?;
        let metadata = checked(file.metadata(), "proof-program-metadata")?;
        require(
            metadata.file_attributes() & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT)
                == 0
                && (64..=16 * 1024 * 1024).contains(&metadata.len()),
            "bounded-proof-program",
        )?;
        let mut digest = Sha256::new();
        let mut buffer = [0; 64 * 1024];
        let mut count = 0_u64;
        loop {
            let read = checked(file.read(&mut buffer), "hash-proof-program")?;
            if read == 0 {
                break;
            }
            count += read as u64;
            require(count <= metadata.len(), "stable-proof-program")?;
            digest.update(&buffer[..read]);
        }
        require(count == metadata.len(), "stable-proof-program")?;
        pins.push(file);
        Ok(Self {
            path,
            hash: format!("{:x}", digest.finalize()),
            _files: pins,
        })
    }
}

fn hook_command(helper: &Path, port: u16) -> Result<String> {
    let path = helper.to_str().ok_or("utf8-hook-path")?;
    let path = path.strip_prefix(r"\\?\").unwrap_or(path);
    let bytes = path.as_bytes();
    require(
        port != 0
            && bytes.len() >= 3
            && bytes.len() <= 4096
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'/' | b'\\')
            && !path.chars().any(char::is_control),
        "ordinary-absolute-hook-path",
    )?;
    let escaped = path.replace('\\', "/").replace('\'', "'\"'\"'");
    Ok(format!("'{escaped}' claude-session {port}"))
}

fn arguments(settings: &Path, native_id: &str, resume: bool) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "--tools",
        "",
        "--disallowedTools",
        "mcp__*",
        "--mcp-config",
        "{\"mcpServers\":{}}",
        "--strict-mcp-config",
        "--setting-sources",
        "",
        "--settings",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    args.push(settings.as_os_str().to_owned());
    args.extend(
        [
            "--no-chrome",
            "--max-turns",
            "1",
            "--model",
            "claude-sonnet-4-6",
            "--output-format",
            "json",
            if resume { "--resume" } else { "--session-id" },
            native_id,
            "-p",
            if resume { RESUME_PROMPT } else { START_PROMPT },
        ]
        .into_iter()
        .map(OsString::from),
    );
    args
}

fn same_path(left: &Path, right: &Path) -> Result<bool> {
    Ok(checked(left.canonicalize(), "canonical-synthetic-project")?
        .as_os_str()
        .eq_ignore_ascii_case(
            checked(right.canonicalize(), "canonical-observed-project")?.as_os_str(),
        ))
}

fn observed_start(
    receiver: &ClaudeSessionEventReceiver,
    identity: &LaunchIdentity,
    native_id: &str,
    project: &Path,
) -> Result<bool> {
    let mut starts = 0;
    // The hook returns only after the receiver queued its event. SessionEnd is
    // supplementary; only the exact C09 owner can prove process cleanup.
    while let Some(event) = checked(receiver.try_next(), "native-hook-receiver")? {
        require(
            event.instance_id == identity.instance_id()
                && event.thread_id == THREAD
                && event.session_id == native_id
                && same_path(project, Path::new(&event.cwd))?,
            "exact-native-hook-binding",
        )?;
        if event.kind == SessionEventKind::SessionStart {
            starts += 1;
        }
    }
    let status = receiver.status();
    require(
        !status.failed && !status.queue_overflowed && status.rejected_requests == 0,
        "complete-authenticated-hook-observation",
    )?;
    require(starts == 1, "one-genuine-session-start")?;
    Ok(true)
}

fn native_result(bytes: &[u8], native_id: &str, marker: &str) -> Result<()> {
    let value: Value = checked(serde_json::from_slice(bytes), "bounded-native-json-result")?;
    require(
        value["type"] == "result"
            && value["is_error"] == false
            && value["session_id"].as_str() == Some(native_id)
            && value["result"]
                .as_str()
                .is_some_and(|text| text.trim() == marker),
        "exact-native-session-result",
    )
}

/// Test-only inspection of files this fixture caused Claude to create. This is
/// not a product archive reader and cannot accept any user-selected path.
fn transcript(case: &PrivateCase, native_id: &str, resume: bool) -> Result<PathBuf> {
    let projects = case.config.join("projects");
    checked(
        StorageLocation::preflight(&projects),
        "private-native-projects",
    )?;
    let mut found = None;
    for (index, entry) in checked(fs::read_dir(&projects), "native-project-directory")?.enumerate()
    {
        require(index < 32, "bounded-native-project-count")?;
        let entry = checked(entry, "native-project-entry")?;
        let metadata = checked(
            fs::symlink_metadata(entry.path()),
            "native-project-metadata",
        )?;
        require(
            metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
            "ordinary-native-project",
        )?;
        if !metadata.is_dir() {
            continue;
        }
        let candidate = entry.path().join(format!("{native_id}.jsonl"));
        if checked(candidate.try_exists(), "native-transcript-exists")? {
            require(found.is_none(), "unambiguous-native-transcript")?;
            found = Some(candidate);
        }
    }
    let path = found.ok_or("native-transcript-missing")?;
    let mut file = checked(
        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path),
        "pin-synthetic-transcript",
    )?;
    let metadata = checked(file.metadata(), "native-transcript-metadata")?;
    require(
        metadata.file_attributes() & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) == 0
            && metadata.len() > 0
            && metadata.len() <= TRANSCRIPT_CAP as u64,
        "bounded-native-transcript",
    )?;
    let mut bytes = Vec::new();
    checked(
        (&mut file)
            .take((TRANSCRIPT_CAP + 1) as u64)
            .read_to_end(&mut bytes),
        "read-synthetic-transcript",
    )?;
    require(bytes.len() <= TRANSCRIPT_CAP, "bounded-native-transcript")?;
    let mut observations = [false; 4];
    let expected = [
        ("user", START_PROMPT),
        ("assistant", START_MARKER),
        ("user", RESUME_PROMPT),
        ("assistant", RESUME_MARKER),
    ];
    for (index, line) in bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .enumerate()
    {
        require(
            index < 128 && line.len() <= 256 * 1024,
            "bounded-native-transcript-record",
        )?;
        let value: Value = checked(serde_json::from_slice(line), "synthetic-native-jsonl")?;
        if !matches!(value["type"].as_str(), Some("user" | "assistant")) {
            continue;
        }
        require(
            value["sessionId"].as_str() == Some(native_id),
            "native-record-session-id",
        )?;
        if let Some(cwd) = value["cwd"].as_str() {
            require(
                same_path(&case.project, Path::new(cwd))?,
                "native-record-project",
            )?;
        }
        let content = &value["message"]["content"];
        for (slot, (role, marker)) in observations.iter_mut().zip(expected) {
            if value["type"] == role
                && (content.as_str() == Some(marker)
                    || content.as_array().is_some_and(|blocks| {
                        blocks.iter().any(|block| {
                            block["type"] == "text" && block["text"].as_str() == Some(marker)
                        })
                    }))
            {
                *slot = true;
            }
        }
    }
    require(
        observations[0] && observations[1] && (!resume || observations[2] && observations[3]),
        "native-transcript-retains-exact-turns",
    )?;
    Ok(path)
}

fn run_case(
    fixture: &mut Fixture,
    case: &PrivateCase,
    helper: &PinnedProgram,
    bash: &PinnedProgram,
    native_id: &str,
    resume: bool,
) -> Result<(Value, PathBuf, String)> {
    let operation = if resume { "resume" } else { "start" };
    let ready = fixture.phase("case-ready")?;
    require(
        ready["operation"] == operation,
        "ordered-native-resume-cases",
    )?;
    let identity = checked(
        LaunchIdentity::generate(THREAD),
        "fresh-native-launch-identity",
    )?;
    let receiver = checked(
        ClaudeSessionEventReceiver::start(identity.record(), &case.project),
        "private-authenticated-hook-receiver",
    )?;
    let settings = case.home.join(format!("{operation}-hooks.json"));
    let command = hook_command(&helper.path, receiver.port())?;
    let body = json!({"disableAllHooks":false,"hooks":{
        "SessionStart":[{"matcher":if resume {"resume"} else {"startup"},
            "hooks":[{"type":"command","command":command,"timeout":5}]}],
        "SessionEnd":[{"hooks":[{"type":"command","command":command,"timeout":5}]}]
    }});
    let mut file = checked(
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&settings),
        "new-private-hook-settings",
    )?;
    checked(
        file.write_all(&checked(serde_json::to_vec(&body), "fixed-hook-settings")?),
        "write-private-hook-settings",
    )?;
    checked(file.sync_all(), "flush-private-hook-settings")?;
    drop(file);
    let environment = BTreeMap::from([
        (
            OsString::from("SHINATAMA_INSTANCE"),
            OsString::from(identity.instance_id()),
        ),
        (
            OsString::from("SHINATAMA_INSTANCE_TOKEN"),
            identity.child_secret_override().value().to_owned(),
        ),
        (
            OsString::from("CLAUDE_CODE_GIT_BASH_PATH"),
            bash.path.as_os_str().to_owned(),
        ),
    ]);
    let native = fixture.claude_command(
        case,
        true,
        arguments(&settings, native_id, resume),
        environment,
    )?;
    let owner = fixture.start_claude(native)?;
    let exit = fixture.wait_exit(owner, Duration::from_secs(11))?;
    require(exit == 0, "native-resume-case-exit")?;
    native_result(
        fixture.output(owner)?.0,
        native_id,
        if resume { RESUME_MARKER } else { START_MARKER },
    )?;
    let hook = observed_start(&receiver, &identity, native_id, &case.project)?;
    fixture.release(owner)?;
    receiver.stop();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !receiver.status().stopped {
        require(Instant::now() < deadline, "native-hook-listener-rundown")?;
        std::thread::sleep(Duration::from_millis(5));
    }
    let file = transcript(case, native_id, resume)?;
    fixture.send(
        json!({"operation":operation,"exitCode":exit,"markerObserved":true,
        "ownerReleased":true,"hookObserved":hook,"nativeIdMatches":true,"outputBounded":true}),
    )?;
    let record = fixture.phase("case-finished")?;
    require(
        record["operation"] == operation,
        "matching-native-resume-receipt",
    )?;
    require(
        !resume || record["resumeHistoryObserved"] == true,
        "native-resume-history-observed",
    )?;
    Ok((record, file, identity.instance_id().to_owned()))
}

fn qualify() -> Result<Value> {
    let mut fixture = Fixture::prepare(Scenario::NativeResume)?;
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    // Keep executable/path pins alive through cleanup, including failed cases.
    let helper = PinnedProgram::open(workspace.join("target/debug/shinatama-hook.exe"))?;
    let bash = PinnedProgram::open(PathBuf::from(BASH))?;
    require(bash.hash == BASH_SHA, "exact-installed-git-bash")?;
    let result: Result<(Value, Value, Value)> = (|| {
        fixture.start_sidecar()?;
        let case = fixture.create_case("native-session")?;
        let native_id = uuid::Uuid::new_v4().hyphenated().to_string();
        let (start, first_path, first_instance) =
            run_case(&mut fixture, &case, &helper, &bash, &native_id, false)?;
        let (resume, resumed_path, resumed_instance) =
            run_case(&mut fixture, &case, &helper, &bash, &native_id, true)?;
        require(first_path == resumed_path, "same-exact-native-transcript")?;
        require(
            first_instance != resumed_instance,
            "distinct-native-launch-identities",
        )?;
        let child = fixture.shutdown()?;
        Ok((child, start, resume))
    })();
    // Always attempt every owned cleanup before propagating any proof failure.
    fixture.release_all()?;
    let (child, start, resume) = result?;
    let mut evidence = fixture.evidence(child)?;
    evidence["scope"] = json!(
        "real Claude headless SessionStart, private native persistence and exact UUID resume; no TUI/UI or C11 authority qualification"
    );
    evidence["nativeSessionProof"] = json!({"start":start,"resume":resume,
        "authenticatedSessionStarts":2,"sourceMatchersVerified":true,"sameNativeUuid":true,
        "sameNativeTranscript":true,"persistedBothTurns":true,"distinctLaunchIdentities":true,
        "hookHelperSha256":helper.hash,"gitBashSha256":bash.hash,"parentEnvironmentUnchanged":true,
        "normalC11AuthorityQualified":false,"nativeTuiQualified":false,"resumeUiQualified":false});
    Ok(evidence)
}

#[test]
#[ignore = "coordinator must explicitly schedule the fixed synthetic Claude hook/native-resume recipe"]
fn installed_claude_emits_authenticated_hooks_and_resumes_exact_saved_uuid() {
    let names = [
        "CLAUDE_CONFIG_DIR",
        "SHINATAMA_INSTANCE",
        "SHINATAMA_INSTANCE_TOKEN",
        "CLAUDE_CODE_GIT_BASH_PATH",
        "NODE_EXTRA_CA_CERTS",
        "SSL_CERT_FILE",
        "NODE_TLS_REJECT_UNAUTHORIZED",
        "BUN_TLS_REJECT_UNAUTHORIZED",
    ];
    let before = names.map(std::env::var_os);
    let result = qualify();
    // Never format environment values or native output, including on failure.
    assert!(
        before == names.map(std::env::var_os),
        "parent-environment-changed"
    );
    let evidence = result.unwrap_or_else(|code| panic!("{code}"));
    let written = write_evidence("claude-native-resume.json", &evidence);
    assert!(
        written.is_ok(),
        "{}",
        written.err().unwrap_or("proof-evidence-unavailable")
    );
}
