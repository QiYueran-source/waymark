//! `waymark parent set|clear`: the single hierarchy edge.
//!
//! The command is argument parsing and output routing; the rules — one parent at
//! most, cross-category parentage allowed, no cycle, no self reference, no
//! archived endpoint — live in `waymark_core::ops::parent`.

use clap::{Args, Subcommand};
use waymark_core::envelope::success_envelope;
use waymark_core::error::WaymarkError;
use waymark_core::ops::parent::{self, ParentOutcome};
use waymark_core::ops::OpOptions;
use waymark_core::Project;

use super::{emit, err_line, item_json};

/// Arguments of `waymark parent`.
#[derive(Debug, Args)]
pub struct ParentArgs {
    #[command(subcommand)]
    pub command: ParentCommand,
}

/// The parent family.
#[derive(Debug, Subcommand)]
pub enum ParentCommand {
    /// Give ID a parent, replacing any parent it already has.
    Set(SetArgs),
    /// Remove ID's parent.
    Clear(ClearArgs),
}

/// `parent set ID PARENT_ID`.
#[derive(Debug, Args)]
pub struct SetArgs {
    /// Stable ID of the child.
    #[arg(value_name = "ID")]
    pub id: String,

    /// Stable ID of the parent.
    #[arg(value_name = "PARENT_ID")]
    pub parent: String,

    /// Report the change without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// `parent clear ID`.
#[derive(Debug, Args)]
pub struct ClearArgs {
    /// Stable ID of the item whose parent is removed.
    #[arg(value_name = "ID")]
    pub id: String,

    /// Report the change without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// Which verb ran, so the human narrative cannot disagree with the operation.
#[derive(Debug, Clone, Copy)]
enum Verb {
    Set,
    Clear,
}

/// Run `waymark parent`.
pub fn run(
    project: &Project,
    args: ParentArgs,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    match args.command {
        ParentCommand::Set(args) => {
            let options = dry_run(options, args.dry_run);
            let outcome = parent::set_parent(project, &args.id, &args.parent, &options)?;
            report(&outcome, options.dry_run, json, Verb::Set)
        }
        ParentCommand::Clear(args) => {
            let options = dry_run(options, args.dry_run);
            let outcome = parent::clear_parent(project, &args.id, &options)?;
            report(&outcome, options.dry_run, json, Verb::Clear)
        }
    }
}

fn dry_run(options: &OpOptions, requested: bool) -> OpOptions {
    if requested {
        options.clone().dry_run()
    } else {
        options.clone()
    }
}

fn report(
    outcome: &ParentOutcome,
    dry_run: bool,
    json: bool,
    verb: Verb,
) -> Result<(), WaymarkError> {
    if json {
        emit(&success_envelope(
            payload(outcome, verb),
            outcome.meta.clone(),
        ))?;
        return Ok(());
    }

    let prefix = if dry_run { "would be " } else { "" };
    if !outcome.changed {
        err_line(&format!(
            "{} already has {} parent",
            outcome.item.id,
            match verb {
                Verb::Set => "that",
                Verb::Clear => "no",
            }
        ));
        return Ok(());
    }
    let narrative = match (verb, &outcome.parent) {
        (Verb::Set, Some(parent_id)) => {
            format!("{prefix}{} parent set to {parent_id}", outcome.item.id)
        }
        (Verb::Clear, _) => format!("{prefix}{} parent cleared", outcome.item.id),
        (Verb::Set, None) => format!("{prefix}{} parent cleared", outcome.item.id),
    };
    err_line(&narrative);
    for warning in &outcome.meta.warnings {
        err_line(&format!("  {}: {}", warning.code, warning.message));
    }
    Ok(())
}

/// The `data` block of a parent result.
fn payload(outcome: &ParentOutcome, verb: Verb) -> serde_json::Value {
    serde_json::json!({
        "id": outcome.item.id.as_str(),
        "path": outcome.path.to_string(),
        "verb": match verb {
            Verb::Set => "set",
            Verb::Clear => "clear",
        },
        "parent": outcome.parent.as_ref().map(|id| id.as_str()),
        "previous_parent": outcome.previous_parent.as_ref().map(|id| id.as_str()),
        "changed": outcome.changed,
        "written": outcome.written,
        "item": item_json(&outcome.item, &outcome.path),
    })
}
