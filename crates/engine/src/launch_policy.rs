//! The user's launch policy: whether agents start without their own protections
//! ("full access") and which extra variables every real session receives.
//!
//! The desktop sets it once from the saved settings and again on every save; a
//! launcher reads it while composing a real session. Qualification trials never
//! read it, so the fixed, hardened trial recipes stay what they were proved as.
//! Values are secrets: `Debug` shows names only and nothing here logs a value.
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fmt,
    sync::{OnceLock, RwLock},
};

pub const MAX_ENTRIES: usize = 64;
pub const MAX_NAME_BYTES: usize = 128;
pub const MAX_VALUE_BYTES: usize = 8 * 1024;

/// Names a user variable can never replace: they carry the private home, the
/// proxy and CA chain, or Shinatama's and each agent's own session wiring.
const RESERVED_NAMES: &[&str] = &[
    "PATH",
    "PATHEXT",
    "SYSTEMROOT",
    "WINDIR",
    "HOME",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "TEMP",
    "TMP",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "NODE_EXTRA_CA_CERTS",
    "SSL_CERT_FILE",
    "NODE_TLS_REJECT_UNAUTHORIZED",
    "BUN_TLS_REJECT_UNAUTHORIZED",
];
const RESERVED_PREFIXES: &[&str] = &[
    "SHINATAMA_",
    "MUKADE_",
    "ANTHROPIC_",
    "CLAUDE",
    "CODEX_",
    "OPENCODE_",
    "XDG_",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyError {
    TooMany,
    InvalidName,
    ReservedName,
    DuplicateName,
    InvalidValue,
}
impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooMany => "Too many variables.",
            Self::InvalidName => {
                "A variable name uses letters, digits and underscores, and does not start with a digit."
            }
            Self::ReservedName => {
                "That variable name belongs to Shinatama or to an agent and cannot be replaced."
            }
            Self::DuplicateName => "A variable name appears twice.",
            Self::InvalidValue => "A variable value is too long or contains a NUL.",
        })
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct LaunchPolicy {
    full_access: bool,
    environment: Vec<(String, String)>,
}
impl LaunchPolicy {
    pub fn new(full_access: bool, environment: Vec<(String, String)>) -> Result<Self, PolicyError> {
        if environment.len() > MAX_ENTRIES {
            return Err(PolicyError::TooMany);
        }
        let mut seen = Vec::with_capacity(environment.len());
        for (name, value) in &environment {
            validate_name(name)?;
            // An empty value is valid: Git uses one to clear an earlier helper list.
            if value.len() > MAX_VALUE_BYTES || value.contains('\0') {
                return Err(PolicyError::InvalidValue);
            }
            let folded = name.to_ascii_uppercase();
            if seen.contains(&folded) {
                return Err(PolicyError::DuplicateName);
            }
            seen.push(folded);
        }
        Ok(Self {
            full_access,
            environment,
        })
    }
    pub fn full_access(&self) -> bool {
        self.full_access
    }
    /// Adds the user's variables. Reserved names cannot be present, so the
    /// private home, proxy and CA values a launcher already set stay as they are.
    pub fn apply_environment(&self, environment: &mut BTreeMap<OsString, OsString>) {
        for (name, value) in &self.environment {
            environment.retain(|key, _| !key.eq_ignore_ascii_case(name));
            environment.insert(OsString::from(name), OsString::from(value));
        }
    }
}
impl fmt::Debug for LaunchPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LaunchPolicy")
            .field("full_access", &self.full_access)
            .field(
                "names",
                &self
                    .environment
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

pub fn validate_name(name: &str) -> Result<(), PolicyError> {
    let mut chars = name.chars();
    let first = chars.next().ok_or(PolicyError::InvalidName)?;
    if name.len() > MAX_NAME_BYTES
        || !(first.is_ascii_alphabetic() || first == '_')
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(PolicyError::InvalidName);
    }
    let folded = name.to_ascii_uppercase();
    if RESERVED_NAMES.contains(&folded.as_str())
        || RESERVED_PREFIXES
            .iter()
            .any(|prefix| folded.starts_with(prefix))
    {
        return Err(PolicyError::ReservedName);
    }
    Ok(())
}

