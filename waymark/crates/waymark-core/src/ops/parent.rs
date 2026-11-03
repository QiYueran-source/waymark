//! `waymark parent set|clear`: the single hierarchy edge.
//!
//! A parent is containment and nothing else. The spec is explicit about what it
//! is *not*: it does not block readiness, it does not imply a dependency, and it
//! does not cascade lifecycle. That is why this module never consults readiness
//! and never touches status — the only fields a parent change may move are
//! `parent` and `updated_at`, and only on the child.
//!
//! # The rules
//!
//! - an item has **at most one** parent: setting a second one replaces the
//!   first, it does not accumulate;
//! - parentage may cross categories; nothing here looks at the category;
//! - re-setting the same parent is a true no-op: no write, no `updated_at` move,
//!   so a repeat does not churn the file or the fingerprint;
//! - a parent edge that would close a hierarchy cycle is rejected with
//!   `CYCLE_DETECTED {kind: "parent", cycle}` (never written, then diagnosed);
//! - a self-parent is `INVALID_RELATIONSHIP`;
//! - the child must be active (`ITEM_ARCHIVED`), and so must the parent: an
//!   archived item is not a place in the hierarchy;
//! - a parent that does not exist is `ITEM_NOT_FOUND`.
//!
//! A cycle introduced by an external edit is *not* repaired here. The graph
//! diagnoses it (`readiness::Graph::diagnostics`) and a tree reports the
//! structure it actually finds.
//!
//! # Only the child changes
//!
//! Moving a parent edge writes one file. The former parent's document never
//! mentions its children — `children` is derived — so there is nothing to clean
//! up on the other side and nothing that could be left half-done.

use camino::Utf8PathBuf;

use crate::envelope::Meta;
use crate::error::WaymarkError;
use crate::model::WorkItem;
use crate::ops::mediated::{self, Loaded};
use crate::ops::OpOptions;
use crate::readiness::Graph;
use crate::store::{Fingerprint, Project};
use crate::WorkItemId;

/// The relation name parent errors carry, so the details of a rejected parent
/// edge read like every other relationship rejection.
pub const PARENT: &str = "parent";

/// What a parent change produced.
#[derive(Debug, Clone)]
pub struct ParentOutcome {
    /// The child as written, or as it would have been written on a dry run.
    pub item: WorkItem,
    /// The parent the child now names, `None` after a clear.
    pub parent: Option<WorkItemId>,
    /// The parent the child named before the change.
    pub previous_parent: Option<WorkItemId>,
    /// The child's document.
    pub path: Utf8PathBuf,
    /// Whether the persisted parent actually changed.
    pub changed: bool,
    /// Whether the document on disk changed.
    pub written: bool,
    /// The fingerprint of the written bytes; `None` on a dry run.
    pub fingerprint: Option<Fingerprint>,
    /// The `_meta` block this write justifies.
    pub meta: Meta,
}

/// Make `parent` the parent of `id`, replacing any existing parent.
pub fn set_parent(
    project: &Project,
    id: &str,
    parent: &str,
    options: &OpOptions,
) -> Result<ParentOutcome, WaymarkError> {
    crate::lock::with_project_lock(project, options.lock_wait, || {
        let loaded = mediated::load_for_write(project, id, "parent set")?;
        let parent = crate::ops::read::normalize(parent)?;
        apply_set(project, &loaded, &parent, options)
    })
}

/// Remove the parent of `id`.
///
/// Clearing an item that has no parent is a true no-op rather than an error: the
/// caller asked for a state, not for an edge to exist, and the state already
/// holds. The alternative — an error — would make a clear in a script fail on
/// the second run for no reason.
pub fn clear_parent(
    project: &Project,
    id: &str,
    options: &OpOptions,
) -> Result<ParentOutcome, WaymarkError> {
    crate::lock::with_project_lock(project, options.lock_wait, || {
        let loaded = mediated::load_for_write(project, id, "parent clear")?;
        apply_clear(project, &loaded, options)
    })
}

fn apply_set(
    project: &Project,
    loaded: &Loaded,
    parent: &WorkItemId,
    options: &OpOptions,
) -> Result<ParentOutcome, WaymarkError> {
    let child = loaded.id().clone();
    if parent == &child {
        return Err(WaymarkError::invalid_relationship(
            PARENT,
            &child,
            parent,
            "an item cannot be its own parent",
        ));
    }

    // One scan answers both questions the rule needs: does the parent exist and
    // is it active, and would the new edge close a cycle.
    let graph = Graph::from_project(project)?;
    if graph.get(parent).is_none() {
        return Err(describe_missing_parent(project, parent));
    }
    if let Some(cycle) = parent_cycle(&graph, &child, parent) {
        return Err(WaymarkError::cycle_detected(PARENT, cycle));
    }

    let previous_parent = loaded.item.parent.clone();
    let mut item = loaded.item.clone();
    item.parent = Some(parent.clone());
    finish(project, loaded, item, previous_parent, options)
}

