//! `waymark project info`: where the project is and what it declares.
//!
//! This is the one command that answers "which project am I in". It reads the
//! configuration and counts the inventory; it never writes and never takes the
//! lock.

use clap::{Args, Subcommand};
use serde_json::Value;
use waymark_core::envelope::{success_envelope, Meta};
use waymark_core::error::WaymarkError;
use waymark_core::Project;

use super::{emit, err_line, open_project};

/// Arguments of `waymark project`.
#[derive(Debug, Args)]
pub struct ProjectArgs {
    #[command(subcommand)]
    pub command: ProjectCommand,
}

/// The `project` subcommands.
#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// Where the project is, which version it declares, and how big it is.
    Info,
}

/// Run `waymark project …`.
pub fn run(
    args: ProjectArgs,
    project: Option<&camino::Utf8Path>,
    json: bool,
) -> Result<(), WaymarkError> {
    match args.command {
        ProjectCommand::Info => {
            let project = open_project(project)?;
            render_info(&project, json)
        }
    }
}

fn render_info(project: &Project, json: bool) -> Result<(), WaymarkError> {
    let config = project.config();
    let inventory = project.inventory()?;
    let root = project.root().to_string();

    if json {
        emit(&success_envelope(
            serde_json::json!({
                "root": root,
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
                            serde_json::json!({
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
            }),
            Meta::direct_file_scoped(),
        ))?;
        return Ok(());
    }

    err_line(&format!("project       {root}"));
    err_line(&format!("  metadata    {}", project.metadata_dir()));
    err_line(&format!(
        "  version     {} (revision {})",
        config.version, config.version.revision
    ));
    err_line(&format!("  storage     {}", config.storage_root));
    err_line(&format!(
        "  categories  {}",
        config
            .categories
            .keys()
            .map(|key| key.as_str().to_owned())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    err_line(&format!("  fields      {}", config.fields.len()));
    err_line(&format!("  items       {}", inventory.entries.len()));
    if !inventory.problems.is_empty() {
        err_line(&format!("  problems    {}", inventory.problems.len()));
    }
    Ok(())
}
