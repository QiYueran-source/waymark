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
use waymark_core::ops::{archive, create, link, owner, parent, r#move, relationship, status};

use crate::envelope::{self, ToolResult};
use crate::server::WaymarkServer;
use crate::tools::params;
use waymark_core::view::{item_fields, item_json};

/// Inputs of `waymark_create_work_item`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateWorkItemArgs {
    /// Short single-line label.
    pub title: String,

    /// Category key; must already be declared in the project configuration.
    pub category: String,

    /// Accountable identity. Trimmed; must be non-empty and single line.
    pub owner: String,

    /// Provenance identity. Trimmed and validated like `owner`; defaults to the
    /// configured operator when omitted.
    #[serde(default)]
    pub creator: Option<String>,

    /// Markdown body. Omitted means an empty body.
    #[serde(default)]
    pub body: Option<String>,

    /// Declared custom fields to set, keyed by field name. A JSON string,
    /// integer, boolean, or homogeneous list of those; `null` is rejected.
    #[serde(default)]
    pub custom_fields: BTreeMap<String, Value>,

    /// Stable ID of the parent the item starts under.
    #[serde(default)]
    pub parent: Option<String>,

    /// Stable IDs the item waits for. All must exist and be active.
    #[serde(default)]
    pub dependencies: Vec<String>,

    /// Stable IDs to soft-link to. Stored symmetrically on both sides.
    #[serde(default)]
    pub soft_links: Vec<String>,

    /// Validate and report what would be written, without writing.
    #[serde(default)]
    pub dry_run: bool,

    /// Wait at most this many milliseconds for the project write lock.
    /// Omitted uses the shared default wait; `0` fails immediately with the
    /// retryable `LOCK_TIMEOUT` code.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
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

    /// Wait at most this many milliseconds for the project write lock.
    /// Omitted uses the shared default wait; `0` fails immediately with the
    /// retryable `LOCK_TIMEOUT` code.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
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

    /// Wait at most this many milliseconds for the project write lock.
    /// Omitted uses the shared default wait; `0` fails immediately with the
    /// retryable `LOCK_TIMEOUT` code.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
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

    /// Wait at most this many milliseconds for the project write lock.
    /// Omitted uses the shared default wait; `0` fails immediately with the
    /// retryable `LOCK_TIMEOUT` code.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
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
    /// callers cannot supply them. The request may also carry a creator, custom
    /// fields, a parent, dependencies, and soft links, all written as one
    /// atomic operation. Returns the complete created item, its path, every
    /// rewritten soft-link partner, one fingerprint per written document, and
    /// whether the store changed. `dry_run` performs every check and reports the
    /// same shape without writing.
    ///
    /// `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
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
            if let Some(creator) = args.creator {
                request = request.with_creator(creator);
            }
            if let Some(body) = args.body {
                request = request.with_body(body);
            }
            for (key, value) in create_fields(&args.custom_fields)? {
                request = request.with_field(key, value);
            }
            if let Some(parent) = args.parent {
                request = request.with_parent(parent);
            }
            for dependency in args.dependencies {
                request = request.with_dependency(dependency);
            }
            for link in args.soft_links {
                request = request.with_soft_link(link);
            }
            let created = create::create(
                &project,
                request,
                &params::options(args.dry_run, args.lock_timeout_ms),
            )?;
            Ok(envelope::success(
                json!({
                    "id": created.item.id.as_str(),
                    "path": created.path.to_string(),
                    "written": created.written,
                    "item": item_json(&created.item, &created.path),
                    "partners": created.partners.iter().map(|partner| {
                        item_json(&partner.item, &partner.path)
                    }).collect::<Vec<_>>(),
                    "fingerprints": created.fingerprints.iter().map(|fingerprint| fingerprint.as_str()).collect::<Vec<_>>(),
                }),
                Meta::direct_file_scoped(),
            ))
        })
    }

    /// Replace the title of one item. A repeat of the current title is a true
    /// no-op. Returns the complete item and the `changed`/`written` flags.
    ///
    /// `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
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
                &params::options(args.dry_run, args.lock_timeout_ms),
            )?;
            Ok(edit_result(&outcome))
        })
    }

    /// Replace the body of one item. An empty body clears it; a present body is
    /// canonicalized to exactly one trailing newline. Returns the complete item
    /// and the `changed`/`written` flags.
    ///
    /// `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
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
                &params::options(args.dry_run, args.lock_timeout_ms),
            )?;
            Ok(edit_result(&outcome))
        })
    }

    /// Apply one explicit custom-field change map to one item: `set` values and
    /// `clear` keys, validated against the declared types before the single
    /// write. A `null` set value, an unknown key, a duplicate/conflicting key,
    /// or clearing a required field is `VALIDATION_ERROR`. Returns the complete
    /// item, the changed keys, and the `changed`/`written` flags.
    ///
    /// `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
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
                &params::options(args.dry_run, args.lock_timeout_ms),
            )?;
            Ok(fields_result(&outcome))
        })
    }

    /// Set the lifecycle status of one item to `open`, `in_progress`, or
    /// `closed`. Repeating the current status is a true no-op. Returns the
    /// previous status, the new status, `closed_at`, and the complete item.
    ///
    /// `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
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
            let outcome = status::set_status(
                &project,
                &args.id,
                status,
                &params::options(args.dry_run, args.lock_timeout_ms),
            )?;
            Ok(status_result(&outcome))
        })
    }

    /// Replace the accountable owner of one item. The new identity is validated
    /// with the same rule creation uses; repeating the current owner is a true
    /// no-op. A configured operator that differs from the owner produces the
    /// advisory `OWNER_MISMATCH` warning in `_meta` without blocking the write.
    ///
    /// `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
    #[tool(
        name = "waymark_transfer_owner",
        annotations(
            title = "Transfer a work-item owner",
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn waymark_transfer_owner(
        &self,
        Parameters(args): Parameters<TransferOwnerArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let outcome = owner::transfer_owner(
                &project,
                &args.id,
                &args.owner,
                &params::options(args.dry_run, args.lock_timeout_ms),
            )?;
            Ok(owner_result(&outcome))
        })
    }

    /// Move one item to another declared category: the document is relocated to
    /// the category's directory and the persisted `category` changes. Refused
    /// with `WRITE_CONFLICT` if the destination is occupied, and a same-category
    /// request is a true no-op.
    ///
    /// `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
    #[tool(
        name = "waymark_move_work_item",
        annotations(
            title = "Move a work item to another category",
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn waymark_move_work_item(
        &self,
        Parameters(args): Parameters<MoveWorkItemArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let category = params::category_key(&args.category)?;
            let project = self.project()?;
            let outcome = r#move::move_item(
                &project,
                &args.id,
                &category,
                &params::options(args.dry_run, args.lock_timeout_ms),
            )?;
            Ok(move_result(&outcome))
        })
    }

    /// Give one item a parent, replacing any parent it already has. Re-setting
    /// the same parent is a true no-op; a cycle, a self-parent, an archived
    /// endpoint, and a missing parent are all refused by the domain.
    ///
    /// `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
    #[tool(
        name = "waymark_set_parent",
        annotations(
            title = "Set a work-item parent",
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn waymark_set_parent(&self, Parameters(args): Parameters<SetParentArgs>) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let outcome = parent::set_parent(
                &project,
                &args.id,
                &args.parent,
                &params::options(args.dry_run, args.lock_timeout_ms),
            )?;
            Ok(parent_result(&outcome, ParentVerb::Set))
        })
    }

    /// Remove one item's parent. Clearing an item that has no parent is a true
    /// no-op.
    ///
    /// `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
    #[tool(
        name = "waymark_clear_parent",
        annotations(
            title = "Clear a work-item parent",
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn waymark_clear_parent(
        &self,
        Parameters(args): Parameters<ClearParentArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let outcome = parent::clear_parent(
                &project,
                &args.id,
                &params::options(args.dry_run, args.lock_timeout_ms),
            )?;
            Ok(parent_result(&outcome, ParentVerb::Clear))
        })
    }

    /// Make one item wait for a blocking item. A self-reference, a duplicate
    /// edge, and an edge that would close a cycle are refused by the domain;
    /// the target must exist and be active.
    ///
    /// `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
    #[tool(
        name = "waymark_add_dependency",
        annotations(
            title = "Add a dependency edge",
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn waymark_add_dependency(
        &self,
        Parameters(args): Parameters<DependencyArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let outcome = relationship::add_dependency(
                &project,
                &args.id,
                &args.blocking,
                &params::options(args.dry_run, args.lock_timeout_ms),
            )?;
            Ok(dependency_result(&outcome))
        })
    }

    /// Remove the hard dependency of one item on a blocking item. Removing an
    /// edge that is not there is `INVALID_RELATIONSHIP`; a missing target is
    /// allowed, so a dangling pointer can be cleaned up.
    ///
    /// `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
    #[tool(
        name = "waymark_remove_dependency",
        annotations(
            title = "Remove a dependency edge",
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn waymark_remove_dependency(
        &self,
        Parameters(args): Parameters<DependencyArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let outcome = relationship::remove_dependency(
                &project,
                &args.id,
                &args.blocking,
                &params::options(args.dry_run, args.lock_timeout_ms),
            )?;
            Ok(dependency_result(&outcome))
        })
    }

    /// Associate two items with a symmetric, non-blocking soft link. Both
    /// documents are replaced as one all-or-nothing batch.
    ///
    /// `cascade` is the consent flag for a cross-item write: off by default,
    /// it also rewrites the other item's document, which requires you to own
    /// it. `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
    #[tool(
        name = "waymark_add_soft_link",
        annotations(
            title = "Add a soft link",
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn waymark_add_soft_link(
        &self,
        Parameters(args): Parameters<SoftLinkArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let outcome = link::add_soft_link(
                &project,
                &args.id,
                &args.other,
                &params::write_options(args.dry_run, args.cascade, args.lock_timeout_ms),
            )?;
            Ok(link_result(&outcome, LinkVerb::Add))
        })
    }

    /// Remove the soft link between two items. Removing a link that is not there
    /// is `INVALID_RELATIONSHIP`; when the other document is missing or
    /// archived, the surviving side alone is cleaned up.
    ///
    /// `cascade` is the consent flag for a cross-item write: off by default,
    /// it also rewrites the other item's document, which requires you to own
    /// it. `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
    #[tool(
        name = "waymark_remove_soft_link",
        annotations(
            title = "Remove a soft link",
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn waymark_remove_soft_link(
        &self,
        Parameters(args): Parameters<SoftLinkArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let outcome = link::remove_soft_link(
                &project,
                &args.id,
                &args.other,
                &params::write_options(args.dry_run, args.cascade, args.lock_timeout_ms),
            )?;
            Ok(link_result(&outcome, LinkVerb::Remove))
        })
    }

    /// Archive one item: the logical tombstone, not a lifecycle transition. An
    /// optional reason is stored; inbound edges from active items are removed in
    /// the same all-or-nothing batch. Re-archiving is `ALREADY_ARCHIVED`, and an
    /// active child blocks it with `HAS_ACTIVE_CHILDREN`.
    ///
    /// `cascade` is the consent flag for a cross-item write: off by default,
    /// it also cleans up inbound references from other items you own.
    /// `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
    #[tool(
        name = "waymark_archive_work_item",
        annotations(
            title = "Archive a work item",
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn waymark_archive_work_item(
        &self,
        Parameters(args): Parameters<ArchiveWorkItemArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let outcome = archive::archive(
                &project,
                &args.id,
                args.reason.as_deref(),
                &params::write_options(args.dry_run, args.cascade, args.lock_timeout_ms),
            )?;
            Ok(archive_result(&outcome))
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

    /// Wait at most this many milliseconds for the project write lock.
    /// Omitted uses the shared default wait; `0` fails immediately with the
    /// retryable `LOCK_TIMEOUT` code.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
}

/// Render one mediated write: the shared core fields plus this operation's own.
fn write_data(
    item: &waymark_core::WorkItem,
    path: &camino::Utf8Path,
    changed: bool,
    written: bool,
    extra: Value,
) -> Value {
    waymark_core::view::write_result_json_with(
        item,
        path,
        changed,
        written,
        extra.as_object().cloned().unwrap_or_default(),
    )
}

/// One title or body edit as a successful tool result.
fn edit_result(outcome: &EditOutcome) -> ToolResult {
    envelope::success(
        waymark_core::view::write_result_json(
            &outcome.item,
            &outcome.path,
            outcome.changed,
            outcome.written,
        ),
        outcome.meta.clone(),
    )
}

/// Translate a create request's custom-field map into the domain's typed list.
///
/// The adapter owns only what the core type cannot represent: a JSON `null` and
/// values that are not scalars (a float or object) are refused here with the
/// shared `VALIDATION_ERROR` shape. Every well-formed value is handed to the
/// domain, which owns the declared-type, required-field, and duplicate rules.
fn create_fields(
    fields: &BTreeMap<String, Value>,
) -> Result<Vec<(FieldKey, waymark_core::FieldValue)>, waymark_core::WaymarkError> {
    use waymark_core::error::ValidationIssue;
    use waymark_core::WaymarkError;

    let mut issues = Vec::new();
    let mut out = Vec::with_capacity(fields.len());
    for (key, value) in fields {
        let path = format!("custom_fields.{key}");
        let parsed = match FieldKey::parse(key) {
            Ok(parsed) => parsed,
            Err(error) => {
                issues.push(ValidationIssue::new(
                    path,
                    format!("`{key}` is not a valid custom-field key: {error}"),
                ));
                continue;
            }
        };
        if value.is_null() {
            issues.push(ValidationIssue::new(
                path,
                "null is not a custom-field value; omit the key instead",
            ));
            continue;
        }
        match params::field_value(value) {
            Ok(value) => out.push((parsed, value)),
            Err(reason) => issues.push(ValidationIssue::new(path, reason)),
        }
    }
    if issues.is_empty() {
        Ok(out)
    } else {
        Err(WaymarkError::validation("custom_fields", issues).at_field("custom_fields"))
    }
}

/// One custom-field update as a successful tool result.
fn fields_result(outcome: &FieldOutcome) -> ToolResult {
    let changed_fields: Vec<&str> = outcome
        .changed_fields
        .iter()
        .map(FieldKey::as_str)
        .collect();
    envelope::success(
        write_data(
            &outcome.item,
            &outcome.path,
            outcome.changed,
            outcome.written,
            json!({"changed_fields": changed_fields}),
        ),
        outcome.meta.clone(),
    )
}

/// One status transition as a successful tool result.
fn status_result(outcome: &status::StatusOutcome) -> ToolResult {
    envelope::success(
        write_data(
            &outcome.item,
            &outcome.path,
            outcome.changed,
            outcome.written,
            json!({
                "previous_status": outcome.previous.as_str(),
                "status": outcome.item.status.as_str(),
                "closed_at": outcome.item.closed_at.map(|value| value.to_string()),
            }),
        ),
        outcome.meta.clone(),
    )
}

/// Inputs of `waymark_transfer_owner`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TransferOwnerArgs {
    /// Stable ID of the item.
    pub id: String,

    /// New accountable identity. Trimmed; must be non-empty and single line.
    pub owner: String,

    /// Report the change without writing it.
    #[serde(default)]
    pub dry_run: bool,

    /// Wait at most this many milliseconds for the project write lock.
    /// Omitted uses the shared default wait; `0` fails immediately with the
    /// retryable `LOCK_TIMEOUT` code.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
}

