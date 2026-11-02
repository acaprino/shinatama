//! Pure child-only environment changes, separate from launch admission.
//!
//! Sources: FRD-0002/R35-R38, ADR-0014 and the P20 criterion (2026-10-28).
//! P20's Windows observations are partial; HM11/DS11 and nested interactive starts
//! remain unqualified. This module neither probes nor declares native support.
//! It never reads the process environment, files, credentials or configuration.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fmt;

const MAX_INHERITED_NAMES: usize = 2048;
const MAX_NAME_BYTES: usize = 4096;
const MAX_ENVIRONMENT_BYTES: usize = 1024 * 1024;
const MAX_OVERRIDE_VALUE_BYTES: usize = 64 * 1024;

const CLAUDE_MARKERS: &[&str] = &[
    "AI_AGENT",
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_INVOKED_SKILLS",
    "CLAUDE_CODE_SSE_PORT",
    "CLAUDE_EFFORT",
    "CLAUDE_PID",
    "CLAUDE_PROJECT_DIR",
    "CLAUDE_ENV_FILE",
    "TRACEPARENT",
];
const CODEX_MARKERS: &[&str] = &[
    "CODEX_CI",
    "CODEX_THREAD_ID",
    "CODEX_SESSION_ID",
    "CODEX_TOOL_CALL_ID",
    "CODEX_VERSION",
    "CODEX_APPLY_PATCH_PRESERVE_LINE_ENDINGS",
    "CODEX_SANDBOX",
    "CODEX_SANDBOX_NETWORK_DISABLED",
    "SBX_NONET_ACTIVE",
];
const OPENCODE_MARKERS: &[&str] = &[
    "AI_AGENT",
    "AGENT",
    "OPENCODE",
    "OPENCODE_SESSION_ID",
    "OPENCODE_TERMINAL",
    "OPENCODE_CLIENT",
];
const PI_MARKERS: &[&str] = &[
    "AI_AGENT",
    "PI_CODING_AGENT",
    "PI_SESSION_ID",
    "PI_SESSION_FILE",
    "PI_PROVIDER",
    "PI_MODEL",
    "PI_REASONING_LEVEL",
];
const CLAUDE_CONDITIONAL: &[&str] = &["GIT_EDITOR"];
// These are P20's exact spellings, including the plural GIT_ALLOW_PROTOCOLS.
// GIT_ALLOW_PROTOCOL is a distinct user setting and is never inferred here.
const CODEX_SANDBOX_GENERIC: &[&str] = &[
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerAdapter {
    ClaudeCode,
    Codex,
    OpenCode,
    Pi,
    HermesAgent,
    DeepSeekHarness,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclarationStatus {
    RecordedWindowsPartial,
    PendingHm11,
    PendingDs11,
}

#[derive(Clone, Copy, Debug)]
pub struct MarkerDeclaration {
    pub adapter: MarkerAdapter,
    pub status: DeclarationStatus,
    pub recorded_version: Option<&'static str>,
    pub always: &'static [&'static str],
    pub conditional: &'static [&'static str],
}

