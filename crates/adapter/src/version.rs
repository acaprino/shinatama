use serde::Serialize;
use std::fmt;

const MAX_VERSION_BYTES: usize = 128;

/// Exact identity includes prerelease and build metadata; fixtures are not inferred
/// to apply to a different build merely because SemVer orders them together.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Version {
    major: u32,
    minor: u32,
    patch: u32,
    prerelease: Option<String>,
    build: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionError {
    TooLong,
    InvalidFormat,
    InvalidNumber,
    InvalidIdentifier,
    AmbiguousOutput,
}

impl Version {
    pub fn major(&self) -> u32 {
        self.major
    }
    pub fn minor(&self) -> u32 {
        self.minor
    }
    pub fn patch(&self) -> u32 {
        self.patch
    }
    pub fn prerelease(&self) -> Option<&str> {
        self.prerelease.as_deref()
    }
    pub fn build(&self) -> Option<&str> {
        self.build.as_deref()
    }
    pub fn parse(input: &str) -> Result<Self, VersionError> {
        if input.len() > MAX_VERSION_BYTES {
            return Err(VersionError::TooLong);
        }
        if input.is_empty() || !input.is_ascii() {
            return Err(VersionError::InvalidFormat);
        }
        let (without_build, build) = split_once(input, '+');
        let (numbers, prerelease) = split_once(without_build, '-');
        let mut parts = numbers.split('.');
        let major = number(parts.next().ok_or(VersionError::InvalidFormat)?)?;
        let minor = number(parts.next().ok_or(VersionError::InvalidFormat)?)?;
        let patch = number(parts.next().ok_or(VersionError::InvalidFormat)?)?;
        if parts.next().is_some() {
            return Err(VersionError::InvalidFormat);
        }
        for (value, numeric_zeros) in [(prerelease, false), (build, true)] {
            if let Some(value) = value {
                identifiers(value, numeric_zeros)?;
            }
        }
        Ok(Self {
            major,
            minor,
            patch,
            prerelease: prerelease.map(str::to_owned),
            build: build.map(str::to_owned),
        })
    }
}

fn split_once(input: &str, separator: char) -> (&str, Option<&str>) {
    input
        .split_once(separator)
        .map_or((input, None), |(a, b)| (a, Some(b)))
}
fn number(input: &str) -> Result<u32, VersionError> {
    if input.is_empty()
        || !input.bytes().all(|byte| byte.is_ascii_digit())
        || (input.len() > 1 && input.starts_with('0'))
    {
        return Err(VersionError::InvalidNumber);
    }
    input.parse().map_err(|_| VersionError::InvalidNumber)
}
fn identifiers(input: &str, allow_numeric_zeros: bool) -> Result<(), VersionError> {
    for part in input.split('.') {
        if part.is_empty()
            || !part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || (!allow_numeric_zeros
                && part.len() > 1
                && part.starts_with('0')
                && part.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return Err(VersionError::InvalidIdentifier);
        }
    }
    Ok(())
}
impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(value) = &self.prerelease {
            write!(f, "-{value}")?;
        }
        if let Some(value) = &self.build {
            write!(f, "+{value}")?;
        }
        Ok(())
    }
}

/// No raw stdout/stderr is retained in diagnostics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ObservedVersion {
    Known { version: Version },
    Unknown { reason: VersionUnknown },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionUnknown {
    NotProbed,
    NeedsContainedProbe,
    WrapperUnresolved,
    TimedOut,
    OutputLimit,
    NonZeroExit,
    UnexpectedOutput,
    MissingProgram,
}

/// Bounded fixture-output decoder. Successful parsing is not runtime evidence,
/// an isolation recipe, or a compatibility claim. C11 supplies safe probes.
pub fn parse_version_output(agent: &str, output: &[u8]) -> Result<Version, VersionError> {
    if output.len() > 512 {
        return Err(VersionError::TooLong);
    }
    let line = std::str::from_utf8(output)
        .map_err(|_| VersionError::InvalidFormat)?
        .trim();
    if line.lines().count() != 1 || line.chars().any(char::is_control) {
        return Err(VersionError::AmbiguousOutput);
    }
    let value = match agent {
        "claude_code" => line
            .strip_suffix(" (Claude Code)")
            .ok_or(VersionError::InvalidFormat)?,
        "codex" => line
            .strip_prefix("codex-cli ")
            .ok_or(VersionError::InvalidFormat)?,
        "opencode" => line.strip_prefix('v').unwrap_or(line),
        "pi" => line.strip_prefix("pi ").unwrap_or(line),
        // Their installed version formats and isolation remain unqualified.
        "hermes_agent" | "deepseek_harness" => return Err(VersionError::InvalidFormat),
        _ => return Err(VersionError::InvalidFormat),
    };
    Version::parse(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_versions_preserve_qualification_identity() {
        let first = Version::parse("0.2.1-rc.5+fixture.01").unwrap();
        assert_eq!(first.to_string(), "0.2.1-rc.5+fixture.01");
        assert_ne!(first, Version::parse("0.2.1-rc.5+fixture.02").unwrap());
        for invalid in [
            "",
            "01.2.3",
            "1.2",
            "1.2.3.4",
            "1.2.3-01",
            "1.2.3+",
            "1.2.3-",
            "1.2.3+one+two",
            "4294967296.1.1",
            "1.2.3\n",
        ] {
            assert!(Version::parse(invalid).is_err(), "{invalid:?}");
        }
    }
    #[test]
    fn probe_output_never_guesses_among_banners_or_other_versions() {
        assert_eq!(
            parse_version_output("claude_code", b"2.1.316 (Claude Code)\r\n")
                .unwrap()
                .to_string(),
            "2.1.316"
        );
        assert_eq!(
            parse_version_output("codex", b"codex-cli 0.164.0\n")
                .unwrap()
                .to_string(),
            "0.164.0"
        );
        assert!(parse_version_output("codex", b"node 24.14.1\ncodex-cli 0.164.0").is_err());
        assert!(parse_version_output("claude_code", b"warning token=secret 2.1.316").is_err());
        assert!(parse_version_output("opencode", &vec![b'x'; 513]).is_err());
        assert!(parse_version_output("hermes_agent", b"hermes-agent 1.0.0").is_err());
        assert!(parse_version_output("deepseek_harness", b"dsh 1.0.0").is_err());
    }
}
