//! `waymark query`, `waymark search`, and `waymark stats`.
//!
//! `query` is the structured form of `list`: the CLI is the one interface whose
//! flags cannot express every predicate, so it accepts a JSON filter document.
//! `search` is the only read that opens the body. `stats` is counts only.
//!
//! `search` and `stats` reuse the argument structs in [`super::list`], so the
//! filter, sort, window, and projection vocabulary cannot drift between them.
//! `query` takes the shared window/projection flags but not the filter flags:
//! its filter document is the only filter input, so a flag can never silently
//! contradict the document or make the rendered metadata disagree with the
//! range that was actually read.

use clap::Args;
use serde_json::Value;
use waymark_core::envelope::success_envelope;
use waymark_core::error::{ValidationIssue, WaymarkError};
use waymark_core::ops::list::Query;
use waymark_core::query::{projected_row, Filters};
use waymark_core::search::{self, SearchRequest};
use waymark_core::stats::{self, statistics_json, StatsRequest};
use waymark_core::Project;

use super::colour::Palette;
use super::list::{self as list_cmd, ListArgs, ViewArgs};
use super::{emit, err_line};

/// `waymark query`: a list read driven by a structured filter document.
#[derive(Debug, Args)]
pub struct QueryArgs {
    /// A JSON filter object, `@path` to read one from a file, or `-` for stdin.
    #[arg(long, value_name = "JSON|@FILE|-")]
    pub filter: String,

    /// Ordering, window, and projection. The document owns every filter,
    /// including the archive range, so there are no filter flags here.
    #[command(flatten)]
    pub view: ViewArgs,
}

/// `waymark search <text>`.
#[derive(Debug, Args)]
pub struct SearchArgs {
    /// The case-insensitive substring to look for in titles, frontmatter, and
    /// bodies.
    #[arg(value_name = "TEXT")]
    pub needle: String,

    #[command(flatten)]
    pub options: ListArgs,
}

/// `waymark stats`.
#[derive(Debug, Args)]
pub struct StatsArgs {
    /// Report the top-N distribution over one declared string field: `N` alone
    /// uses `--top-field`.
    #[arg(long, value_name = "N")]
    pub top: Option<usize>,

    /// The declared string field to distribute over when `--top` is given.
    #[arg(long, value_name = "KEY", requires = "top")]
    pub top_field: Option<String>,

    /// Which archive slice to count.
    #[arg(long, value_name = "active|archived|all", default_value = "active")]
    pub archive: String,
}

/// Read a JSON filter document from a literal, a file, or stdin.
fn read_filter(source: &str) -> Result<Value, WaymarkError> {
    let text = if source == "-" {
        let mut buffer = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buffer)
            .map_err(|error| WaymarkError::io("stdin", "read", &error))?;
        buffer
    } else if let Some(path) = source.strip_prefix('@') {
        std::fs::read_to_string(path).map_err(|error| WaymarkError::io(path, "read", &error))?
    } else {
        source.to_owned()
    };
    serde_json::from_str(&text).map_err(|error| {
        WaymarkError::validation(
            "query",
            vec![ValidationIssue::new(
                "filter",
                format!("the filter is not valid JSON: {error}"),
            )],
        )
        .at_field("filter")
    })
}

/// Run `query`.
pub fn run_query(
    project: &Project,
    args: &QueryArgs,
    json: bool,
    colour: Palette,
) -> Result<(), WaymarkError> {
    // The structured document is the only filter input; the flags own sort,
    // window, and projection, and an empty document means "no filters at all".
    let filters = Filters::from_json(&read_filter(&args.filter)?)?;
    let (sort, window) = args.view.sort_and_window()?;
    let query = Query {
        filters,
        sort,
        window,
    };
    let listing = list_cmd::run_with_query(project, &query, list_cmd::Which::All)?;
    // Report the archive range that was actually read: it came from the
    // document, not from a flag this command no longer has.
    list_cmd::render(
        &listing,
        &args.view.projection()?,
        query.filters.archive,
        json,
        colour,
    )
}

/// Build the shared query for `search`, which validates its own arguments.
fn search_request(args: &SearchArgs) -> Result<(Query, SearchRequest), WaymarkError> {
    if args.needle.trim().is_empty() {
        return Err(WaymarkError::validation(
            "search",
            vec![ValidationIssue::new(
                "text",
                "search needs a non-empty substring",
            )],
        )
        .at_field("text"));
    }
    let query = args.options.query()?;
    let request = SearchRequest::new(args.needle.clone()).with_query(query.clone());
    Ok((query, request))
}

