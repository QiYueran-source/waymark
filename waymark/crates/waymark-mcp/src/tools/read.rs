//! Read and query tools: one item, collections, search, trees, and counts.
//!
//! Every tool here is a thin renderer over `waymark_core::ops`: the filter
//! vocabulary, ordering, pagination, projection, tree shape, readiness, and
//! statistics all come from the shared library. The adapter owns argument
//! translation and the machine representation, nothing else.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use waymark_core::error::{ValidationIssue, WaymarkError};
use waymark_core::model::FieldKey;
use waymark_core::ops::list::{self, Listing};
use waymark_core::ops::read;
use waymark_core::ops::tree::{dependency_tree, hierarchy_tree, HierarchyTree};
use waymark_core::query::{projected_row, ArchiveRange, Projection, Query};
use waymark_core::stats::{self, statistics_json, StatsRequest};
use waymark_core::view;
use waymark_core::Project;

use crate::envelope::{self, ToolResult};
use crate::server::WaymarkServer;
use crate::tools::params;

/// Inputs for reading one work item.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetWorkItemArgs {
    /// Stable ID, for example `wm-3f7k9q2m4x8c`. Compared case-insensitively.
    pub id: String,
}

/// The collection arguments shared by `list`, `ready`, and `blocked`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListArgs {
    /// Structured filter document, the same vocabulary as `waymark query
    /// --filter`. Keys: `status` (string or array), `category` (string or
    /// array), `owner`, `creator`, `parent`, `parent_present` (bool),
    /// `dependencies` (`any`/`none` or bool), `ready` or `blocked` (bool),
    /// `field` (array of `{key, op, value?}` predicates), `text`, `leaves`
    /// (bool), and `archive` (`active`/`archived`/`all`). Unknown keys,
    /// unknown operations, and wrong value types are `VALIDATION_ERROR`.
    #[serde(default)]
    pub filter: Option<Value>,

    /// Archive slice to read: `active` (default), `archived`, or `all`.
    /// Archived items are never mixed into `active`. When present this
    /// overrides `filter.archive`, because one request has one range.
    #[serde(default)]
    pub archive: Option<String>,

    /// Ordering: `rank` (default), `created_at`, `updated_at`, `title`,
    /// `status`, or `field:<key>` for a declared string or integer field.
    #[serde(default)]
    pub sort: Option<String>,

    /// Reverse the primary ordering; the stable ID stays the final tie-break.
    #[serde(default)]
    pub desc: bool,

    /// Maximum rows. Defaults to 50 for MCP; `0` means unlimited.
    #[serde(default)]
    pub limit: Option<usize>,

    /// Rows to skip. Defaults to 0.
    #[serde(default)]
    pub offset: Option<usize>,

    /// Field names to project, for example `["id", "title", "status"]`.
    /// Mutually exclusive with `compact`.
    #[serde(default)]
    pub fields: Option<Vec<String>>,

    /// Render only the identity columns and omit empty values. Mutually
    /// exclusive with `fields`.
    #[serde(default)]
    pub compact: bool,
}

/// The collection arguments of `search`, which has no filter or archive range.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchArgs {
    /// Case-insensitive substring to look for in the title, the frontmatter,
    /// and the body. Must be non-empty.
    pub text: String,

    /// Ordering: `rank` (default), `created_at`, `updated_at`, `title`,
    /// `status`, or `field:<key>`.
    #[serde(default)]
    pub sort: Option<String>,

    /// Reverse the primary ordering inside each hit group.
    #[serde(default)]
    pub desc: bool,

    /// Maximum rows. Defaults to 50 for MCP; `0` means unlimited.
    #[serde(default)]
    pub limit: Option<usize>,

    /// Rows to skip. Defaults to 0.
    #[serde(default)]
    pub offset: Option<usize>,

    /// Field names to project. Mutually exclusive with `compact`.
    #[serde(default)]
    pub fields: Option<Vec<String>>,

    /// Render only the identity columns and omit empty values.
    #[serde(default)]
    pub compact: bool,
}

/// Inputs for `waymark_get_tree`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TreeArgs {
    /// Stable ID to start at; omitted means the whole hierarchy.
    #[serde(default)]
    pub root: Option<String>,

    /// Stop this many levels below the start. `0` is the start alone;
    /// omitted means unlimited.
    #[serde(default)]
    pub depth: Option<usize>,
}

