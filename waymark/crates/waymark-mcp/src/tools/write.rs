//! Work-item writes: creation, ordinary editing, lifecycle, relationships, and
//! archive.
//!
//! Every tool here addresses exactly one item and calls one shared operation.
//! The adapter owns argument translation and the machine representation; the
//! lock, fingerprint conflict detection, archived-item rejection,
//! stale-generation rejection, and owner-mismatch warning all come from
//! `waymark_core` and are surfaced unchanged in `_meta`.
//!
//! The request shapes mirror the CLI's families: `create`; title, body, and
//! custom fields; status; owner; category move; parent set/clear; dependency
//! add/remove; soft-link add/remove; archive. There is no combined edit request,
//! no batch list, and no editor path: `waymark_update_custom_fields` takes an
//! explicit `set`/`clear` map whose values the domain validates against the
//! declared field type.

use std::collections::BTreeMap;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use waymark_core::envelope::Meta;
use waymark_core::model::FieldKey;
use waymark_core::ops::edit::{self, EditOutcome, FieldOutcome};
use waymark_core::ops::{create, status};

use crate::envelope::{self, ToolResult};
use crate::server::WaymarkServer;
use crate::tools::params;
use crate::tools::read::item_json;

/// Inputs of `waymark_create_work_item`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateWorkItemArgs {
    /// Short single-line label.
    pub title: String,

    /// Category key; must already be declared in the project configuration.
    pub category: String,

    /// Accountable identity. Trimmed; must be non-empty and single line.
    pub owner: String,

    /// Markdown body. Omitted means an empty body.
    #[serde(default)]
    pub body: Option<String>,

    /// Validate and report what would be written, without writing.
    #[serde(default)]
    pub dry_run: bool,
}

/// Inputs of `waymark_set_title`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetTitleArgs {
    /// Stable ID of the item.
    pub id: String,

    /// The new title. Surrounding whitespace is trimmed; internal whitespace is
    /// preserved. Must be non-empty and single line.
    pub title: String,

    /// Report the change without writing it.
    #[serde(default)]
    pub dry_run: bool,
}

/// Inputs of `waymark_replace_body`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReplaceBodyArgs {
    /// Stable ID of the item.
    pub id: String,

    /// The complete new body. Surrounding whitespace is preserved and exactly
    /// one trailing newline is canonicalized; an empty string clears it.
    pub body: String,

    /// Report the change without writing it.
    #[serde(default)]
    pub dry_run: bool,
}

/// Inputs of `waymark_update_custom_fields`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateCustomFieldsArgs {
    /// Stable ID of the item.
    pub id: String,

    /// The explicit change map: `set` values and `clear` keys. Required; there
    /// is no editor and no implicit "current value" form.
    pub custom_fields: CustomFieldChanges,

    /// Report the change without writing it.
    #[serde(default)]
    pub dry_run: bool,
}

/// One custom-field change map.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct CustomFieldChanges {
    /// Fields to set, keyed by declared field name. A JSON string, integer,
    /// boolean, or homogeneous list of those; an explicit `null` is rejected.
    #[serde(default)]
    pub set: BTreeMap<String, Value>,

    /// Declared field keys to remove. Clearing a required field is rejected.
    #[serde(default)]
    pub clear: Vec<String>,
}

