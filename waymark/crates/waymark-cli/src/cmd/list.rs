//! `waymark list`, `waymark ready`, and `waymark blocked`.
//!
//! Three commands over one population. They share one argument struct, so a flag
//! added for `list` is a flag on all three, and they share one projection, so
//! `ready` cannot accidentally omit `blocked_by`.
//!
//! `ready` and `blocked` are derived views: neither writes, and neither invents a
//! status. That is why `--status` on them still means the persisted lifecycle
//! state, even though their own selection is about readiness.
//!
//! `--archived` asks for the archived-only range instead of the active one. It
//! is never a mix: `list --archived` lists retired work, and `ready --archived`
//! / `blocked --archived` are empty because readiness is a fact about active
//! work.

use clap::Args;
use serde_json::Value;
use waymark_core::envelope::success_envelope;
use waymark_core::error::{ValidationIssue, WaymarkError};
use waymark_core::ops::list::{self, Listing};
use waymark_core::ops::status::parse_status;
use waymark_core::query::{
    projected_row, ArchiveRange, FieldKey, FieldOp, FieldPredicate, Filters, Projection, Query,
    Sort, SortField, Window,
};
use waymark_core::{CategoryKey, FieldValue, Project, WorkItemId};

use super::{emit, err_line};

/// Which question a list command asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// Every item in the archive range.
    All,
    /// Active, not closed, nothing outstanding.
    Ready,
    /// Active, not closed, at least one outstanding dependency.
    Blocked,
}

/// Arguments shared by `list`, `ready`, `blocked`, and (without the population
/// flags) by `query` and `search`.
#[derive(Debug, Args)]
pub struct ListArgs {
    /// Only items in this persisted status; repeat for several.
    #[arg(long, value_name = "STATUS", action = clap::ArgAction::Append)]
    pub status: Vec<String>,

    /// Only items in this category; repeat for several.
    #[arg(long, value_name = "KEY", action = clap::ArgAction::Append)]
    pub category: Vec<String>,

    /// Only items with this owner.
    #[arg(long, value_name = "NAME")]
    pub owner: Option<String>,

    /// Only items with this creator.
    #[arg(long, value_name = "NAME")]
    pub creator: Option<String>,

    /// Only children of this item.
    #[arg(long, value_name = "ID")]
    pub parent: Option<String>,

    /// Keep only items with a parent (roots are dropped).
    #[arg(long, conflicts_with = "roots")]
    pub has_parent: bool,

    /// Keep only root items.
    #[arg(long)]
    pub roots: bool,

    /// `any` keeps items that declare a dependency, `none` keeps roots of the
    /// dependency graph.
    #[arg(long, value_name = "any|none")]
    pub dependencies: Option<String>,

    /// Keep only items with no children.
    #[arg(long)]
    pub leaves: bool,

    /// Keep only ready (`ready`) or only blocked (`blocked`) items.
    #[arg(long, value_name = "ready|blocked")]
    pub readiness: Option<String>,

    /// A custom-field predicate written as one argument: `"KEY OP [VALUE]"`,
    /// where OP is one of exists, missing, eq, ne, lt, le, gt, ge, prefix,
    /// contains, list_eq, list_ne, list_contains. Repeatable; predicates
    /// combine by conjunction. `query --filter` is the structured alternative.
    #[arg(long, value_name = "\"KEY OP [VALUE]\"", action = clap::ArgAction::Append)]
    pub r#where: Vec<String>,

    /// A case-insensitive substring of the ID or the title (never the body).
    #[arg(long, value_name = "TEXT")]
    pub text: Option<String>,

    /// Which archive slice to read.
    #[arg(long, value_name = "active|archived|all", default_value = "active")]
    pub archive: String,

    /// Primary ordering: rank, created_at, updated_at, title, status, or
    /// `field:<key>`.
    #[arg(long, value_name = "FIELD", default_value = "rank")]
    pub sort: String,

    /// Reverse the primary ordering; the stable ID stays the final tie-break.
    #[arg(long)]
    pub desc: bool,

    /// Maximum rows; `0` means unlimited.
    #[arg(long, value_name = "N", default_value_t = list::DEFAULT_LIMIT)]
    pub limit: usize,

    /// Rows to skip.
    #[arg(long, value_name = "N", default_value_t = 0)]
    pub offset: usize,

    /// Comma-separated field names to project.
    #[arg(long, value_name = "LIST", conflicts_with = "compact")]
    pub fields: Option<String>,

    /// Identity columns only.
    #[arg(long)]
    pub compact: bool,

    /// Read the archived-only range; the short form of `--archive archived`.
    #[arg(long)]
    pub archived: bool,
}

