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
use serde_json::json;
use waymark_core::envelope::Meta;
use waymark_core::error::WaymarkError;
use waymark_core::ops::config::{self, ConfigOutcome, FieldAttribute, Scalar};
use waymark_core::ops::doctor::{self, DoctorOptions, Report};

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

    /// Wait at most this many milliseconds for the project write lock.
    /// Omitted uses the shared default wait; `0` fails immediately with the
    /// retryable `LOCK_TIMEOUT` code.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
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

    /// Wait at most this many milliseconds for the project write lock.
    /// Omitted uses the shared default wait; `0` fails immediately with the
    /// retryable `LOCK_TIMEOUT` code.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
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

    /// Wait at most this many milliseconds for the project write lock.
    /// Omitted uses the shared default wait; `0` fails immediately with the
    /// retryable `LOCK_TIMEOUT` code.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
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

    /// Wait at most this many milliseconds for the project write lock.
    /// Omitted uses the shared default wait; `0` fails immediately with the
    /// retryable `LOCK_TIMEOUT` code.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
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

    /// Wait at most this many milliseconds for the project write lock.
    /// Omitted uses the shared default wait; `0` fails immediately with the
    /// retryable `LOCK_TIMEOUT` code.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
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

    /// Wait at most this many milliseconds for the project write lock.
    /// Omitted uses the shared default wait; `0` fails immediately with the
    /// retryable `LOCK_TIMEOUT` code.
    #[serde(default)]
    pub lock_timeout_ms: Option<u64>,
}

#[tool_router(router = config_router, vis = "pub(crate)")]
impl WaymarkServer {
    /// Read both configuration documents and the values they resolve to: the
    /// shared project configuration, the local user configuration, and the
    /// effective operator, time zone, and cache settings with the source of
    /// each. Reads both documents from disk on every call.
    #[tool(
        name = "waymark_config_read",
        annotations(title = "Read the effective configuration")
    )]
    async fn waymark_config_read(&self) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let bundle = waymark_core::ops::local_config::bundle(&project)?;
            Ok(envelope::success(
                waymark_core::view::config_bundle_json(&bundle),
                waymark_core::view::config_bundle_meta(&bundle),
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
            // project, so the file is located without parsing it. Core decides
            // which documents that path covers, so both documents are always
            // reviewed together.
            let path = config::locate_config_path(self.start())?;
            let findings = config::validate_documents(&path);
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
    /// including the non-blocking held-lock probe (`LOCK_HELD`), Git
    /// availability (`NOT_A_GIT_REPOSITORY` / `GIT_UNAVAILABLE`), projection
    /// completeness, and graph diagnostics. Never writes business data and
    /// never installs Git or initializes a repository.
    #[tool(name = "waymark_doctor", annotations(title = "Diagnose the project"))]
    async fn waymark_doctor(&self) -> ToolResult {
        envelope::respond(|| Ok(report_result(diagnose(self.start())?)))
    }

    /// Set one local user configuration scalar: `operator`, `timezone`,
    /// `cache.enabled`, or `cache.capacity`. Takes the project write lock,
    /// writes the local document only, and leaves the shared configuration —
    /// and therefore its revision — untouched. An empty `operator` clears it.
    /// `dry_run` performs every check and writes nothing.
    ///
    /// `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
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
            let outcome = waymark_core::ops::local_config::set_scalar(
                &project,
                scalar,
                &args.value,
                &params::options(args.dry_run, args.lock_timeout_ms),
            )?;
            Ok(local_result(&outcome))
        })
    }

    /// Declare a new category. The directory is created on a real run; a dry
    /// run validates the path and creates nothing.
    ///
    /// `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
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
                &params::options(args.dry_run, args.lock_timeout_ms),
            )?;
            Ok(config_result(&outcome))
        })
    }

    /// Change a category's display name. Items and directories are untouched;
    /// a directory move is a storage migration.
    ///
    /// `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
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
                &params::options(args.dry_run, args.lock_timeout_ms),
            )?;
            Ok(config_result(&outcome))
        })
    }

    /// Remove a category that no work item uses, active or archived. The
    /// refusal names every item that still uses it.
    ///
    /// `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
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
            let outcome = config::remove_category(
                &project,
                &key,
                &params::options(args.dry_run, args.lock_timeout_ms),
            )?;
            Ok(config_result(&outcome))
        })
    }

    /// Declare an optional custom field. A required declaration is a schema
    /// migration and is refused with `ACTION_REQUIRES_MIGRATION`.
    ///
    /// `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
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
                &params::options(args.dry_run, args.lock_timeout_ms),
            )?;
            Ok(config_result(&outcome))
        })
    }

    /// Change one metadata attribute of a declared custom field: relax
    /// `required` to false, or set/clear the `default`. A type change and
    /// making a field required are migrations and are refused with
    /// `ACTION_REQUIRES_MIGRATION`.
    ///
    /// `lock_timeout_ms` bounds the wait for the project write lock; `0`
    /// fails immediately with retryable `LOCK_TIMEOUT`.
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
                &params::options(args.dry_run, args.lock_timeout_ms),
            )?;
            Ok(config_result(&outcome))
        })
    }
}

/// One local configuration write as a successful tool result.
///
/// The shape mirrors the CLI's `--json` outcome so both adapters report the same
/// facts: the local document, what changed, and the shared revision, which a
/// local write never moves.
fn local_result(outcome: &waymark_core::ops::local_config::LocalOutcome) -> ToolResult {
    envelope::success(
        waymark_core::view::local_outcome_json(outcome),
        Meta::direct_file_scoped(),
    )
}

/// One configuration write as a successful tool result.
///
/// The shape matches the CLI's `--json` outcome: the effective configuration
/// plus what the write changed and the revision it moved from and to.
fn config_result(outcome: &ConfigOutcome) -> ToolResult {
    envelope::success(
        waymark_core::view::config_outcome_json(outcome),
        Meta::direct_file_scoped(),
    )
}

/// Emit a diagnostic report as a successful tool result.
fn report_result(report: Report) -> ToolResult {
    envelope::success(report.data_json(), report.meta.clone())
}

/// Resolve the project and run the shared diagnostics.
///
/// The core entry point owns the project-root derivation and maps a broken
/// configuration to a configuration-scoped report, exactly as the CLI does.
fn diagnose(start: &Utf8Path) -> Result<Report, WaymarkError> {
    doctor::diagnose_from(start, DoctorOptions::new())
}
