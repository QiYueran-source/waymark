//! `waymark link add|remove`: symmetric soft links.
//!
//! A soft link is stored on both ends and both documents are replaced as one
//! all-or-nothing batch; that rule lives in `waymark_core::ops::link`, and this
//! module only parses the two IDs and renders the result.

use clap::{Args, Subcommand};
use waymark_core::envelope::success_envelope;
use waymark_core::error::WaymarkError;
use waymark_core::ops::link::{self, LinkOutcome};
use waymark_core::ops::OpOptions;
use waymark_core::Project;

use super::{emit, err_line, item_json};

/// Arguments of `waymark link`.
#[derive(Debug, Args)]
pub struct LinkArgs {
    #[command(subcommand)]
    pub command: LinkCommand,
}

/// The link family.
#[derive(Debug, Subcommand)]
pub enum LinkCommand {
    /// Associate two items with each other.
    Add(LinkPair),
    /// Drop the association.
    Remove(LinkPair),
}

/// The two IDs a link is made of.
#[derive(Debug, Args)]
pub struct LinkPair {
    /// Stable ID of one end.
    #[arg(value_name = "ID")]
    pub id: String,

    /// Stable ID of the other end.
    #[arg(value_name = "OTHER_ID")]
    pub other: String,

    /// Report the change without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// Which verb ran.
#[derive(Debug, Clone, Copy)]
enum Verb {
    Add,
    Remove,
}

/// Run `waymark link`.
pub fn run(
    project: &Project,
    args: LinkArgs,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    match args.command {
        LinkCommand::Add(pair) => apply(project, &pair, options, json, Verb::Add),
        LinkCommand::Remove(pair) => apply(project, &pair, options, json, Verb::Remove),
    }
}

fn apply(
    project: &Project,
    pair: &LinkPair,
    options: &OpOptions,
    json: bool,
    verb: Verb,
) -> Result<(), WaymarkError> {
    let options = if pair.dry_run {
        options.clone().dry_run()
    } else {
        options.clone()
    };
    let outcome = match verb {
        Verb::Add => link::add_soft_link(project, &pair.id, &pair.other, &options)?,
        Verb::Remove => link::remove_soft_link(project, &pair.id, &pair.other, &options)?,
    };

    if json {
        emit(&success_envelope(
            payload(&outcome, verb),
            outcome.meta.clone(),
        ))?;
        return Ok(());
    }

    let prefix = if options.dry_run { "would be " } else { "" };
    let relation = match verb {
        Verb::Add => "linked to",
        Verb::Remove => "no longer linked to",
    };
    err_line(&format!(
        "{prefix}{} {relation} {}",
        outcome.item.id, outcome.other_id
    ));
    if outcome.other.is_none() {
        err_line(&format!(
            "  {} had no document; only {} was updated",
            outcome.other_id, outcome.item.id
        ));
    }
    for warning in &outcome.meta.warnings {
        err_line(&format!("  {}: {}", warning.code, warning.message));
    }
    Ok(())
}

/// The `data` block of a link result.
fn payload(outcome: &LinkOutcome, verb: Verb) -> serde_json::Value {
    serde_json::json!({
        "id": outcome.item.id.as_str(),
        "path": outcome.path.to_string(),
        "other": outcome.other_id.as_str(),
        "other_path": outcome.other_path.as_ref().map(|path| path.to_string()),
        "verb": match verb {
            Verb::Add => "add",
            Verb::Remove => "remove",
        },
        "soft_links": outcome.item.soft_links.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        "changed": outcome.changed,
        "written": outcome.written,
        "fingerprints": outcome.fingerprints.iter().map(|f| f.as_str()).collect::<Vec<_>>(),
        "item": item_json(&outcome.item, &outcome.path),
        "other_item": outcome.other.as_ref().zip(outcome.other_path.as_ref()).map(|(item, path)| item_json(item, path)),
    })
}
