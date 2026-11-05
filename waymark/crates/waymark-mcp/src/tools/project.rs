//! Project lifecycle tools: initialisation and the project summary.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use waymark_core::envelope::Meta;
use waymark_core::error::WaymarkError;
use waymark_core::ops::config::InitRequest;
use waymark_core::Project;

use crate::envelope::{self, ToolResult};
use crate::server::WaymarkServer;

/// Inputs of `waymark_project_init`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProjectInitArgs {
    /// Category key to declare. Defaults to `task`.
    #[serde(default)]
    pub category: Option<String>,

    /// Repository-relative directory that will hold the category directories.
    /// Defaults to `work`.
    #[serde(default)]
    pub storage_root: Option<String>,

    /// Advisory operator identity seeded into the local user configuration.
    #[serde(default)]
    pub operator: Option<String>,

    /// IANA time zone seeded into the local user configuration. Defaults to the
    /// machine's zone, then `UTC`.
    #[serde(default)]
    pub timezone: Option<String>,

    /// Replace an existing configuration instead of reusing it. An existing
    /// project is reused by default, so init is safe to re-run.
    #[serde(default)]
    pub force: bool,
}

impl ProjectInitArgs {
    fn request(&self) -> InitRequest {
        let mut request = InitRequest::new();
        if let Some(category) = &self.category {
            request = request.with_category(category.clone());
        }
        if let Some(storage_root) = &self.storage_root {
            request = request.with_storage_root(storage_root.clone());
        }
        if let Some(operator) = &self.operator {
            request = request.with_operator(operator.clone());
        }
        if let Some(timezone) = &self.timezone {
            request = request.with_timezone(timezone.clone());
        }
        request
    }
}

#[tool_router(router = project_router, vis = "pub(crate)")]
impl WaymarkServer {
    /// Create the Waymark project in the server's project directory and return
    /// it. Defaults: category `task`, storage root `work`, operator and time
    /// zone seeded from the Git identity and the machine, and no forced
    /// overwrite. Re-running init reuses an existing configuration unless
    /// `force` is true, and never rewrites an existing local user document.
    #[tool(
        name = "waymark_project_init",
        annotations(title = "Initialise the Waymark project")
    )]
    async fn waymark_project_init(
        &self,
        Parameters(args): Parameters<ProjectInitArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let project =
                waymark_core::ops::config::init(self.start(), args.request(), args.force)?;
            let mut data = project_data(&project)?;
            data["local_config_path"] = json!(project.user_config_path().to_string());
            data["local_config_tracked"] =
                json!(waymark_core::ops::local_config::tracked(&project));
            Ok(envelope::success(data, Meta::direct_file_scoped()))
        })
    }

    /// Describe the project this server operates on: storage root, generation,
    /// revision, categories, declared fields, and item counts. Reads the
    /// configuration and counts the inventory; never writes. The local user
    /// settings are reported by `waymark_config_read`.
    #[tool(
        name = "waymark_project_info",
        annotations(title = "Read the Waymark project summary")
    )]
    async fn waymark_project_info(&self) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            Ok(envelope::success(
                project_data(&project)?,
                Meta::direct_file_scoped(),
            ))
        })
    }
}

/// The project summary, stable across `init` and `info`.
fn project_data(project: &Project) -> Result<Value, WaymarkError> {
    let config = project.config();
    let inventory = project.inventory()?;
    Ok(json!({
        "root": project.root().to_string(),
        "metadata_dir": project.metadata_dir().to_string(),
        "config_path": project.config_path().to_string(),
        "storage_root": config.storage_root,
        "version": config.version.to_string(),
        "generation": config.version.generation,
        "revision": config.version.revision,
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
        "field_count": config.fields.len(),
        "item_count": inventory.entries.len(),
        "problem_count": inventory.problems.len(),
    }))
}