pub fn inherited_markers(adapter: MarkerAdapter) -> MarkerDeclaration {
    let (status, recorded_version, always, conditional): (
        DeclarationStatus,
        Option<&'static str>,
        &'static [&'static str],
        &'static [&'static str],
    ) = match adapter {
        MarkerAdapter::ClaudeCode => (
            DeclarationStatus::RecordedWindowsPartial,
            Some("2.1.312"),
            CLAUDE_MARKERS,
            CLAUDE_CONDITIONAL,
        ),
        MarkerAdapter::Codex => (
            DeclarationStatus::RecordedWindowsPartial,
            Some("0.164.0"),
            CODEX_MARKERS,
            CODEX_SANDBOX_GENERIC,
        ),
        MarkerAdapter::OpenCode => (
            DeclarationStatus::RecordedWindowsPartial,
            Some("2.1.4"),
            OPENCODE_MARKERS,
            &[],
        ),
        MarkerAdapter::Pi => (
            DeclarationStatus::RecordedWindowsPartial,
            Some("1.4.0"),
            PI_MARKERS,
            &[],
        ),
        MarkerAdapter::HermesAgent => (DeclarationStatus::PendingHm11, None, &[], &[]),
        MarkerAdapter::DeepSeekHarness => (DeclarationStatus::PendingDs11, None, &[], &[]),
    };
    MarkerDeclaration {
        adapter,
        status,
        recorded_version,
        always,
        conditional,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvironmentError {
    LimitExceeded,
    InvalidInheritedName,
    InvalidInheritedValue,
    InvalidOverrideValue,
    DuplicateOverride,
    StaleEnvironment,
}

/// Explicit child overrides in C11.1 cannot replace home/provider/user settings.
/// Placement-specific overrides are a separate C11.2 responsibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OverrideName {
    Instance,
    Thread,
    Database,
    InstanceToken,
    Terminal,
    ColorTerminal,
}
impl OverrideName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Instance => "SHINATAMA_INSTANCE",
            Self::Thread => "SHINATAMA_THREAD",
            Self::Database => "SHINATAMA_DB",
            Self::InstanceToken => "SHINATAMA_INSTANCE_TOKEN",
            Self::Terminal => "TERM",
            Self::ColorTerminal => "COLORTERM",
        }
    }
}

#[derive(Clone)]
pub struct ChildOverride {
    name: OverrideName,
    value: OsString,
}
impl ChildOverride {
    pub fn new(name: OverrideName, value: OsString) -> Result<Self, EnvironmentError> {
        if value.len() > MAX_OVERRIDE_VALUE_BYTES {
            return Err(EnvironmentError::LimitExceeded);
        }
        if value.as_encoded_bytes().contains(&0) {
            return Err(EnvironmentError::InvalidOverrideValue);
        }
        Ok(Self { name, value })
    }
    pub fn name(&self) -> OverrideName {
        self.name
    }
    /// Deliberate launch-boundary access; never include this value in diagnostics.
    pub fn value(&self) -> &OsStr {
        &self.value
    }
}
impl fmt::Debug for ChildOverride {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChildOverride")
            .field("name", &self.name)
            .field("value", &"[redacted]")
            .finish()
    }
}

/// An exact command supplied by a separately qualified Claude/version profile.
/// P20 does not specify a universal helper syntax, so there is no substring or
/// path-prefix recognizer and no guessed built-in command. Unknown means kept.
#[derive(Clone)]
pub struct KnownClaudeEditor(OsString);
impl KnownClaudeEditor {
    pub fn exact(command: OsString) -> Result<Self, EnvironmentError> {
        if command.is_empty() || command.len() > MAX_OVERRIDE_VALUE_BYTES {
            return Err(EnvironmentError::LimitExceeded);
        }
        if command.as_encoded_bytes().contains(&0) {
            return Err(EnvironmentError::InvalidOverrideValue);
        }
        Ok(Self(command))
    }
}
impl fmt::Debug for KnownClaudeEditor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("KnownClaudeEditor([redacted])")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverageGap {
    HermesMarkersHm11,
    DeepSeekMarkersDs11,
    PiPrependedPathP20,
}

