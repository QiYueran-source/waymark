//! `waymark init`: create the project configuration and metadata directory.

use camino::Utf8Path;
use clap::Args;
use waymark_core::envelope::{success_envelope, Meta};
use waymark_core::error::WaymarkError;
use waymark_core::ops::config::InitRequest;

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

    let mut request = InitRequest::new()
        .with_storage_root(args.storage_root)
        .with_category(args.category);
    if let Some(operator) = args.operator {
        request = request.with_operator(operator);
    }
    if let Some(timezone) = args.timezone {
        request = request.with_timezone(timezone);
    }
    let project = waymark_core::ops::config::init(&root, request, args.force)?;

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
