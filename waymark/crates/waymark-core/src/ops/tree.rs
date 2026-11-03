//! Bounded tree reads: the hierarchy and the dependency graph as trees.
//!
//! Both reads answer the same kind of question — "what hangs off this item?" —
//! and both are deliberately *bounded*: a caller passes a depth, and the walk
//! never goes deeper. `None` means unlimited, `Some(0)` means the root alone, and
//! `Some(n)` means `n` levels below the root.
//!
//! # Unresolved references are nodes, not omissions
//!
//! A parent or dependency that no longer exists is still a fact recorded in the
//! document that names it, so the trees keep it: an unresolved target appears as
//! a node with no item behind it ([`TreeNode::item`] is `None`). An active item
//! the hierarchy cannot attach to any root — a missing parent, an archived
//! parent, or a parent loop created outside Waymark — is reported in
//! [`HierarchyTree::unresolved`] rather than hidden or promoted to a root, which
//! is what the Web view promises. Nothing here repairs anything: the diagnostics
//! in `_meta` say what is dangling, and `doctor` (T14) decides what to offer.
//!
//! # Shape choices (provisional)
//!
//! These are recorded in `docs/implementation-decisions.md`:
//!
//! - Siblings are ordered by stable ID, matching the list family's final
//!   tie-break, so one snapshot always renders the same way.
//! - The dependency graph is a DAG, not a tree, so the read renders a depth-first
//!   spanning tree: the first time an item is reached it is expanded, and every
//!   later reach is a [`TreeNode::repeated`] leaf. That anchors the output to the
//!   item count as well as to the depth, and the same marker closes a dependency
//!   cycle created by an external edit.
//! - The flat form is a pre-order row list carrying `depth` and `from`, which is
//!   everything a renderer needs to rebuild the indentation.

use crate::envelope::Meta;
use crate::error::WaymarkError;
use crate::model::WorkItem;
use crate::ops::list::complete_meta;
use crate::readiness::Graph;
use crate::store::Project;
use crate::WorkItemId;

/// One node of a tree read.
#[derive(Debug, Clone)]
pub struct TreeNode {
    /// The stable ID this node stands for.
    pub id: WorkItemId,
    /// The document, or `None` when nothing active in the project has that ID.
    pub item: Option<WorkItem>,
    /// How many edges below the read's starting point this node sits at; the
    /// starting point is `0`.
    pub depth: usize,
    /// Whether this ID was already reached on the way here, so the node is drawn
    /// as a leaf even though it has further edges. Only the dependency tree and
    /// an externally created parent loop can set this.
    pub repeated: bool,
    /// The child nodes, in stable-ID order.
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    /// Whether the ID in this node has no document behind it.
    pub fn is_unresolved(&self) -> bool {
        self.item.is_none()
    }

    /// This node and its descendants, in pre-order.
    pub fn walk(&self) -> Vec<&TreeNode> {
        let mut out = Vec::new();
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            out.push(node);
            stack.extend(node.children.iter().rev());
        }
        out
    }
}

/// Why an active item could not be attached to the hierarchy tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnresolvedReason {
    /// The named parent has no document at all.
    ParentMissing,
    /// The named parent exists but is archived, so it is not part of the active
    /// tree.
    ParentArchived,
    /// The parent chain loops, so it never reaches a root.
    ParentCycle,
}

impl UnresolvedReason {
    /// The stable wire name, for adapters and the flat form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ParentMissing => "parent_missing",
            Self::ParentArchived => "parent_archived",
            Self::ParentCycle => "parent_cycle",
        }
    }
}

/// An active item the hierarchy tree could not attach to a root.
#[derive(Debug, Clone)]
pub struct UnresolvedItem {
    /// The item that cannot be placed.
    pub id: WorkItemId,
    /// Its document.
    pub item: WorkItem,
    /// The parent it names.
    pub parent: WorkItemId,
    /// Why that parent could not host it.
    pub reason: UnresolvedReason,
}

/// The hierarchy tree: parents to children.
#[derive(Debug, Clone)]
pub struct HierarchyTree {
    /// The roots actually walked, in stable-ID order.
    pub roots: Vec<TreeNode>,
    /// Active items whose parent could not host them, in stable-ID order.
    pub unresolved: Vec<UnresolvedItem>,
    /// The `_meta` block: complete, with the graph's diagnostics.
    pub meta: Meta,
}

