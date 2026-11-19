//! Bounded, read-only discovery. A discovered executable is not a qualified adapter.
//! Vendor store layouts remain owned by the separately qualified Mukade provider.
use crate::pe_version::{PeVersionMetadata, PeVersionUnavailable, inspect_pe_version};
use crate::{AgentId, ObservedVersion, VersionUnknown};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub const MAX_PATH_DIRECTORIES: usize = 32;
pub const MAX_CANDIDATES: usize = 128;
const MAX_PE_INSPECTIONS: usize = 32;

#[derive(Clone, Copy, Debug)]
pub struct KnownProgram {
    pub agent: &'static str,
    pub display_name: &'static str,
    pub executable: &'static str,
}

pub const KNOWN_PROGRAMS: [KnownProgram; 6] = [
    KnownProgram {
        agent: "claude_code",
        display_name: "Claude Code",
        executable: "claude",
    },
    KnownProgram {
        agent: "codex",
        display_name: "Codex",
        executable: "codex",
    },
    KnownProgram {
        agent: "opencode",
        display_name: "OpenCode",
        executable: "opencode",
    },
    KnownProgram {
        agent: "pi",
        display_name: "Pi",
        executable: "pi",
    },
    KnownProgram {
        agent: "hermes_agent",
        display_name: "Hermes Agent",
        executable: "hermes",
    },
    KnownProgram {
        agent: "deepseek_harness",
        display_name: "DeepSeek Harness",
        executable: "dsh",
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgramKind {
    DirectExe,
    CmdWrapper,
    PowerShellWrapper,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProgramCandidate {
    pub agent: AgentId,
    pub path: PathBuf,
    pub kind: ProgramKind,
    pub version: ObservedVersion,
    pub file_version_metadata: PeVersionMetadata,
}

impl ProgramCandidate {
    /// The arguments shown in launch review, never passed to a shell by this SDK.
    pub fn argument_status(&self) -> &'static str {
        match self.kind {
            ProgramKind::DirectExe => "needs_c11_typed_launch_plan",
            ProgramKind::CmdWrapper | ProgramKind::PowerShellWrapper => {
                "wrapper_needs_safe_launch_resolution"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryIssue {
    UnsupportedPlatform,
    PathDirectoryLimit,
    CandidateLimit,
    RelativePathSkipped,
    NetworkPathSkipped,
    UnsupportedDriveSkipped,
    InvalidPathSkipped,
    ReparsePointSkipped,
    InspectionFailed,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CatalogSnapshot {
    pub candidates: Vec<ProgramCandidate>,
    pub issues: Vec<DiscoveryIssue>,
    pub provider: String,
}

/// Implementations may provide the qualified Mukade revision without introducing
/// a second set of native session-layout rules into the engine.
pub trait InstallationCatalog {
    fn inspect(&self) -> CatalogSnapshot;
}

pub struct PathCatalog {
    directories: Vec<PathBuf>,
    windows: bool,
}

impl PathCatalog {
    pub fn from_environment() -> Self {
        // PATH contains executable locations only. No HOME, token, agent config,
        // credential store or transcript is read.
        let directories = std::env::var_os("PATH")
            .map(|value| {
                std::env::split_paths(&value)
                    .take(MAX_PATH_DIRECTORIES + 1)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            directories,
            windows: cfg!(windows),
        }
    }
}

impl InstallationCatalog for PathCatalog {
    fn inspect(&self) -> CatalogSnapshot {
        let mut snapshot = discover_with_preflight(
            &self.directories,
            self.windows,
            preflight_local_directory,
            inspect_local,
        );
        let mut inspected = 0;
        for candidate in &mut snapshot.candidates {
            candidate.file_version_metadata = if candidate.kind != ProgramKind::DirectExe {
                PeVersionMetadata::Unavailable {
                    reason: PeVersionUnavailable::Wrapper,
                }
            } else if inspected == MAX_PE_INSPECTIONS {
                PeVersionMetadata::Unavailable {
                    reason: PeVersionUnavailable::InspectionLimit,
                }
            } else {
                inspected += 1;
                inspect_pe_version(&candidate.path)
            };
        }
        snapshot
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryStatus {
    Missing,
    RegularFile,
    ReparsePoint,
    InspectionFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirectoryStatus {
    Ready,
    Network,
    UnsupportedDrive,
    ReparsePoint,
    Invalid,
    InspectionFailed,
}

/// Pure scanner supports deterministic fixtures. It never executes a candidate.
pub fn discover(
    directories: &[PathBuf],
    windows: bool,
    inspect: impl Fn(&Path) -> EntryStatus,
) -> CatalogSnapshot {
    discover_with_preflight(directories, windows, |_| DirectoryStatus::Ready, inspect)
}

fn discover_with_preflight(
    directories: &[PathBuf],
    windows: bool,
    preflight: impl Fn(&Path) -> DirectoryStatus,
    inspect: impl Fn(&Path) -> EntryStatus,
) -> CatalogSnapshot {
    let mut snapshot = CatalogSnapshot {
        provider: "bounded_path_v1".to_owned(),
        ..Default::default()
    };
    if !windows {
        snapshot.issues.push(DiscoveryIssue::UnsupportedPlatform);
        return snapshot;
    }
    if directories.len() > MAX_PATH_DIRECTORIES {
        snapshot.issues.push(DiscoveryIssue::PathDirectoryLimit);
    }
    let mut seen = BTreeSet::new();
    for directory in directories.iter().take(MAX_PATH_DIRECTORIES) {
        let raw = directory.to_string_lossy();
        if raw.starts_with(r"\\") || raw.starts_with("//") {
            push_issue(&mut snapshot, DiscoveryIssue::NetworkPathSkipped);
            continue;
        }
        if raw.len() > 4096 || raw.chars().any(char::is_control) {
            push_issue(&mut snapshot, DiscoveryIssue::InvalidPathSkipped);
            continue;
        }
        // Windows path qualification is lexical so fixtures also run on a Unix
        // test host. Root-relative and drive-relative forms are not accepted.
        let bytes = raw.as_bytes();
        let absolute = bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/');
        if !absolute {
            push_issue(&mut snapshot, DiscoveryIssue::RelativePathSkipped);
            continue;
        }
        if bytes.iter().skip(2).any(|byte| *byte == b':')
            || raw.split(['\\', '/']).any(|part| part == "..")
        {
            push_issue(&mut snapshot, DiscoveryIssue::InvalidPathSkipped);
            continue;
        }
        if !seen.insert(raw.to_ascii_lowercase()) {
            continue;
        }
        let issue = match preflight(directory) {
            DirectoryStatus::Ready => None,
            DirectoryStatus::Network => Some(DiscoveryIssue::NetworkPathSkipped),
            DirectoryStatus::UnsupportedDrive => Some(DiscoveryIssue::UnsupportedDriveSkipped),
            DirectoryStatus::ReparsePoint => Some(DiscoveryIssue::ReparsePointSkipped),
            DirectoryStatus::Invalid => Some(DiscoveryIssue::InvalidPathSkipped),
            DirectoryStatus::InspectionFailed => Some(DiscoveryIssue::InspectionFailed),
        };
        if let Some(issue) = issue {
            push_issue(&mut snapshot, issue);
            continue;
        }
        for program in KNOWN_PROGRAMS {
            for (extension, kind) in [
                ("exe", ProgramKind::DirectExe),
                ("cmd", ProgramKind::CmdWrapper),
                ("ps1", ProgramKind::PowerShellWrapper),
            ] {
                let path = directory.join(format!("{}.{}", program.executable, extension));
                match inspect(&path) {
                    EntryStatus::Missing => {}
                    EntryStatus::ReparsePoint => {
                        push_issue(&mut snapshot, DiscoveryIssue::ReparsePointSkipped)
                    }
                    EntryStatus::InspectionFailed => {
                        push_issue(&mut snapshot, DiscoveryIssue::InspectionFailed)
                    }
                    EntryStatus::RegularFile => {
                        if snapshot.candidates.len() == MAX_CANDIDATES {
                            push_issue(&mut snapshot, DiscoveryIssue::CandidateLimit);
                            return snapshot;
                        }
                        snapshot.candidates.push(ProgramCandidate {
                            agent: AgentId::new(program.agent).expect("static agent ID"),
                            path,
                            kind,
                            version: ObservedVersion::Unknown {
                                reason: match kind {
                                    ProgramKind::DirectExe => VersionUnknown::NeedsContainedProbe,
                                    ProgramKind::CmdWrapper | ProgramKind::PowerShellWrapper => {
                                        VersionUnknown::WrapperUnresolved
                                    }
                                },
                            },
                            file_version_metadata: PeVersionMetadata::NotInspected,
                        });
                    }
                }
            }
        }
    }
    snapshot
}

fn push_issue(snapshot: &mut CatalogSnapshot, issue: DiscoveryIssue) {
    if !snapshot.issues.contains(&issue) {
        snapshot.issues.push(issue);
    }
}

fn inspect_local(path: &Path) -> EntryStatus {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return EntryStatus::ReparsePoint;
                }
            }
            if metadata.file_type().is_symlink() {
                EntryStatus::ReparsePoint
            } else if metadata.is_file() {
                EntryStatus::RegularFile
            } else {
                EntryStatus::Missing
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => EntryStatus::Missing,
        Err(_) => EntryStatus::InspectionFailed,
    }
}

/// Check the drive before metadata, then walk ancestors from the root so a known
/// junction is rejected before a child read can follow it to a network share.
#[cfg(windows)]
fn preflight_local_directory(directory: &Path) -> DirectoryStatus {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    let raw = directory.to_string_lossy();
    let bytes = raw.as_bytes();
    // This also keeps the helper safe if called independently of discover.
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !matches!(bytes[2], b'\\' | b'/')
        || raw.len() > 4096
        || raw.chars().any(char::is_control)
        || bytes.iter().skip(2).any(|byte| *byte == b':')
        || raw.split(['\\', '/']).any(|part| part == "..")
    {
        return DirectoryStatus::Invalid;
    }
    let root = [u16::from(bytes[0]), u16::from(b':'), u16::from(b'\\'), 0];
    // SAFETY: root is a NUL-terminated drive root and no path is dereferenced.
    match unsafe { GetDriveTypeW(root.as_ptr()) } {
        3 => {}                               // DRIVE_FIXED
        4 => return DirectoryStatus::Network, // DRIVE_REMOTE
        _ => return DirectoryStatus::UnsupportedDrive,
    }
    let ancestors: Vec<_> = directory.ancestors().take(65).collect();
    if ancestors.len() > 64 {
        return DirectoryStatus::Invalid;
    }
    for ancestor in ancestors.into_iter().rev() {
        let metadata = match std::fs::symlink_metadata(ancestor) {
            Ok(metadata) => metadata,
            Err(_) => return DirectoryStatus::InspectionFailed,
        };
        if metadata.file_attributes() & 0x400 != 0 {
            return DirectoryStatus::ReparsePoint;
        }
        if !metadata.is_dir() {
            return DirectoryStatus::InspectionFailed;
        }
    }
    DirectoryStatus::Ready
}

#[cfg(not(windows))]
fn preflight_local_directory(_directory: &Path) -> DirectoryStatus {
    DirectoryStatus::UnsupportedDrive
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn wrappers_remain_unresolved_and_are_never_read_or_executed() {
        let calls = Cell::new(0);
        let snapshot = discover(&[PathBuf::from("C:/tools")], true, |path| {
            calls.set(calls.get() + 1);
            if path.ends_with("codex.exe") || path.ends_with("pi.cmd") {
                EntryStatus::RegularFile
            } else {
                EntryStatus::Missing
            }
        });
        assert_eq!(calls.get(), KNOWN_PROGRAMS.len() * 3);
        assert_eq!(snapshot.candidates.len(), 2);
        assert!(matches!(
            snapshot.candidates[0].version,
            ObservedVersion::Unknown {
                reason: VersionUnknown::NeedsContainedProbe
            }
        ));
        assert!(matches!(
            snapshot.candidates[1].version,
            ObservedVersion::Unknown {
                reason: VersionUnknown::WrapperUnresolved
            }
        ));
    }

    #[test]
    fn unsafe_path_forms_and_unsupported_platform_do_not_touch_filesystem() {
        let paths = [
            PathBuf::from("."),
            PathBuf::from("C:relative"),
            PathBuf::from(r"\\server\tools"),
            PathBuf::from("C:/tools/../other"),
            PathBuf::from("C:/tools:stream"),
        ];
        let snapshot = discover(&paths, true, |_| panic!("unsafe path inspected"));
        assert!(snapshot.candidates.is_empty());
        assert!(
            snapshot
                .issues
                .contains(&DiscoveryIssue::NetworkPathSkipped)
        );
        let snapshot = discover(&[PathBuf::from("C:/tools")], false, |_| {
            panic!("non-Windows probe")
        });
        assert_eq!(snapshot.issues, vec![DiscoveryIssue::UnsupportedPlatform]);
    }

    #[test]
    fn path_and_candidate_limits_are_explicit() {
        let paths: Vec<_> = (0..100)
            .map(|i| PathBuf::from(format!("C:/tools/{i}")))
            .collect();
        let snapshot = discover(&paths, true, |_| EntryStatus::RegularFile);
        assert_eq!(snapshot.candidates.len(), MAX_CANDIDATES);
        assert!(
            snapshot
                .issues
                .contains(&DiscoveryIssue::PathDirectoryLimit)
        );
        assert!(snapshot.issues.contains(&DiscoveryIssue::CandidateLimit));
    }

    #[test]
    fn rejected_drive_or_ancestor_never_reaches_candidate_file_inspection() {
        for (status, issue) in [
            (DirectoryStatus::Network, DiscoveryIssue::NetworkPathSkipped),
            (
                DirectoryStatus::UnsupportedDrive,
                DiscoveryIssue::UnsupportedDriveSkipped,
            ),
            (
                DirectoryStatus::ReparsePoint,
                DiscoveryIssue::ReparsePointSkipped,
            ),
            (DirectoryStatus::Invalid, DiscoveryIssue::InvalidPathSkipped),
            (
                DirectoryStatus::InspectionFailed,
                DiscoveryIssue::InspectionFailed,
            ),
        ] {
            let inspected = Cell::new(0);
            let snapshot = discover_with_preflight(
                &[PathBuf::from("Z:/tools")],
                true,
                |_| status,
                |_| {
                    inspected.set(inspected.get() + 1);
                    EntryStatus::RegularFile
                },
            );
            assert_eq!(inspected.get(), 0);
            assert!(snapshot.candidates.is_empty());
            assert_eq!(snapshot.issues, vec![issue]);
        }
    }

    #[test]
    fn candidate_files_are_inspected_only_after_directory_admission() {
        let admitted = Cell::new(false);
        let snapshot = discover_with_preflight(
            &[PathBuf::from("C:/tools")],
            true,
            |_| {
                admitted.set(true);
                DirectoryStatus::Ready
            },
            |_| {
                assert!(admitted.get());
                EntryStatus::Missing
            },
        );
        assert!(admitted.get());
        assert!(snapshot.candidates.is_empty());
        assert!(snapshot.issues.is_empty());
    }
}