/// No parent environment reference is retained, and Debug exposes names only.
#[derive(Clone, Debug)]
pub struct ChildEnvironmentPlan {
    removals: Vec<&'static str>,
    overrides: Vec<ChildOverride>,
    inherited_sandbox: bool,
    remove_claude_editor: bool,
    known_claude_editor: Option<KnownClaudeEditor>,
}
impl ChildEnvironmentPlan {
    /// Only names and, when present, GIT_EDITOR's value participate in decisions.
    /// SBX_NONET_ACTIVE is checked for presence before its own removal; its value
    /// is neither inspected nor interpreted as a boolean.
    pub fn build(
        inherited_names: &[OsString],
        inherited_git_editor: Option<&OsStr>,
        known_claude_editor: Option<&KnownClaudeEditor>,
        overrides: Vec<ChildOverride>,
    ) -> Result<Self, EnvironmentError> {
        check_names(inherited_names.iter().map(OsString::as_os_str))?;
        let has_name = |name: &str| {
            inherited_names
                .iter()
                .any(|candidate| exact_name(candidate, name))
        };
        let sandbox = has_name("SBX_NONET_ACTIVE");
        let claude_editor = has_name("GIT_EDITOR")
            && known_claude_editor.is_some_and(|known| {
                inherited_git_editor.is_some_and(|value| value == known.0.as_os_str())
            });
        let mut removals = BTreeSet::new();
        for adapter in [
            MarkerAdapter::ClaudeCode,
            MarkerAdapter::Codex,
            MarkerAdapter::OpenCode,
            MarkerAdapter::Pi,
        ] {
            removals.extend(inherited_markers(adapter).always.iter().copied());
        }
        if sandbox {
            removals.extend(CODEX_SANDBOX_GENERIC.iter().copied());
        }
        if claude_editor {
            removals.insert("GIT_EDITOR");
        }
        let mut unique = BTreeSet::new();
        for item in &overrides {
            if !unique.insert(item.name) {
                return Err(EnvironmentError::DuplicateOverride);
            }
        }
        Ok(Self {
            removals: removals.into_iter().collect(),
            overrides,
            inherited_sandbox: sandbox,
            remove_claude_editor: claude_editor,
            known_claude_editor: known_claude_editor.cloned(),
        })
    }

    pub fn removal_names(&self) -> &[&'static str] {
        &self.removals
    }
    pub fn overrides(&self) -> &[ChildOverride] {
        &self.overrides
    }
    pub fn coverage_gaps(&self) -> &'static [CoverageGap] {
        &[
            CoverageGap::HermesMarkersHm11,
            CoverageGap::DeepSeekMarkersDs11,
            CoverageGap::PiPrependedPathP20,
        ]
    }

    /// Pure fixture/composition support. Pass-through values are cloned opaquely;
    /// they do not influence policy and are never exposed by Debug/Serialize.
    /// Actual launch code must apply these changes only to its child builder.
    pub fn child_from(
        &self,
        inherited: &BTreeMap<OsString, OsString>,
    ) -> Result<ChildEnvironment, EnvironmentError> {
        check_names(inherited.keys().map(OsString::as_os_str))?;
        let sandbox = inherited
            .keys()
            .any(|name| exact_name(name, "SBX_NONET_ACTIVE"));
        let editor = self.known_claude_editor.as_ref().is_some_and(|known| {
            inherited
                .iter()
                .any(|(name, value)| exact_name(name, "GIT_EDITOR") && value == &known.0)
        });
        if sandbox != self.inherited_sandbox || editor != self.remove_claude_editor {
            return Err(EnvironmentError::StaleEnvironment);
        }
        let mut bytes = 0usize;
        for (name, value) in inherited {
            bytes = bytes.saturating_add(name.len()).saturating_add(value.len());
            if bytes > MAX_ENVIRONMENT_BYTES {
                return Err(EnvironmentError::LimitExceeded);
            }
            if value.as_encoded_bytes().contains(&0) {
                return Err(EnvironmentError::InvalidInheritedValue);
            }
        }
        let mut child = inherited.clone();
        child.retain(|name, _| {
            !self
                .removals
                .iter()
                .any(|removed| exact_name(name, removed))
        });
        for item in &self.overrides {
            child.retain(|name, _| !exact_name(name, item.name.as_str()));
            child.insert(item.name.as_str().into(), item.value.clone());
        }
        if child.iter().fold(0usize, |total, (name, value)| {
            total.saturating_add(name.len()).saturating_add(value.len())
        }) > MAX_ENVIRONMENT_BYTES
        {
            return Err(EnvironmentError::LimitExceeded);
        }
        Ok(ChildEnvironment(child))
    }
}