/// Inputs for `waymark_get_dependency_tree`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DependencyTreeArgs {
    /// Stable ID of the item whose dependencies are walked.
    pub id: String,

    /// Stop this many levels below the start. `0` is the start alone;
    /// omitted means unlimited.
    #[serde(default)]
    pub depth: Option<usize>,

    /// Emit the pre-order row form (`rows`) as well as the nested tree.
    #[serde(default)]
    pub flat: bool,
}

/// Inputs for `waymark_get_stats`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StatsArgs {
    /// Report the top-N distribution over one declared string field; requires
    /// `top_field`. Omitted means no distribution.
    #[serde(default)]
    pub top: Option<usize>,

    /// The declared string custom field to distribute over.
    #[serde(default)]
    pub top_field: Option<String>,

    /// Archive slice to count: `active` (default), `archived`, or `all`.
    #[serde(default)]
    pub archive: Option<String>,
}

impl ListArgs {
    fn query(&self) -> Result<Query, WaymarkError> {
        params::query_from(
            self.filter.as_ref(),
            self.archive.as_deref(),
            self.sort.as_deref(),
            self.desc,
            self.limit,
            self.offset,
        )
    }

    fn projection(&self) -> Result<Projection, WaymarkError> {
        params::projection(self.fields.as_deref(), self.compact)
    }
}

impl SearchArgs {
    fn query(&self) -> Result<Query, WaymarkError> {
        if self.text.trim().is_empty() {
            return Err(WaymarkError::validation(
                "search",
                vec![ValidationIssue::new(
                    "text",
                    "search needs a non-empty substring",
                )],
            )
            .at_field("text"));
        }
        let mut query = params::query_from(
            None,
            None,
            self.sort.as_deref(),
            self.desc,
            self.limit,
            self.offset,
        )?;
        query.filters.archive = ArchiveRange::Active;
        Ok(query)
    }

    fn projection(&self) -> Result<Projection, WaymarkError> {
        params::projection(self.fields.as_deref(), self.compact)
    }
}

