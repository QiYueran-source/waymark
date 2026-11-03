//! Configuration and project-diagnostic tools.
//!
//! `config_validate`, `validate`, and `doctor` are diagnostic reads: they answer
//! with a report in the success envelope even when the report contains errors,
//! because "here is what is wrong" is the answer that was asked for. Only an
//! inability to produce a report at all (no project, unreadable location) is a
//! business failure.

use camino::Utf8Path;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use waymark_core::envelope::Meta;
use waymark_core::error::{Code, WaymarkError};
use waymark_core::ops::config::{self, ConfigOutcome, FieldAttribute, Scalar};
use waymark_core::ops::doctor::{self, DoctorOptions, Report};
use waymark_core::{Config, Project};

use crate::envelope::{self, ToolResult};
use crate::server::WaymarkServer;
use crate::tools::params;

/// Inputs of `waymark_config_set`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfigSetArgs {
    /// Scalar name: `operator`, `timezone`, `cache.enabled`, or `cache.capacity`.
    pub name: String,

    /// New value, in the scalar's literal form (`frank`, `Europe/Berlin`,
    /// `true`, `50`). An empty `operator` clears the key.
    pub value: String,

    /// Validate and report the change without writing.
    #[serde(default)]
    pub dry_run: bool,
}

/// Inputs of `waymark_config_add_category`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfigAddCategoryArgs {
    /// Category key, for example `bug`.
    pub key: String,

    /// Repository-relative directory, for example `work/bugs`.
    pub path: String,

    /// Display name. Defaults to the key's conventional name.
    #[serde(default)]
    pub name: Option<String>,

    /// Validate and report the change without writing.
    #[serde(default)]
    pub dry_run: bool,
}

/// Inputs of `waymark_config_update_category`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfigUpdateCategoryArgs {
    /// Category key to change.
    pub key: String,

    /// New display name. The directory and the items are untouched; moving a
    /// category directory is a storage migration.
    pub name: String,

    /// Validate and report the change without writing.
    #[serde(default)]
    pub dry_run: bool,
}

/// Inputs of `waymark_config_remove_category`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfigRemoveCategoryArgs {
    /// Category key to remove. Refused while any active or archived item uses
    /// it.
    pub key: String,

    /// Validate and report the change without writing.
    #[serde(default)]
    pub dry_run: bool,
}

/// Inputs of `waymark_config_add_field`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfigAddFieldArgs {
    /// Custom-field key.
    pub key: String,

    /// Declared value shape: `string`, `integer`, `boolean`, `list:string`,
    /// `list:integer`, or `list:boolean`.
    #[serde(rename = "type")]
    pub ty: String,

    /// Declare the field as required. Refused by the ordinary configuration
    /// surface with `ACTION_REQUIRES_MIGRATION`; use the schema migration.
    #[serde(default)]
    pub required: bool,

    /// Static default, in the declared type's literal form.
    #[serde(default)]
    pub default: Option<String>,

    /// Validate and report the change without writing.
    #[serde(default)]
    pub dry_run: bool,
}

/// Inputs of `waymark_config_update_field`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfigUpdateFieldArgs {
    /// Declared custom-field key.
    pub key: String,

    /// Attribute to change: `required` or `default`. `type` is a schema
    /// migration; declaring `required` is one too.
    pub attribute: String,

    /// New attribute value, in its literal form (`false`, `42`, `null`).
    pub value: String,

    /// Validate and report the change without writing.
    #[serde(default)]
    pub dry_run: bool,
}

