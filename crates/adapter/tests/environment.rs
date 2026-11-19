#![cfg(windows)]

use shinatama_adapter::launch::environment::{
    ChildEnvironment, ChildEnvironmentPlan, ChildOverride, EnvironmentError, KnownClaudeEditor,
    OverrideName,
};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const CHILD_MODE: &str = "SHINATAMA_C11_TEST_ONLY_CHILD";
const SCENARIO: &str = "SHINATAMA_C11_TEST_ONLY_SCENARIO";
const KNOWN_EDITOR: &str = "fictional-claude-editor --edit";
const OUTPUT_PREFIX: &str = "C11_NAMES_ONLY ";
const OUTPUT_LIMIT: usize = 8 * 1024;

// Representative exact markers from every recorded adapter, including case aliases.
// No live host environment or adapter executable participates in this fixture.
const EXACT_MARKERS: &[&str] = &[
    "AI_AGENT",
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "TRACEPARENT",
    "CODEX_THREAD_ID",
    "CODEX_SANDBOX",
    "SBX_NONET_ACTIVE",
    "OPENCODE",
    "OPENCODE_SESSION_ID",
    "AGENT",
    "PI_CODING_AGENT",
    "PI_SESSION_ID",
    "PI_MODEL",
];
const CONDITIONAL_NAMES: &[&str] = &[
    "NPM_CONFIG_OFFLINE",
    "CARGO_NET_OFFLINE",
    "PIP_DISABLE_PIP_VERSION_CHECK",
    "PIP_NO_INDEX",
    "GIT_ALLOW_PROTOCOLS",
    "GIT_HTTP_PROXY",
    "GIT_HTTPS_PROXY",
    "GIT_SSH_COMMAND",
    "LESS",
];

fn fixture(scenario: &str) -> BTreeMap<OsString, OsString> {
    let mut snapshot: BTreeMap<OsString, OsString> = [
        (CHILD_MODE, "controlled-synthetic-environment-v1"),
        (SCENARIO, scenario),
        ("HOME", r"C:\fictional-c11\home-never-read"),
        ("USERPROFILE", r"C:\fictional-c11\profile-never-read"),
        ("XDG_CONFIG_HOME", r"C:\fictional-c11\xdg-never-read"),
        ("CLAUDE_CONFIG_DIR", r"C:\fictional-c11\claude-never-read"),
        ("CODEX_HOME", r"C:\fictional-c11\codex-never-read"),
        (
            "OPENCODE_CONFIG_DIR",
            r"C:\fictional-c11\opencode-never-read",
        ),
        ("PI_CODING_AGENT_DIR", r"C:\fictional-c11\pi-never-read"),
        ("HERMES_HOME", r"C:\fictional-c11\hermes-never-read"),
        ("DSH_HOME", r"C:\fictional-c11\deepseek-never-read"),
        ("DSH_AGENTS_HOME", r"C:\fictional-c11\dsh-agents-never-read"),
        (
            "ANTHROPIC_API_KEY",
            "fictional-provider-sentinel-never-print",
        ),
        ("OPENAI_API_KEY", "fictional-second-provider-never-print"),
        (
            "SYNTHETIC_CREDENTIAL_SENTINEL",
            "fictional-credential-never-print",
        ),
        ("CLAUDECODE_EXTRA", "fictional-prefix-neighbour-never-print"),
        (
            "CODEX_THREAD_ID_EXTRA",
            "fictional-thread-neighbour-never-print",
        ),
        ("PI_MODEL_EXTRA", "fictional-model-neighbour-never-print"),
        (
            "NPM_CONFIG_OFFLINE_EXTRA",
            "fictional-config-neighbour-never-print",
        ),
        (
            "GIT_ALLOW_PROTOCOL",
            "fictional-singular-setting-never-print",
        ),
        ("pAtH", r"C:\fictional-c11\bin-never-used"),
        ("SystemRoot", r"C:\fictional-c11\system-never-used"),
        (
            "SHINATAMA_INSTANCE",
            "fictional-parent-instance-never-print",
        ),
        ("SHINATAMA_THREAD", "fictional-parent-thread-never-print"),
        ("SHINATAMA_DB", r"C:\fictional-c11\parent-db-never-read"),
        (
            "SHINATAMA_INSTANCE_TOKEN",
            "fictional-parent-token-never-print",
        ),
        ("TERM", "fictional-parent-terminal-never-print"),
        ("COLORTERM", "fictional-parent-color-never-print"),
    ]
    .into_iter()
    .map(|(name, value)| (name.into(), value.into()))
    .collect();
    for marker in EXACT_MARKERS {
        if *marker != "SBX_NONET_ACTIVE" {
            // Intentionally use a mixed-case spelling for one exact removal.
            let name = if *marker == "CLAUDECODE" {
                "ClAuDeCoDe"
            } else {
                marker
            };
            snapshot.insert((*name).into(), "fictional-marker-never-print".into());
        }
    }
    for name in CONDITIONAL_NAMES {
        snapshot.insert(
            (*name).into(),
            "fictional-generic-setting-never-print".into(),
        );
    }
    if scenario != "no-sandbox-user-editor" {
        snapshot.insert(
            "sBx_NoNeT_aCtIvE".into(),
            if scenario == "sandbox-empty" { "" } else { "0" }.into(),
        );
        snapshot.insert("GIT_EDITOR".into(), KNOWN_EDITOR.into());
    } else {
        snapshot.insert(
            "GIT_EDITOR".into(),
            "fictional-user-wrapper fictional-claude-editor --edit".into(),
        );
    }
    snapshot
}

