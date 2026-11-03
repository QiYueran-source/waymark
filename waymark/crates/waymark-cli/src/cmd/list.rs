//! `waymark list`, `waymark ready`, and `waymark blocked`.
//!
//! Three commands over one population. They share [`Filters`], so a flag added
//! for `list` is a flag on all three, and they share one row projection, so
//! `ready` cannot accidentally omit `blocked_by`.
//!
//! `ready` and `blocked` are derived views: neither writes, and neither invents
//! a status. That is why `--status` on them still means the persisted lifecycle
//! state, even though their own selection is about readiness.
//!
//! `--archived` asks for the archived-only range instead of the active one. It
//! is never a mix: `list --archived` lists retired work, and `ready --archived`
//! / `blocked --archived` are empty because readiness is a fact about active
//! work.

use clap::Args;
use waymark_core::envelope::success_envelope;
use waymark_core::error::WaymarkError;
use waymark_core::ops::list::{self, Filters, ListedItem, Listing};
use waymark_core::ops::status::parse_status;
use waymark_core::{CategoryKey, Project};

use super::{emit, err_line, item_fields};

/// Which question a list command asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// Every active item.
    All,
    /// Active, not closed, nothing outstanding.
    Ready,
    /// Active, not closed, at least one outstanding dependency.
    Blocked,
}

/// Arguments shared by `list`, `ready`, and `blocked`.
#[derive(Debug, Args)]
pub struct ListArgs {
    /// Only items in this persisted status.
    #[arg(long, value_name = "STATUS")]
    pub status: Option<String>,

    /// Only items in this category.
    #[arg(long, value_name = "KEY")]
    pub category: Option<String>,

    /// Read the archived-only range instead of the active one.
    #[arg(long)]
    pub archived: bool,
}

impl ListArgs {
    fn filters(&self) -> Result<Filters, WaymarkError> {
        let mut filters = Filters::new();
        if self.archived {
            filters = filters.with_archived();
        }
        if let Some(status) = &self.status {
            filters = filters.with_status(parse_status(status)?);
        }
        if let Some(category) = &self.category {
            filters = filters.with_category(CategoryKey::parse(category).map_err(|_| {
                WaymarkError::validation(
                    "list",
                    vec![waymark_core::error::ValidationIssue::new(
                        "category",
                        format!("{category:?} is not a category key"),
                    )],
                )
                .at_field("category")
            })?);
        }
        Ok(filters)
    }
}

/// Run one of the three list commands.
pub fn run(
    project: &Project,
    args: &ListArgs,
    which: Which,
    json: bool,
) -> Result<(), WaymarkError> {
    let filters = args.filters()?;
    let listing = match which {
        Which::All => list::list(project, &filters)?,
        Which::Ready => list::ready(project, &filters)?,
        Which::Blocked => list::blocked(project, &filters)?,
    };
    render(&listing, json)
}

fn render(listing: &Listing, json: bool) -> Result<(), WaymarkError> {
    if json {
        let rows: Vec<serde_json::Value> = listing.items.iter().map(row_json).collect();
        emit(&success_envelope(
            serde_json::json!({
                "items": rows,
                "count": listing.len(),
            }),
            listing.meta.clone(),
        ))?;
        return Ok(());
    }

    if listing.is_empty() {
        err_line("no items");
    }
    for row in &listing.items {
        err_line(&format!(
            "{}  {:<12} {:<8} {}",
            row.item.id,
            row.item.status.as_str(),
            if row.item.is_archived() {
                "archived"
            } else if row.readiness.blocked {
                "blocked"
            } else if row.readiness.ready {
                "ready"
            } else {
                "-"
            },
            row.item.title
        ));
        if !row.readiness.blocked_by.is_empty() {
            let ids: Vec<&str> = row
                .readiness
                .blocked_by
                .iter()
                .map(|id| id.as_str())
                .collect();
            err_line(&format!("    blocked by {}", ids.join(", ")));
        }
    }
    for finding in &listing.meta.diagnostics {
        err_line(&format!("  {}: {}", finding.code, finding.message));
    }
    Ok(())
}

/// One row of a list result.
fn row_json(row: &ListedItem) -> serde_json::Value {
    // A list row is frontmatter plus the derived columns. The body is not read
    // for a query and is not projected: `show --raw` is where prose belongs.
    let mut item = item_fields(&row.item);
    if let Some(map) = item.as_object_mut() {
        map.remove("body");
    }
    serde_json::json!({
        "item": item,
        "id": row.item.id.as_str(),
        "status": row.item.status.as_str(),
        "ready": row.readiness.ready,
        "blocked": row.readiness.blocked,
        "blocked_by": row.readiness.blocked_by.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        "children": row.children.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        "dependents": row.dependents.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
    })
}
