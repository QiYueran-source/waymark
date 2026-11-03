//! Migration tools: version, schema, and storage.
//!
//! Each tool performs exactly one migration kind and, within schema and storage,
//! exactly one action. The action is a tagged union in the schema, not a set of
//! optional flags the handler has to disambiguate, so "two actions in one call"
//! is not expressible and a missing action is rejected by the protocol layer.
//! That mirrors the CLI's `ArgGroup(required = true, multiple = false)`.
//!
//! The domain plans, re-checks every fingerprint, and executes; the adapter only
//! translates the action and renders [`MigrationReport`]. A dry run passes the
//! same request with the dry-run flag, so it performs the complete read-only
//! preflight and reports the affected files without writing.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use waymark_core::config::FieldType;
use waymark_core::error::{ValidationIssue, WaymarkError};
use waymark_core::model::{CategoryKey, FieldKey};
use waymark_core::ops::migrate::{self, MigrationReport, SchemaAction, StorageAction};

use crate::envelope::{self, ToolResult};
use crate::server::WaymarkServer;
use crate::tools::params;

/// Inputs of `waymark_migrate_version`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MigrateVersionArgs {
    /// Target format generation. Only the generation this release writes is
    /// accepted; an older or newer one is `MIGRATION_UNSUPPORTED_GENERATION`.
    pub target_generation: u32,

    /// Perform the complete read-only check and report the affected files
    /// without writing.
    #[serde(default)]
    pub dry_run: bool,
}

/// Inputs of `waymark_migrate_schema`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MigrateSchemaArgs {
    /// The one schema action to perform.
    pub action: SchemaActionArg,

    /// Perform the complete read-only check and report the affected files
    /// without writing.
    #[serde(default)]
    pub dry_run: bool,
}

/// The single action a schema migration may take.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[schemars(inline)]
pub enum SchemaActionArg {
    /// Delete one declaration and its key from every work item, archived ones
    /// included.
    RemoveField {
        /// Declared custom-field key.
        key: String,
    },

    /// Declare a field as required and supply its default wherever an item has
    /// no value. Items with a value the declared type cannot hold abort the
    /// migration.
    AddRequiredField {
        /// Custom-field key.
        key: String,

        /// Declared value shape: `string`, `integer`, `boolean`, `list:string`,
        /// `list:integer`, or `list:boolean`.
        #[serde(rename = "type")]
        ty: String,

        /// Default value, in the declared type's literal form, written wherever
        /// an item has no value.
        #[serde(default)]
        default: Option<String>,
    },
}

/// Inputs of `waymark_migrate_storage`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MigrateStorageArgs {
    /// The one storage action to perform.
    pub action: StorageActionArg,

    /// Perform the complete read-only check and report the planned moves
    /// without writing.
    #[serde(default)]
    pub dry_run: bool,
}

/// The single action a storage migration may take.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[schemars(inline)]
pub enum StorageActionArg {
    /// Move the storage root and re-base every category directory under it.
    Root {
        /// New repository-relative storage root.
        path: String,
    },

    /// Move one category directory and point its declaration at the new path.
    CategoryPath {
        /// Category key to move.
        key: String,
        /// New repository-relative path.
        path: String,
    },

    /// Rename a category key, move its directory, and rewrite the persisted
    /// `category` value on every affected item.
    CategoryKey {
        /// Declared key to replace.
        from: String,
        /// New key.
        to: String,
    },
}

#[tool_router(router = migrate_router, vis = "pub(crate)")]
impl WaymarkServer {
    /// Rewrite every older-generation document and raise the configuration
    /// generation to the target. The target must be the generation this release
    /// writes. Returns every affected item, the per-target outcomes, and the
    /// configuration revision move. `dry_run` checks everything and writes
    /// nothing.
    #[tool(
        name = "waymark_migrate_version",
        annotations(
            title = "Migrate the format generation",
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn waymark_migrate_version(
        &self,
        Parameters(args): Parameters<MigrateVersionArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let report = migrate::migrate_version(
                &project,
                args.target_generation,
                &params::options(args.dry_run),
            )?;
            Ok(migration_result(&report))
        })
    }

    /// Remove one declared custom field, or add one required field with a
    /// default. Exactly one action per call. Returns the affected files and
    /// count, the per-target outcomes, and the configuration revision move.
    /// `dry_run` checks everything and writes nothing.
    #[tool(
        name = "waymark_migrate_schema",
        annotations(
            title = "Migrate one custom-field declaration",
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn waymark_migrate_schema(
        &self,
        Parameters(args): Parameters<MigrateSchemaArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let action = schema_action(&args.action)?;
            let project = self.project()?;
            let report = migrate::migrate_schema(&project, action, &params::options(args.dry_run))?;
            Ok(migration_result(&report))
        })
    }

    /// Move the storage root, move one category directory, or rename one
    /// category key. Exactly one action per call. Returns the planned or
    /// completed directory moves, the rewritten category values, and the
    /// affected files. `dry_run` checks everything and writes nothing.
    #[tool(
        name = "waymark_migrate_storage",
        annotations(
            title = "Migrate the storage layout",
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn waymark_migrate_storage(
        &self,
        Parameters(args): Parameters<MigrateStorageArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let action = storage_action(&args.action)?;
            let project = self.project()?;
            let report =
                migrate::migrate_storage(&project, &action, &params::options(args.dry_run))?;
            Ok(migration_result(&report))
        })
    }
}

/// Translate the one schema action into the domain request.
fn schema_action(action: &SchemaActionArg) -> Result<SchemaAction, WaymarkError> {
    match action {
        SchemaActionArg::RemoveField { key } => Ok(SchemaAction::RemoveField(field_key(key)?)),
        SchemaActionArg::AddRequiredField { key, ty, default } => {
            let ty = FieldType::parse(ty).map_err(|reason| params::validation("type", reason))?;
            let default = match default {
                Some(raw) => Some(
                    ty.parse_value(raw)
                        .map_err(|reason| params::validation("default", reason))?,
                ),
                None => None,
            };
            Ok(SchemaAction::AddRequiredField {
                key: field_key(key)?,
                ty,
                default,
            })
        }
    }
}

/// Translate the one storage action into the domain request.
fn storage_action(action: &StorageActionArg) -> Result<StorageAction, WaymarkError> {
    match action {
        StorageActionArg::Root { path } => Ok(StorageAction::Root { path: path.clone() }),
        StorageActionArg::CategoryPath { key, path } => Ok(StorageAction::CategoryPath {
            key: category_key(key)?,
            path: path.clone(),
        }),
        StorageActionArg::CategoryKey { from, to } => Ok(StorageAction::CategoryKey {
            from: category_key(from)?,
            to: category_key(to)?,
        }),
    }
}

/// A category key from an action argument.
fn category_key(literal: &str) -> Result<CategoryKey, WaymarkError> {
    CategoryKey::parse(literal).map_err(|error| {
        WaymarkError::validation(
            "category",
            vec![ValidationIssue::new("category", error.to_string())],
        )
        .at_field("category")
    })
}

/// A custom-field key from an action argument.
fn field_key(literal: &str) -> Result<FieldKey, WaymarkError> {
    FieldKey::parse(literal).map_err(|error| params::validation("key", error.to_string()))
}

/// One migration report as a successful tool result.
fn migration_result(report: &MigrationReport) -> ToolResult {
    envelope::success(report.data_json(), report.meta.clone())
}
