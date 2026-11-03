//! `waymark owner transfer`: hand one item's accountability to another identity.
//!
//! The rule — full replacement, identical-to-current is a no-op, archived and
//! migration-pending items refuse, `OWNER_MISMATCH` warns without blocking — all
//! lives in `waymark_core::ops::owner`. This module parses the two positionals
//! and the `--dry-run` flag and routes the outcome into the shared envelope.

use clap::{Args, Subcommand};
use waymark_core::envelope::success_envelope;
use waymark_core::error::WaymarkError;
use waymark_core::ops::owner::{self, OwnerOutcome};
use waymark_core::ops::OpOptions;
use waymark_core::Project;

use super::{emit, err_line, item_json};

/// Arguments of `waymark owner`.
#[derive(Debug, Args)]
pub struct OwnerArgs {
    #[command(subcommand)]
    pub command: OwnerCommand,
}

/// The owner family.
#[derive(Debug, Subcommand)]
pub enum OwnerCommand {
    /// Replace the accountable owner of one item.
    Transfer(TransferArgs),
}

/// Arguments of `waymark owner transfer`.
#[derive(Debug, Args)]
pub struct TransferArgs {
    /// Stable ID of the item.
    #[arg(value_name = "ID")]
    pub id: String,

    /// The new accountable identity. Surrounding whitespace is trimmed.
    #[arg(value_name = "OWNER")]
    pub owner: String,

    /// Report the transfer without writing it.
    #[arg(long)]
    pub dry_run: bool,
}

/// Run `waymark owner`.
pub fn run(
    project: &Project,
    args: OwnerArgs,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    match args.command {
        OwnerCommand::Transfer(args) => transfer(project, &args, options, json),
    }
}

fn transfer(
    project: &Project,
    args: &TransferArgs,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    let options = if args.dry_run {
        options.clone().dry_run()
    } else {
        options.clone()
    };
    let outcome = owner::transfer_owner(project, &args.id, &args.owner, &options)?;

    if json {
        emit(&success_envelope(payload(&outcome), outcome.meta.clone()))?;
        return Ok(());
    }

    if outcome.changed {
        let prefix = if outcome.written { "" } else { "would be " };
        err_line(&format!(
            "{prefix}transferred {} owner {} -> {}",
            outcome.item.id, outcome.previous_owner, outcome.item.owner
        ));
    } else {
        err_line(&format!(
            "unchanged {} (owner {})",
            outcome.item.id, outcome.item.owner
        ));
    }
    for warning in &outcome.meta.warnings {
        err_line(&format!("  {}: {}", warning.code, warning.message));
    }
    Ok(())
}

/// The `data` block of an owner transfer result.
fn payload(outcome: &OwnerOutcome) -> serde_json::Value {
    serde_json::json!({
        "id": outcome.item.id.as_str(),
        "path": outcome.path.to_string(),
        "previous_owner": outcome.previous_owner,
        "owner": outcome.item.owner,
        "changed": outcome.changed,
        "written": outcome.written,
        "item": item_json(&outcome.item, &outcome.path),
    })
}