/// Inputs of `waymark_move_work_item`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MoveWorkItemArgs {
    /// Stable ID of the item.
    pub id: String,

    /// Category key to move the item to; must already be declared.
    pub category: String,

    /// Report the change without writing it.
    #[serde(default)]
    pub dry_run: bool,

    /// Wait at most this many milliseconds for the project write lock.
    /// Omitted uses the shared default wait; `0` fails immediately with the
    /// retryable `LOCK_TIMEOUT` code.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
}

/// Inputs of `waymark_set_parent`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetParentArgs {
    /// Stable ID of the child.
    pub id: String,

    /// Stable ID of the new parent.
    pub parent: String,

    /// Report the change without writing it.
    #[serde(default)]
    pub dry_run: bool,

    /// Wait at most this many milliseconds for the project write lock.
    /// Omitted uses the shared default wait; `0` fails immediately with the
    /// retryable `LOCK_TIMEOUT` code.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
}

/// Inputs of `waymark_clear_parent`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ClearParentArgs {
    /// Stable ID of the item whose parent is removed.
    pub id: String,

    /// Report the change without writing it.
    #[serde(default)]
    pub dry_run: bool,

    /// Wait at most this many milliseconds for the project write lock.
    /// Omitted uses the shared default wait; `0` fails immediately with the
    /// retryable `LOCK_TIMEOUT` code.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
}

