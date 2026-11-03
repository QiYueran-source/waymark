//! `waymark archive`: retire one item without deleting it.
//!
//! The rules — the marker, the child refusal, the inbound cleanup batch, the
//! archived-write rejection — all live in `waymark_core::ops::archive`. This
//! module parses the two flags and renders the result, including every planned
//! relationship removal on a dry run.

use clap::Args;
use waymark_core::envelope::success_envelope;
use waymark_core::error::WaymarkError;
use waymark_core::ops::archive::{self, ArchiveOutcome, PlannedRemoval};
use waymark_core::ops::OpOptions;
use waymark_core::Project;

use super::{emit, err_line};

/// Arguments of `waymark archive`.
#[derive(Debug, Args)]
pub struct ArchiveArgs {
    /// Stable ID of the item to retire.
    #[arg(value_name = "ID")]
    pub id: String,

    /// Why it is retired. Free text; stored as `archive_reason`.
    #[arg(long, value_name = "TEXT")]
    pub reason: Option<String>,

    /// Report the plan, including every relationship removal, without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// Run `waymark archive`.
pub fn run(
    project: &Project,
    args: &ArchiveArgs,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    let options = if args.dry_run {
        options.clone().dry_run()
    } else {
        options.clone()
    };
    let outcome = archive::archive(project, &args.id, args.reason.as_deref(), &options)?;

    if json {
        emit(&success_envelope(payload(&outcome), outcome.meta.clone()))?;
        return Ok(());
    }

    let prefix = if outcome.written { "" } else { "would be " };
    err_line(&format!(
        "{prefix}archived {} {}",
        outcome.item.id, outcome.archived_at
    ));
    if let Some(reason) = &outcome.archive_reason {
        err_line(&format!("  reason      {reason}"));
    }
    err_line(&format!(
        "  status      {} (unchanged)",
        outcome.item.status.as_str()
    ));
    for removal in &outcome.removals {
        err_line(&format!(
            "  {}  removes {}",
            removal.from,
            removal.relation.as_str()
        ));
    }
    if outcome.removals.is_empty() {
        err_line("  no inbound relationships to remove");
    }
    for warning in &outcome.meta.warnings {
        err_line(&format!("  {}: {}", warning.code, warning.message));
    }
    Ok(())
}

/// The `data` block of an archive result.
fn payload(outcome: &ArchiveOutcome) -> serde_json::Value {
    serde_json::json!({
        "id": outcome.item.id.as_str(),
        "path": outcome.path.to_string(),
        "archived_at": outcome.archived_at.to_string(),
        "archive_reason": outcome.archive_reason,
        "status": outcome.item.status.as_str(),
        "closed_at": outcome.item.closed_at.map(|value| value.to_string()),
        "written": outcome.written,
        "removals": outcome.removals.iter().map(removal_json).collect::<Vec<_>>(),
        "affected": outcome
            .affected
            .iter()
            .map(|item| serde_json::json!({
                "id": item.id.as_str(),
                "path": item.path.to_string(),
            }))
            .collect::<Vec<_>>(),
        "item": waymark_core::view::item_fields(&outcome.item),
    })
}

fn removal_json(removal: &PlannedRemoval) -> serde_json::Value {
    serde_json::json!({
        "from": removal.from.as_str(),
        "relation": removal.relation.as_str(),
    })
}
