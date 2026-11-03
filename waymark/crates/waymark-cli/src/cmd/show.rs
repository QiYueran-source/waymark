//! `waymark show`: read one work item by stable ID.

use clap::Args;
use waymark_core::envelope::success_envelope;
use waymark_core::error::WaymarkError;
use waymark_core::ops::read;
use waymark_core::Project;

use super::{emit, err_line, item_json, render_field_value};

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

/// Run `waymark show`.
pub fn run(project: &Project, args: &ShowArgs, json: bool) -> Result<(), WaymarkError> {
    let read = read::read(project, &args.id)?;

    if json {
        emit(&success_envelope(
            serde_json::json!({
                "id": read.item.id.as_str(),
                "path": read.path.to_string(),
                "category": read.category.as_str(),
                "fingerprint": read.fingerprint.as_str(),
                "item": item_json(&read.item, &read.path),
                "relationships": {
                    "parent": read.item.parent.as_ref().map(|id| id.as_str().to_owned()),
                    "dependencies": read.item.dependencies.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
                    "soft_links": read.item.soft_links.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
                },
            }),
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
        if let Some(creator) = &read.item.creator {
            err_line(&format!("  creator     {creator}"));
        }
        err_line(&format!("  created     {}", read.item.created_at));
        err_line(&format!("  updated     {}", read.item.updated_at));
        if let Some(closed_at) = read.item.closed_at {
            err_line(&format!("  closed      {closed_at}"));
        }
        if let Some(parent) = &read.item.parent {
            err_line(&format!("  parent      {parent}"));
        }
        if !read.item.dependencies.is_empty() {
            let ids: Vec<&str> = read
                .item
                .dependencies
                .iter()
                .map(|id| id.as_str())
                .collect();
            err_line(&format!("  depends on  {}", ids.join(", ")));
        }
        if !read.item.soft_links.is_empty() {
            let ids: Vec<&str> = read.item.soft_links.iter().map(|id| id.as_str()).collect();
            err_line(&format!("  soft links  {}", ids.join(", ")));
        }
        for (key, value) in &read.item.custom_fields {
            err_line(&format!("  {key:<11} {}", render_field_value(value)));
        }
        if let Some(archived_at) = read.item.archived_at {
            err_line(&format!("  archived    {archived_at}"));
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
