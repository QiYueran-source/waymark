//! `waymark dependency add|remove`: hard, directed dependency edges.
//!
//! The command is argument parsing and output routing; every rule — self
//! reference, duplicate, cycle, archived or missing target — lives in
//! `waymark_core::ops::relationship`.

use clap::{Args, Subcommand};
use waymark_core::envelope::success_envelope;
use waymark_core::error::WaymarkError;
use waymark_core::ops::relationship::{self, RelationshipOutcome};
use waymark_core::ops::OpOptions;
use waymark_core::Project;

use super::{emit, err_line};

/// Arguments of `waymark dependency`.
#[derive(Debug, Args)]
pub struct DependencyArgs {
    #[command(subcommand)]
    pub command: DependencyCommand,
}

/// The dependency family.
#[derive(Debug, Subcommand)]
pub enum DependencyCommand {
    /// Make ID wait for a blocking item.
    Add(EdgeArgs),
    /// Drop the wait.
    Remove(EdgeArgs),
}

/// The two IDs an edge is made of.
#[derive(Debug, Args)]
pub struct EdgeArgs {
    /// Stable ID of the item that waits.
    #[arg(value_name = "ID")]
    pub id: String,

    /// Stable ID of the item it waits for.
    #[arg(value_name = "BLOCKING_ID")]
    pub blocking: String,

    /// Report the change without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// Run `waymark dependency`.
pub fn run(
    project: &Project,
    args: DependencyArgs,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    match args.command {
        DependencyCommand::Add(edge) => apply(
            project,
            &edge,
            options,
            json,
            EdgeVerb::Add,
            relationship::add_dependency,
        ),
        DependencyCommand::Remove(edge) => apply(
            project,
            &edge,
            options,
            json,
            EdgeVerb::Remove,
            relationship::remove_dependency,
        ),
    }
}

/// Which verb the human narrative uses; the operation itself is passed as a
/// function so the two cannot disagree about which one ran.
#[derive(Debug, Clone, Copy)]
enum EdgeVerb {
    Add,
    Remove,
}

fn apply(
    project: &Project,
    edge: &EdgeArgs,
    options: &OpOptions,
    json: bool,
    verb: EdgeVerb,
    operation: fn(&Project, &str, &str, &OpOptions) -> Result<RelationshipOutcome, WaymarkError>,
) -> Result<(), WaymarkError> {
    let options = if edge.dry_run {
        options.clone().dry_run()
    } else {
        options.clone()
    };
    let outcome = operation(project, &edge.id, &edge.blocking, &options)?;

    if json {
        emit(&success_envelope(payload(&outcome), outcome.meta))?;
    } else {
        let prefix = if outcome.written { "" } else { "would be " };
        let relation = match verb {
            EdgeVerb::Add => "depends on",
            EdgeVerb::Remove => "no longer depends on",
        };
        err_line(&format!(
            "{prefix}{} {} {}",
            outcome.item.id, relation, outcome.dependency
        ));
        for warning in &outcome.meta.warnings {
            err_line(&format!("  {}: {}", warning.code, warning.message));
        }
    }
    Ok(())
}

/// The `data` block of a relationship result.
fn payload(outcome: &RelationshipOutcome) -> serde_json::Value {
    super::write_json(
        &outcome.item,
        &outcome.path,
        outcome.changed,
        outcome.written,
        serde_json::json!({
            "dependency": outcome.dependency.as_str(),
            "dependencies": outcome.item.dependencies.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        }),
    )
}
