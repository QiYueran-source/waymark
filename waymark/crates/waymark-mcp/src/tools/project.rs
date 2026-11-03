//! Project lifecycle tools: initialisation and the project summary.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use waymark_core::config::{self, Category};
use waymark_core::envelope::Meta;
use waymark_core::error::WaymarkError;
use waymark_core::store::InitOptions;
use waymark_core::time::parse_timezone;
use waymark_core::{CategoryKey, Config, Project};

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

    /// Advisory operator identity recorded in the configuration.
    #[serde(default)]
    pub operator: Option<String>,

    /// IANA time zone used for human display. Defaults to `UTC`.
    #[serde(default)]
    pub timezone: Option<String>,

    /// Replace an existing configuration instead of reusing it. An existing
    /// project is reused by default, so init is safe to re-run.
    #[serde(default)]
    pub force: bool,
}

impl ProjectInitArgs {
    fn to_config(&self) -> Result<Config, WaymarkError> {
        let key = CategoryKey::parse(
            self.category
                .as_deref()
                .unwrap_or(config::DEFAULT_CATEGORY_KEY),
        )
        .map_err(|error| {
            WaymarkError::config_invalid("init", vec![format!("category: {error}")])
        })?;

        let mut config = Config::fresh();
        config.storage_root = self
            .storage_root
            .clone()
            .unwrap_or_else(|| config::DEFAULT_STORAGE_ROOT.to_owned());
        config.categories.clear();
        config.categories.insert(
            key.clone(),
            Category {
                path: format!("{}/{}", config.storage_root, key.as_str()),
                name: config::default_category_name(&key),
            },
        );
        if let Some(operator) = &self.operator {
            config.operator = Some(operator.clone());
        }
        if let Some(timezone) = &self.timezone {
            config.timezone = parse_timezone(timezone).map_err(|error| {
                WaymarkError::config_invalid("init", vec![format!("timezone: {error}")])
            })?;
        }
        Ok(config)
    }
}

#[tool_router(router = project_router, vis = "pub(crate)")]
impl WaymarkServer {
    /// Create the Waymark project in the server's project directory and return
    /// it. Defaults: category `task`, storage root `work`, operator unset,
    /// time zone `UTC`, and no forced overwrite. Re-running init reuses an
    /// existing configuration unless `force` is true.
    #[tool(
        name = "waymark_project_init",
        annotations(title = "Initialise the Waymark project")
    )]
    async fn waymark_project_init(
        &self,
        Parameters(args): Parameters<ProjectInitArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let config = args.to_config()?;
            let project = Project::init(
                self.start(),
                InitOptions {
                    force: args.force,
                    config: Some(config),
                    operator: None,
                },
            )?;
            Ok(envelope::success(
                project_data(&project)?,
                Meta::direct_file_scoped(),
            ))
        })
    }

    /// Describe the project this server operates on: storage root, generation,
    /// revision, operator, time zone, categories, declared fields, and item
    /// counts. Reads the configuration and counts the inventory; never writes.
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
        "timezone": config.timezone.name(),
        "operator": config.operator,
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