/// Inputs of `waymark_add_dependency` and `waymark_remove_dependency`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DependencyArgs {
    /// Stable ID of the item that waits.
    pub id: String,

    /// Stable ID of the item it waits for.
    pub blocking: String,

    /// Report the change without writing it.
    #[serde(default)]
    pub dry_run: bool,

    /// Wait at most this many milliseconds for the project write lock.
    /// Omitted uses the shared default wait; `0` fails immediately with the
    /// retryable `LOCK_TIMEOUT` code.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
}

/// Inputs of `waymark_add_soft_link` and `waymark_remove_soft_link`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SoftLinkArgs {
    /// Stable ID of one end.
    pub id: String,

    /// Stable ID of the other end.
    pub other: String,

    /// Consent to a cross-item write: also rewrite the other item's document,
    /// which requires you to own it. Defaults to off.
    #[serde(default)]
    pub cascade: bool,

    /// Report the change without writing it.
    #[serde(default)]
    pub dry_run: bool,

    /// Wait at most this many milliseconds for the project write lock.
    /// Omitted uses the shared default wait; `0` fails immediately with the
    /// retryable `LOCK_TIMEOUT` code.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
}

/// Inputs of `waymark_archive_work_item`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ArchiveWorkItemArgs {
    /// Stable ID of the item to archive.
    pub id: String,

    /// Free-text reason stored as `archive_reason`. A whitespace-only value
    /// stores no reason.
    #[serde(default)]
    pub reason: Option<String>,

    /// Consent to a cross-item write: also clean up inbound references from
    /// other items you own. Defaults to off, which rewrites nothing but the
    /// archived item. Another owner's document is never rewritten; it is
    /// reported in `untouched` either way.
    #[serde(default)]
    pub cascade: bool,

    /// Report the archive and every planned relationship removal without
    /// writing.
    #[serde(default)]
    pub dry_run: bool,

    /// Wait at most this many milliseconds for the project write lock.
    /// Omitted uses the shared default wait; `0` fails immediately with the
    /// retryable `LOCK_TIMEOUT` code.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
}

