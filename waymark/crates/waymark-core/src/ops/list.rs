//! Reads: `list`, `ready`, and `blocked`.
//!
//! The three share one filter vocabulary and one completeness story, because
//! they answer three questions about the same population:
//!
//! - `list` — every active item (archived items are never listed);
//! - `ready` — active, not closed, and every dependency target closed;
//! - `blocked` — active, not closed, and at least one dependency target
//!   unfinished or missing.
//!
//! # The archive range
//!
//! [`Filters::archive`] is the one deliberate way to look at retired work. The
//! default is [`ArchiveRange::Active`], so an existing call keeps its meaning and
//! archived items never leak into an active answer. [`ArchiveRange::Archived`]
//! selects the archived population *instead* — never mixed in — and reports each
//! row with [`Readiness::closed()`], because archived is neither ready nor
//! blocked. `ready` and `blocked` with that range are empty by construction:
//! their selection is about readiness, and an archived item has none.
//!
//! All three scan the whole project, so every one of them reports
//! `completeness: complete` and `source: memory_projection`. Readiness is
//! computed on the fly and never persisted.

use crate::envelope::{Finding, Meta};
use crate::error::WaymarkError;
use crate::model::{CategoryKey, Status, WorkItem};
use crate::readiness::{Graph, Readiness};
use crate::store::Project;
use crate::WorkItemId;

/// The filter vocabulary shared by `list`, `ready`, and `blocked`.
///
/// Fields are additive on purpose: T8 widens this struct rather than introducing
/// a second one, so a command written today keeps compiling and keeps meaning
/// the same thing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filters {
    /// Only items in this status.
    pub status: Option<Status>,
    /// Only items in this category.
    pub category: Option<CategoryKey>,
    /// Which archive population to read; active by default.
    pub archive: ArchiveRange,
}

/// Which side of the archive line a collection read covers.
///
/// The two ranges are exclusive: an archived item is never mixed into an active
/// answer, and an archived-only answer never includes active work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ArchiveRange {
    /// Only items without an archive marker. The default.
    #[default]
    Active,
    /// Only items with an archive marker.
    Archived,
}

impl Filters {
    /// No filters: every item the command's population allows.
    pub fn new() -> Self {
        Self::default()
    }

    /// Only `status`.
    pub fn with_status(mut self, status: Status) -> Self {
        self.status = Some(status);
        self
    }

    /// Only `category`.
    pub fn with_category(mut self, category: CategoryKey) -> Self {
        self.category = Some(category);
        self
    }

    /// Read the archived-only range.
    pub fn with_archived(mut self) -> Self {
        self.archive = ArchiveRange::Archived;
        self
    }

    /// Read the default, active-only range.
    pub fn with_active(mut self) -> Self {
        self.archive = ArchiveRange::Active;
        self
    }

    /// Whether `item` survives every filter.
    pub fn matches(&self, item: &WorkItem) -> bool {
        if let Some(status) = self.status {
            if item.status != status {
                return false;
            }
        }
        if let Some(category) = &self.category {
            if &item.category != category {
                return false;
            }
        }
        true
    }
}

/// One row of a read result: the item plus its derived readiness.
#[derive(Debug, Clone)]
pub struct ListedItem {
    /// The document.
    pub item: WorkItem,
    /// Its derived readiness. A closed item is neither ready nor blocked.
    pub readiness: Readiness,
    /// Its active children, sorted by stable ID.
    pub children: Vec<WorkItemId>,
    /// The active items that depend on it, sorted by stable ID.
    pub dependents: Vec<WorkItemId>,
}

/// The result of a list-family read.
#[derive(Debug, Clone)]
pub struct Listing {
    /// The matching items, in stable-ID order.
    pub items: Vec<ListedItem>,
    /// The `_meta` block: complete, fresh, with the graph's diagnostics.
    pub meta: Meta,
}

impl Listing {
    /// The number of returned items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether nothing matched.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The items only, for a caller that does not need the derived columns.
    pub fn into_items(self) -> Vec<WorkItem> {
        self.items.into_iter().map(|row| row.item).collect()
    }
}

/// The `_meta` a whole-project read justifies.
///
/// `source` is `memory_projection` rather than `direct_file` because the answer
/// was assembled from an in-memory graph over every document; `completeness` is
/// `complete` because the scan covered the project, and the diagnostics are the
/// graph's dangling references and cycles.
pub fn complete_meta(diagnostics: Vec<Finding>) -> Meta {
    Meta::memory_projection_complete().with_diagnostics(diagnostics)
}

/// Every active item matching `filters`.
pub fn list(project: &Project, filters: &Filters) -> Result<Listing, WaymarkError> {
    read(project, filters, |_readiness| true)
}

/// Every ready item matching `filters`: active, not closed, nothing outstanding.
pub fn ready(project: &Project, filters: &Filters) -> Result<Listing, WaymarkError> {
    read(project, filters, |readiness| readiness.ready)
}

/// Every blocked item matching `filters`: active, not closed, at least one
/// unfinished or missing dependency.
pub fn blocked(project: &Project, filters: &Filters) -> Result<Listing, WaymarkError> {
    read(project, filters, |readiness| readiness.blocked)
}

fn read(
    project: &Project,
    filters: &Filters,
    keep: impl Fn(&Readiness) -> bool,
) -> Result<Listing, WaymarkError> {
    let graph = Graph::from_project(project)?;
    let mut items = Vec::new();
    let population: Vec<&WorkItem> = match filters.archive {
        ArchiveRange::Active => graph.items().collect(),
        ArchiveRange::Archived => graph.archived_items().collect(),
    };
    for item in population {
        if !filters.matches(item) {
            continue;
        }
        // An archived item is not a graph node, so it has no readiness either:
        // `closed()` is the truthful "neither ready nor blocked" answer, and it
        // is what excludes it from `ready`/`blocked` without a second rule.
        let Some(readiness) = graph.readiness_any(&item.id) else {
            continue;
        };
        if !keep(&readiness) {
            continue;
        }
        items.push(ListedItem {
            readiness,
            children: graph.children(&item.id),
            dependents: graph.dependents(&item.id),
            item: item.clone(),
        });
    }
    Ok(Listing {
        items,
        meta: complete_meta(graph.diagnostics()),
    })
}
