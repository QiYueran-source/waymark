//! `waymark status set` and the `start` / `close` / `reopen` conveniences.
//!
//! The conveniences are not separate operations: they are the same validated
//! status transition with the argument filled in, so `waymark close ID` and
//! `waymark status set ID closed` cannot drift apart.

use clap::{Args, Subcommand};
use waymark_core::envelope::success_envelope;
use waymark_core::error::WaymarkError;
use waymark_core::ops::status::{self, StatusOutcome};
use waymark_core::ops::OpOptions;
use waymark_core::{Project, Status};

use super::{emit, err_line, item_json};

/// Arguments of `waymark status`.
#[derive(Debug, Args)]
pub struct StatusArgs {
    #[command(subcommand)]
    pub command: StatusCommand,
}

/// The status family.
#[derive(Debug, Subcommand)]
pub enum StatusCommand {
    /// Move one item to a lifecycle state.
    Set(SetArgs),
}

/// Arguments of `waymark status set`.
#[derive(Debug, Args)]
pub struct SetArgs {
    /// Stable ID of the item.
    #[arg(value_name = "ID")]
    pub id: String,

    /// One of `open`, `in_progress`, `closed`.
    #[arg(value_name = "STATUS")]
    pub status: String,

    /// Report the transition without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// Arguments of `waymark start`, `waymark close`, and `waymark reopen`.
#[derive(Debug, Args)]
pub struct TransitionArgs {
    /// Stable ID of the item.
    #[arg(value_name = "ID")]
    pub id: String,

    /// Report the transition without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// Run `waymark status`.
pub fn run(
    project: &Project,
    args: StatusArgs,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    match args.command {
        StatusCommand::Set(args) => {
            let status = status::parse_status(&args.status)?;
            set(project, &args.id, status, args.dry_run, options, json)
        }
    }
}

/// Run one of the conveniences: a fixed status on the same operation.
pub fn convenience(
    project: &Project,
    args: &TransitionArgs,
    status: Status,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    set(project, &args.id, status, args.dry_run, options, json)
}

fn set(
    project: &Project,
    id: &str,
    status: Status,
    dry_run: bool,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    let options = if dry_run {
        options.clone().dry_run()
    } else {
        options.clone()
    };
    let outcome = status::set_status(project, id, status, &options)?;

    if json {
        emit(&success_envelope(payload(&outcome), outcome.meta))?;
    } else {
        let verb = if outcome.changed { "set" } else { "unchanged" };
        let prefix = if outcome.written { "" } else { "would be " };
        err_line(&format!(
            "{prefix}{verb} {} {} -> {}",
            outcome.item.id,
            outcome.previous.as_str(),
            outcome.item.status.as_str()
        ));
        for warning in &outcome.meta.warnings {
            err_line(&format!("  {}: {}", warning.code, warning.message));
        }
    }
    Ok(())
}

/// The `data` block of a status result.
fn payload(outcome: &StatusOutcome) -> serde_json::Value {
    serde_json::json!({
        "id": outcome.item.id.as_str(),
        "path": outcome.path.to_string(),
        "previous_status": outcome.previous.as_str(),
        "status": outcome.item.status.as_str(),
        "changed": outcome.changed,
        "written": outcome.written,
        "closed_at": outcome.item.closed_at.map(|value| value.to_string()),
        "item": item_json(&outcome.item, &outcome.path),
    })
}