#[derive(Clone)]
pub struct ChildEnvironment(BTreeMap<OsString, OsString>);
impl ChildEnvironment {
    /// Explicit, non-loggable access for the eventual contained launch boundary.
    pub fn entries(&self) -> impl Iterator<Item = (&OsStr, &OsStr)> {
        self.0
            .iter()
            .map(|(name, value)| (name.as_os_str(), value.as_os_str()))
    }
}
impl fmt::Debug for ChildEnvironment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChildEnvironment")
            .field("names", &self.0.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

fn exact_name(actual: &OsStr, expected: &str) -> bool {
    actual.eq_ignore_ascii_case(expected)
}

fn check_names<'a>(names: impl Iterator<Item = &'a OsStr>) -> Result<(), EnvironmentError> {
    let mut bytes = 0usize;
    for (index, name) in names.enumerate() {
        bytes = bytes.saturating_add(name.len());
        if index >= MAX_INHERITED_NAMES
            || name.len() > MAX_NAME_BYTES
            || bytes > MAX_ENVIRONMENT_BYTES
        {
            return Err(EnvironmentError::LimitExceeded);
        }
        let encoded = name.as_encoded_bytes();
        // Windows permits its drive-current-directory pseudo-names (=C:).
        // Other '=' characters would change the effective name in name=value
        // serialization and could bypass an exact marker-name decision.
        let drive_current_directory = encoded.len() == 3
            && encoded[0] == b'='
            && encoded[1].is_ascii_alphabetic()
            && encoded[2] == b':';
        if name.is_empty()
            || encoded.contains(&0)
            || (encoded.contains(&b'=') && !drive_current_directory)
        {
            return Err(EnvironmentError::InvalidInheritedName);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn environment(values: &[(&str, &str)]) -> BTreeMap<OsString, OsString> {
        values
            .iter()
            .map(|(key, value)| ((*key).into(), (*value).into()))
            .collect()
    }
    fn plan(
        parent: &BTreeMap<OsString, OsString>,
        helper: Option<&KnownClaudeEditor>,
        overrides: Vec<ChildOverride>,
    ) -> ChildEnvironmentPlan {
        let names: Vec<_> = parent.keys().cloned().collect();
        let editor = parent
            .iter()
            .find(|(name, _)| exact_name(name, "GIT_EDITOR"))
            .map(|(_, value)| value.as_os_str());
        ChildEnvironmentPlan::build(&names, editor, helper, overrides).unwrap()
    }
    fn value<'a>(child: &'a ChildEnvironment, name: &str) -> Option<&'a OsStr> {
        child
            .entries()
            .find(|(key, _)| exact_name(key, name))
            .map(|(_, value)| value)
    }

    #[test]
    fn every_documented_adapter_contributes_to_union_without_prefix_removal() {
        let mut parent = environment(&[
            ("CLAUDE_CONFIG_DIR", "user-home"),
            ("CLAUDE_CODE_USE_BEDROCK", "1"),
            ("CODEX_HOME", "codex-home"),
            ("OPENCODE_CONFIG_DIR", "opencode-home"),
            ("PI_CODING_AGENT_DIR", "pi-home"),
            ("HOME", "home"),
            ("PATH", "user-path"),
            ("CLAUDECODE_EXTRA", "keep"),
            ("CODEX_THREAD_ID_EXTRA", "keep"),
        ]);
        for adapter in [
            MarkerAdapter::ClaudeCode,
            MarkerAdapter::Codex,
            MarkerAdapter::OpenCode,
            MarkerAdapter::Pi,
        ] {
            for name in inherited_markers(adapter).always {
                parent.insert((*name).into(), "synthetic-marker".into());
            }
        }
        let child = plan(&parent, None, vec![]).child_from(&parent).unwrap();
        for adapter in [
            MarkerAdapter::ClaudeCode,
            MarkerAdapter::Codex,
            MarkerAdapter::OpenCode,
            MarkerAdapter::Pi,
        ] {
            for name in inherited_markers(adapter).always {
                assert!(value(&child, name).is_none());
            }
        }
        for name in [
            "CLAUDE_CONFIG_DIR",
            "CLAUDE_CODE_USE_BEDROCK",
            "CODEX_HOME",
            "OPENCODE_CONFIG_DIR",
            "PI_CODING_AGENT_DIR",
            "HOME",
            "PATH",
            "CLAUDECODE_EXTRA",
            "CODEX_THREAD_ID_EXTRA",
        ] {
            assert_eq!(
                value(&child, name),
                parent.get(OsStr::new(name)).map(OsString::as_os_str)
            );
        }
    }

    #[test]
    fn sandbox_generic_names_require_marker_presence_even_if_its_value_is_empty() {
        for sandbox_value in [None, Some(""), Some("0"), Some("1")] {
            let mut parent = environment(&[
                ("GIT_ALLOW_PROTOCOL", "user-protocol-setting"),
                ("NPM_CONFIG_OFFLINE_EXTRA", "keep"),
            ]);
            for name in CODEX_SANDBOX_GENERIC {
                parent.insert((*name).into(), "user-value".into());
            }
            if let Some(marker) = sandbox_value {
                parent.insert("SBX_NONET_ACTIVE".into(), marker.into());
            }
            let child = plan(&parent, None, vec![]).child_from(&parent).unwrap();
            for name in CODEX_SANDBOX_GENERIC {
                assert_eq!(value(&child, name).is_some(), sandbox_value.is_none());
            }
            assert_eq!(
                value(&child, "GIT_ALLOW_PROTOCOL"),
                Some(OsStr::new("user-protocol-setting"))
            );
            assert!(value(&child, "NPM_CONFIG_OFFLINE_EXTRA").is_some());
            assert!(value(&child, "SBX_NONET_ACTIVE").is_none());
        }
    }

    #[test]
    fn git_editor_requires_an_exact_known_helper_and_keeps_user_editor_commands() {
        let helper = KnownClaudeEditor::exact("synthetic-claude-helper --edit".into()).unwrap();
        for (editor, remove) in [
            ("synthetic-claude-helper --edit", true),
            ("code --wait", false),
            ("user-wrapper synthetic-claude-helper --edit", false),
        ] {
            let parent = environment(&[("GIT_EDITOR", editor)]);
            let child = plan(&parent, Some(&helper), vec![])
                .child_from(&parent)
                .unwrap();
            assert_eq!(value(&child, "GIT_EDITOR").is_none(), remove);
        }
        let parent = environment(&[("GIT_EDITOR", "synthetic-claude-helper --edit")]);
        assert!(
            value(
                &plan(&parent, None, vec![]).child_from(&parent).unwrap(),
                "GIT_EDITOR"
            )
            .is_some()
        );
    }

    #[test]
    fn parent_and_siblings_remain_unchanged_and_debug_never_contains_values() {
        let parent = environment(&[
            ("CLAUDE_CODE_CHILD_SESSION", "marker-payload"),
            ("SHINATAMA_INSTANCE_TOKEN", "outer-secret"),
            ("CODEX_HOME", "user-home-secret"),
            ("ANTHROPIC_API_KEY", "synthetic-provider-secret"),
        ]);
        let original = parent.clone();
        let sibling = parent.clone();
        let child_plan = plan(
            &parent,
            None,
            vec![ChildOverride::new(OverrideName::InstanceToken, "new-secret".into()).unwrap()],
        );
        let child = child_plan.child_from(&parent).unwrap();
        assert_eq!(parent, original);
        assert_eq!(sibling, original);
        assert_eq!(
            value(&child, "SHINATAMA_INSTANCE_TOKEN"),
            Some(OsStr::new("new-secret"))
        );
        assert_eq!(
            value(&child, "CODEX_HOME"),
            Some(OsStr::new("user-home-secret"))
        );
        assert_eq!(
            value(&child, "ANTHROPIC_API_KEY"),
            Some(OsStr::new("synthetic-provider-secret"))
        );
        let debug = format!("{child_plan:?} {child:?}");
        for sensitive in [
            "marker-payload",
            "outer-secret",
            "user-home-secret",
            "new-secret",
            "synthetic-provider-secret",
        ] {
            assert!(!debug.contains(sensitive));
        }
    }

    #[test]
    fn windows_exact_names_are_case_insensitive_and_override_case_aliases_collapse() {
        let parent = environment(&[
            ("claudecode", "marker"),
            ("ClAuDe_CoNfIg_DiR", "keep"),
            ("shinatama_instance", "old"),
            ("SHINATAMA_INSTANCE", "older"),
        ]);
        let child = plan(
            &parent,
            None,
            vec![ChildOverride::new(OverrideName::Instance, "new".into()).unwrap()],
        )
        .child_from(&parent)
        .unwrap();
        assert!(value(&child, "CLAUDECODE").is_none());
        assert_eq!(value(&child, "CLAUDE_CONFIG_DIR"), Some(OsStr::new("keep")));
        assert_eq!(
            child
                .entries()
                .filter(|(name, _)| exact_name(name, "SHINATAMA_INSTANCE"))
                .count(),
            1
        );
    }

    #[test]
    fn pending_marker_lists_and_pi_path_are_reported_without_inventing_rules() {
        assert_eq!(
            inherited_markers(MarkerAdapter::HermesAgent).status,
            DeclarationStatus::PendingHm11
        );
        assert_eq!(
            inherited_markers(MarkerAdapter::DeepSeekHarness).status,
            DeclarationStatus::PendingDs11
        );
        let parent = environment(&[
            ("HERMES_HOME", "keep"),
            ("DSH_HOME", "keep"),
            ("DSH_AGENTS_HOME", "keep"),
            ("PATH", "inherited-pi-bin;user-path"),
        ]);
        let child_plan = plan(&parent, None, vec![]);
        assert_eq!(child_plan.coverage_gaps().len(), 3);
        let child = child_plan.child_from(&parent).unwrap();
        assert_eq!(child.entries().count(), parent.len());
    }

    #[test]
    fn duplicate_overrides_and_oversized_inputs_refuse_without_partial_changes() {
        let parent = environment(&[("HOME", "keep")]);
        let names = [OsString::from("HOME")];
        let make = || ChildOverride::new(OverrideName::Instance, "synthetic".into()).unwrap();
        assert_eq!(
            ChildEnvironmentPlan::build(&names, None, None, vec![make(), make()]).unwrap_err(),
            EnvironmentError::DuplicateOverride
        );
        assert_eq!(
            ChildEnvironmentPlan::build(
                &vec![OsString::from("x"); MAX_INHERITED_NAMES + 1],
                None,
                None,
                vec![]
            )
            .unwrap_err(),
            EnvironmentError::LimitExceeded
        );
        assert_eq!(
            parent.get(OsStr::new("HOME")),
            Some(&OsString::from("keep"))
        );
    }

    #[test]
    fn conditional_plan_cannot_be_reused_with_a_different_parent_snapshot() {
        let parent = environment(&[
            ("NPM_CONFIG_OFFLINE", "user-setting"),
            ("GIT_EDITOR", "synthetic-helper"),
        ]);
        let helper = KnownClaudeEditor::exact("synthetic-helper".into()).unwrap();
        let child_plan = plan(&parent, Some(&helper), vec![]);
        let mut changed = parent.clone();
        changed.insert("SBX_NONET_ACTIVE".into(), "".into());
        assert_eq!(
            child_plan.child_from(&changed).unwrap_err(),
            EnvironmentError::StaleEnvironment
        );
        let mut changed = parent.clone();
        changed.insert("GIT_EDITOR".into(), "code --wait".into());
        assert_eq!(
            child_plan.child_from(&changed).unwrap_err(),
            EnvironmentError::StaleEnvironment
        );
        assert_eq!(
            changed.get(OsStr::new("GIT_EDITOR")),
            Some(&OsString::from("code --wait"))
        );
    }

    #[test]
    fn inherited_and_override_value_limits_cover_the_final_child_without_logging_values() {
        let parent = environment(&[("USER_CONFIG", &"x".repeat(MAX_ENVIRONMENT_BYTES - 20))]);
        let override_value =
            ChildOverride::new(OverrideName::InstanceToken, "synthetic-token-value".into())
                .unwrap();
        let child_plan = plan(&parent, None, vec![override_value]);
        assert_eq!(
            child_plan.child_from(&parent).unwrap_err(),
            EnvironmentError::LimitExceeded
        );
        assert_eq!(
            parent.get(OsStr::new("USER_CONFIG")).unwrap().len(),
            MAX_ENVIRONMENT_BYTES - 20
        );
        assert_eq!(
            ChildEnvironmentPlan::build(&[OsString::from("bad\0name")], None, None, vec![])
                .unwrap_err(),
            EnvironmentError::InvalidInheritedName
        );
    }
}
