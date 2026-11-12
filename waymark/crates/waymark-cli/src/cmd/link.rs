//! `waymark link add|remove`: symmetric soft links.
//!
//! A soft link is stored on both ends and both documents are replaced as one
//! all-or-nothing batch; that rule lives in `waymark_core::ops::link`, and this
//! module only parses the IDs and flags and renders the result.

use clap::{Args, Subcommand};
use waymark_core::envelope::success_envelope;
use waymark_core::error::WaymarkError;
use waymark_core::ops::link::{self, LinkOutcome};
use waymark_core::ops::OpOptions;
use waymark_core::Project;

use super::{emit, err_line};
use waymark_core::view::item_json;

/// Arguments of `waymark link`.
#[derive(Debug, Args)]
pub struct LinkArgs {
    #[command(subcommand)]
    pub command: LinkCommand,
}

/// The link family.
///
/// `add` and `remove` take their own argument sets instead of sharing one
/// struct, because the two verbs carry different consent flags: `add` can
/// rewrite the partner, `remove` cannot.
#[derive(Debug, Subcommand)]
pub enum LinkCommand {
    /// Associate two items with each other.
    Add(LinkAddArgs),
    /// Drop the association.
    Remove(LinkRemoveArgs),
}

/// Arguments of `waymark link add`.
#[derive(Debug, Args)]
pub struct LinkAddArgs {
    /// Stable ID of one end.
    #[arg(value_name = "ID")]
    pub id: String,

    /// Stable ID of the other end.
    #[arg(value_name = "OTHER_ID")]
    pub other: String,

    /// Consent to a cross-item write: also record the link on the other end
    /// when you own it.
    #[arg(long)]
    pub cascade: bool,

    /// Report the change without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// Arguments of `waymark link remove`.
#[derive(Debug, Args)]
pub struct LinkRemoveArgs {
    /// Stable ID of one end.
    #[arg(value_name = "ID")]
    pub id: String,

    /// Stable ID of the other end.
    #[arg(value_name = "OTHER_ID")]
    pub other: String,

    /// Consent to a cross-item write: also drop the link from the other end
    /// when you own it.
    #[arg(long)]
    pub cascade: bool,

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
        LinkCommand::Add(args) => apply(project, &args, args.cascade, options, json, Verb::Add),
        LinkCommand::Remove(args) => {
            apply(project, &args, args.cascade, options, json, Verb::Remove)
        }
    }
}

/// The two ends and the two flags either verb accepts.
trait LinkArguments {
    fn id(&self) -> &str;
    fn other(&self) -> &str;
    fn dry_run(&self) -> bool;
}

impl LinkArguments for LinkAddArgs {
    fn id(&self) -> &str {
        &self.id
    }

    fn other(&self) -> &str {
        &self.other
    }

    fn dry_run(&self) -> bool {
        self.dry_run
    }
}

impl LinkArguments for LinkRemoveArgs {
    fn id(&self) -> &str {
        &self.id
    }

    fn other(&self) -> &str {
        &self.other
    }

    fn dry_run(&self) -> bool {
        self.dry_run
    }
}

fn apply(
    project: &Project,
    pair: &impl LinkArguments,
    cascade: bool,
    options: &OpOptions,
    json: bool,
    verb: Verb,
) -> Result<(), WaymarkError> {
    let mut options = options.clone();
    if cascade {
        options.cascade = true;
    }
    if pair.dry_run() {
        options = options.dry_run();
    }
    let outcome = match verb {
        Verb::Add => link::add_soft_link(project, pair.id(), pair.other(), &options)?,
        Verb::Remove => link::remove_soft_link(project, pair.id(), pair.other(), &options)?,
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
    if let Some(untouched) = &outcome.other_untouched {
        err_line(&format!(
            "  {} {unchanged}",
            untouched.id,
            unchanged = if options.dry_run {
                "is left alone and still lists this link"
            } else {
                "was left alone"
            }
        ));
    } else if outcome.other.is_none() {
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
    super::write_json(
        &outcome.item,
        &outcome.path,
        outcome.changed,
        outcome.written,
        serde_json::json!({
            "other": outcome.other_id.as_str(),
            "other_path": outcome.other_path.as_ref().map(|path| path.to_string()),
            "verb": match verb {
                Verb::Add => "add",
                Verb::Remove => "remove",
            },
            "soft_links": outcome.item.soft_links.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
            "fingerprints": outcome.fingerprints.iter().map(|f| f.as_str()).collect::<Vec<_>>(),
            "other_item": outcome.other.as_ref().zip(outcome.other_path.as_ref()).map(|(item, path)| item_json(item, path)),
            "other_untouched": outcome.other_untouched.as_ref().map(waymark_core::view::link_untouched_json),
        }),
    )
}