fn apply_clear(
    project: &Project,
    loaded: &Loaded,
    options: &OpOptions,
) -> Result<ParentOutcome, WaymarkError> {
    let previous_parent = loaded.item.parent.clone();
    let mut item = loaded.item.clone();
    item.parent = None;
    finish(project, loaded, item, previous_parent, options)
}

/// Commit a computed child, or report that the bytes would not change.
///
/// The comparison is against the parent that was loaded, not against the
/// caller's intent, so "set the parent it already has" and "clear an item with
/// none" both land here as a no-op: no `updated_at` move, no write, no new
/// fingerprint.
fn finish(
    project: &Project,
    loaded: &Loaded,
    mut item: WorkItem,
    previous_parent: Option<WorkItemId>,
    options: &OpOptions,
) -> Result<ParentOutcome, WaymarkError> {
    if item.parent == previous_parent {
        return Ok(ParentOutcome {
            parent: item.parent.clone(),
            previous_parent,
            meta: mediated::write_meta(mediated::owner_warnings(project, &item, options)),
            path: loaded.path.clone(),
            item,
            changed: false,
            written: false,
            fingerprint: None,
        });
    }

    mediated::touch(&mut item, options);
    let fingerprint = mediated::commit(loaded, &item, options)?;
    Ok(ParentOutcome {
        parent: item.parent.clone(),
        previous_parent,
        meta: mediated::write_meta(mediated::owner_warnings(project, &item, options)),
        path: loaded.path.clone(),
        item,
        changed: true,
        written: fingerprint.is_some(),
        fingerprint,
    })
}

/// Why the parent could not be used: missing, or archived.
fn describe_missing_parent(project: &Project, parent: &WorkItemId) -> WaymarkError {
    match project.locate(parent) {
        Ok(_) => WaymarkError::item_archived(parent, "parent set"),
        Err(error) if error.code() == crate::error::Code::ItemNotFound => {
            WaymarkError::referenced_item_not_found(parent, PARENT)
        }
        Err(error) => error,
    }
}

/// The cycle the proposed parent edge would close, as a closed path starting at
/// the child.
///
/// The walk follows parent edges from the proposed parent upward. A dangling
/// parent ends the walk: it cannot complete a cycle, and inventing a finding for
/// it would report a cycle that is not there.
fn parent_cycle(graph: &Graph, child: &WorkItemId, parent: &WorkItemId) -> Option<Vec<WorkItemId>> {
    let mut cycle = vec![child.clone()];
    let mut cursor = parent.clone();
    // Bounded by the number of active items: a walk that exceeds it entered a
    // cycle created by an external edit, which this operation neither repairs
    // nor promises to name exactly.
    for _ in 0..=graph.len() {
        cycle.push(cursor.clone());
        if &cursor == child {
            return Some(cycle);
        }
        cursor = graph.get(&cursor).and_then(|item| item.parent.clone())?;
    }
    Some(cycle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fixtures;

    fn id(literal: &str) -> WorkItemId {
        WorkItemId::parse(literal).unwrap()
    }

    fn item(literal: &str, parent: Option<&str>) -> WorkItem {
        let mut item = fixtures::minimal_item(literal);
        item.parent = parent.map(id);
        item
    }

    #[test]
    fn the_reported_cycle_is_closed_and_names_the_proposed_edge() {
        let graph = Graph::from_items(&[
            item("wm-000000000002", Some("wm-000000000003")),
            item("wm-000000000003", Some("wm-000000000001")),
        ]);
        let cycle = parent_cycle(&graph, &id("wm-000000000001"), &id("wm-000000000002"))
            .expect("the edge closes a cycle");
        assert_eq!(cycle.first(), Some(&id("wm-000000000001")));
        assert_eq!(cycle.last(), Some(&id("wm-000000000001")));
        assert!(cycle.contains(&id("wm-000000000002")));
        assert!(cycle.contains(&id("wm-000000000003")));
    }

    #[test]
    fn an_upward_walk_that_ends_closes_no_cycle() {
        let graph = Graph::from_items(&[
            item("wm-000000000002", None),
            item("wm-000000000003", Some("wm-000000000004")),
        ]);
        assert!(parent_cycle(&graph, &id("wm-000000000001"), &id("wm-000000000002")).is_none());
        assert!(parent_cycle(&graph, &id("wm-000000000001"), &id("wm-000000000003")).is_none());
    }
}
