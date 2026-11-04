//! Reads: `list`, `ready`, and `blocked`.
//!
//! The three share one filter vocabulary, one ordering, and one pagination model
//! ([`crate::query`]), because they answer three questions about the same
//! population:
//!
//! - `list` — every item in the archive range (active by default);
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
//! `Filters` is `crate::query::Filters`: it was widened in place rather than
//! duplicated. All three scan the whole project, so every one of them reports
//! `completeness: complete` and `source: memory_projection`. Readiness is
//! computed on the fly and never persisted.

use crate::envelope::{Finding, Meta};
use crate::error::WaymarkError;
use crate::query::{listing_from_sorted, sort_listed, ListedItem, Population, Snapshot};
use crate::store::Project;

pub use crate::query::{
    ArchiveRange, FieldName, Filters, Listing, Projection, Query, Sort, SortField, Window,
    DEFAULT_LIMIT, MCP_DEFAULT_LIMIT,
};

/// The `_meta` a whole-project read justifies.
///
/// `source` is `memory_projection` rather than `direct_file` because the answer
/// was assembled from an in-memory graph over every document; `completeness` is
/// `complete` because the scan covered the project, and the diagnostics are the
/// graph's dangling references and cycles.
pub fn complete_meta(diagnostics: Vec<Finding>) -> Meta {
    Meta::memory_projection_complete().with_diagnostics(diagnostics)
}

/// Every item in the archive range matching `query`.
pub fn list(project: &Project, query: &Query) -> Result<Listing, WaymarkError> {
    read(project, query, Population::All)
}

/// Every ready item matching `query`: active, not closed, nothing outstanding.
pub fn ready(project: &Project, query: &Query) -> Result<Listing, WaymarkError> {
    read(project, query, Population::Ready)
}

/// Every blocked item matching `query`: active, not closed, at least one
/// unfinished or missing dependency.
pub fn blocked(project: &Project, query: &Query) -> Result<Listing, WaymarkError> {
    read(project, query, Population::Blocked)
}

/// One scan, one ordering, one window: the single implementation behind the
/// three commands and behind `query`.
pub fn read(
    project: &Project,
    query: &Query,
    population: Population,
) -> Result<Listing, WaymarkError> {
    query.validate(project.config())?;
    let snapshot = Snapshot::load(project)?;
    Ok(listing_from_snapshot(&snapshot, query, population))
}

/// Answer `query` from an already loaded [`Snapshot`].
///
/// Useful to adapters that must read once and answer several questions, and to
/// `search`, which needs the body of every candidate.
pub fn listing_from_snapshot(
    snapshot: &Snapshot,
    query: &Query,
    population: Population,
) -> Listing {
    let filters = &query.filters;
    let mut rows: Vec<ListedItem> = Vec::new();

    if population.includes_archived() {
        for item in snapshot.archived() {
            let row = snapshot.row(item);
            if filters.matches(&row.item, Some(&row.readiness), row.children.len()) {
                rows.push(row);
            }
        }
    }
    if filters.archive != ArchiveRange::Archived {
        for item in snapshot.graph().items() {
            let Some(readiness) = snapshot.graph().readiness(&item.id) else {
                continue;
            };
            if !population.keeps(&readiness) {
                continue;
            }
            if !filters.matches(
                item,
                Some(&readiness),
                snapshot.graph().children(&item.id).len(),
            ) {
                continue;
            }
            rows.push(snapshot.row(item));
        }
    }

    sort_listed(&mut rows, &query.sort, snapshot.ranks());
    listing_from_sorted(rows, &query.window, snapshot.meta())
}
