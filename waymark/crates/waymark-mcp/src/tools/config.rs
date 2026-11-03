//! Configuration and project-diagnostic tools.
//!
//! `config_validate`, `validate`, and `doctor` are diagnostic reads: they answer
//! with a report in the success envelope even when the report contains errors,
//! because "here is what is wrong" is the answer that was asked for. Only an
//! inability to produce a report at all (no project, unreadable location) is a
//! business failure.

use camino::Utf8Path;
use rmcp::tool;
use rmcp::tool_router;
use serde_json::{json, Value};
use waymark_core::envelope::Meta;
use waymark_core::error::{Code, WaymarkError};
use waymark_core::ops::config;
use waymark_core::ops::doctor::{self, DoctorOptions, Report};
use waymark_core::{Config, Project};

use crate::envelope::{self, ToolResult};
use crate::server::WaymarkServer;

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
