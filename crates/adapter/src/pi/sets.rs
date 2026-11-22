//! Per-instance Pi resource sets. A selected skill or extension reaches Pi only
//! as an explicit path inside that instance's own materialized directory.
//! Discovery stays disabled by the TUI recipe, so the user's skill folders
//! (`~/.pi/agent/skills`, `~/.agents/skills`, a project's `.pi/`) never enter
//! an instance set. MCP has no qualified per-launch override (PI7): refused.
use crate::sets::{SetKind, SetManifest};
use std::ffi::OsString;
use std::fmt;
use std::path::{Component, Path, PathBuf, Prefix};

/// One catalog entry after a trusted writer placed it in the instance directory.
#[derive(Clone)]
pub struct MaterializedResource {
    pub kind: SetKind,
    pub id: String,
    pub path: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PiSetError {
    McpUnsupported,
    MissingResource,
    UnexpectedResource,
    OutsideInstance,
    InvalidPath,
}

/// Explicit `--skill <path>` / `-e <path>` tokens in canonical manifest order.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct ResourceArguments {
    tokens: Vec<OsString>,
}
impl ResourceArguments {
    pub fn empty() -> Self {
        Self::default()
    }
    pub fn tokens(&self) -> &[OsString] {
        &self.tokens
    }
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
}
impl fmt::Debug for ResourceArguments {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResourceArguments")
            .field("token_count", &self.tokens.len())
            .finish_non_exhaustive()
    }
}

pub fn plan_resources(
    manifest: &SetManifest,
    instance_root: &Path,
    materialized: &[MaterializedResource],
) -> Result<ResourceArguments, PiSetError> {
    let root = drive_components(instance_root)?;
    if materialized.len() != manifest.entries().len() {
        return Err(if materialized.len() > manifest.entries().len() {
            PiSetError::UnexpectedResource
        } else {
            PiSetError::MissingResource
        });
    }
    let mut tokens = Vec::with_capacity(manifest.entries().len() * 2);
    for entry in manifest.entries() {
        let flag = match entry.kind() {
            SetKind::Skill => "--skill",
            SetKind::Plugin => "-e",
            SetKind::Mcp => return Err(PiSetError::McpUnsupported),
        };
        let mut matches = materialized
            .iter()
            .filter(|item| item.kind == entry.kind() && item.id == entry.id());
        let resource = matches.next().ok_or(PiSetError::MissingResource)?;
        if matches.next().is_some() {
            return Err(PiSetError::UnexpectedResource);
        }
        let parts = drive_components(&resource.path)?;
        let inside = parts.len() > root.len()
            && root
                .iter()
                .zip(&parts)
                .all(|(left, right)| left.eq_ignore_ascii_case(right));
        if !inside {
            return Err(PiSetError::OutsideInstance);
        }
        tokens.push(OsString::from(flag));
        tokens.push(resource.path.as_os_str().to_owned());
    }
    Ok(ResourceArguments { tokens })
}

/// Fixed Pi wiring of Marionette's Pi adapter, referenced where it was verified
/// (read-only): its policy extension with `-e` and its skills folder with
/// `--skill`, the two entries its `package.json` declares. The adapter must be
/// an absolute local drive path.
pub fn plan_marionette(adapter: &Path) -> Result<ResourceArguments, PiSetError> {
    let wiring = [
        ("-e", adapter.join("extensions").join("policy.js")),
        ("--skill", adapter.join("skills")),
    ];
    let mut tokens = Vec::with_capacity(wiring.len() * 2);
    for (flag, path) in wiring {
        drive_components(&path)?;
        tokens.push(OsString::from(flag));
        tokens.push(path.into_os_string());
    }
    Ok(ResourceArguments { tokens })
}

