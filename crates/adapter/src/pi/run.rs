//! Pi interactive TUI recipe. The argument vector always starts with Pi's
//! discovery switches, then names each per-instance resource explicitly
//! (`pi --help`/docs/cli.md for 1.4.0: explicit `--skill`/`-e` paths still load
//! when discovery is disabled). Nothing here selects a model or sends a prompt.
use super::sets::ResourceArguments;
use std::ffi::{OsStr, OsString};

/// Shared by the isolated trial receipt and the interactive permit.
pub const TUI_PROFILE: &str = "pi-system-box-explicit-set-tui-v1";

/// Disables discovered and configured skills, extensions (built-ins included)
/// and prompt templates, so an instance only gets what its set names.
pub const DISCOVERY_SWITCHES: [&str; 3] =
    ["--no-skills", "--no-extensions", "--no-prompt-templates"];

pub fn tui_arguments(resources: &ResourceArguments) -> Vec<OsString> {
    DISCOVERY_SWITCHES
        .iter()
        .map(OsString::from)
        .chain(resources.tokens().iter().cloned())
        .collect()
}

/// Pi's own process controls fixed by the recipe. The isolated trial is also
/// offline: no model-catalog refresh or tool download during qualification.
pub fn fixed_environment(trial: bool) -> Vec<(&'static str, &'static str)> {
    let mut values = vec![("PI_SKIP_VERSION_CHECK", "1"), ("PI_TELEMETRY", "0")];
    if trial {
        values.push(("PI_OFFLINE", "1"));
    }
    values
}

/// Inherited names that could redirect Pi's agent dir, sessions or package,
/// mark the child as a nested agent, or inject code into Node. Removed before
/// the recipe applies its own values; user provider keys pass through.
pub fn is_inherited_override(name: &OsStr) -> bool {
    let upper = name.to_string_lossy().to_ascii_uppercase();
    upper.starts_with("PI_")
        || matches!(
            upper.as_str(),
            "NODE_OPTIONS" | "NODE_PATH" | "NODE_REPL_EXTERNAL_MODULE" | "AI_AGENT"
        )
}

/// First interactive frame of the qualified build (24x80 ConPTY), observed on
/// 1.4.0 with an empty agent dir and no sign-in: the key-hint line plus the
/// startup-help pointer. A version line, help text or error text never matches.
pub fn startup_screen_seen(lowercase: &[u8]) -> bool {
    let contains = |needle: &[u8]| lowercase.windows(needle.len()).any(|part| part == needle);
    contains(b"escape interrupt")
        && contains(b"clear/exit")
        && (contains(b"startup help") || contains(b"pi can explain"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pi::sets::{MaterializedResource, plan_resources};
    use crate::sets::{SET_SCHEMA_VERSION, SetEntry, SetKind, SetManifest};
    use std::path::Path;

    #[test]
    fn empty_set_disables_discovery_and_adds_nothing() {
        assert_eq!(
            tui_arguments(&ResourceArguments::empty()),
            DISCOVERY_SWITCHES.map(OsString::from)
        );
        let root = Path::new(r"C:\Data\agents\pi\instances\i_aaaaaaaaaaaa");
        let manifest = SetManifest::new(
            SET_SCHEMA_VERSION,
            "work",
            vec![SetEntry::new(SetKind::Skill, "review", 1).unwrap()],
        )
        .unwrap();
        let resources = plan_resources(
            &manifest,
            root,
            &[MaterializedResource {
                kind: SetKind::Skill,
                id: "review".into(),
                path: root.join("skills").join("review"),
            }],
        )
        .unwrap();
        let arguments = tui_arguments(&resources);
        assert_eq!(&arguments[..3], DISCOVERY_SWITCHES.map(OsString::from));
        assert_eq!(arguments[3], "--skill");
        assert!(!arguments.iter().any(|token| token == "--approve"
            || token == "-a"
            || token == "--mode"
            || token == "--offline"));
    }

    #[test]
    fn inherited_redirections_are_removed_but_provider_keys_pass() {
        for name in [
            "PI_CODING_AGENT_DIR",
            "pi_coding_agent_session_dir",
            "PI_PACKAGE_DIR",
            "PI_CODING_AGENT",
            "NODE_OPTIONS",
            "node_path",
            "AI_AGENT",
        ] {
            assert!(is_inherited_override(OsStr::new(name)), "{name}");
        }
        for name in ["ANTHROPIC_API_KEY", "PATH", "PIP_INDEX_URL", "HTTPS_PROXY"] {
            assert!(!is_inherited_override(OsStr::new(name)), "{name}");
        }
        assert!(fixed_environment(true).contains(&("PI_OFFLINE", "1")));
        assert!(
            !fixed_environment(false)
                .iter()
                .any(|(name, _)| *name == "PI_OFFLINE")
        );
    }

    #[test]
    fn startup_frame_is_specific_to_the_interactive_tui() {
        // Bytes captured from 1.4.0 at 24x80, lowercased; SGR codes kept inline.
        let frame = b"\x1b[2mv1.4.0\x1b[22m\x1b[k\r\n \x1b[2mescape interrupt \xc2\xb7 ctrl+c/ctrl+d clear/exit \xc2\xb7 / commands \xc2\xb7 ! bash\x1b[22m\r\n \x1b[2mctrl+o more\x1b[22m\r\n \x1b[2mpress ctrl+o to show full startup help and loaded resources.";
        assert!(startup_screen_seen(frame));
        for other in [
            b"1.4.0\n".as_slice(),
            b"usage: pi [options] [@files...] [messages...]",
            b"error: unknown option --no-skills",
            b"opencode\nask anything",
            b"welcome to codex\nsign in with chatgpt",
        ] {
            assert!(!startup_screen_seen(other));
        }
    }
}
