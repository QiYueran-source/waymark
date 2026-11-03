//! Derived readiness: `ready`, `blocked`, `blocked_by`, children, dependents.
//!
//! Nothing in this module writes. A [`Graph`] is built in memory from parsed
//! documents and thrown away; the persisted files never learn what readiness
//! said about them.
//!
//! # The rules
//!
//! - An item is **closed** when its persisted status is `closed`. A closed item
//!   is never blocked and never blocks: closing a dependency is what makes its
//!   dependents ready.
//! - A **dependency** is a hard, directed edge: `a.dependencies = [b]` means `a`
//!   cannot start until `b` is closed.
//! - `blocked_by` lists every dependency target that is not closed, **including
//!   a target that does not exist**. A missing target is not ignored: it is a
//!   real obstacle that no transition can clear, so it keeps the dependent
//!   blocked and is reported as a dangling reference.
//! - `ready` means "not closed and nothing outstanding". `blocked` is its
//!   complement for active items, so an adapter never has to say "not ready but
//!   not blocked" by accident.
//!
//! The graph is deliberately small: a `BTreeMap` of items plus two adjacency
//! maps. Everything a caller needs — T8's queries, T9's projection, T10's Web
//! dependency overlay — reads the same computation rather than reimplementing
//! it.

use std::collections::{BTreeMap, BTreeSet};

use crate::envelope::Finding;
use crate::error::WaymarkError;
use crate::id::WorkItemId;
use crate::model::WorkItem;
use crate::store::Project;

/// The derived readiness of one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Readiness {
    /// Not closed, and every dependency target is closed.
    pub ready: bool,
    /// Not closed, and at least one dependency target is unfinished or missing.
    pub blocked: bool,
    /// The dependency targets that are unfinished or missing, sorted by ID.
    pub blocked_by: Vec<WorkItemId>,
}

impl Readiness {
    /// The readiness of a closed item: not ready for work, never blocked.
    pub fn closed() -> Self {
        Self {
            ready: false,
            blocked: false,
            blocked_by: Vec::new(),
        }
    }

    /// The readiness of an item every one of whose dependencies is closed.
    pub fn ready() -> Self {
        Self {
            ready: true,
            blocked: false,
            blocked_by: Vec::new(),
        }
    }
}

/// The in-memory relationship graph of one project.
///
/// Only **active** (non-archived) items are nodes: an archived item is not work
/// anybody can be waiting on, and its dependency edges are not traversed. A
/// reference to an archived item therefore reads exactly like a reference to a
/// missing one — an obstruction — which is the truthful answer for readiness.
#[derive(Debug, Clone, Default)]
pub struct Graph {
    items: BTreeMap<WorkItemId, WorkItem>,
    archived: BTreeSet<WorkItemId>,
    children: BTreeMap<WorkItemId, Vec<WorkItemId>>,
    dependents: BTreeMap<WorkItemId, Vec<WorkItemId>>,
}

impl Graph {
    /// Build the graph from parsed items.
    ///
    /// Archived items become known-but-inactive: they are remembered so a
    /// reference to one is not mistaken for a typo, but they are not nodes, they
    /// do not block, and their edges are not traversed.
    pub fn from_items(items: &[WorkItem]) -> Self {
        let mut graph = Self::default();
        for item in items {
            if item.is_archived() {
                graph.archived.insert(item.id.clone());
            } else {
                graph.items.insert(item.id.clone(), item.clone());
            }
        }
        graph.rebuild_adjacency();
        graph
    }

    /// Build the graph by scanning the whole project.
    ///
    /// This is a complete scan: a document that cannot be parsed fails the whole
    /// build with that document's error rather than silently shrinking the
    /// graph. The projection ticket owns the "scope local reads around a broken
    /// sibling" rule; until then, a global answer says it covered the whole
    /// project because it did.
    pub fn from_project(project: &Project) -> Result<Self, WaymarkError> {
        let inventory = project.inventory()?;
        let mut items = Vec::with_capacity(inventory.entries.len());
        for entry in &inventory.entries {
            items.push(crate::parse::parse_item_file(&entry.path)?);
        }
        Ok(Self::from_items(&items))
    }

