//! `search`: case-insensitive text over the title, the frontmatter, and the body.
//!
//! Search is the one read that opens the body. It follows the same archive-range
//! rules and the same sort, pagination, and projection model as
//! [`crate::query`]; the only thing it adds is the match and the title-first
//! ranking.
//!
//! - Matching is a case-insensitive substring test, not a word or prefix match.
//! - The searched surface is the title, every frontmatter scalar (ID, category,
//!   status, owner, creator, parent, relationships, archive metadata, and every
//!   custom-field scalar or list element), and the body.
//! - A row that matches in the title is a *title hit*; title hits are ordered
//!   before body/frontmatter-only hits, and within each group the shared sort
//!   decides. The stable ID remains the final tie-break.
//! - No snippets are returned: a result names the item and how it matched, and
//!   the body stays out of the payload.
//!
//! A search with an empty needle matches everything, which is what an adapter
//! that already validated the argument wants; the CLI rejects a blank needle as a
//! usage error instead.

use crate::envelope::Meta;
use crate::error::WaymarkError;
use crate::model::{FieldValue, WorkItem};
use crate::ops::list::listing_from_snapshot;
use crate::query::{
    listing_from_sorted, sort_listed, FieldName, ListedItem, Listing, Population, Query, Snapshot,
};
use crate::store::Project;

/// One search request: the needle plus the shared collection options.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchRequest {
    /// The substring to look for, case-insensitively.
    pub text: String,
    /// Filters, ordering, and window shared with `list`.
    pub query: Query,
}

impl SearchRequest {
    /// A request for `text` with the default options.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            query: Query::new(),
        }
    }

    /// Replace the shared options.
    pub fn with_query(mut self, query: Query) -> Self {
        self.query = query;
        self
    }

    /// Set the page limit.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.query = self.query.with_limit(limit);
        self
    }

    /// Set the page offset.
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.query = self.query.with_offset(offset);
        self
    }
}

/// One search hit: the shared row plus how it matched.
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// The item and its derived columns.
    pub row: ListedItem,
    /// Whether the needle matched the title.
    pub title_hit: bool,
    /// Whether the needle matched the body.
    pub body_hit: bool,
}

/// Run one search over the project.
pub fn search(project: &Project, request: &SearchRequest) -> Result<SearchListing, WaymarkError> {
    request.query.validate(project.config())?;
    let snapshot = Snapshot::load(project)?;
    Ok(search_snapshot(&snapshot, request))
}

/// Run one search over an already loaded snapshot.
pub fn search_snapshot(snapshot: &Snapshot, request: &SearchRequest) -> SearchListing {
    let mut hits: Vec<SearchHit> = Vec::new();
    let base = listing_from_snapshot(snapshot, &candidate_query(request), Population::All);

    for row in base.items {
        let Some(hit) = classify(&row.item, &request.text) else {
            continue;
        };
        hits.push(SearchHit {
            row,
            title_hit: hit.title_hit,
            body_hit: hit.body_hit,
        });
    }

    let mut rows: Vec<ListedItem> = hits.iter().map(|hit| hit.row.clone()).collect();
    sort_listed(&mut rows, &request.query.sort, snapshot.ranks());
    // Title hits first, preserving the shared order inside each group: the sort
    // above is stable, so a second stable sort by the hit flag is enough.
    let mut ordered: Vec<SearchHit> = Vec::with_capacity(rows.len());
    let mut lookup: std::collections::BTreeMap<_, _> = hits
        .into_iter()
        .map(|hit| (hit.row.item.id.clone(), hit))
        .collect();
    for row in rows {
        if let Some(hit) = lookup.remove(&row.item.id) {
            ordered.push(SearchHit { row, ..hit });
        }
    }
    ordered.sort_by_key(|hit| !hit.title_hit);

    SearchListing::assemble(ordered, &request.query, snapshot.meta())
}

fn candidate_query(request: &SearchRequest) -> Query {
    let mut query = request.query.clone();
    // The text filter is `search`'s own concern; leaving it in would make the
    // needle match only IDs and titles and never the body.
    query.filters.text = None;
    query.window = crate::query::Window {
        limit: 0,
        offset: 0,
    };
    query
}