#[tool_router(router = config_router, vis = "pub(crate)")]
impl WaymarkServer {
    /// Read the effective configuration: the format generation and revision,
    /// storage root, operator, time zone, categories, declared custom fields,
    /// and cache settings. Reads the document from disk on every call.
    #[tool(
        name = "waymark_config_read",
        annotations(title = "Read the effective configuration")
    )]
    async fn waymark_config_read(&self) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let config = config::show(&project)?;
            Ok(envelope::success(
                json!({"config": config_json(&config)}),
                Meta::direct_file_scoped(),
            ))
        })
    }

    /// Validate the project configuration and return every finding. A document
    /// that cannot be parsed still yields a report (one `CONFIG_INVALID`
    /// finding); an absent project is `PROJECT_NOT_FOUND`. Findings are also
    /// repeated in `_meta.diagnostics`.
    #[tool(
        name = "waymark_config_validate",
        annotations(title = "Validate the configuration")
    )]
    async fn waymark_config_validate(&self) -> ToolResult {
        envelope::respond(|| {
            // Validation must work when the document cannot be opened as a
            // project, so the file is located without parsing it.
            let path = config::locate_config_path(self.start())?;
            let findings = config::validate_config_path(&path);
            let meta = Meta::direct_file_scoped().with_diagnostics(findings.clone());
            Ok(envelope::success(
                json!({"valid": findings.is_empty(), "findings": findings}),
                meta,
            ))
        })
    }

    /// Validate the whole project against the current configuration: one report
    /// with every finding (severity, registry code, location, message,
    /// fixable), counts, scope, and repairs. Never writes business data.
    #[tool(
        name = "waymark_validate",
        annotations(title = "Validate the whole project")
    )]
    async fn waymark_validate(&self) -> ToolResult {
        envelope::respond(|| Ok(report_result(diagnose(self.start())?)))
    }

    /// Run the project health check: the same report as `waymark_validate`,
    /// including the non-blocking held-lock probe (`LOCK_HELD`), projection
    /// completeness, and graph diagnostics. Never writes business data.
    #[tool(name = "waymark_doctor", annotations(title = "Diagnose the project"))]
    async fn waymark_doctor(&self) -> ToolResult {
        envelope::respond(|| Ok(report_result(diagnose(self.start())?)))
    }

    /// Set one configuration scalar: `operator`, `timezone`, `cache.enabled`,
    /// or `cache.capacity`. Takes the shared write lock, bumps the revision,
    /// and returns the updated configuration. An empty `operator` clears it.
    /// `dry_run` performs every check and writes nothing.
    #[tool(
        name = "waymark_config_set",
        annotations(
            title = "Set a configuration scalar",
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn waymark_config_set(&self, Parameters(args): Parameters<ConfigSetArgs>) -> ToolResult {
        envelope::respond(|| {
            let scalar = Scalar::parse(&args.name)?;
            let project = self.project()?;
            let outcome = config::set_scalar(
                &project,
                scalar,
                &args.value,
                &params::options(args.dry_run),
            )?;
            Ok(config_result(&outcome))
        })
    }

    /// Declare a new category. The directory is created on a real run; a dry
    /// run validates the path and creates nothing.
    #[tool(
        name = "waymark_config_add_category",
        annotations(
            title = "Add a category",
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn waymark_config_add_category(
        &self,
        Parameters(args): Parameters<ConfigAddCategoryArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let key = params::category_key(&args.key)?;
            let project = self.project()?;
            let outcome = config::add_category(
                &project,
                &key,
                &args.path,
                args.name.as_deref(),
                &params::options(args.dry_run),
            )?;
            Ok(config_result(&outcome))
        })
    }

    /// Change a category's display name. Items and directories are untouched;
    /// a directory move is a storage migration.
    #[tool(
        name = "waymark_config_update_category",
        annotations(
            title = "Rename a category",
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn waymark_config_update_category(
        &self,
        Parameters(args): Parameters<ConfigUpdateCategoryArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let key = params::category_key(&args.key)?;
            let project = self.project()?;
            let outcome = config::set_category_name(
                &project,
                &key,
                &args.name,
                &params::options(args.dry_run),
            )?;
            Ok(config_result(&outcome))
        })
    }

    /// Remove a category that no work item uses, active or archived. The
    /// refusal names every item that still uses it.
    #[tool(
        name = "waymark_config_remove_category",
        annotations(
            title = "Remove a category",
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn waymark_config_remove_category(
        &self,
        Parameters(args): Parameters<ConfigRemoveCategoryArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let key = params::category_key(&args.key)?;
            let project = self.project()?;
            let outcome = config::remove_category(&project, &key, &params::options(args.dry_run))?;
            Ok(config_result(&outcome))
        })
    }

    /// Declare an optional custom field. A required declaration is a schema
    /// migration and is refused with `ACTION_REQUIRES_MIGRATION`.
    #[tool(
        name = "waymark_config_add_field",
        annotations(
            title = "Declare a custom field",
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn waymark_config_add_field(
        &self,
        Parameters(args): Parameters<ConfigAddFieldArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let key = params::field_key(&args.key)?;
            let ty = params::field_type(&args.ty)?;
            let project = self.project()?;
            let outcome = config::add_field(
                &project,
                &key,
                ty,
                args.required,
                args.default.as_deref(),
                &params::options(args.dry_run),
            )?;
            Ok(config_result(&outcome))
        })
    }

    /// Change one metadata attribute of a declared custom field: relax
    /// `required` to false, or set/clear the `default`. A type change and
    /// making a field required are migrations and are refused with
    /// `ACTION_REQUIRES_MIGRATION`.
    #[tool(
        name = "waymark_config_update_field",
        annotations(
            title = "Change a custom-field declaration",
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn waymark_config_update_field(
        &self,
        Parameters(args): Parameters<ConfigUpdateFieldArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let key = params::field_key(&args.key)?;
            let attribute = FieldAttribute::parse(&args.attribute)?;
            let project = self.project()?;
            let outcome = config::set_field(
                &project,
                &key,
                attribute,
                &args.value,
                &params::options(args.dry_run),
            )?;
            Ok(config_result(&outcome))
        })
    }
}