/// Run `search`.
pub fn run_search(project: &Project, args: &SearchArgs, json: bool) -> Result<(), WaymarkError> {
    let (_query, request) = search_request(args)?;
    let projection = args.options.projection()?;
    let listing = search::search(project, &request)?;

    if json {
        let rows: Vec<Value> = listing
            .hits
            .iter()
            .map(|hit| {
                let mut row = projected_row(&hit.row, &projection);
                if let Some(object) = row.as_object_mut() {
                    object.insert("title_hit".to_owned(), Value::Bool(hit.title_hit));
                }
                row
            })
            .collect();
        emit(&success_envelope(
            serde_json::json!({
                "items": rows,
                "count": listing.len(),
                "total": listing.total,
                "offset": listing.offset,
                "limit": listing.limit,
                "text": args.needle,
                "snippets": false,
            }),
            listing.meta.clone(),
        ))?;
        return Ok(());
    }

    if listing.is_empty() {
        err_line("no matches");
    }
    for hit in &listing.hits {
        err_line(&format!(
            "{}  {:<12} {}  {}",
            hit.row.item.id,
            hit.row.item.status.as_str(),
            if hit.title_hit { "title" } else { "body " },
            hit.row.item.title
        ));
    }
    err_line(&format!(
        "  {} of {} match (offset {}, limit {})",
        listing.len(),
        listing.total,
        listing.offset,
        listing.limit
    ));
    for finding in &listing.meta.diagnostics {
        err_line(&format!("  {}: {}", finding.code, finding.message));
    }
    Ok(())
}

/// Run `stats`.
pub fn run_stats(project: &Project, args: &StatsArgs, json: bool) -> Result<(), WaymarkError> {
    let archive = waymark_core::query::ArchiveRange::parse(&args.archive).ok_or_else(|| {
        WaymarkError::validation(
            "stats",
            vec![ValidationIssue::new(
                "archive",
                format!("{:?} is not `active`, `archived`, or `all`", args.archive),
            )],
        )
        .at_field("archive")
    })?;
    let mut request = StatsRequest::new().with_archive(archive);
    if let Some(top) = args.top {
        let field = match &args.top_field {
            Some(key) => key.clone(),
            None => {
                return Err(WaymarkError::validation(
                    "stats",
                    vec![ValidationIssue::new(
                        "top",
                        "--top needs --top-field naming a declared string field",
                    )],
                )
                .at_field("top"))
            }
        };
        let key = waymark_core::model::FieldKey::parse(&field).map_err(|error| {
            WaymarkError::validation(
                "stats",
                vec![ValidationIssue::new("top", error.to_string())],
            )
            .at_field("top")
        })?;
        request = request.with_top(key, top);
    }

    let statistics = stats::stats(project, &request)?;
    if json {
        emit(&success_envelope(
            statistics_json(&statistics),
            statistics.meta.clone(),
        ))?;
        return Ok(());
    }

    err_line(&format!(
        "{} items (archive: {}): {}",
        statistics.total,
        statistics.archive.as_str(),
        [
            waymark_core::Status::Open,
            waymark_core::Status::InProgress,
            waymark_core::Status::Closed,
        ]
        .iter()
        .map(|status| format!(
            "{} {}",
            statistics.by_status.get(status).copied().unwrap_or(0),
            status.as_str()
        ))
        .collect::<Vec<_>>()
        .join(", ")
    ));
    for (category, count) in &statistics.by_category {
        err_line(&format!("  {category}: {count}"));
    }
    err_line(&format!(
        "  ready {}, blocked {}",
        statistics.ready, statistics.blocked
    ));
    if let Some(distribution) = &statistics.distribution {
        for bucket in &distribution.buckets {
            err_line(&format!(
                "  {} = {}: {}",
                distribution.field,
                bucket.value.as_deref().unwrap_or("(unset)"),
                bucket.count
            ));
        }
    }
    err_line(&format!(
        "  projection {} and {}",
        statistics.completeness.as_str(),
        statistics.freshness.as_str()
    ));
    Ok(())
}
