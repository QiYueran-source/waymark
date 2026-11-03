//! `waymark dependency add|remove`: directed hard dependency edges.
//!
//! A dependency is a hard edge with one meaning: `from` cannot start until `to`
//! is closed. It is stored as one entry in `from`'s `dependencies` list; the
//! reverse ("dependents") is always derived.
//!
//! # What is rejected
//!
//! - a self-reference (`INVALID_RELATIONSHIP`);
//! - an edge that already exists (`INVALID_RELATIONSHIP`, never a silent no-op —
//!   a caller that asked to add an edge should learn it was already there);
//! - any relation name other than `dependencies`
//!   (`INVALID_RELATIONSHIP`), so a caller can never smuggle a parent or soft
//!   link through this operation;
//! - an edge that would close a cycle (`CYCLE_DETECTED`, with the cycle path);
//! - a target that does not exist (`ITEM_NOT_FOUND`) or is archived
//!   (`ITEM_ARCHIVED`);
//! - an archived source (`ITEM_ARCHIVED`).
//!
//! # Removal is deliberately more permissive
//!
//! Removing an edge that is not there is still an `INVALID_RELATIONSHIP`: the
//! caller named a relationship that does not exist. But removing an edge whose
//! *target* is missing is allowed, because that is exactly how a dangling
//! pointer introduced by an external deletion gets cleaned up.
//!
//! Every accepted change replaces the document under the project lock with a
//! fingerprint check and moves `updated_at`.

use crate::envelope::Meta;
use crate::error::WaymarkError;
use crate::model::WorkItem;
use crate::ops::mediated::{self, Loaded};
use crate::ops::OpOptions;
use crate::readiness::Graph;
use crate::store::{Fingerprint, Project};
use crate::time::Timestamp;
use crate::WorkItemId;

/// The one relation name this operation edits.
pub const DEPENDENCIES: &str = "dependencies";

/// What a relationship change produced.
#[derive(Debug, Clone)]
pub struct RelationshipOutcome {
    /// The source item as written, or as it would have been written on a dry run.
    pub item: WorkItem,
    /// The other end of the edge, as normalized.
    pub dependency: WorkItemId,
    /// The document the source item lives in.
    pub path: camino::Utf8PathBuf,
    /// Whether the persisted list actually changed.
    pub changed: bool,
    /// Whether the document on disk changed.
    pub written: bool,
    /// The fingerprint of the written bytes; `None` on a dry run.
    pub fingerprint: Option<Fingerprint>,
    /// The `_meta` block this write justifies.
    pub meta: Meta,
}

/// Add a hard dependency: `id` now waits for `blocking`.
pub fn add_dependency(
    project: &Project,
    id: &str,
    blocking: &str,
    options: &OpOptions,
) -> Result<RelationshipOutcome, WaymarkError> {
    add_relationship(project, id, DEPENDENCIES, blocking, options)
}

/// Remove a hard dependency: `id` no longer waits for `blocking`.
pub fn remove_dependency(
    project: &Project,
    id: &str,
    blocking: &str,
    options: &OpOptions,
) -> Result<RelationshipOutcome, WaymarkError> {
    crate::lock::with_project_lock(project, options.lock_wait, || {
        let loaded = mediated::load_for_write(project, id, "dependency remove")?;
        let target = crate::ops::read::normalize(blocking)?;
        apply_remove(project, &loaded, &target, options)
    })
}

/// Add an edge of the named relation.
///
/// Only `dependencies` exists today; the name is still an explicit argument so a
/// caller asking for `parent` or `soft_links` is told the relation kind is
/// wrong rather than silently getting a dependency.
pub fn add_relationship(
    project: &Project,
    id: &str,
    relation: &str,
    other: &str,
    options: &OpOptions,
) -> Result<RelationshipOutcome, WaymarkError> {
    crate::lock::with_project_lock(project, options.lock_wait, || {
        let loaded = mediated::load_for_write(project, id, "dependency add")?;
        let target = crate::ops::read::normalize(other)?;
        apply_add(project, &loaded, relation, &target, options)
    })
}