#[tool_router(router = read_router, vis = "pub(crate)")]
impl WaymarkServer {
    /// Read one work item by stable ID: the full document, the document path
    /// and fingerprint, direct relationships (parent, dependencies, soft
    /// links, children, dependents), derived readiness, and archive metadata.
    /// The read is scoped, so a broken sibling yields diagnostics in `_meta`
    /// rather than failing the request.
    #[tool(
        name = "waymark_get_work_item",
        annotations(title = "Read one work item")
    )]
    async fn waymark_get_work_item(
        &self,
        Parameters(args): Parameters<GetWorkItemArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let read = read::read_scoped(&project, &args.id)?;
            Ok(envelope::success(view::read_json(&read), read.meta))
        })
    }

    /// List work items matching the shared filters, ordering, page, and
    /// projection. Returns the page rows, `count`, `total`, `offset`, `limit`,
    /// and the archive range, plus `_meta`. MCP default limit is 50.
    #[tool(
        name = "waymark_list_work_items",
        annotations(title = "List work items")
    )]
    async fn waymark_list_work_items(&self, Parameters(args): Parameters<ListArgs>) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let query = args.query()?;
            let projection = args.projection()?;
            let listing = list::list(&project, &query)?;
            Ok(envelope::success(
                listing_data(&listing, &query, &projection),
                listing.meta.clone(),
            ))
        })
    }

    /// Search titles, frontmatter, and bodies case-insensitively. Title hits
    /// are returned before body/frontmatter-only hits; within each group the
    /// shared sort decides. No snippets and no body text are returned. The
    /// same page, ordering, and projection arguments as list apply, and the
    /// default limit is 50.
    #[tool(
        name = "waymark_search_work_items",
        annotations(title = "Search work items")
    )]
    async fn waymark_search_work_items(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let query = args.query()?;
            let projection = args.projection()?;
            let request = waymark_core::search::SearchRequest::new(args.text.clone())
                .with_query(query.clone());
            let listing = waymark_core::search::search(&project, &request)?;
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
            Ok(envelope::success(
                json!({
                    "items": rows,
                    "count": listing.len(),
                    "total": listing.total,
                    "offset": listing.offset,
                    "limit": listing.limit,
                    "text": args.text,
                    "snippets": false,
                }),
                listing.meta.clone(),
            ))
        })
    }

    /// Read the hierarchy tree, optionally from one root and bounded by depth.
    /// Siblings are in stable-ID order; an item whose parent is missing,
    /// archived, or part of a loop is reported in `unresolved` rather than
    /// hidden or promoted.
    #[tool(
        name = "waymark_get_tree",
        annotations(title = "Read the hierarchy tree")
    )]
    async fn waymark_get_tree(&self, Parameters(args): Parameters<TreeArgs>) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let tree = hierarchy_tree(&project, args.root.as_deref(), args.depth)?;
            Ok(envelope::success(
                tree_data(&tree, &args),
                tree.meta.clone(),
            ))
        })
    }

    /// Walk the dependency graph of one item, bounded by depth. The nested
    /// form always exists; `flat` adds a pre-order row list carrying `depth`
    /// and `from`. A shared prerequisite is expanded once and repeated as a
    /// leaf afterwards.
    #[tool(
        name = "waymark_get_dependency_tree",
        annotations(title = "Read the dependency tree")
    )]
    async fn waymark_get_dependency_tree(
        &self,
        Parameters(args): Parameters<DependencyTreeArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let tree = dependency_tree(&project, &args.id, args.depth, args.flat)?;
            Ok(envelope::success(
                json!({
                    "id": args.id,
                    "depth": args.depth,
                    "flat": args.flat,
                    "tree": view::tree_node_json(&tree.root),
                    "rows": tree.flat.iter().map(view::dependency_row_json).collect::<Vec<_>>(),
                }),
                tree.meta.clone(),
            ))
        })
    }

    /// List ready items: active, not closed, and every dependency target
    /// closed. Accepts the shared list arguments (filters, ordering, page,
    /// projection, archive range); the MCP default limit is 50.
    #[tool(
        name = "waymark_get_ready_items",
        annotations(title = "List ready items")
    )]
    async fn waymark_get_ready_items(&self, Parameters(args): Parameters<ListArgs>) -> ToolResult {
        collection_result(self, args, list::ready)
    }

    /// List blocked items: active, not closed, and at least one dependency
    /// target unfinished or missing. Each row carries `blocked_by`. Accepts
    /// the shared list arguments; the MCP default limit is 50.
    #[tool(
        name = "waymark_get_blocked_items",
        annotations(title = "List blocked items")
    )]
    async fn waymark_get_blocked_items(
        &self,
        Parameters(args): Parameters<ListArgs>,
    ) -> ToolResult {
        collection_result(self, args, list::blocked)
    }

    /// Report counts only: total, by status, by category, ready, blocked, and
    /// an optional top-N distribution over one declared string field. The
    /// archive range defaults to `active`.
    #[tool(name = "waymark_get_stats", annotations(title = "Count work items"))]
    async fn waymark_get_stats(&self, Parameters(args): Parameters<StatsArgs>) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let mut request = StatsRequest::new().with_archive(match args.archive.as_deref() {
                Some(literal) => params::archive_range(literal)?,
                None => ArchiveRange::Active,
            });
            if let Some(top) = args.top {
                let Some(field) = args.top_field.as_deref() else {
                    return Err(WaymarkError::validation(
                        "stats",
                        vec![ValidationIssue::new(
                            "top",
                            "`top` needs `top_field` naming a declared string field",
                        )],
                    )
                    .at_field("top"));
                };
                let key = FieldKey::parse(field).map_err(|error| {
                    WaymarkError::validation(
                        "stats",
                        vec![ValidationIssue::new("top", error.to_string())],
                    )
                    .at_field("top")
                })?;
                request = request.with_top(key, top);
            }
            let statistics = stats::stats(&project, &request)?;
            Ok(envelope::success(
                statistics_json(&statistics),
                statistics.meta.clone(),
            ))
        })
    }
}

/// Run one of the three collection populations and render it.
fn collection_result(
    server: &WaymarkServer,
    args: ListArgs,
    population: fn(&Project, &Query) -> Result<Listing, WaymarkError>,
) -> ToolResult {
    envelope::respond(|| {
        let project = server.project()?;
        let query = args.query()?;
        let projection = args.projection()?;
        let listing = population(&project, &query)?;
        Ok(envelope::success(
            listing_data(&listing, &query, &projection),
            listing.meta.clone(),
        ))
    })
}

/// The machine form of a collection page.
fn listing_data(listing: &Listing, query: &Query, projection: &Projection) -> Value {
    view::listing_json(listing, projection, query.filters.archive)
}

/// The machine form of one bounded hierarchy read.
fn tree_data(tree: &HierarchyTree, args: &TreeArgs) -> Value {
    json!({
        "root": args.root,
        "depth": args.depth,
        "roots": tree.roots.iter().map(view::tree_node_json).collect::<Vec<_>>(),
        "unresolved": tree.unresolved.iter().map(|entry| json!({
            "id": entry.id.as_str(),
            "parent": entry.parent.as_str(),
            "reason": entry.reason.as_str(),
        })).collect::<Vec<_>>(),
    })
}
