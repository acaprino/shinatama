//! Pure argv preparation. A CommandPlan describes a process; it never authorizes
//! or starts one. The engine still needs prepared paths/identity and C09R admission.
//! No shell strings, discovery, file reads, native flags or resume parser live here.

use crate::{
    AdapterDescriptor, AgentId, Capabilities, ConnectionArchitecture, FixtureEvidence, Operation,
    RefusalReason, Version,
};
use std::cmp::Ordering;
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

const MAX_ARGUMENTS: usize = 128;
const MAX_TOKEN_BYTES: usize = 8192;
const MAX_ARGV_BYTES: usize = 64 * 1024;
const MAX_COMMAND_UTF16_UNITS: usize = 32_767;

#[derive(Clone)]
pub enum CommandForm {
    NativeExe(PathBuf),
    NodeScript {
        node_exe: PathBuf,
        script: PathBuf,
        node_version: Version,
        /// Exact-version profile requirement, not a universal Node minimum.
        /// Pi's Node recipe must explicitly require at least 22.19.0 (FRD-0002/R08).
        minimum_node_version: Option<Version>,
    },
}
impl CommandForm {
    fn executable(&self) -> &Path {
        match self {
            Self::NativeExe(path) => path,
            Self::NodeScript { node_exe, .. } => node_exe,
        }
    }
    fn script(&self) -> Option<&Path> {
        match self {
            Self::NativeExe(_) => None,
            Self::NodeScript { script, .. } => Some(script),
        }
    }
}
impl fmt::Debug for CommandForm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NativeExe(_) => "NativeExe([redacted])",
            Self::NodeScript { .. } => "NodeScript([redacted])",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgvError {
    MissingRecipe,
    Unqualified(RefusalReason),
    WrongOperation,
    InvalidProgram,
    InvalidScript,
    InvalidWorkingDirectory,
    InvalidArgument,
    LimitExceeded,
    MissingNodeMinimum,
    NodeMinimumTooLow,
    NodeVersionTooOld,
}

/// Only trusted profile code creates fixture assertions. No Deserialize or IPC
/// representation can manufacture this binding; no native profile is built in.
#[derive(Clone)]
pub struct QualifiedArgvRecipe {
    agent: AgentId,
    version: Version,
    synthetic: bool,
    operation: Operation,
    form: CommandForm,
    arguments: Vec<OsString>,
    evidence: Capabilities,
}
impl QualifiedArgvRecipe {
    pub fn qualify(
        descriptor: &AdapterDescriptor,
        operation: Operation,
        form: CommandForm,
        arguments: Vec<OsString>,
        evidence: FixtureEvidence,
    ) -> Result<Self, ArgvError> {
        require_descriptor(descriptor, operation)?;
        let mut qualified = Capabilities::default();
        qualified
            .qualify(
                operation,
                &descriptor.agent,
                &descriptor.version,
                descriptor.synthetic,
                evidence,
            )
            .map_err(ArgvError::Unqualified)?;
        validate_command(&descriptor.agent, &form, &arguments)?;
        Ok(Self {
            agent: descriptor.agent.clone(),
            version: descriptor.version.clone(),
            synthetic: descriptor.synthetic,
            operation,
            form,
            arguments,
            evidence: qualified,
        })
    }
}
impl fmt::Debug for QualifiedArgvRecipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QualifiedArgvRecipe")
            .field("agent", &self.agent)
            .field("version", &self.version)
            .field("operation", &self.operation)
            .field("form", &self.form)
            .field("argument_count", &self.arguments.len())
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct CommandPlan {
    agent: AgentId,
    version: Version,
    operation: Operation,
    executable: PathBuf,
    arguments: Vec<OsString>,
    working_directory: PathBuf,
}
impl CommandPlan {
    pub fn agent(&self) -> &AgentId {
        &self.agent
    }
    pub fn version(&self) -> &Version {
        &self.version
    }
    pub fn operation(&self) -> Operation {
        self.operation
    }
    pub fn executable(&self) -> &Path {
        &self.executable
    }
    /// Individual tokens only; Node's resolved script is the first token.
    pub fn argv(&self) -> &[OsString] {
        &self.arguments
    }
    pub fn working_directory(&self) -> &Path {
        &self.working_directory
    }
}
impl fmt::Debug for CommandPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandPlan")
            .field("agent", &self.agent)
            .field("version", &self.version)
            .field("operation", &self.operation)
            .field("argument_count", &self.arguments.len())
            .finish_non_exhaustive()
    }
}

