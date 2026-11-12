//! `waymark archive`: retire one item without deleting it.
//!
//! The rules — the marker, the child refusal, the consent-gated inbound cleanup
//! batch, the archived-write rejection — all live in
//! `waymark_core::ops::archive`. This module parses the flags and renders the
//! result, including every planned relationship removal and every document the
//! archive left untouched, on a dry run as well as a real one.

use clap::Args;
use waymark_core::envelope::success_envelope;
use waymark_core::error::WaymarkError;
use waymark_core::ops::archive;
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

    /// Consent to a cross-item write: also clean up inbound references from
    /// other items you own.
    #[arg(long)]
    pub cascade: bool,
}

/// Run `waymark archive`.
pub fn run(
    project: &Project,
    args: &ArchiveArgs,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    let mut options = options.clone();
    if args.dry_run {
        options = options.dry_run();
    }
    if args.cascade {
        options.cascade = true;
    }
    let outcome = archive::archive(project, &args.id, args.reason.as_deref(), &options)?;

    if json {
        emit(&success_envelope(
            waymark_core::view::archive_result_json(&outcome),
            outcome.meta.clone(),
        ))?;
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
    if let Some(summary) =
        waymark_core::view::archive_leftovers_summary(&outcome.item.id, outcome.untouched.len())
    {
        err_line(&format!("  {summary}"));
        for item in &outcome.untouched {
            err_line(&format!(
                "  {}  left for its owner ({})",
                item.id,
                relations_phrase(&outcome.untouched_relations_for(&item.id))
            ));
        }
    }
    for warning in &outcome.meta.warnings {
        err_line(&format!("  {}: {}", warning.code, warning.message));
    }
    Ok(())
}

/// The relation names of one leftover document, joined for a terminal line.
fn relations_phrase(relations: &[archive::InboundRelation]) -> String {
    relations
        .iter()
        .map(|relation| relation.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}