impl ListArgs {
    /// Translate the flags into filters.
    pub fn filters(&self) -> Result<Filters, WaymarkError> {
        let mut filters = Filters::new();
        for status in &self.status {
            filters = filters.with_status(parse_status(status)?);
        }
        for category in &self.category {
            filters = filters.with_category(CategoryKey::parse(category).map_err(|error| {
                WaymarkError::validation(
                    "list",
                    vec![ValidationIssue::new("category", error.to_string())],
                )
                .at_field("category")
            })?);
        }
        if let Some(owner) = &self.owner {
            filters = filters.with_owner(owner.clone());
        }
        if let Some(creator) = &self.creator {
            filters = filters.with_creator(creator.clone());
        }
        if let Some(parent) = &self.parent {
            filters.parent = Some(WorkItemId::parse(parent).map_err(|error| {
                WaymarkError::validation(
                    "list",
                    vec![ValidationIssue::new("parent", error.to_string())],
                )
                .at_field("parent")
            })?);
        }
        if self.has_parent {
            filters.parent_present = Some(true);
        }
        if self.roots {
            filters.parent_present = Some(false);
        }
        if let Some(dependencies) = &self.dependencies {
            filters.has_dependencies = Some(match dependencies.as_str() {
                "any" | "has" => true,
                "none" => false,
                other => {
                    return Err(WaymarkError::validation(
                        "list",
                        vec![ValidationIssue::new(
                            "dependencies",
                            format!("{other:?} is not `any` or `none`"),
                        )],
                    )
                    .at_field("dependencies"))
                }
            });
        }
        filters.leaves_only = self.leaves;
        if let Some(readiness) = &self.readiness {
            filters = filters.with_readiness(match readiness.as_str() {
                "ready" => waymark_core::query::ReadinessFilter::Ready,
                "blocked" => waymark_core::query::ReadinessFilter::Blocked,
                other => {
                    return Err(WaymarkError::validation(
                        "list",
                        vec![ValidationIssue::new(
                            "readiness",
                            format!("{other:?} is not `ready` or `blocked`"),
                        )],
                    )
                    .at_field("readiness"))
                }
            });
        }
        filters.text = self.text.clone();
        filters.archive = self.archive_range()?;
        for group in &self.r#where {
            filters.fields.push(parse_predicate(group)?);
        }
        Ok(filters)
    }

    /// The archive slice the flags ask for.
    ///
    /// `--archived` is the short form of `--archive archived`; it wins when both
    /// are given, so `--archived --archive all` cannot silently widen the
    /// request to active work.
    pub fn archive_range(&self) -> Result<ArchiveRange, WaymarkError> {
        if self.archived {
            return Ok(ArchiveRange::Archived);
        }
        ArchiveRange::parse(&self.archive).ok_or_else(|| {
            WaymarkError::validation(
                "list",
                vec![ValidationIssue::new(
                    "archive",
                    format!("{:?} is not `active`, `archived`, or `all`", self.archive),
                )],
            )
            .at_field("archive")
        })
    }

    /// Translate the flags into a complete query: filters, ordering, window.
    pub fn query(&self) -> Result<Query, WaymarkError> {
        Ok(Query {
            filters: self.filters()?,
            sort: Sort {
                field: SortField::parse(&self.sort)?,
                descending: self.desc,
            },
            window: Window {
                limit: self.limit,
                offset: self.offset,
            },
        })
    }

    /// The projection the flags ask for.
    pub fn projection(&self) -> Result<Projection, WaymarkError> {
        if let Some(fields) = &self.fields {
            return Projection::parse_list(fields);
        }
        Ok(if self.compact {
            Projection::compact()
        } else {
            Projection::all()
        })
    }
}

/// Parse one `--where "KEY OP [VALUE]"` argument.
///
/// The value is everything after the second separator, so a quoted value may
/// contain spaces; a value that parses as JSON becomes the matching scalar or
/// list. `query --filter` is the unambiguous form for anything richer.
fn parse_predicate(group: &str) -> Result<FieldPredicate, WaymarkError> {
    let mut parts = group.splitn(3, char::is_whitespace);
    let key = parts.next().unwrap_or_default().trim();
    let op = parts.next().map(str::trim);
    let raw = parts.next().map(str::trim);
    let (key, op) = match (key, op) {
        ("", _) | (_, None) => {
            return Err(WaymarkError::validation(
                "list",
                vec![ValidationIssue::new(
                    "where",
                    "--where needs a key and an operation, as in \"effort ge 3\"",
                )],
            )
            .at_field("where"))
        }
        (key, Some(op)) => (key, op),
    };
    let key = FieldKey::parse(key).map_err(|error| {
        WaymarkError::validation(
            "list",
            vec![ValidationIssue::new("where", error.to_string())],
        )
        .at_field("where")
    })?;
    let op = FieldOp::parse(op).ok_or_else(|| {
        WaymarkError::validation(
            "list",
            vec![ValidationIssue::new(
                "where",
                format!(
                    "{op:?} is not a field operation; expected one of exists, missing, eq, ne, lt, le, gt, ge, prefix, contains, list_eq, list_ne, list_contains"
                ),
            )],
        )
        .at_field("where")
    })?;
    if !op.takes_value() {
        return Ok(FieldPredicate::present(key));
    }
    let Some(raw) = raw else {
        return Err(WaymarkError::validation(
            "list",
            vec![ValidationIssue::new(
                "where",
                format!("{} needs a value", op.as_str()),
            )],
        )
        .at_field("where"));
    };
    Ok(FieldPredicate::new(key, op, parse_operand(raw)?))
}