/// One configuration write as a successful tool result.
///
/// The shape matches the CLI's `--json` outcome: the effective configuration
/// plus what the write changed and the revision it moved from and to.
fn config_result(outcome: &ConfigOutcome) -> ToolResult {
    envelope::success(
        json!({
            "config": config_json(&outcome.config),
            "changed": outcome.changed,
            "changed_keys": outcome.changed_keys,
            "previous_revision": outcome.previous_revision,
            "revision": outcome.revision,
        }),
        Meta::direct_file_scoped(),
    )
}

/// Emit a diagnostic report as a successful tool result.
fn report_result(report: Report) -> ToolResult {
    envelope::success(report.data_json(), report.meta.clone())
}

/// Resolve the project and run the shared diagnostics.
///
/// Mirrors the CLI: the configuration file is located without parsing so a
/// broken configuration still gets a configuration-scoped report instead of a
/// bare `CONFIG_INVALID` failure with no findings.
fn diagnose(start: &Utf8Path) -> Result<Report, WaymarkError> {
    let config_path = config::locate_config_path(start)?;
    match Config::load(&config_path) {
        Ok(_) => {
            let root = config_path
                .parent()
                .and_then(Utf8Path::parent)
                .ok_or_else(|| {
                    WaymarkError::internal(format!(
                        "the configuration path {config_path} has no project root"
                    ))
                })?;
            let project = Project::open(root)?;
            doctor::diagnose_with(&project, DoctorOptions::new())
        }
        Err(error) if matches!(error.code(), Code::ConfigInvalid | Code::UnsupportedVersion) => {
            doctor::diagnose_config(&config_path, DoctorOptions::new())
        }
        Err(error) => Err(error),
    }
}

/// The machine form of one configuration document.
fn config_json(config: &Config) -> Value {
    json!({
        "version": config.version.to_string(),
        "generation": config.version.generation,
        "revision": config.version.revision,
        "storage_root": config.storage_root,
        "operator": config.operator,
        "timezone": config.timezone.name(),
        "categories": config
            .categories
            .iter()
            .map(|(key, category)| {
                (
                    key.as_str().to_owned(),
                    json!({
                        "key": key.as_str(),
                        "path": category.path,
                        "name": category.name,
                    }),
                )
            })
            .collect::<serde_json::Map<String, Value>>(),
        "fields": config
            .fields
            .iter()
            .map(|(key, declaration)| {
                (
                    key.as_str().to_owned(),
                    json!({
                        "key": key.as_str(),
                        "type": declaration.ty.as_str(),
                        "required": declaration.required,
                        "default": declaration.default,
                    }),
                )
            })
            .collect::<serde_json::Map<String, Value>>(),
        "cache": {"enabled": config.cache.enabled, "capacity": config.cache.capacity},
    })
}