/// Which parent verb ran, so the payload cannot disagree with the operation.
#[derive(Debug, Clone, Copy)]
enum ParentVerb {
    Set,
    Clear,
}

/// Which link verb ran.
#[derive(Debug, Clone, Copy)]
enum LinkVerb {
    Add,
    Remove,
}

/// One owner transfer as a successful tool result.
fn owner_result(outcome: &owner::OwnerOutcome) -> ToolResult {
    envelope::success(
        write_data(
            &outcome.item,
            &outcome.path,
            outcome.changed,
            outcome.written,
            json!({
                "previous_owner": outcome.previous_owner,
                "owner": outcome.item.owner,
            }),
        ),
        outcome.meta.clone(),
    )
}

/// One category move as a successful tool result.
fn move_result(outcome: &r#move::MoveOutcome) -> ToolResult {
    envelope::success(
        write_data(
            &outcome.item,
            &outcome.path,
            outcome.changed,
            outcome.written,
            json!({
                "previous_path": outcome.previous_path.to_string(),
                "previous_category": outcome.previous_category.as_str(),
                "category": outcome.item.category.as_str(),
            }),
        ),
        outcome.meta.clone(),
    )
}

/// One parent change as a successful tool result.
fn parent_result(outcome: &parent::ParentOutcome, verb: ParentVerb) -> ToolResult {
    envelope::success(
        write_data(
            &outcome.item,
            &outcome.path,
            outcome.changed,
            outcome.written,
            json!({
                "verb": match verb {
                    ParentVerb::Set => "set",
                    ParentVerb::Clear => "clear",
                },
                "parent": outcome.parent.as_ref().map(|id| id.as_str()),
                "previous_parent": outcome.previous_parent.as_ref().map(|id| id.as_str()),
            }),
        ),
        outcome.meta.clone(),
    )
}