/// Interpret a command-line operand as a JSON scalar or list when it clearly is
/// one, and as a string otherwise.
fn parse_operand(raw: &str) -> Result<FieldValue, WaymarkError> {
    if raw == "true" {
        return Ok(FieldValue::Boolean(true));
    }
    if raw == "false" {
        return Ok(FieldValue::Boolean(false));
    }
    if let Ok(number) = raw.parse::<i64>() {
        return Ok(FieldValue::Integer(number));
    }
    if raw.starts_with('[') {
        let parsed: serde_json::Value = serde_json::from_str(raw).map_err(|error| {
            WaymarkError::validation(
                "list",
                vec![ValidationIssue::new("where", error.to_string())],
            )
            .at_field("where")
        })?;
        if let serde_json::Value::Array(items) = parsed {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(match item {
                    serde_json::Value::String(text) => FieldValue::String(text),
                    serde_json::Value::Bool(flag) => FieldValue::Boolean(flag),
                    serde_json::Value::Number(number) => {
                        number.as_i64().map(FieldValue::Integer).ok_or_else(|| {
                            WaymarkError::validation(
                                "list",
                                vec![ValidationIssue::new(
                                    "where",
                                    "a list element must be a string, integer, or boolean",
                                )],
                            )
                            .at_field("where")
                        })?
                    }
                    serde_json::Value::Null => {
                        return Err(WaymarkError::validation(
                            "list",
                            vec![ValidationIssue::new(
                                "where",
                                "null is never a filter value; use `missing`",
                            )],
                        )
                        .at_field("where"))
                    }
                    other => {
                        return Err(WaymarkError::validation(
                            "list",
                            vec![ValidationIssue::new(
                                "where",
                                format!("{other} is not a scalar list element"),
                            )],
                        )
                        .at_field("where"))
                    }
                });
            }
            return Ok(FieldValue::List(values));
        }
    }
    Ok(FieldValue::String(raw.to_owned()))
}

/// Run one of the three list commands.
pub fn run(
    project: &Project,
    args: &ListArgs,
    which: Which,
    json: bool,
) -> Result<(), WaymarkError> {
    let listing = run_query(project, args, which)?;
    render(&listing, args, json)
}

/// Execute the shared query for one population.
pub fn run_query(
    project: &Project,
    args: &ListArgs,
    which: Which,
) -> Result<Listing, WaymarkError> {
    run_with_query(project, &args.query()?, which)
}

/// Execute an already-built query — the path `query` takes, where the filter
/// document supplies the filters and the flags supply everything else.
pub fn run_with_query(
    project: &Project,
    query: &Query,
    which: Which,
) -> Result<Listing, WaymarkError> {
    match which {
        Which::All => list::list(project, query),
        Which::Ready => list::ready(project, query),
        Which::Blocked => list::blocked(project, query),
    }
}

/// Render a listing in the mode the caller asked for.
pub fn render(listing: &Listing, args: &ListArgs, json: bool) -> Result<(), WaymarkError> {
    let projection = args.projection()?;
    if json {
        let rows: Vec<Value> = listing
            .items
            .iter()
            .map(|row| projected_row(row, &projection))
            .collect();
        emit(&success_envelope(
            serde_json::json!({
                "items": rows,
                "count": listing.len(),
                "total": listing.total,
                "offset": listing.offset,
                "limit": listing.limit,
                "archive": args.archive_range().ok().map(|range| range.as_str()),
                "fields": projection.fields.as_ref().map(|fields| {
                    fields.iter().map(|field| field.as_str()).collect::<Vec<_>>()
                }),
                "compact": projection.compact,
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
    err_line(&format!(
        "  {} of {} match (offset {}, limit {})",
        listing.len(),
        listing.total,
        listing.offset,
        if listing.limit == 0 {
            "unlimited".to_owned()
        } else {
            listing.limit.to_string()
        }
    ));
    for finding in &listing.meta.diagnostics {
        err_line(&format!("  {}: {}", finding.code, finding.message));
    }
    Ok(())
}