struct Matching {
    title_hit: bool,
    body_hit: bool,
}

fn classify(item: &WorkItem, needle: &str) -> Option<Matching> {
    let needle = needle.to_lowercase();
    let title_hit = item.title.to_lowercase().contains(&needle);
    let body_hit = item.body.to_lowercase().contains(&needle);
    if title_hit {
        return Some(Matching {
            title_hit: true,
            body_hit,
        });
    }
    if body_hit || frontmatter_matches(item, &needle) {
        return Some(Matching {
            title_hit: false,
            body_hit,
        });
    }
    None
}

/// Whether any frontmatter scalar (or custom value) contains `needle`.
///
/// `needle` is already lowercased by the caller.
fn frontmatter_matches(item: &WorkItem, needle: &str) -> bool {
    let id_match = item.id.as_str().contains(needle);
    if id_match {
        return true;
    }
    let scalars = [
        item.category.as_str(),
        item.status.as_str(),
        item.owner.as_str(),
    ];
    if scalars
        .iter()
        .any(|value| value.to_lowercase().contains(needle))
    {
        return true;
    }
    if let Some(creator) = &item.creator {
        if creator.to_lowercase().contains(needle) {
            return true;
        }
    }
    if let Some(reason) = &item.archive_reason {
        if reason.to_lowercase().contains(needle) {
            return true;
        }
    }
    if let Some(parent) = &item.parent {
        if parent.as_str().contains(needle) {
            return true;
        }
    }
    for id in item.dependencies.iter().chain(item.soft_links.iter()) {
        if id.as_str().contains(needle) {
            return true;
        }
    }
    item.custom_fields.iter().any(|(key, value)| {
        key.to_lowercase().contains(needle) || field_value_matches(value, needle)
    })
}

fn field_value_matches(value: &FieldValue, needle: &str) -> bool {
    match value {
        FieldValue::String(text) => text.to_lowercase().contains(needle),
        FieldValue::Integer(number) => number.to_string().contains(needle),
        FieldValue::Boolean(flag) => flag.to_string().contains(needle),
        FieldValue::List(values) => values
            .iter()
            .any(|value| field_value_matches(value, needle)),
    }
}

/// The result of a search: ordered hits plus the shared `_meta`.
#[derive(Debug, Clone)]
pub struct SearchListing {
    /// The requested page of hits, title hits first.
    pub hits: Vec<SearchHit>,
    /// The number of matches before the window was applied.
    pub total: usize,
    /// How many matches were skipped.
    pub offset: usize,
    /// The effective limit (`0` echoed as unlimited).
    pub limit: usize,
    /// The `_meta` block: complete, fresh, with the graph's diagnostics.
    pub meta: Meta,
}

impl SearchListing {
    fn assemble(hits: Vec<SearchHit>, query: &Query, meta: Meta) -> Self {
        let total = hits.len();
        let offset = query.window.offset;
        let limit = query.window.limit;
        let rows = query.window.apply(hits);
        Self {
            hits: rows,
            total,
            offset,
            limit,
            meta,
        }
    }

    /// The number of returned hits.
    pub fn len(&self) -> usize {
        self.hits.len()
    }

    /// Whether nothing matched.
    pub fn is_empty(&self) -> bool {
        self.hits.is_empty()
    }

    /// The listed rows only, for a projection that does not care how it matched.
    pub fn listing(&self) -> Listing {
        listing_from_sorted(
            self.hits.iter().map(|hit| hit.row.clone()).collect(),
            &crate::query::Window {
                limit: 0,
                offset: 0,
            },
            self.meta.clone(),
        )
    }

    /// Whether a given row matched in its title.
    pub fn is_title_hit(&self, id: &crate::WorkItemId) -> bool {
        self.hits
            .iter()
            .find(|hit| &hit.row.item.id == id)
            .is_some_and(|hit| hit.title_hit)
    }

    /// The projected fields a compact or field-limited search needs.
    pub fn projection_fields() -> &'static [FieldName] {
        FieldName::ALL
    }
}