/// Absolute local drive paths only; no `.`/`..`, UNC, device or relative form.
fn drive_components(path: &Path) -> Result<Vec<String>, PiSetError> {
    let mut components = path.components();
    match components.next() {
        Some(Component::Prefix(prefix))
            if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)) => {}
        _ => return Err(PiSetError::InvalidPath),
    }
    if components.next() != Some(Component::RootDir) {
        return Err(PiSetError::InvalidPath);
    }
    let mut parts = Vec::new();
    for component in components {
        match component {
            Component::Normal(part) => {
                parts.push(part.to_str().ok_or(PiSetError::InvalidPath)?.to_owned())
            }
            _ => return Err(PiSetError::InvalidPath),
        }
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sets::{SET_SCHEMA_VERSION, SetEntry};

    #[test]
    fn marionette_wires_its_extension_and_skills_in_fixed_order() {
        let args = plan_marionette(Path::new(
            r"C:\data\marionette\releases\v0.1.0\marionette\adapters\pi",
        ))
        .unwrap();
        let flags: Vec<_> = args.tokens().iter().step_by(2).collect();
        assert_eq!(flags, ["-e", "--skill"]);
        assert!(
            args.tokens()[1]
                .to_string_lossy()
                .ends_with(r"pi\extensions\policy.js")
        );
        assert!(args.tokens()[3].to_string_lossy().ends_with(r"pi\skills"));
        assert_eq!(
            plan_marionette(Path::new("relative")).unwrap_err(),
            PiSetError::InvalidPath
        );
    }

    fn manifest(entries: &[(SetKind, &str)]) -> SetManifest {
        SetManifest::new(
            SET_SCHEMA_VERSION,
            "work",
            entries
                .iter()
                .map(|(kind, id)| SetEntry::new(*kind, id, 1).unwrap())
                .collect(),
        )
        .unwrap()
    }
    fn resource(kind: SetKind, id: &str, path: &str) -> MaterializedResource {
        MaterializedResource {
            kind,
            id: id.into(),
            path: path.into(),
        }
    }

    #[test]
    fn two_instances_with_different_sets_share_no_resource() {
        let first_root = Path::new(r"C:\Data Zoë\agents\pi\instances\i_aaaaaaaaaaaa");
        let second_root = Path::new(r"C:\Data Zoë\agents\pi\instances\i_bbbbbbbbbbbb");
        let first = plan_resources(
            &manifest(&[(SetKind::Skill, "review")]),
            first_root,
            &[resource(
                SetKind::Skill,
                "review",
                r"C:\Data Zoë\agents\pi\instances\i_aaaaaaaaaaaa\skills\review",
            )],
        )
        .unwrap();
        let second = plan_resources(
            &manifest(&[(SetKind::Plugin, "notify")]),
            second_root,
            &[resource(
                SetKind::Plugin,
                "notify",
                r"C:\Data Zoë\agents\pi\instances\i_bbbbbbbbbbbb\extensions\notify.ts",
            )],
        )
        .unwrap();
        assert_eq!(
            first.tokens(),
            [
                OsString::from("--skill"),
                r"C:\Data Zoë\agents\pi\instances\i_aaaaaaaaaaaa\skills\review".into()
            ]
        );
        assert_eq!(
            second.tokens(),
            [
                OsString::from("-e"),
                r"C:\Data Zoë\agents\pi\instances\i_bbbbbbbbbbbb\extensions\notify.ts".into()
            ]
        );
        assert!(
            first
                .tokens()
                .iter()
                .all(|token| !second.tokens().contains(token))
        );
        // The second instance cannot borrow the first instance's materialized skill.
        assert_eq!(
            plan_resources(
                &manifest(&[(SetKind::Skill, "review")]),
                second_root,
                &[resource(
                    SetKind::Skill,
                    "review",
                    r"C:\Data Zoë\agents\pi\instances\i_aaaaaaaaaaaa\skills\review",
                )],
            ),
            Err(PiSetError::OutsideInstance)
        );
        assert!(
            plan_resources(&manifest(&[]), first_root, &[])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn user_skill_folders_mcp_and_mismatches_are_refused() {
        let root = Path::new(r"C:\Data\agents\pi\instances\i_aaaaaaaaaaaa");
        for user_folder in [
            r"C:\Users\Zoë\.pi\agent\skills\review",
            r"C:\Users\Zoë\.agents\skills\review",
            r"C:\Data\agents\pi\instances\i_aaaaaaaaaaaa\..\..\skills\review",
            r"C:\Data\agents\pi\instances\i_aaaaaaaaaaaa",
            r"\\server\share\skills\review",
            r"skills\review",
        ] {
            assert!(
                matches!(
                    plan_resources(
                        &manifest(&[(SetKind::Skill, "review")]),
                        root,
                        &[resource(SetKind::Skill, "review", user_folder)],
                    ),
                    Err(PiSetError::OutsideInstance | PiSetError::InvalidPath)
                ),
                "{user_folder}"
            );
        }
        assert_eq!(
            plan_resources(
                &manifest(&[(SetKind::Mcp, "files")]),
                root,
                &[resource(
                    SetKind::Mcp,
                    "files",
                    r"C:\Data\agents\pi\instances\i_aaaaaaaaaaaa\mcp\files.json"
                )],
            ),
            Err(PiSetError::McpUnsupported)
        );
        assert_eq!(
            plan_resources(&manifest(&[(SetKind::Skill, "review")]), root, &[]),
            Err(PiSetError::MissingResource)
        );
        assert_eq!(
            plan_resources(
                &manifest(&[]),
                root,
                &[resource(
                    SetKind::Skill,
                    "review",
                    r"C:\Data\agents\pi\instances\i_aaaaaaaaaaaa\skills\review"
                )],
            ),
            Err(PiSetError::UnexpectedResource)
        );
        // Same id, wrong kind: a skill never satisfies an extension entry.
        assert_eq!(
            plan_resources(
                &manifest(&[(SetKind::Plugin, "review")]),
                root,
                &[resource(
                    SetKind::Skill,
                    "review",
                    r"C:\Data\agents\pi\instances\i_aaaaaaaaaaaa\skills\review"
                )],
            ),
            Err(PiSetError::MissingResource)
        );
    }
}