fn cell() -> &'static RwLock<LaunchPolicy> {
    static CELL: OnceLock<RwLock<LaunchPolicy>> = OnceLock::new();
    CELL.get_or_init(|| RwLock::new(LaunchPolicy::default()))
}
/// Replaces the policy for launches that start after this call.
pub fn set_current(policy: LaunchPolicy) {
    *cell()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = policy;
}
pub fn current() -> LaunchPolicy {
    cell()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, value: &str) -> (String, String) {
        (name.to_owned(), value.to_owned())
    }

    #[test]
    fn default_policy_is_protected_and_adds_nothing() {
        let policy = LaunchPolicy::default();
        assert!(!policy.full_access());
        let mut environment = BTreeMap::from([(OsString::from("A"), OsString::from("1"))]);
        policy.apply_environment(&mut environment);
        assert_eq!(environment.len(), 1);
    }

    #[test]
    fn names_that_carry_the_box_or_an_agents_wiring_are_refused() {
        for name in [
            "PATH",
            "path",
            "UserProfile",
            "HTTPS_PROXY",
            "NODE_EXTRA_CA_CERTS",
            "SHINATAMA_HOME",
            "ANTHROPIC_API_KEY",
            "CLAUDE_CODE_SAFE_MODE",
            "CLAUDECODE",
            "CODEX_HOME",
            "OPENCODE_PERMISSION",
            "XDG_CONFIG_HOME",
        ] {
            assert_eq!(
                LaunchPolicy::new(true, vec![entry(name, "x")]).unwrap_err(),
                PolicyError::ReservedName,
                "{name}"
            );
        }
        for name in ["", "1ABC", "A-B", "A B", "A=B", "É"] {
            assert_eq!(
                LaunchPolicy::new(true, vec![entry(name, "x")]).unwrap_err(),
                PolicyError::InvalidName,
                "{name:?}"
            );
        }
    }

    #[test]
    fn values_and_counts_are_bounded_and_names_are_unique_ignoring_case() {
        assert!(LaunchPolicy::new(false, vec![entry("A", "")]).is_ok());
        assert_eq!(
            LaunchPolicy::new(false, vec![entry("A", "a\0b")]).unwrap_err(),
            PolicyError::InvalidValue
        );
        assert_eq!(
            LaunchPolicy::new(false, vec![entry("A", &"x".repeat(MAX_VALUE_BYTES + 1))])
                .unwrap_err(),
            PolicyError::InvalidValue
        );
        assert_eq!(
            LaunchPolicy::new(false, vec![entry("Token", "1"), entry("TOKEN", "2")]).unwrap_err(),
            PolicyError::DuplicateName
        );
        let many = (0..=MAX_ENTRIES)
            .map(|i| entry(&format!("V{i}"), "1"))
            .collect();
        assert_eq!(
            LaunchPolicy::new(false, many).unwrap_err(),
            PolicyError::TooMany
        );
    }

    #[test]
    fn variables_replace_an_inherited_one_of_any_case_and_debug_hides_values() {
        let policy = LaunchPolicy::new(
            true,
            vec![entry("GH_TOKEN", "secret-value"), entry("MY_FLAG", "on")],
        )
        .unwrap();
        let mut environment = BTreeMap::from([
            (OsString::from("gh_token"), OsString::from("old")),
            (OsString::from("KEEP"), OsString::from("1")),
        ]);
        policy.apply_environment(&mut environment);
        assert!(!environment.contains_key(&OsString::from("gh_token")));
        assert_eq!(
            environment.get(&OsString::from("GH_TOKEN")).unwrap(),
            "secret-value"
        );
        assert_eq!(environment.get(&OsString::from("MY_FLAG")).unwrap(), "on");
        assert_eq!(environment.get(&OsString::from("KEEP")).unwrap(), "1");
        let shown = format!("{policy:?}");
        assert!(shown.contains("GH_TOKEN") && !shown.contains("secret-value"));
    }

    #[test]
    fn the_current_policy_is_replaced_whole() {
        set_current(LaunchPolicy::new(true, vec![entry("A", "1")]).unwrap());
        assert!(current().full_access());
        set_current(LaunchPolicy::default());
        assert!(!current().full_access());
    }
}
