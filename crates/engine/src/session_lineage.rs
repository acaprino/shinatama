//! Observed native links inside ONE complete, explicitly selected archive catalog.
//! Never call this with a UI page or a merged set of roots. No paths, transcript,
//! environment, filesystem, parser, process or inferred parent relation is used.
use mukade::reader::SessionInfo;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_LINEAGE_CATALOG_ROWS: usize = 4096;
pub const MAX_ANCESTORS: usize = 32;
pub const MAX_CHILDREN: usize = 50;
pub const MAX_LINEAGE_DEPTH: usize = 64;
const MAX_NATIVE_ID_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineageState {
    UnknownParent,
    UnknownLinks,
    UnresolvedInSelectedArchive,
    Cycle,
    Ambiguous,
    Truncated,
    Resolved,
}

/// A complete catalog does not imply that its reader supplies parent metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParentMetadata {
    Unavailable,
    Complete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineageError {
    InvalidIdentifier,
}
impl std::fmt::Display for LineageError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("Native lineage contains an invalid identifier.")
    }
}
impl std::error::Error for LineageError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineageNode {
    pub native_id: String,
    pub state: LineageState,
    /// Explicit read-only selection only: never authorization to start/resume/hop.
    /// False for ambiguous, missing and cyclic links, even when a label is known.
    pub can_select: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLineage {
    pub native_id: String,
    pub state: LineageState,
    pub selected_state: LineageState,
    pub ancestor_state: LineageState,
    pub children_state: LineageState,
    pub ancestors: Vec<LineageNode>,
    /// Direct children only, in stable native-ID order, not guessed chronology.
    pub children: Vec<LineageNode>,
    pub total_direct_children: Option<usize>,
}
impl SessionLineage {
    fn unavailable(native_id: &str, state: LineageState) -> Self {
        Self {
            native_id: native_id.to_owned(),
            state,
            selected_state: state,
            ancestor_state: state,
            children_state: state,
            ancestors: Vec::new(),
            children: Vec::new(),
            total_direct_children: None,
        }
    }
}
#[derive(Clone, Copy)]
struct Group {
    first: usize,
    ambiguous: bool,
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_NATIVE_ID_BYTES && !id.chars().any(char::is_control)
}
fn node(id: &str, state: LineageState) -> LineageNode {
    LineageNode {
        native_id: id.to_owned(),
        state,
        can_select: state == LineageState::Resolved
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
    }
}

/// Caller supplies the entire catalog returned for the same root/revision.
/// Uniqueness cannot be established from a page: oversized input produces no
/// navigable links, rather than silently ignoring potential duplicate IDs.
pub fn derive_lineage(
    catalog: &[SessionInfo],
    exact_native_id: &str,
    parent_metadata: ParentMetadata,
) -> Result<SessionLineage, LineageError> {
    if !valid_id(exact_native_id) {
        return Err(LineageError::InvalidIdentifier);
    }
    if catalog.len() > MAX_LINEAGE_CATALOG_ROWS {
        return Ok(SessionLineage::unavailable(
            exact_native_id,
            LineageState::Truncated,
        ));
    }
    let mut groups = BTreeMap::<&str, Group>::new();
    for (index, row) in catalog.iter().enumerate() {
        if !valid_id(&row.id)
            || row
                .parent
                .as_deref()
                .is_some_and(|parent| !valid_id(parent))
        {
            return Err(LineageError::InvalidIdentifier);
        }
        groups
            .entry(&row.id)
            .and_modify(|group| group.ambiguous = true)
            .or_insert(Group {
                first: index,
                ambiguous: false,
            });
    }
    let Some(selected) = groups.get(exact_native_id) else {
        return Ok(SessionLineage::unavailable(
            exact_native_id,
            LineageState::UnresolvedInSelectedArchive,
        ));
    };
    if selected.ambiguous {
        return Ok(SessionLineage::unavailable(
            exact_native_id,
            LineageState::Ambiguous,
        ));
    }

    let mut ancestors = Vec::with_capacity(MAX_ANCESTORS);
    let mut seen = BTreeSet::from([exact_native_id]);
    let mut parent = catalog[selected.first].parent.as_deref();
    let mut ancestor_state = LineageState::UnknownParent;
    for _ in 0..MAX_LINEAGE_DEPTH {
        let Some(id) = parent else {
            break;
        };
        if ancestors.len() == MAX_ANCESTORS {
            ancestor_state = LineageState::Truncated;
            break;
        }
        if seen.contains(id) {
            ancestors.push(node(id, LineageState::Cycle));
            ancestor_state = LineageState::Cycle;
            break;
        }
        let Some(group) = groups.get(id) else {
            ancestors.push(node(id, LineageState::UnresolvedInSelectedArchive));
            ancestor_state = LineageState::UnresolvedInSelectedArchive;
            break;
        };
        if group.ambiguous {
            ancestors.push(node(id, LineageState::Ambiguous));
            ancestor_state = LineageState::Ambiguous;
            break;
        }
        seen.insert(id);
        ancestors.push(node(id, LineageState::Resolved));
        parent = catalog[group.first].parent.as_deref();
        // None means the reader did not record a parent. It NEVER proves that the
        // session is a root, including after one or more resolved ancestor links.
        ancestor_state = if parent.is_some() {
            LineageState::Truncated
        } else {
            LineageState::UnknownParent
        };
    }

    // Inspect every bounded input row so duplicate child IDs never select a first
    // parent record. A child with ANY duplicate record is labelled ambiguous.
    let child_ids: BTreeSet<&str> = catalog
        .iter()
        .filter(|row| row.parent.as_deref() == Some(exact_native_id))
        .map(|row| row.id.as_str())
        .collect();
    let total_direct_children = child_ids.len();
    let mut children = Vec::with_capacity(total_direct_children.min(MAX_CHILDREN));
    let mut children_state = if parent_metadata == ParentMetadata::Complete {
        LineageState::Resolved
    } else {
        LineageState::UnknownLinks
    };
    for id in child_ids.iter().take(MAX_CHILDREN) {
        let group = groups
            .get(*id)
            .expect("child was indexed from the same bounded catalog");
        let state = if group.ambiguous {
            LineageState::Ambiguous
        } else if seen.contains(*id) {
            LineageState::Cycle
        } else {
            LineageState::Resolved
        };
        if state == LineageState::Ambiguous {
            children_state = LineageState::Ambiguous;
        } else if state == LineageState::Cycle && children_state != LineageState::Ambiguous {
            children_state = LineageState::Cycle;
        }
        children.push(node(id, state));
    }
    if total_direct_children > MAX_CHILDREN {
        children_state = LineageState::Truncated;
    }
    let state = [
        LineageState::Ambiguous,
        LineageState::Cycle,
        LineageState::Truncated,
        LineageState::UnresolvedInSelectedArchive,
        LineageState::UnknownLinks,
        LineageState::UnknownParent,
    ]
    .into_iter()
    .find(|state| *state == ancestor_state || *state == children_state)
    .unwrap_or(LineageState::Resolved);
    Ok(SessionLineage {
        native_id: exact_native_id.to_owned(),
        state,
        selected_state: LineageState::Resolved,
        ancestor_state,
        children_state,
        ancestors,
        children,
        total_direct_children: (parent_metadata == ParentMetadata::Complete)
            .then_some(total_direct_children),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn derive_lineage(catalog: &[SessionInfo], id: &str) -> Result<SessionLineage, LineageError> {
        super::derive_lineage(catalog, id, ParentMetadata::Complete)
    }
    fn row(id: &str, parent: Option<&str>) -> SessionInfo {
        SessionInfo {
            id: id.to_owned(),
            parent: parent.map(str::to_owned),
            ..SessionInfo::default()
        }
    }
    #[test]
    fn observed_cycles_missing_and_duplicate_links_never_invent_or_select_a_parent() {
        let cycle = derive_lineage(&[row("a", Some("b")), row("b", Some("a"))], "a").unwrap();
        assert_eq!(cycle.state, LineageState::Cycle);
        assert_eq!(
            cycle
                .ancestors
                .iter()
                .map(|node| node.native_id.as_str())
                .collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert!(cycle.ancestors[0].can_select);
        assert!(!cycle.ancestors[1].can_select);
        assert_eq!(cycle.children[0].state, LineageState::Cycle);
        assert!(!cycle.children[0].can_select);
        let missing = derive_lineage(&[row("a", Some("absent"))], "a").unwrap();
        assert_eq!(
            missing.ancestor_state,
            LineageState::UnresolvedInSelectedArchive
        );
        assert!(!missing.ancestors[0].can_select);
        let duplicate = derive_lineage(
            &[
                row("a", Some("b")),
                row("b", None),
                row("b", Some("elsewhere")),
            ],
            "a",
        )
        .unwrap();
        assert_eq!(duplicate.ancestor_state, LineageState::Ambiguous);
        assert!(!duplicate.ancestors[0].can_select);
        let ambiguous_selected =
            derive_lineage(&[row("a", None), row("a", Some("b")), row("b", None)], "a").unwrap();
        assert_eq!(ambiguous_selected.selected_state, LineageState::Ambiguous);
        assert!(ambiguous_selected.ancestors.is_empty() && ambiguous_selected.children.is_empty());
        let unknown = derive_lineage(&[row("a", None)], "a").unwrap();
        assert_eq!(unknown.ancestor_state, LineageState::UnknownParent);
        assert_eq!(unknown.selected_state, LineageState::Resolved);
        let unobserved = super::derive_lineage(
            &[row("a", None), row("b", None)],
            "a",
            ParentMetadata::Unavailable,
        )
        .unwrap();
        assert_eq!(unobserved.children_state, LineageState::UnknownLinks);
        assert_eq!(unobserved.total_direct_children, None);
        let path_label =
            derive_lineage(&[row("a", Some("../label")), row("../label", None)], "a").unwrap();
        assert_eq!(path_label.ancestors[0].state, LineageState::Resolved);
        assert!(!path_label.ancestors[0].can_select);
    }
    #[test]
    fn output_depth_and_catalog_limits_remain_finite_and_never_certify_partial_uniqueness() {
        let chain: Vec<_> = (0..80)
            .map(|index| {
                row(
                    &format!("id-{index}"),
                    (index < 79).then(|| format!("id-{}", index + 1)).as_deref(),
                )
            })
            .collect();
        let limited = derive_lineage(&chain, "id-0").unwrap();
        assert_eq!(limited.ancestors.len(), MAX_ANCESTORS);
        assert_eq!(limited.ancestor_state, LineageState::Truncated);
        let mut children = vec![row("a", None)];
        children.extend((0..100).map(|index| row(&format!("child-{index:03}"), Some("a"))));
        let limited = derive_lineage(&children, "a").unwrap();
        assert_eq!(limited.children.len(), MAX_CHILDREN);
        assert_eq!(limited.total_direct_children, Some(100));
        assert_eq!(limited.children_state, LineageState::Truncated);
        let oversized = vec![row("a", None); MAX_LINEAGE_CATALOG_ROWS + 1];
        let no_guess = derive_lineage(&oversized, "a").unwrap();
        assert_eq!(no_guess.selected_state, LineageState::Truncated);
        assert!(no_guess.ancestors.is_empty() && no_guess.children.is_empty());
        assert_eq!(
            derive_lineage(&[row("a", Some("bad\nparent"))], "a"),
            Err(LineageError::InvalidIdentifier)
        );
    }
}
