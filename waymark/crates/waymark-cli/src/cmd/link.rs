//! `waymark link add|remove`: symmetric soft links.
//!
//! A soft link is stored on both ends, but only a consented change writes the
//! other end: without it the pair is left one-sided and this module says so in
//! both output modes, naming the partner it left alone and its owner. Those
//! rules live in `waymark_core::ops::link`, the JSON block in
//! `waymark_core::view::link_result_json`; this module only parses the IDs and
//! flags and prints the result.

use clap::{Args, Subcommand};
use waymark_core::envelope::success_envelope;
use waymark_core::error::WaymarkError;
use waymark_core::ops::link;
use waymark_core::ops::OpOptions;
use waymark_core::Project;

use super::{emit, err_line};

/// Arguments of `waymark link`.
#[derive(Debug, Args)]
pub struct LinkArgs {
    #[command(subcommand)]
    pub command: LinkCommand,
}

/// The link family.
///
/// `add` and `remove` take their own argument sets instead of sharing one
/// struct, because the two verbs carry different consent flags: `add` without
/// `--cascade` records the caller's side only, while `remove` without it
/// removes the caller's side only.
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
    let options = options.clone().flags(cascade, pair.dry_run());
    let outcome = match verb {
        Verb::Add => link::add_soft_link(project, pair.id(), pair.other(), &options)?,
        Verb::Remove => link::remove_soft_link(project, pair.id(), pair.other(), &options)?,
    };

    if json {
        let verb = match verb {
            Verb::Add => "add",
            Verb::Remove => "remove",
        };
        emit(&success_envelope(
            waymark_core::view::link_result_json(&outcome, verb),
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
        // A change without consent: the partner keeps its document, its bytes,
        // and the link list it already had. The owner is printed because the
        // next question is whose file still needs the other half.
        err_line(&format!(
            "  {} {owned} {unchanged} ({owner})",
            untouched.id,
            owned = if untouched.owned {
                "left alone"
            } else {
                "left for its owner"
            },
            unchanged = if options.dry_run {
                "and keeps the link it lists"
            } else {
                "and still lists the link"
            },
            owner = untouched.owner
        ));
        if outcome.partner_unsynchronised {
            // The caller's document now records a link the partner does not.
            err_line("  the link is recorded on one side only; re-run with --cascade to record it there too");
        }
    } else if outcome.partner_unsynchronised {
        err_line(&format!(
            "  {} was not changed: the link is recorded on one side only; re-run with --cascade to record it there too",
            outcome.other_id
        ));
    } else if outcome.other.is_none() {
        // Only a partnered change reaches here: the other document is missing
        // or archived, so the caller's side is all there was to write.
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