pub fn plan_arguments(
    descriptor: &AdapterDescriptor,
    operation: Operation,
    working_directory: &Path,
    recipe: Option<&QualifiedArgvRecipe>,
) -> Result<CommandPlan, ArgvError> {
    require_descriptor(descriptor, operation)?;
    let recipe = recipe.ok_or(ArgvError::MissingRecipe)?;
    if recipe.agent != descriptor.agent
        || recipe.version != descriptor.version
        || recipe.synthetic != descriptor.synthetic
    {
        return Err(ArgvError::Unqualified(RefusalReason::VersionMismatch));
    }
    if recipe.operation != operation {
        return Err(ArgvError::WrongOperation);
    }
    recipe
        .evidence
        .validate_for(&descriptor.agent, &descriptor.version, descriptor.synthetic)
        .map_err(ArgvError::Unqualified)?;
    recipe
        .evidence
        .require(operation)
        .map_err(ArgvError::Unqualified)?;
    if !absolute_windows_path(working_directory) {
        return Err(ArgvError::InvalidWorkingDirectory);
    }
    // All count/byte/unit checks precede cloning paths or argument values.
    validate_command(&descriptor.agent, &recipe.form, &recipe.arguments)?;
    let mut arguments =
        Vec::with_capacity(recipe.arguments.len() + usize::from(recipe.form.script().is_some()));
    if let Some(script) = recipe.form.script() {
        arguments.push(script.as_os_str().to_owned());
    }
    arguments.extend(recipe.arguments.iter().cloned());
    Ok(CommandPlan {
        agent: descriptor.agent.clone(),
        version: descriptor.version.clone(),
        operation,
        executable: recipe.form.executable().to_owned(),
        arguments,
        working_directory: working_directory.to_owned(),
    })
}

fn require_descriptor(
    descriptor: &AdapterDescriptor,
    operation: Operation,
) -> Result<(), ArgvError> {
    if descriptor.synthetic != matches!(descriptor.architecture, ConnectionArchitecture::InMemory)
        || descriptor.synthetic != descriptor.agent.as_str().starts_with("fake_")
    {
        return Err(ArgvError::Unqualified(RefusalReason::NotQualified));
    }
    descriptor
        .capabilities
        .validate_for(&descriptor.agent, &descriptor.version, descriptor.synthetic)
        .map_err(ArgvError::Unqualified)?;
    descriptor
        .capabilities
        .require(operation)
        .map_err(ArgvError::Unqualified)
}

fn validate_command(
    agent: &AgentId,
    form: &CommandForm,
    arguments: &[OsString],
) -> Result<(), ArgvError> {
    let executable = form.executable();
    if !absolute_windows_path(executable) || !has_extension(executable, &["exe"]) {
        return Err(ArgvError::InvalidProgram);
    }
    if let CommandForm::NodeScript {
        script,
        node_version,
        minimum_node_version,
        ..
    } = form
    {
        if !absolute_windows_path(script) || !has_extension(script, &["js", "mjs", "cjs"]) {
            return Err(ArgvError::InvalidScript);
        }
        if agent.as_str() == "pi" {
            let minimum = minimum_node_version
                .as_ref()
                .ok_or(ArgvError::MissingNodeMinimum)?;
            if !version_at_least(
                minimum,
                &Version::parse("22.19.0").expect("documented Pi minimum"),
            ) {
                return Err(ArgvError::NodeMinimumTooLow);
            }
        }
        if minimum_node_version
            .as_ref()
            .is_some_and(|minimum| !version_at_least(node_version, minimum))
        {
            return Err(ArgvError::NodeVersionTooOld);
        }
    }
    let count = arguments
        .len()
        .saturating_add(usize::from(form.script().is_some()));
    if count > MAX_ARGUMENTS {
        return Err(ArgvError::LimitExceeded);
    }
    let tokens = std::iter::once(executable.as_os_str())
        .chain(form.script().map(Path::as_os_str))
        .chain(arguments.iter().map(OsString::as_os_str));
    let mut bytes = 0usize;
    let mut quoted_units = 1usize; // final NUL
    for token in tokens {
        if token.len() > MAX_TOKEN_BYTES {
            return Err(ArgvError::LimitExceeded);
        }
        let text = token.to_str().ok_or(ArgvError::InvalidArgument)?;
        if text.contains('\0') {
            return Err(ArgvError::InvalidArgument);
        }
        bytes = bytes.saturating_add(token.len());
        // Conservative upper bound for Windows CRT quoting: every input unit
        // may need doubling, plus two quotes and one separator. No command line
        // is constructed here, and metacharacters never become shell syntax.
        quoted_units = quoted_units
            .saturating_add(text.encode_utf16().count().saturating_mul(2))
            .saturating_add(3);
        if bytes > MAX_ARGV_BYTES || quoted_units > MAX_COMMAND_UTF16_UNITS {
            return Err(ArgvError::LimitExceeded);
        }
    }
    Ok(())
}

