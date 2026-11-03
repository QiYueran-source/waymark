//! `waymark init`: create the project configuration and metadata directory.

use camino::Utf8Path;
use clap::Args;
use waymark_core::config::Config;
use waymark_core::envelope::{success_envelope, Meta};
use waymark_core::error::WaymarkError;
use waymark_core::store::InitOptions;
use waymark_core::Project;

use super::{emit, err_line, invocation_dir};

/// Arguments of `waymark init`.
#[derive(Debug, Args)]
pub struct InitArgs {
    /// Category key to declare.
    #[arg(long, value_name = "KEY", default_value = waymark_core::config::DEFAULT_CATEGORY_KEY)]
    pub category: String,

    /// Repository-relative storage root.
    #[arg(long, value_name = "PATH", default_value = waymark_core::config::DEFAULT_STORAGE_ROOT)]
    pub storage_root: String,

    /// Advisory operator identity recorded in the configuration.
    #[arg(long, value_name = "NAME")]
    pub operator: Option<String>,

    /// IANA time zone used for human display.
    #[arg(long, value_name = "ZONE")]
    pub timezone: Option<String>,

    /// Replace an existing configuration instead of reusing it.
    #[arg(long)]
    pub force: bool,
}

/// Run `waymark init`.
pub fn run(args: InitArgs, explicit: Option<&Utf8Path>, json: bool) -> Result<(), WaymarkError> {
    let root = match explicit {
        Some(dir) => dir.to_path_buf(),
        None => invocation_dir()?,
    };

    let key = waymark_core::CategoryKey::parse(&args.category).map_err(|error| {
        WaymarkError::config_invalid("init", vec![format!("category: {error}")])
    })?;

    let mut config = Config::fresh();
    config.storage_root = args.storage_root.clone();
    config.categories.clear();
    config.categories.insert(
        key.clone(),
        waymark_core::config::Category {
            path: format!("{}/{}", args.storage_root, key.as_str()),
            name: waymark_core::config::default_category_name(&key),
        },
    );
    if let Some(operator) = &args.operator {
        config.operator = Some(operator.clone());
    }
    if let Some(timezone) = &args.timezone {
        config.timezone = waymark_core::time::parse_timezone(timezone)
            .map_err(|error| WaymarkError::config_invalid("init", vec![error.to_string()]))?;
    }

    let project = Project::init(
        &root,
        InitOptions {
            force: args.force,
            config: Some(config),
            operator: None,
        },
    )?;

    if json {
        let categories: Vec<serde_json::Value> = project
            .config()
            .categories
            .iter()
            .map(|(key, category)| {
                serde_json::json!({
                    "key": key.as_str(),
                    "path": category.path,
                    "name": category.name,
                })
            })
            .collect();
        emit(&success_envelope(
            serde_json::json!({
                "root": project.root().to_string(),
                "metadata_dir": project.metadata_dir().to_string(),
                "config_path": project.config_path().to_string(),
                "storage_root": project.config().storage_root,
                "categories": categories,
            }),
            Meta::direct_file_scoped(),
        ))?;
    } else {
        err_line(&format!(
            "initialised Waymark project at {}",
            project.root()
        ));
        err_line(&format!("  configuration  {}", project.config_path()));
        for (key, category) in &project.config().categories {
            err_line(&format!("  category {}  {}", key, category.path));
        }
    }
    Ok(())
}