/// One dependency change as a successful tool result.
fn dependency_result(outcome: &relationship::RelationshipOutcome) -> ToolResult {
    envelope::success(
        write_data(
            &outcome.item,
            &outcome.path,
            outcome.changed,
            outcome.written,
            json!({
                "dependency": outcome.dependency.as_str(),
                "dependencies": outcome.item.dependencies.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
            }),
        ),
        outcome.meta.clone(),
    )
}

/// One soft-link change as a successful tool result.
fn link_result(outcome: &link::LinkOutcome, verb: LinkVerb) -> ToolResult {
    envelope::success(
        write_data(
            &outcome.item,
            &outcome.path,
            outcome.changed,
            outcome.written,
            json!({
                "other": outcome.other_id.as_str(),
                "other_path": outcome.other_path.as_ref().map(|path| path.to_string()),
                "verb": match verb {
                    LinkVerb::Add => "add",
                    LinkVerb::Remove => "remove",
                },
                "soft_links": outcome.item.soft_links.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
                "fingerprints": outcome.fingerprints.iter().map(|fingerprint| fingerprint.as_str()).collect::<Vec<_>>(),
                "other_item": outcome.other.as_ref().zip(outcome.other_path.as_ref()).map(|(item, path)| item_json(item, path)),
            }),
        ),
        outcome.meta.clone(),
    )
}

/// One archive as a successful tool result.
fn archive_result(outcome: &archive::ArchiveOutcome) -> ToolResult {
    envelope::success(
        json!({
            "id": outcome.item.id.as_str(),
            "path": outcome.path.to_string(),
            "archived_at": outcome.archived_at.to_string(),
            "archive_reason": outcome.archive_reason,
            "status": outcome.item.status.as_str(),
            "closed_at": outcome.item.closed_at.map(|value| value.to_string()),
            "written": outcome.written,
            "removals": outcome.removals.iter().map(|removal| json!({
                "from": removal.from.as_str(),
                "relation": removal.relation.as_str(),
            })).collect::<Vec<_>>(),
            "affected": outcome
                .affected
                .iter()
                .map(|item| json!({
                    "id": item.id.as_str(),
                    "path": item.path.to_string(),
                }))
                .collect::<Vec<_>>(),
            "untouched": waymark_core::view::archive_leftovers_json(
                &outcome.untouched,
                &outcome.untouched_relations,
            ),
            "item": item_fields(&outcome.item),
        }),
        outcome.meta.clone(),
    )
}