fn override_value(name: OverrideName, value: &str) -> ChildOverride {
    ChildOverride::new(name, value.into()).unwrap()
}

fn expected_names(scenario: &str, first: bool) -> BTreeSet<String> {
    let mut names = fixture(scenario)
        .keys()
        .map(|name| name.to_string_lossy().to_ascii_uppercase())
        .collect::<BTreeSet<_>>();
    for name in EXACT_MARKERS {
        names.remove(*name);
    }
    if scenario != "no-sandbox-user-editor" {
        for name in CONDITIONAL_NAMES {
            names.remove(*name);
        }
        if first {
            names.remove("GIT_EDITOR");
        }
    }
    names
}

fn value<'a>(snapshot: &'a BTreeMap<OsString, OsString>, name: &str) -> Option<&'a OsStr> {
    snapshot
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_os_str())
}

fn read_bounded(stream: impl Read) -> Vec<u8> {
    let mut output = Vec::new();
    stream
        .take(OUTPUT_LIMIT as u64 + 1)
        .read_to_end(&mut output)
        .unwrap_or_else(|_| panic!("Cannot read the controlled child's bounded output."));
    assert!(
        output.len() <= OUTPUT_LIMIT,
        "The controlled child exceeded its output bound."
    );
    output
}

struct ControlledChild {
    child: Child,
}

impl ControlledChild {
    fn spawn(environment: &ChildEnvironment) -> Self {
        let executable = std::env::current_exe()
            .unwrap_or_else(|_| panic!("Cannot locate the controlled test executable."));
        let child = Command::new(executable)
            .args([
                "--exact",
                "synthetic_environment_child_names_only",
                "--nocapture",
                "--test-threads=1",
                "--color=never",
            ])
            .env_clear()
            .envs(environment.entries())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW; this fixture has no descendants.
            .spawn()
            .unwrap_or_else(|_| panic!("Cannot start the controlled test executable."));
        Self { child }
    }

    fn names(&mut self, scenario: &str) -> BTreeSet<String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = self
                .child
                .try_wait()
                .unwrap_or_else(|_| panic!("Cannot inspect the owned child's status."))
            {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "The controlled environment child exceeded five seconds."
            );
            thread::sleep(Duration::from_millis(10));
        };
        // Only the known fixture binary runs: its names record is below one pipe
        // buffer and it has no descendants. Read at most 8 KiB after confirmed exit.
        let output = read_bounded(self.child.stdout.take().expect("Owned stdout must exist."));
        let errors = read_bounded(self.child.stderr.take().expect("Owned stderr must exist."));
        // Never forward a child's raw output, even on assertion failure.
        // Check privacy before decoding its sole names-only protocol record.
        for private in fixture(scenario).values().filter(|value| value.len() > 8) {
            let bytes = private.as_encoded_bytes();
            assert!(
                !output.windows(bytes.len()).any(|window| window == bytes),
                "A synthetic private value appeared in child output."
            );
            assert!(
                !errors.windows(bytes.len()).any(|window| window == bytes),
                "A synthetic private value appeared in child diagnostics."
            );
        }
        for private in [
            "fictional-child-a-never-print",
            "fictional-child-b-never-print",
            "fictional-child-a-token-never-print",
            "fictional-child-a-thread-never-print",
        ] {
            assert!(
                !output
                    .windows(private.len())
                    .any(|window| window == private.as_bytes()),
                "A synthetic child override appeared in output."
            );
            assert!(
                !errors
                    .windows(private.len())
                    .any(|window| window == private.as_bytes()),
                "A synthetic child override appeared in diagnostics."
            );
        }
        assert!(
            status.success(),
            "The controlled child rejected its synthetic environment; raw diagnostics are withheld."
        );
        let output = String::from_utf8(output)
            .unwrap_or_else(|_| panic!("The child names record was not UTF-8."));
        let records = output
            .lines()
            .filter_map(|line| {
                line.find(OUTPUT_PREFIX)
                    .map(|start| &line[start + OUTPUT_PREFIX.len()..])
            })
            .collect::<Vec<_>>();
        assert!(
            records.len() == 1,
            "Exactly one bounded names-only child record is required."
        );
        serde_json::from_str(records[0])
            .unwrap_or_else(|_| panic!("The bounded child names record was malformed."))
    }
}

