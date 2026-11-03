//! `waymark move`: relocate one item to another category.
//!
//! The rule — destination path from the target category, persisted `category`
//! rewrite, destination refusal, archived and migration-pending refusal — lives in
//! `waymark_core::ops::r#move`. This module parses the category key, which is the
//! one thing the domain signature takes already validated, and renders the result.

use clap::Args;
use waymark_core::envelope::success_envelope;
use waymark_core::error::{ValidationIssue, WaymarkError};
use waymark_core::ops::r#move::{self, MoveOutcome};
use waymark_core::ops::OpOptions;
use waymark_core::{CategoryKey, Project};

use super::{emit, err_line, item_json};

/// Arguments of `waymark move`.
#[derive(Debug, Args)]
pub struct MoveArgs {
    /// Stable ID of the item.
    #[arg(value_name = "ID")]
    pub id: String,

    /// Category key to move the item to; must already be declared.
    #[arg(value_name = "CATEGORY")]
    pub category: String,

    /// Report the destination and the change without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// Run `waymark move`.
pub fn run(
    project: &Project,
    args: MoveArgs,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    let category = CategoryKey::parse(args.category.trim()).map_err(|error| {
        WaymarkError::validation(
            "move",
            vec![ValidationIssue::new("category", error.to_string())],
        )
        .at_field("category")
    })?;
    let options = if args.dry_run {
        options.clone().dry_run()
    } else {
        options.clone()
    };
    let outcome = r#move::move_item(project, &args.id, &category, &options)?;

    if json {
        emit(&success_envelope(payload(&outcome), outcome.meta.clone()))?;
        return Ok(());
    }

    if outcome.changed {
        let prefix = if outcome.written { "" } else { "would be " };
        err_line(&format!(
            "{prefix}moved {} {} -> {} ({} -> {})",
            outcome.item.id,
            outcome.previous_category,
            outcome.item.category,
            outcome.previous_path,
            outcome.path
        ));
    } else {
        err_line(&format!(
            "unchanged {} (category {})",
            outcome.item.id, outcome.item.category
        ));
    }
    for warning in &outcome.meta.warnings {
        err_line(&format!("  {}: {}", warning.code, warning.message));
    }
    Ok(())
}

/// The `data` block of a move result.
fn payload(outcome: &MoveOutcome) -> serde_json::Value {
    serde_json::json!({
        "id": outcome.item.id.as_str(),
        "path": outcome.path.to_string(),
        "previous_path": outcome.previous_path.to_string(),
        "previous_category": outcome.previous_category.as_str(),
        "category": outcome.item.category.as_str(),
        "changed": outcome.changed,
        "written": outcome.written,
        "item": item_json(&outcome.item, &outcome.path),
    })
}