/// One row of the flat form of a dependency tree.
#[derive(Debug, Clone)]
pub struct DependencyRow {
    /// The stable ID this row stands for.
    pub id: WorkItemId,
    /// The document, or `None` when nothing active has that ID.
    pub item: Option<WorkItem>,
    /// How many edges below the start this row sits at; the start is `0`.
    pub depth: usize,
    /// The row this one was reached from; `None` for the first row.
    pub from: Option<WorkItemId>,
    /// Whether this ID was already shown above this row.
    pub repeated: bool,
}

/// The dependency tree of one item.
#[derive(Debug, Clone)]
pub struct DependencyTree {
    /// The nested form; always built, because it costs nothing extra.
    pub root: TreeNode,
    /// The flat form in pre-order; empty unless the caller asked for it.
    pub flat: Vec<DependencyRow>,
    /// The `_meta` block: complete, with the graph's diagnostics.
    pub meta: Meta,
}

/// The hierarchy below `root`, or the whole hierarchy when it is `None`.
///
/// Archived items appear nowhere: they are not roots, not children, and not
/// hosts. An item whose parent is archived is reported as unresolved rather than
/// promoted, because the parent it records did exist.
pub fn hierarchy_tree(
    project: &Project,
    root: Option<&str>,
    depth: Option<usize>,
) -> Result<HierarchyTree, WaymarkError> {
    let graph = Graph::from_project(project)?;
    let meta = complete_meta(graph.diagnostics());

    if let Some(root) = root {
        let root = crate::ops::read::normalize(root)?;
        if graph.get(&root).is_none() {
            return Err(describe_unusable_root(project, &root));
        }
        return Ok(HierarchyTree {
            roots: vec![build_hierarchy(&graph, &root, depth)],
            // A scoped read reports the subtree it was asked for; an item
            // elsewhere in the project is not this read's business.
            unresolved: Vec::new(),
            meta,
        });
    }

    let mut roots: Vec<WorkItemId> = Vec::new();
    for item in graph.items() {
        if item.parent.is_none() {
            roots.push(item.id.clone());
        }
    }
    roots.sort();

    // Reachability, not just "has a parent": a chain that leads into a loop has
    // no root either, and those items must be reported rather than dropped.
    let reachable = reachable_from(&graph, &roots);

    let mut unresolved: Vec<UnresolvedItem> = Vec::new();
    for item in graph.items() {
        if reachable.contains(&item.id) {
            continue;
        }
        let Some(parent) = item.parent.clone() else {
            // A root is a root: it was walked above.
            continue;
        };
        let reason = if graph.get(&parent).is_some() {
            UnresolvedReason::ParentCycle
        } else if graph.is_archived(&parent) {
            UnresolvedReason::ParentArchived
        } else {
            UnresolvedReason::ParentMissing
        };
        unresolved.push(UnresolvedItem {
            id: item.id.clone(),
            item: item.clone(),
            parent,
            reason,
        });
    }
    unresolved.sort_by(|left, right| left.id.cmp(&right.id));

    Ok(HierarchyTree {
        roots: roots
            .iter()
            .map(|id| build_hierarchy(&graph, id, depth))
            .collect(),
        unresolved,
        meta,
    })
}

/// The dependency tree of `id`: everything it waits for, bounded by `depth`.
///
/// `flat` asks for the row form as well. The walk descends `dependencies`; an
/// item reached twice is a [`TreeNode::repeated`] leaf, so a shared prerequisite
/// appears under each branch that needs it but is expanded once.
pub fn dependency_tree(
    project: &Project,
    id: &str,
    depth: Option<usize>,
    flat: bool,
) -> Result<DependencyTree, WaymarkError> {
    let graph = Graph::from_project(project)?;
    let start = crate::ops::read::normalize(id)?;
    if graph.get(&start).is_none() {
        return Err(describe_unusable_root(project, &start));
    }
    let meta = complete_meta(graph.diagnostics());

    let records = walk(
        &start,
        depth,
        |cursor| graph.get(cursor).cloned(),
        |cursor| {
            let mut targets = graph
                .get(cursor)
                .map(|item| item.dependencies.clone())
                .unwrap_or_default();
            targets.sort();
            targets.dedup();
            targets
        },
    );
    let flat = if flat { rows(&records) } else { Vec::new() };
    let mut nested = nest(records);
    let root = nested
        .pop()
        .expect("the walk always starts at the requested item");

    Ok(DependencyTree { root, flat, meta })
}