impl Drop for ControlledChild {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_some() {
            return;
        }
        let _ = self.child.kill(); // Only this known test binary, using its owned process handle.
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn plan(snapshot: &BTreeMap<OsString, OsString>) -> ChildEnvironmentPlan {
    let names = snapshot.keys().cloned().collect::<Vec<_>>();
    ChildEnvironmentPlan::build(&names, None, None, vec![]).unwrap()
}

#[test]
fn inherited_equals_payload_is_not_a_valid_variable_name() {
    let names = vec![OsString::from("CLAUDECODE=bad")];
    assert!(
        matches!(
            ChildEnvironmentPlan::build(&names, None, None, vec![]),
            Err(EnvironmentError::InvalidInheritedName)
        ),
        "A serialized name payload must be refused before environment composition."
    );
}

#[test]
fn inherited_nul_payload_is_rejected_without_logging_or_changing_input() {
    let snapshot = BTreeMap::from([(
        OsString::from("SYNTHETIC_CREDENTIAL_SENTINEL"),
        OsString::from("fictional-value\0hidden-tail"),
    )]);
    let original = snapshot.clone();
    assert!(
        matches!(
            plan(&snapshot).child_from(&snapshot),
            Err(EnvironmentError::InvalidInheritedValue)
        ),
        "Inherited NUL data must be refused before creating a child environment."
    );
    assert!(
        snapshot == original,
        "The parent input must remain unchanged."
    );
}

#[test]
fn two_real_children_from_each_synthetic_snapshot_keep_values_private_and_isolated() {
    for scenario in ["sandbox-empty", "sandbox-zero", "no-sandbox-user-editor"] {
        let snapshot = fixture(scenario);
        let original = snapshot.clone();
        let names = snapshot.keys().cloned().collect::<Vec<_>>();
        let helper = KnownClaudeEditor::exact(KNOWN_EDITOR.into()).unwrap();
        // Compose the sibling first. Creating the first child's independent plan
        // must change neither that sibling nor the in-memory parent input.
        let sibling_plan = ChildEnvironmentPlan::build(
            &names,
            value(&snapshot, "GIT_EDITOR"),
            None,
            vec![override_value(
                OverrideName::Instance,
                "fictional-child-b-never-print",
            )],
        )
        .unwrap();
        let sibling = sibling_plan.child_from(&snapshot).unwrap();
        let sibling_before = sibling
            .entries()
            .map(|(name, value)| (name.to_os_string(), value.to_os_string()))
            .collect::<BTreeMap<_, _>>();
        let first_plan = ChildEnvironmentPlan::build(
            &names,
            value(&snapshot, "GIT_EDITOR"),
            Some(&helper),
            vec![
                override_value(OverrideName::Instance, "fictional-child-a-never-print"),
                override_value(OverrideName::Thread, "fictional-child-a-thread-never-print"),
                override_value(
                    OverrideName::Database,
                    r"C:\fictional-c11\child-a-db-never-read",
                ),
                override_value(
                    OverrideName::InstanceToken,
                    "fictional-child-a-token-never-print",
                ),
                override_value(OverrideName::Terminal, "xterm-256color"),
                override_value(OverrideName::ColorTerminal, "truecolor"),
            ],
        )
        .unwrap();
        let first = first_plan.child_from(&snapshot).unwrap();
        let mut running_first = ControlledChild::spawn(&first);
        let mut running_sibling = ControlledChild::spawn(&sibling);
        assert!(
            running_first.names(scenario) == expected_names(scenario, true),
            "The first child received unexpected variable names."
        );
        assert!(
            running_sibling.names(scenario) == expected_names(scenario, false),
            "The sibling received unexpected variable names."
        );
        assert!(
            snapshot == original,
            "Child planning changed its parent input."
        );
        let sibling_after = sibling
            .entries()
            .map(|(name, value)| (name.to_os_string(), value.to_os_string()))
            .collect::<BTreeMap<_, _>>();
        assert!(
            sibling_after == sibling_before,
            "A sibling environment was changed by another child's plan."
        );
    }
}

#[test]
fn synthetic_environment_child_names_only() {
    // During ordinary cargo test this helper returns before reading any host
    // environment entries. Only env-cleared controlled copies enter the proof.
    if std::env::var_os(CHILD_MODE).as_deref()
        != Some(OsStr::new("controlled-synthetic-environment-v1"))
    {
        return;
    }
    let observed = std::env::vars_os().collect::<BTreeMap<_, _>>();
    let scenario = match value(&observed, SCENARIO) {
        Some(value) if value == OsStr::new("sandbox-empty") => "sandbox-empty",
        Some(value) if value == OsStr::new("sandbox-zero") => "sandbox-zero",
        Some(value) if value == OsStr::new("no-sandbox-user-editor") => "no-sandbox-user-editor",
        _ => panic!("Unknown controlled child scenario."),
    };
    let first = match value(&observed, "SHINATAMA_INSTANCE") {
        Some(value) if value == OsStr::new("fictional-child-a-never-print") => true,
        Some(value) if value == OsStr::new("fictional-child-b-never-print") => false,
        _ => panic!("Unknown controlled child role."),
    };
    let original = fixture(scenario);
    for (name, expected) in &original {
        let upper = name.to_string_lossy().to_ascii_uppercase();
        if expected_names(scenario, first).contains(&upper)
            && !(first
                && [
                    "SHINATAMA_INSTANCE",
                    "SHINATAMA_THREAD",
                    "SHINATAMA_DB",
                    "SHINATAMA_INSTANCE_TOKEN",
                    "TERM",
                    "COLORTERM",
                ]
                .contains(&upper.as_str()))
            && upper != "SHINATAMA_INSTANCE"
        {
            assert!(
                value(&observed, &upper) == Some(expected.as_os_str()),
                "An opaque home/config/provider value changed in the controlled child."
            );
        }
    }
    if first {
        for (name, expected) in [
            ("SHINATAMA_THREAD", "fictional-child-a-thread-never-print"),
            ("SHINATAMA_DB", r"C:\fictional-c11\child-a-db-never-read"),
            (
                "SHINATAMA_INSTANCE_TOKEN",
                "fictional-child-a-token-never-print",
            ),
            ("TERM", "xterm-256color"),
            ("COLORTERM", "truecolor"),
        ] {
            assert!(
                value(&observed, name) == Some(OsStr::new(expected)),
                "A child-only override was not applied exactly."
            );
        }
    }
    let names = observed
        .keys()
        .map(|name| name.to_string_lossy().to_ascii_uppercase())
        .collect::<BTreeSet<_>>();
    assert!(
        names == expected_names(scenario, first),
        "An exact marker survived or an allowed name was removed."
    );
    let encoded = serde_json::to_string(&names)
        .unwrap_or_else(|_| panic!("Cannot encode synthetic variable names."));
    println!("{OUTPUT_PREFIX}{encoded}");
}

#[test]
fn windows_drive_current_directory_pseudo_name_is_preserved_opaquely() {
    let snapshot = BTreeMap::from([(
        OsString::from("=C:"),
        OsString::from(r"C:\fictional-c11-current-directory"),
    )]);
    let original = snapshot.clone();
    let child = plan(&snapshot).child_from(&snapshot).unwrap();
    assert!(
        child
            .entries()
            .any(|(name, value)| name == OsStr::new("=C:")
                && Some(value) == snapshot.get(name).map(OsString::as_os_str)),
        "A valid Windows drive pseudo variable must pass through without a file read."
    );
    assert!(
        snapshot == original,
        "The parent input must remain unchanged."
    );
}
