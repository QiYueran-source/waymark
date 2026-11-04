//! `waymark show`: read one work item by stable ID.

use clap::Args;
use waymark_core::envelope::success_envelope;
use waymark_core::error::WaymarkError;
use waymark_core::ops::read;
use waymark_core::readiness::Readiness;
use waymark_core::view::{display_state, DisplayState};
use waymark_core::{Project, WorkItem};

use super::colour::Palette;
use super::{emit, err_line, render_field_value};

/// Arguments of `waymark show`.
#[derive(Debug, Args)]
pub struct ShowArgs {
    /// Stable ID, for example `wm-3f7k9q2m4x8c`.
    #[arg(value_name = "ID")]
    pub id: String,

    /// Print the document's Markdown on stdout instead of a field summary.
    #[arg(long)]
    pub raw: bool,
}

/// The `state` and `blocked by` lines of a human `show`.
///
/// The shared decision lives in [`waymark_core::view::display_state`]. This
/// only maps it onto the CLI's long-standing vocabulary: an archived item is
/// `archived`, and an unblocked in-progress item still says `ready` here
/// because the lifecycle `status` line above already says `in_progress`.
fn readiness_lines(readiness: &Readiness, item: &WorkItem, colour: Palette) -> Vec<String> {
    let state = display_state(readiness, item);
    let label = match state {
        DisplayState::Closed if item.is_archived() => "archived",
        DisplayState::InProgress => "ready",
        other => other.as_str(),
    };
    let mut lines = vec![format!("  state       {}", colour.paint(state, label))];
    if !readiness.blocked_by.is_empty() {
        let ids: Vec<&str> = readiness.blocked_by.iter().map(|id| id.as_str()).collect();
        lines.push(format!("  blocked by  {}", ids.join(", ")));
    }
    lines
}

/// Run `waymark show`.
pub fn run(
    project: &Project,
    args: &ShowArgs,
    json: bool,
    colour: Palette,
) -> Result<(), WaymarkError> {
    // A scoped local read: one item plus its direct relationships. A broken
    // sibling does not make `show` fail; the answer says it is scoped and why.
    let read = read::read_scoped(project, &args.id)?;

    if json {
        emit(&success_envelope(
            waymark_core::view::read_json(&read),
            read.meta,
        ))?;
    } else if args.raw {
        // The document itself on stdout, so it can be piped or redirected.
        print!("{}", waymark_core::serialize::to_document(&read.item));
    } else {
        err_line(&format!("{}  {}", read.item.id, read.item.title));
        err_line(&format!("  category    {}", read.item.category));
        err_line(&format!("  status      {}", read.item.status.as_str()));
        err_line(&format!("  owner       {}", read.item.owner));
        if read.item.needs_migration() {
            err_line(&format!(
                "  migration   needed (generation {}; this release writes {})",
                read.item.schema,
                waymark_core::model::FORMAT_GENERATION
            ));
        }
        if let Some(creator) = &read.item.creator {
            err_line(&format!("  creator     {creator}"));
        }
        err_line(&format!("  created     {}", read.item.created_at));
        err_line(&format!("  updated     {}", read.item.updated_at));
        if let Some(closed_at) = read.item.closed_at {
            err_line(&format!("  closed      {closed_at}"));
        }
        let historical = if read.item.is_archived() {
            " (historical)"
        } else {
            ""
        };
        if let Some(parent) = &read.item.parent {
            err_line(&format!("  parent      {parent}{historical}"));
        }
        if !read.item.dependencies.is_empty() {
            let ids: Vec<&str> = read
                .item
                .dependencies
                .iter()
                .map(|id| id.as_str())
                .collect();
            err_line(&format!("  depends on  {}{historical}", ids.join(", ")));
        }
        if !read.item.soft_links.is_empty() {
            let ids: Vec<&str> = read.item.soft_links.iter().map(|id| id.as_str()).collect();
            err_line(&format!("  soft links  {}{historical}", ids.join(", ")));
        }
        if let Some(readiness) = &read.readiness {
            for line in readiness_lines(readiness, &read.item, colour) {
                err_line(&line);
            }
        } else {
            err_line("  state       unknown (the project could not be scanned)");
        }
        if !read.children.is_empty() {
            let ids: Vec<&str> = read.children.iter().map(|id| id.as_str()).collect();
            err_line(&format!("  children    {}", ids.join(", ")));
        }
        if !read.dependents.is_empty() {
            let ids: Vec<&str> = read.dependents.iter().map(|id| id.as_str()).collect();
            err_line(&format!("  dependents  {}", ids.join(", ")));
        }
        for (key, value) in &read.item.custom_fields {
            err_line(&format!("  {key:<11} {}", render_field_value(value)));
        }
        if let Some(archived_at) = read.item.archived_at {
            err_line(&format!("  archived    {archived_at}"));
        }
        for finding in &read.meta.diagnostics {
            err_line(&format!("  {}: {}", finding.code, finding.message));
        }
        err_line(&format!("  file        {}", read.path));
        if !read.item.body.is_empty() {
            err_line("");
            for line in read.item.body.lines() {
                err_line(line);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use waymark_core::testing::fixtures;
    use waymark_core::WorkItemId;

    fn blocked_item() -> (WorkItem, Readiness) {
        let blocker = WorkItemId::parse("wm-000000000002").expect("a valid stable ID");
        let mut item = fixtures::work_item("wm-000000000001", "task", "Dependent").build();
        item.dependencies = vec![blocker.clone()];
        (
            item,
            Readiness {
                ready: false,
                blocked: true,
                blocked_by: vec![blocker],
            },
        )
    }

    #[test]
    fn a_blocked_item_keeps_its_state_and_blocked_by_lines() {
        let (item, readiness) = blocked_item();
        assert_eq!(
            readiness_lines(&readiness, &item, Palette::Plain),
            vec![
                "  state       blocked".to_owned(),
                "  blocked by  wm-000000000002".to_owned(),
            ]
        );
    }
}