    /// Recompute both adjacency maps from the nodes.
    ///
    /// Two passes, not insertion order: a child may be parsed before its parent,
    /// so edges are built once every node is known.
    fn rebuild_adjacency(&mut self) {
        self.children.clear();
        self.dependents.clear();
        for (id, item) in &self.items {
            if let Some(parent) = &item.parent {
                self.children
                    .entry(parent.clone())
                    .or_default()
                    .push(id.clone());
            }
            for target in &item.dependencies {
                self.dependents
                    .entry(target.clone())
                    .or_default()
                    .push(id.clone());
            }
        }
        for siblings in self.children.values_mut() {
            siblings.sort();
            siblings.dedup();
        }
        for sources in self.dependents.values_mut() {
            sources.sort();
            sources.dedup();
        }
    }

    /// The number of active items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the graph holds no active item.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The active item with this ID.
    pub fn get(&self, id: &WorkItemId) -> Option<&WorkItem> {
        self.items.get(id)
    }

    /// Every active item, in stable-ID order.
    pub fn items(&self) -> impl Iterator<Item = &WorkItem> {
        self.items.values()
    }

    /// The active items whose `parent` is `id`, sorted by stable ID.
    pub fn children(&self, id: &WorkItemId) -> Vec<WorkItemId> {
        self.children.get(id).cloned().unwrap_or_default()
    }

    /// The active items that list `id` as a dependency, sorted by stable ID.
    pub fn dependents(&self, id: &WorkItemId) -> Vec<WorkItemId> {
        self.dependents.get(id).cloned().unwrap_or_default()
    }

    /// The dependency targets of `id` that are unfinished or missing, sorted.
    ///
    /// A missing target is included on purpose: it blocks the same way an open
    /// one does, and hiding it would let `ready` claim work can start when it
    /// cannot.
    pub fn blocked_by(&self, id: &WorkItemId) -> Vec<WorkItemId> {
        let Some(item) = self.items.get(id) else {
            return Vec::new();
        };
        if item.status.is_closed() {
            return Vec::new();
        }
        let mut blocked_by: Vec<WorkItemId> = item
            .dependencies
            .iter()
            .filter(|target| match self.items.get(*target) {
                Some(target) => !target.status.is_closed(),
                None => true,
            })
            .cloned()
            .collect();
        blocked_by.sort();
        blocked_by.dedup();
        blocked_by
    }

    /// The derived readiness of `id`, or `None` when it is not an active item.
    pub fn readiness(&self, id: &WorkItemId) -> Option<Readiness> {
        let item = self.items.get(id)?;
        if item.status.is_closed() {
            return Some(Readiness::closed());
        }
        let blocked_by = self.blocked_by(id);
        Some(Readiness {
            ready: blocked_by.is_empty(),
            blocked: !blocked_by.is_empty(),
            blocked_by,
        })
    }

    /// The readiness of any item the graph knows about, archived included.
    ///
    /// An archived item is neither ready nor blocked: it is not work waiting on
    /// anything, and nothing waits on it. Reporting it as "unknown" would be
    /// wrong — the graph knows exactly what an archived item is.
    pub fn readiness_any(&self, id: &WorkItemId) -> Option<Readiness> {
        if self.archived.contains(id) {
            return Some(Readiness::closed());
        }
        self.readiness(id)
    }

    /// Whether `id` names an archived document in this project.
    pub fn is_archived(&self, id: &WorkItemId) -> bool {
        self.archived.contains(id)
    }

    /// Whether every dependency of `id` is closed and `id` itself is not.
    pub fn is_ready(&self, id: &WorkItemId) -> bool {
        self.readiness(id).is_some_and(|readiness| readiness.ready)
    }

    /// Whether `id` has at least one unfinished or missing dependency.
    pub fn is_blocked(&self, id: &WorkItemId) -> bool {
        self.readiness(id)
            .is_some_and(|readiness| readiness.blocked)
    }