#[tool_router(router = write_router, vis = "pub(crate)")]
impl WaymarkServer {
    /// Create one work item. The domain mints the stable ID and the timestamps;
    /// callers cannot supply them. Returns the complete created item, its path,
    /// and whether the store changed. `dry_run` validates and reports the ID and
    /// path it would allocate without writing.
    #[tool(
        name = "waymark_create_work_item",
        annotations(
            title = "Create one work item",
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn waymark_create_work_item(
        &self,
        Parameters(args): Parameters<CreateWorkItemArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let mut request = create::CreateRequest::new(args.title, args.category, args.owner);
            if let Some(body) = args.body {
                request = request.with_body(body);
            }
            let created = create::create(&project, request, &params::options(args.dry_run))?;
            Ok(envelope::success(
                json!({
                    "id": created.item.id.as_str(),
                    "path": created.path.to_string(),
                    "written": created.written,
                    "item": item_json(&created.item, &created.path),
                }),
                Meta::direct_file_scoped(),
            ))
        })
    }

    /// Replace the title of one item. A repeat of the current title is a true
    /// no-op. Returns the complete item and the `changed`/`written` flags.
    #[tool(
        name = "waymark_set_title",
        annotations(
            title = "Replace a work-item title",
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn waymark_set_title(&self, Parameters(args): Parameters<SetTitleArgs>) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let outcome = edit::set_title(
                &project,
                &args.id,
                &args.title,
                &params::options(args.dry_run),
            )?;
            Ok(edit_result(&outcome))
        })
    }

    /// Replace the body of one item. An empty body clears it; a present body is
    /// canonicalized to exactly one trailing newline. Returns the complete item
    /// and the `changed`/`written` flags.
    #[tool(
        name = "waymark_replace_body",
        annotations(
            title = "Replace a work-item body",
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn waymark_replace_body(
        &self,
        Parameters(args): Parameters<ReplaceBodyArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let outcome = edit::replace_body(
                &project,
                &args.id,
                Some(&args.body),
                &params::options(args.dry_run),
            )?;
            Ok(edit_result(&outcome))
        })
    }

    /// Apply one explicit custom-field change map to one item: `set` values and
    /// `clear` keys, validated against the declared types before the single
    /// write. A `null` set value, an unknown key, a duplicate/conflicting key,
    /// or clearing a required field is `VALIDATION_ERROR`. Returns the complete
    /// item, the changed keys, and the `changed`/`written` flags.
    #[tool(
        name = "waymark_update_custom_fields",
        annotations(
            title = "Set and clear custom fields",
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn waymark_update_custom_fields(
        &self,
        Parameters(args): Parameters<UpdateCustomFieldsArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let changes =
                params::field_changes(&args.custom_fields.set, &args.custom_fields.clear)?;
            let project = self.project()?;
            let outcome = edit::update_custom_fields(
                &project,
                &args.id,
                &changes,
                &params::options(args.dry_run),
            )?;
            Ok(fields_result(&outcome))
        })
    }

    /// Set the lifecycle status of one item to `open`, `in_progress`, or
    /// `closed`. Repeating the current status is a true no-op. Returns the
    /// previous status, the new status, `closed_at`, and the complete item.
    #[tool(
        name = "waymark_set_status",
        annotations(
            title = "Set a work-item status",
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn waymark_set_status(&self, Parameters(args): Parameters<SetStatusArgs>) -> ToolResult {
        envelope::respond(|| {
            let status = status::parse_status(&args.status)?;
            let project = self.project()?;
            let outcome =
                status::set_status(&project, &args.id, status, &params::options(args.dry_run))?;
            Ok(status_result(&outcome))
        })
    }
}

/// Inputs of `waymark_set_status`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetStatusArgs {
    /// Stable ID of the item.
    pub id: String,

    /// New status: `open`, `in_progress` (also `in progress` or
    /// `in-progress`), or `closed`.
    pub status: String,

    /// Report the change without writing it.
    #[serde(default)]
    pub dry_run: bool,
}

/// One title or body edit as a successful tool result.
fn edit_result(outcome: &EditOutcome) -> ToolResult {
    envelope::success(
        json!({
            "id": outcome.item.id.as_str(),
            "path": outcome.path.to_string(),
            "changed": outcome.changed,
            "written": outcome.written,
            "item": item_json(&outcome.item, &outcome.path),
        }),
        outcome.meta.clone(),
    )
}

/// One custom-field update as a successful tool result.
fn fields_result(outcome: &FieldOutcome) -> ToolResult {
    let changed_fields: Vec<&str> = outcome
        .changed_fields
        .iter()
        .map(FieldKey::as_str)
        .collect();
    envelope::success(
        json!({
            "id": outcome.item.id.as_str(),
            "path": outcome.path.to_string(),
            "changed": outcome.changed,
            "written": outcome.written,
            "changed_fields": changed_fields,
            "item": item_json(&outcome.item, &outcome.path),
        }),
        outcome.meta.clone(),
    )
}

/// One status transition as a successful tool result.
fn status_result(outcome: &status::StatusOutcome) -> ToolResult {
    envelope::success(
        json!({
            "id": outcome.item.id.as_str(),
            "path": outcome.path.to_string(),
            "previous_status": outcome.previous.as_str(),
            "status": outcome.item.status.as_str(),
            "changed": outcome.changed,
            "written": outcome.written,
            "closed_at": outcome.item.closed_at.map(|value| value.to_string()),
            "item": item_json(&outcome.item, &outcome.path),
        }),
        outcome.meta.clone(),
    )
}