/// The hierarchy below `start`, as a nested tree.
fn build_hierarchy(graph: &Graph, start: &WorkItemId, depth: Option<usize>) -> TreeNode {
    let records = walk(
        start,
        depth,
        |cursor| graph.get(cursor).cloned(),
        |cursor| graph.children(cursor),
    );
    let mut nested = nest(records);
    nested
        .pop()
        .expect("the walk always starts at the requested item")
}

/// The IDs reachable from `roots` through child edges, ignoring depth.
fn reachable_from(graph: &Graph, roots: &[WorkItemId]) -> std::collections::BTreeSet<WorkItemId> {
    let mut reachable: std::collections::BTreeSet<WorkItemId> = roots.iter().cloned().collect();
    let mut stack: Vec<WorkItemId> = roots.to_vec();
    while let Some(id) = stack.pop() {
        for child in graph.children(&id) {
            if reachable.insert(child.clone()) {
                stack.push(child);
            }
        }
    }
    reachable
}

/// One visited node before the records are nested.
#[derive(Debug)]
struct Record {
    id: WorkItemId,
    item: Option<WorkItem>,
    /// Index of the record this one hangs off, if any.
    parent: Option<usize>,
    depth: usize,
    repeated: bool,
}

/// Depth-first pre-order walk from `start`, bounded by `depth`.
///
/// Nothing recurses: the frontier lives in an explicit stack, and the visited
/// set makes a DAG render as a spanning tree while doubling as the cycle guard.
fn walk(
    start: &WorkItemId,
    depth: Option<usize>,
    mut item_of: impl FnMut(&WorkItemId) -> Option<WorkItem>,
    mut children_of: impl FnMut(&WorkItemId) -> Vec<WorkItemId>,
) -> Vec<Record> {
    let mut records: Vec<Record> = Vec::new();
    let mut visited: std::collections::BTreeSet<WorkItemId> = std::collections::BTreeSet::new();
    // (parent record index, id, depth). Popping gives pre-order because children
    // are pushed in reverse.
    let mut stack: Vec<(Option<usize>, WorkItemId, usize)> = vec![(None, start.clone(), 0)];

    while let Some((parent, id, node_depth)) = stack.pop() {
        // Reaching an ID twice makes the second node a leaf: the dependency graph
        // shares prerequisites and an external edit can close a loop, and either
        // way the interesting edges were already drawn above.
        let repeated = !visited.insert(id.clone());
        let item = item_of(&id);
        let index = records.len();
        records.push(Record {
            id: id.clone(),
            item,
            parent,
            depth: node_depth,
            repeated,
        });

        // An unresolved target has nowhere to go; a node already drawn is not
        // expanded again; and a node at the depth limit is a leaf by request.
        if repeated || depth.is_some_and(|limit| node_depth >= limit) {
            continue;
        }
        let children = children_of(&id);
        for child in children.into_iter().rev() {
            stack.push((Some(index), child, node_depth + 1));
        }
    }
    records
}

/// Turn pre-order records into nested nodes.
///
/// Children are contiguous children of their parent, so a reverse pass can place
/// every node in its parent's list without recursion.
fn nest(records: Vec<Record>) -> Vec<TreeNode> {
    let mut pending: Vec<Vec<TreeNode>> = vec![Vec::new(); records.len()];
    let mut tops: Vec<TreeNode> = Vec::new();
    for (index, record) in records.into_iter().enumerate().rev() {
        let mut children = std::mem::take(&mut pending[index]);
        children.reverse();
        let node = TreeNode {
            id: record.id,
            item: record.item,
            depth: record.depth,
            repeated: record.repeated,
            children,
        };
        match record.parent {
            Some(parent) => pending[parent].push(node),
            None => tops.push(node),
        }
    }
    tops.reverse();
    tops
}

fn rows(records: &[Record]) -> Vec<DependencyRow> {
    records
        .iter()
        .map(|record| DependencyRow {
            id: record.id.clone(),
            item: record.item.clone(),
            depth: record.depth,
            from: record
                .parent
                .and_then(|index| records.get(index))
                .map(|parent| parent.id.clone()),
            repeated: record.repeated,
        })
        .collect()
}

/// Why the requested starting item cannot be read: missing, or archived.
fn describe_unusable_root(project: &Project, id: &WorkItemId) -> WaymarkError {
    match project.locate(id) {
        Ok(_) => WaymarkError::item_archived(id, "tree"),
        Err(error) if error.code() == crate::error::Code::ItemNotFound => {
            WaymarkError::item_not_found(id)
        }
        Err(error) => error,
    }
}