fn apply_add(
    project: &Project,
    loaded: &Loaded,
    relation: &str,
    target: &WorkItemId,
    options: &OpOptions,
) -> Result<RelationshipOutcome, WaymarkError> {
    let from = loaded.id().clone();
    if relation != DEPENDENCIES {
        return Err(WaymarkError::invalid_relationship(
            relation,
            &from,
            target,
            format!("only `{DEPENDENCIES}` may be edited here"),
        ));
    }
    if target == &from {
        return Err(WaymarkError::invalid_relationship(
            relation,
            &from,
            target,
            "an item cannot depend on itself",
        ));
    }
    if loaded.item.dependencies.contains(target) {
        return Err(WaymarkError::invalid_relationship(
            relation,
            &from,
            target,
            "the dependency already exists",
        ));
    }

    // Every other item is needed twice: once to prove the target exists and is
    // active, once to prove the new edge closes no cycle. One scan answers both.
    let graph = Graph::from_project(project)?;
    if graph.get(target).is_none() {
        // Either no document claims the ID, or the document is archived. The
        // store knows which; the caller needs the difference.
        return Err(describe_missing_target(
            project,
            target,
            relation,
            "dependency add",
        ));
    }
    if let Some(cycle) = dependency_cycle(&graph, &from, target) {
        return Err(WaymarkError::cycle_detected("dependency", cycle));
    }

    let mut item = loaded.item.clone();
    item.dependencies.push(target.clone());
    item.dependencies.sort();
    item.dependencies.dedup();
    let now = Timestamp::new(options.clock.now());
    item.updated_at = now;
    let fingerprint = mediated::commit(loaded, &item, options)?;
    Ok(RelationshipOutcome {
        meta: mediated::write_meta(mediated::owner_warnings(project, &item, options)),
        item,
        dependency: target.clone(),
        path: loaded.path.clone(),
        changed: true,
        written: fingerprint.is_some(),
        fingerprint,
    })
}

fn apply_remove(
    project: &Project,
    loaded: &Loaded,
    target: &WorkItemId,
    options: &OpOptions,
) -> Result<RelationshipOutcome, WaymarkError> {
    let from = loaded.id().clone();
    if target == &from {
        return Err(WaymarkError::invalid_relationship(
            DEPENDENCIES,
            &from,
            target,
            "an item cannot depend on itself",
        ));
    }
    if !loaded.item.dependencies.contains(target) {
        return Err(WaymarkError::invalid_relationship(
            DEPENDENCIES,
            &from,
            target,
            "the item does not depend on that target",
        ));
    }

    let mut item = loaded.item.clone();
    item.dependencies.retain(|known| known != target);
    let now = Timestamp::new(options.clock.now());
    item.updated_at = now;
    let fingerprint = mediated::commit(loaded, &item, options)?;
    Ok(RelationshipOutcome {
        meta: mediated::write_meta(mediated::owner_warnings(project, &item, options)),
        item,
        dependency: target.clone(),
        path: loaded.path.clone(),
        changed: true,
        written: fingerprint.is_some(),
        fingerprint,
    })
}

/// Why the target could not be used: missing, or archived.
///
/// The graph only holds active items, so both cases look the same there; the
/// store tells them apart, and the caller needs the difference because one is
/// recoverable by unarchiving and the other is a typo.
fn describe_missing_target(
    project: &Project,
    target: &WorkItemId,
    relation: &str,
    operation: &str,
) -> WaymarkError {
    match project.locate(target) {
        Ok(_) => WaymarkError::item_archived(target, operation),
        Err(error) if error.code() == crate::error::Code::ItemNotFound => {
            WaymarkError::referenced_item_not_found(target, relation)
        }
        Err(error) => error,
    }
}

/// The cycle `from -> target` would close, as a closed path starting at `from`.
fn dependency_cycle(
    graph: &Graph,
    from: &WorkItemId,
    target: &WorkItemId,
) -> Option<Vec<WorkItemId>> {
    let path = graph.path(target, from)?;
    let mut cycle = vec![from.clone()];
    cycle.extend(path);
    Some(cycle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Status;
    use crate::testing::fixtures;

    fn id(literal: &str) -> WorkItemId {
        WorkItemId::parse(literal).unwrap()
    }

    fn item(literal: &str, deps: &[&str]) -> WorkItem {
        let mut item = fixtures::minimal_item(literal);
        item.status = Status::Open;
        item.dependencies = deps.iter().map(|value| id(value)).collect();
        item
    }

    #[test]
    fn the_reported_cycle_is_closed_and_names_the_proposed_edge() {
        let graph = Graph::from_items(&[
            item("wm-000000000002", &["wm-000000000003"]),
            item("wm-000000000003", &["wm-000000000001"]),
        ]);
        let cycle = dependency_cycle(&graph, &id("wm-000000000001"), &id("wm-000000000002"))
            .expect("the edge closes a cycle");
        assert_eq!(cycle.first(), Some(&id("wm-000000000001")));
        assert_eq!(cycle.last(), Some(&id("wm-000000000001")));
        assert!(cycle.contains(&id("wm-000000000002")));
        assert!(cycle.contains(&id("wm-000000000003")));
    }

    #[test]
    fn an_unreachable_target_closes_no_cycle() {
        let graph =
            Graph::from_items(&[item("wm-000000000002", &[]), item("wm-000000000003", &[])]);
        assert!(dependency_cycle(&graph, &id("wm-000000000001"), &id("wm-000000000002")).is_none());
    }
}
