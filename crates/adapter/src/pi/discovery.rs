//! Windows npm layout of Pi. The `pi.cmd` shim is recognized by its exact text
//! and never executed: its two fixed targets become a `node.exe` + `cli.js`
//! argument vector. No `.cmd`, `.ps1`, `cmd.exe` or PowerShell is used to start Pi.
use crate::Version;
use std::path::{Path, PathBuf};

/// The installed package name (PI10): the earlier `@mariozechner/...` name is
/// not recognized.
pub const PACKAGE: &str = "@earendil-works/pi-coding-agent";
/// Name of the npm command shim looked up on PATH.
pub const SHIM_NAME: &str = "pi.cmd";
/// Upper bound for reading the shim; the reviewed text is 365 bytes.
pub const MAX_SHIM_BYTES: u64 = 1024;
/// Pi's own documented Node floor (FRD-0002/R08), applied to the observed Node.
pub const MINIMUM_NODE: &str = "22.19.0";
const SCRIPT: [&str; 6] = [
    "node_modules",
    "@earendil-works",
    "pi-coding-agent",
    "dist",
    "bundle",
    "cli.js",
];
const PACKAGE_MANIFEST: [&str; 4] = [
    "node_modules",
    "@earendil-works",
    "pi-coding-agent",
    "package.json",
];

/// The cmd-shim npm writes for this package. Comparison normalizes CRLF only;
/// any other wrapper (custom script, different package, PowerShell) is refused.
const NPM_SHIM: &str = concat!(
    "@ECHO off\n",
    "GOTO start\n",
    ":find_dp0\n",
    "SET dp0=%~dp0\n",
    "EXIT /b\n",
    ":start\n",
    "SETLOCAL\n",
    "CALL :find_dp0\n",
    "\n",
    "IF EXIST \"%dp0%\\node.exe\" (\n",
    "  SET \"_prog=%dp0%\\node.exe\"\n",
    ") ELSE (\n",
    "  SET \"_prog=node\"\n",
    "  SET PATHEXT=%PATHEXT:;.JS;=;%\n",
    ")\n",
    "\n",
    "endLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  ",
    "\"%dp0%\\node_modules\\@earendil-works\\pi-coding-agent\\dist\\bundle\\cli.js\" %*\n",
);

pub fn is_npm_shim(text: &str) -> bool {
    text.replace("\r\n", "\n") == NPM_SHIM
}

/// `%dp0%\node_modules\@earendil-works\pi-coding-agent\dist\bundle\cli.js`.
pub fn script_path(shim_directory: &Path) -> PathBuf {
    SCRIPT
        .iter()
        .fold(shim_directory.to_owned(), |path, part| path.join(part))
}

/// The package manifest next to the script; hashed, never trusted for a version.
pub fn package_manifest_path(shim_directory: &Path) -> PathBuf {
    PACKAGE_MANIFEST
        .iter()
        .fold(shim_directory.to_owned(), |path, part| path.join(part))
}

/// Where the shim looks first. When it is absent the shim falls back to `node`
/// on PATH; the engine then resolves the first direct `node.exe` in PATH order.
pub fn colocated_node(shim_directory: &Path) -> PathBuf {
    shim_directory.join("node.exe")
}

/// `node --version` prints one `v<semver>` line.
pub fn parse_node_version(output: &[u8]) -> Option<Version> {
    if output.len() > 128 {
        return None;
    }
    let line = std::str::from_utf8(output).ok()?.trim();
    if line.lines().count() != 1 || line.chars().any(char::is_control) {
        return None;
    }
    Version::parse(line.strip_prefix('v')?).ok()
}

/// `node cli.js --version` prints the bare Pi version.
pub fn parse_pi_version(output: &[u8]) -> Option<Version> {
    crate::version::parse_version_output("pi", output).ok()
}

pub fn minimum_node() -> Version {
    Version::parse(MINIMUM_NODE).expect("documented Pi Node minimum")
}

/// A prerelease of the minimum does not satisfy it.
pub fn node_supported(version: &Version) -> bool {
    crate::launch::arguments::version_at_least(version, &minimum_node())
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTALLED: &str = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \"_prog=node\"\r\n  SET PATHEXT=%PATHEXT:;.JS;=;%\r\n)\r\n\r\nendLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \"%dp0%\\node_modules\\@earendil-works\\pi-coding-agent\\dist\\bundle\\cli.js\" %*\r\n";

    #[test]
    fn only_the_exact_npm_shim_is_recognized() {
        assert!(is_npm_shim(INSTALLED));
        assert!(is_npm_shim(&INSTALLED.replace("\r\n", "\n")));
        assert_eq!(INSTALLED.len(), 365);
        // A different package, an extra command or a different interpreter is
        // never interpreted as Pi, even though it would run under cmd.exe.
        for changed in [
            INSTALLED.replace("@earendil-works", "@mariozechner"),
            INSTALLED.replace("%*", "%* & calc"),
            INSTALLED.replace("\"%_prog%\"", "\"bun\""),
            format!("{INSTALLED}del /q *\r\n"),
            INSTALLED.trim_end().to_owned(),
        ] {
            assert!(!is_npm_shim(&changed), "{changed:?}");
        }
    }

    #[test]
    fn fixed_targets_keep_spaces_and_non_ascii_as_one_path() {
        let directory = Path::new(r"C:\Users\Zoë Ünïcode\AppData\Roaming\npm global");
        let script = script_path(directory);
        assert_eq!(
            script,
            PathBuf::from(
                r"C:\Users\Zoë Ünïcode\AppData\Roaming\npm global\node_modules\@earendil-works\pi-coding-agent\dist\bundle\cli.js"
            )
        );
        assert_eq!(
            colocated_node(directory),
            PathBuf::from(r"C:\Users\Zoë Ünïcode\AppData\Roaming\npm global\node.exe")
        );
        assert!(package_manifest_path(directory).ends_with(r"pi-coding-agent\package.json"));
    }

    #[test]
    fn node_floor_is_pi_specific_and_prerelease_of_the_floor_is_too_old() {
        assert_eq!(
            parse_node_version(b"v24.14.1\r\n"),
            Some(Version::parse("24.14.1").unwrap())
        );
        for invalid in [
            b"24.14.1".as_slice(),
            b"v24.14.1\nv22.0.0",
            b"node v24.14.1",
            b"",
        ] {
            assert_eq!(parse_node_version(invalid), None, "{invalid:?}");
        }
        for (version, supported) in [
            ("22.19.0", true),
            ("24.14.1", true),
            ("22.18.9", false),
            ("22.19.0-rc.1", false),
            ("20.11.0", false),
        ] {
            assert_eq!(
                node_supported(&Version::parse(version).unwrap()),
                supported,
                "{version}"
            );
        }
        assert_eq!(
            parse_pi_version(b"1.4.0\n"),
            Some(Version::parse("1.4.0").unwrap())
        );
        assert_eq!(parse_pi_version(b"Usage: pi [options]\nmore"), None);
    }
}