    /// Every dependency edge in the graph, in a deterministic order.
    pub fn edges(&self) -> Vec<(WorkItemId, WorkItemId)> {
        let mut edges = Vec::new();
        for (id, item) in &self.items {
            for target in &item.dependencies {
                edges.push((id.clone(), target.clone()));
            }
        }
        edges
    }

    /// The diagnostics this graph justifies: dangling references and cycles.
    ///
    /// Both are observations about the project, not failures of the read that
    /// found them: a scan can report a dangling reference and still answer the
    /// query. A caller attaches these to `_meta.diagnostics`.
    pub fn diagnostics(&self) -> Vec<Finding> {
        let mut findings = self.dangling_findings();
        findings.extend(self.cycle_findings());
        findings
    }

    fn dangling_findings(&self) -> Vec<Finding> {
        let mut findings = Vec::new();
        for item in self.items.values() {
            let mut missing_dependencies: Vec<WorkItemId> = item
                .dependencies
                .iter()
                .filter(|target| !self.known(target))
                .cloned()
                .collect();
            missing_dependencies.sort();
            missing_dependencies.dedup();
            if !missing_dependencies.is_empty() {
                findings.push(Finding::from_error(&WaymarkError::dangling_reference(
                    &item.id,
                    "dependencies",
                    &missing_dependencies,
                )));
            }

            if let Some(parent) = &item.parent {
                if !self.known(parent) {
                    findings.push(Finding::from_error(&WaymarkError::dangling_reference(
                        &item.id,
                        "parent",
                        std::slice::from_ref(parent),
                    )));
                }
            }

            let missing_links: Vec<WorkItemId> = item
                .soft_links
                .iter()
                .filter(|target| !self.known(target))
                .cloned()
                .collect();
            if !missing_links.is_empty() {
                findings.push(Finding::from_error(&WaymarkError::dangling_reference(
                    &item.id,
                    "soft_links",
                    &missing_links,
                )));
            }
        }
        findings
    }

    /// Whether `id` exists at all, active or archived.
    fn known(&self, id: &WorkItemId) -> bool {
        self.items.contains_key(id) || self.archived.contains(id)
    }

    fn cycle_findings(&self) -> Vec<Finding> {
        let mut findings = Vec::new();
        for cycle in self.dependency_cycles() {
            findings.push(Finding::from_error(&WaymarkError::cycle_detected(
                "dependency",
                cycle,
            )));
        }
        findings
    }

    /// Every dependency cycle, each reported once, as a closed path.
    ///
    /// The traversal is iterative on purpose: a hand-edited project can create a
    /// very deep chain, and recursion would turn a diagnostic into a stack
    /// overflow.
    fn dependency_cycles(&self) -> Vec<Vec<WorkItemId>> {
        let mut seen: BTreeSet<Vec<WorkItemId>> = BTreeSet::new();
        let mut cycles: Vec<Vec<WorkItemId>> = Vec::new();
        let mut done: BTreeSet<WorkItemId> = BTreeSet::new();

        for start in self.items.keys() {
            if done.contains(start) {
                continue;
            }
            // Depth-first walk keeping the path, so a back edge names the loop.
            let mut stack: Vec<(WorkItemId, usize)> = vec![(start.clone(), 0)];
            let mut path: Vec<WorkItemId> = vec![start.clone()];
            let mut on_path: BTreeSet<WorkItemId> = BTreeSet::from([start.clone()]);

            while let Some((node, next_index)) = stack.pop() {
                let neighbours = match self.items.get(&node) {
                    Some(item) => &item.dependencies,
                    None => {
                        path.pop();
                        on_path.remove(&node);
                        continue;
                    }
                };
                if next_index >= neighbours.len() {
                    done.insert(node.clone());
                    path.pop();
                    on_path.remove(&node);
                    continue;
                }
                stack.push((node.clone(), next_index + 1));
                let target = neighbours[next_index].clone();
                if on_path.contains(&target) {
                    if let Some(position) = path.iter().position(|id| *id == target) {
                        let mut cycle: Vec<WorkItemId> = path[position..].to_vec();
                        cycle.push(target);
                        let key = canonical_cycle(&cycle);
                        if seen.insert(key) {
                            cycles.push(cycle);
                        }
                    }
                    continue;
                }
                if !self.items.contains_key(&target) {
                    // A dangling target has no outgoing edges to follow.
                    continue;
                }
                path.push(target.clone());
                on_path.insert(target.clone());
                stack.push((target, 0));
            }
        }
        cycles.sort();
        cycles
    }