fn has_extension(path: &Path, allowed: &[&str]) -> bool {
    path.extension().is_some_and(|extension| {
        allowed
            .iter()
            .any(|allowed| extension.eq_ignore_ascii_case(allowed))
    })
}

fn absolute_windows_path(path: &Path) -> bool {
    let Some(raw) = path.to_str() else {
        return false;
    };
    if raw.len() > MAX_TOKEN_BYTES || raw.chars().any(char::is_control) {
        return false;
    }
    let verbatim = raw.strip_prefix(r"\\?\");
    let raw = verbatim.unwrap_or(raw);
    let parts: Vec<_> = raw
        .split(['\\', '/'])
        .filter(|part| !part.is_empty())
        .collect();
    if parts
        .iter()
        .any(|part| matches!(*part, "." | "..") || part.ends_with(['.', ' ']))
    {
        return false;
    }
    let bytes = raw.as_bytes();
    let disk = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
        && !bytes.iter().skip(2).any(|byte| *byte == b':');
    let unc =
        ((raw.starts_with(r"\\") || raw.starts_with("//")) && parts.len() >= 2 && parts[0] != ".")
            || (verbatim.is_some() && raw.starts_with(r"UNC\") && parts.len() >= 3);
    disk || unc
}

pub(crate) fn version_at_least(actual: &Version, minimum: &Version) -> bool {
    match (actual.major(), actual.minor(), actual.patch()).cmp(&(
        minimum.major(),
        minimum.minor(),
        minimum.patch(),
    )) {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => match (actual.prerelease(), minimum.prerelease()) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(actual), Some(minimum)) => {
                let mut left = actual.split('.');
                let mut right = minimum.split('.');
                loop {
                    match (left.next(), right.next()) {
                        (None, None) | (Some(_), None) => return true,
                        (None, Some(_)) => return false,
                        (Some(left), Some(right)) => {
                            let numeric_left = left.bytes().all(|byte| byte.is_ascii_digit());
                            let numeric_right = right.bytes().all(|byte| byte.is_ascii_digit());
                            let ordering = match (numeric_left, numeric_right) {
                                (true, true) => {
                                    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
                                }
                                (true, false) => Ordering::Less,
                                (false, true) => Ordering::Greater,
                                (false, false) => left.cmp(right),
                            };
                            match ordering {
                                Ordering::Greater => return true,
                                Ordering::Less => return false,
                                Ordering::Equal => {}
                            }
                        }
                    }
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EvidenceKind;

    fn descriptor(id: &str, version: &str, synthetic: bool) -> AdapterDescriptor {
        let agent = AgentId::new(id).unwrap();
        let version = Version::parse(version).unwrap();
        let mut capabilities = Capabilities::default();
        for operation in [Operation::StartNew, Operation::ResumeNative] {
            capabilities
                .qualify(
                    operation,
                    &agent,
                    &version,
                    synthetic,
                    evidence(&agent, &version, synthetic),
                )
                .unwrap();
        }
        AdapterDescriptor {
            agent,
            display_name: "Explicit argv fixture".into(),
            version,
            architecture: if synthetic {
                ConnectionArchitecture::InMemory
            } else {
                ConnectionArchitecture::NativePty
            },
            synthetic,
            capabilities,
        }
    }
    fn evidence(agent: &AgentId, version: &Version, synthetic: bool) -> FixtureEvidence {
        FixtureEvidence::new(
            agent.clone(),
            version.clone(),
            "fixtures/argv-contract-only",
            if synthetic {
                EvidenceKind::SyntheticFixture
            } else {
                EvidenceKind::InstalledRuntimeFixture
            },
        )
        .unwrap()
    }
    fn recipe(
        descriptor: &AdapterDescriptor,
        form: CommandForm,
        args: Vec<OsString>,
    ) -> QualifiedArgvRecipe {
        QualifiedArgvRecipe::qualify(
            descriptor,
            Operation::StartNew,
            form,
            args,
            evidence(&descriptor.agent, &descriptor.version, descriptor.synthetic),
        )
        .unwrap()
    }
    fn native() -> CommandForm {
        CommandForm::NativeExe("C:/Program Files/Synthetic & Agent/agent.exe".into())
    }

    #[test]
    fn native_arguments_keep_spaces_metacharacters_and_empty_tokens_unchanged() {
        let descriptor = descriptor("fake_argv", "1.0.0+fixture", true);
        let args: Vec<OsString> = vec![
            "--project".into(),
            "C:/Work & Notes/a project".into(),
            "a\"b;$()&|".into(),
            "".into(),
        ];
        let original = args.clone();
        let recipe = recipe(&descriptor, native(), args);
        let plan = plan_arguments(
            &descriptor,
            Operation::StartNew,
            Path::new("C:/Work & Notes/a project"),
            Some(&recipe),
        )
        .unwrap();
        assert_eq!(plan.argv(), original);
        assert_eq!(
            plan.executable(),
            Path::new("C:/Program Files/Synthetic & Agent/agent.exe")
        );
    }

    #[test]
    fn missing_or_rebound_recipe_refuses_agent_build_operation_and_fixture_kind() {
        let original = descriptor("fake_argv", "1.0.0+one", true);
        let recipe = recipe(&original, native(), vec![]);
        let cwd = Path::new("C:/work");
        assert_eq!(
            plan_arguments(&original, Operation::StartNew, cwd, None).unwrap_err(),
            ArgvError::MissingRecipe
        );
        for other in [
            descriptor("fake_other", "1.0.0+one", true),
            descriptor("fake_argv", "1.0.0+two", true),
        ] {
            assert_eq!(
                plan_arguments(&other, Operation::StartNew, cwd, Some(&recipe)).unwrap_err(),
                ArgvError::Unqualified(RefusalReason::VersionMismatch)
            );
        }
        assert_eq!(
            plan_arguments(&original, Operation::ResumeNative, cwd, Some(&recipe)).unwrap_err(),
            ArgvError::WrongOperation
        );
        let native_descriptor = descriptor("pi", "1.4.0", false);
        let synthetic = FixtureEvidence::new(
            native_descriptor.agent.clone(),
            native_descriptor.version.clone(),
            "fixtures/synthetic-only",
            EvidenceKind::SyntheticFixture,
        )
        .unwrap();
        assert_eq!(
            QualifiedArgvRecipe::qualify(
                &native_descriptor,
                Operation::StartNew,
                native(),
                vec![],
                synthetic
            )
            .unwrap_err(),
            ArgvError::Unqualified(RefusalReason::SyntheticEvidenceForNativeAdapter)
        );
    }

    #[test]
    fn wrappers_relative_paths_and_nul_tokens_are_refused_without_resolution() {
        let descriptor = descriptor("fake_argv", "1.0.0", true);
        for program in [
            "agent.exe",
            "C:/tools/agent.cmd",
            "C:/tools/agent.bat",
            "C:/tools/agent.ps1",
            "C:/tools/../agent.exe",
            r"UNC\server\share\agent.exe",
        ] {
            assert_eq!(
                QualifiedArgvRecipe::qualify(
                    &descriptor,
                    Operation::StartNew,
                    CommandForm::NativeExe(program.into()),
                    vec![],
                    evidence(&descriptor.agent, &descriptor.version, true)
                )
                .unwrap_err(),
                ArgvError::InvalidProgram
            );
        }
        assert_eq!(
            QualifiedArgvRecipe::qualify(
                &descriptor,
                Operation::StartNew,
                native(),
                vec!["bad\0token".into()],
                evidence(&descriptor.agent, &descriptor.version, true)
            )
            .unwrap_err(),
            ArgvError::InvalidArgument
        );
        for script in [
            "relative.js",
            "C:/tools/agent.cmd",
            "C:/tools/agent.bat",
            "C:/tools/agent.ps1",
        ] {
            let form = CommandForm::NodeScript {
                node_exe: "C:/node/node.exe".into(),
                script: script.into(),
                node_version: Version::parse("24.0.0").unwrap(),
                minimum_node_version: None,
            };
            assert_eq!(
                QualifiedArgvRecipe::qualify(
                    &descriptor,
                    Operation::StartNew,
                    form,
                    vec![],
                    evidence(&descriptor.agent, &descriptor.version, true)
                )
                .unwrap_err(),
                ArgvError::InvalidScript
            );
        }
        let recipe = recipe(&descriptor, native(), vec![]);
        assert_eq!(
            plan_arguments(
                &descriptor,
                Operation::StartNew,
                Path::new("relative-cwd"),
                Some(&recipe)
            )
            .unwrap_err(),
            ArgvError::InvalidWorkingDirectory
        );
    }

    #[test]
    fn count_bytes_and_utf16_quoting_upper_bound_are_checked_before_plan_clones() {
        let descriptor = descriptor("fake_argv", "1.0.0", true);
        for args in [
            vec![OsString::new(); MAX_ARGUMENTS + 1],
            vec!["x".repeat(MAX_TOKEN_BYTES + 1).into()],
            vec!["x".repeat(5000).into(); 4],
            vec!["界".repeat(2500).into(); 9],
        ] {
            assert_eq!(
                QualifiedArgvRecipe::qualify(
                    &descriptor,
                    Operation::StartNew,
                    native(),
                    args,
                    evidence(&descriptor.agent, &descriptor.version, true)
                )
                .unwrap_err(),
                ArgvError::LimitExceeded
            );
        }
    }

    #[test]
    fn node_script_is_one_resolved_token_and_minimum_is_specific_to_recipe() {
        let fake = descriptor("fake_node", "1.0.0", true);
        let form = CommandForm::NodeScript {
            node_exe: "C:/Node Runtime/node.exe".into(),
            script: "C:/Synthetic & Agent/cli.js".into(),
            node_version: Version::parse("20.0.0").unwrap(),
            minimum_node_version: Some(Version::parse("18.0.0").unwrap()),
        };
        let recipe = recipe(&fake, form, vec!["--fixture".into()]);
        let plan = plan_arguments(
            &fake,
            Operation::StartNew,
            Path::new("C:/work"),
            Some(&recipe),
        )
        .unwrap();
        assert_eq!(
            plan.argv(),
            &[
                OsString::from("C:/Synthetic & Agent/cli.js"),
                OsString::from("--fixture")
            ]
        );
        let pi = descriptor("pi", "1.4.0", false);
        for (actual, minimum, error) in [
            ("24.0.0", None, ArgvError::MissingNodeMinimum),
            ("24.0.0", Some("22.18.0"), ArgvError::NodeMinimumTooLow),
            ("22.18.0", Some("22.19.0"), ArgvError::NodeVersionTooOld),
            (
                "22.19.0-rc.1",
                Some("22.19.0"),
                ArgvError::NodeVersionTooOld,
            ),
        ] {
            let form = CommandForm::NodeScript {
                node_exe: "C:/node/node.exe".into(),
                script: "C:/pi/cli.js".into(),
                node_version: Version::parse(actual).unwrap(),
                minimum_node_version: minimum.map(|value| Version::parse(value).unwrap()),
            };
            assert_eq!(
                QualifiedArgvRecipe::qualify(
                    &pi,
                    Operation::StartNew,
                    form,
                    vec![],
                    evidence(&pi.agent, &pi.version, false)
                )
                .unwrap_err(),
                error
            );
        }
        assert!(version_at_least(
            &Version::parse("22.19.0+build").unwrap(),
            &Version::parse("22.19.0").unwrap()
        ));
        assert!(version_at_least(
            &Version::parse("22.20.0-rc.1").unwrap(),
            &Version::parse("22.19.0").unwrap()
        ));
        assert!(!version_at_least(
            &Version::parse("22.19.0-rc.2").unwrap(),
            &Version::parse("22.19.0-rc.10").unwrap()
        ));
    }

    #[test]
    fn debug_contains_no_argv_cwd_or_program_path_and_prepared_plan_is_immutable() {
        let descriptor = descriptor("fake_argv", "1.0.0", true);
        let recipe = recipe(
            &descriptor,
            native(),
            vec!["synthetic-secret-argument".into()],
        );
        let plan = plan_arguments(
            &descriptor,
            Operation::StartNew,
            Path::new("C:/synthetic-secret-working-directory"),
            Some(&recipe),
        )
        .unwrap();
        let clone = plan.clone();
        let debug = format!("{recipe:?} {plan:?} {clone:?}");
        for secret in ["synthetic-secret", "Program Files", "agent.exe"] {
            assert!(!debug.contains(secret));
        }
        assert_eq!(plan.argv(), clone.argv());
    }
}