    /// The shortest path from `from` to `to` over dependency edges, if any.
    ///
    /// Used by the relationship operation to decide whether a proposed edge
    /// would close a cycle, so the operation and the diagnostic share one
    /// traversal.
    pub fn path(&self, from: &WorkItemId, to: &WorkItemId) -> Option<Vec<WorkItemId>> {
        if from == to {
            return Some(vec![from.clone()]);
        }
        let mut parents: BTreeMap<WorkItemId, WorkItemId> = BTreeMap::new();
        let mut queue: std::collections::VecDeque<WorkItemId> = std::collections::VecDeque::new();
        queue.push_back(from.clone());
        let mut visited: BTreeSet<WorkItemId> = BTreeSet::from([from.clone()]);
        while let Some(node) = queue.pop_front() {
            let Some(item) = self.items.get(&node) else {
                continue;
            };
            for target in &item.dependencies {
                if target == to {
                    let mut path = vec![to.clone()];
                    let mut cursor = node.clone();
                    path.push(cursor.clone());
                    while let Some(parent) = parents.get(&cursor) {
                        cursor = parent.clone();
                        path.push(cursor.clone());
                    }
                    path.reverse();
                    return Some(path);
                }
                if visited.insert(target.clone()) {
                    parents.insert(target.clone(), node.clone());
                    queue.push_back(target.clone());
                }
            }
        }
        None
    }
}

/// A cycle key that is the same whatever item the walk started at.
fn canonical_cycle(cycle: &[WorkItemId]) -> Vec<WorkItemId> {
    let body = &cycle[..cycle.len().saturating_sub(1)];
    if body.is_empty() {
        return Vec::new();
    }
    let smallest = body
        .iter()
        .enumerate()
        .min_by_key(|(_, id)| *id)
        .map(|(index, _)| index)
        .unwrap_or(0);
    let mut rotated: Vec<WorkItemId> = body[smallest..].to_vec();
    rotated.extend_from_slice(&body[..smallest]);
    rotated.push(rotated[0].clone());
    rotated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Status;
    use crate::testing::fixtures;

    fn id(literal: &str) -> WorkItemId {
        WorkItemId::parse(literal).unwrap()
    }

    fn item(literal: &str, status: Status, deps: &[&str]) -> WorkItem {
        let mut item = fixtures::minimal_item(literal);
        item.status = status;
        item.dependencies = deps.iter().map(|value| id(value)).collect();
        item
    }

    #[test]
    fn a_cycle_starting_anywhere_produces_one_canonical_path() {
        let graph = Graph::from_items(&[
            item("wm-000000000001", Status::Open, &["wm-000000000002"]),
            item("wm-000000000002", Status::Open, &["wm-000000000001"]),
        ]);
        let cycles = graph.dependency_cycles();
        assert_eq!(cycles.len(), 1, "{cycles:?}");
        assert_eq!(canonical_cycle(&cycles[0]), cycles[0]);
        assert_eq!(cycles[0].first(), cycles[0].last());
    }

    #[test]
    fn a_dangling_target_is_not_traversed() {
        let graph =
            Graph::from_items(&[item("wm-000000000001", Status::Open, &["wm-0000000000zz"])]);
        assert!(graph.dependency_cycles().is_empty());
        assert_eq!(
            graph.path(&id("wm-000000000001"), &id("wm-0000000000zz")),
            Some(vec![id("wm-000000000001"), id("wm-0000000000zz")])
        );
    }
}
