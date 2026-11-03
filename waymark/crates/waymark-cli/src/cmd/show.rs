//! `waymark show`: read one work item by stable ID.

use clap::Args;
use waymark_core::envelope::{success_envelope, Meta};
use waymark_core::error::WaymarkError;
use waymark_core::ops::read;
use waymark_core::readiness::Graph;
use waymark_core::{Project, Readiness};

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

/// The derived columns `show` adds on top of the document itself.
///
/// They come from a whole-project graph, because readiness is a fact about the
/// project and not about one file. A project that cannot be scanned completely
/// (a sibling document that does not parse, a duplicate ID) must not turn `show`
/// — a direct-file read — into a failure: the item is still readable, so the
/// derived columns are reported as unknown and the scan failure becomes a
/// diagnostic on the result.
struct Derived {
    readiness: Option<Readiness>,
    children: Vec<waymark_core::WorkItemId>,
    dependents: Vec<waymark_core::WorkItemId>,
    diagnostics: Vec<waymark_core::envelope::Finding>,
}

fn derive(project: &Project, id: &waymark_core::WorkItemId, meta: &mut Meta) -> Derived {
    match Graph::from_project(project) {
        Ok(graph) => {
            let diagnostics = graph.diagnostics();
            meta.diagnostics = diagnostics.clone();
            Derived {
                readiness: graph.readiness_any(id),
                children: graph.children(id),
                dependents: graph.dependents(id),
                diagnostics,
            }
        }
        Err(error) => {
            let finding = waymark_core::envelope::Finding::new(
                waymark_core::error::Code::IncompleteProjection,
                format!(
                    "readiness is unknown because the project could not be scanned: {}",
                    error.message()
                ),
            );
            meta.diagnostics = vec![finding.clone()];
            Derived {
                readiness: None,
                children: Vec::new(),
                dependents: Vec::new(),
                diagnostics: vec![finding],
            }
        }
    }
}

/// Run `waymark show`.
pub fn run(project: &Project, args: &ShowArgs, json: bool) -> Result<(), WaymarkError> {
    let mut read = read::read(project, &args.id)?;
    let derived = derive(project, &read.item.id, &mut read.meta);

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
                    "children": derived.children.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
                    "dependents": derived.dependents.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
                },
                "ready": derived.readiness.as_ref().map(|readiness| readiness.ready),
                "blocked": derived.readiness.as_ref().map(|readiness| readiness.blocked),
                "blocked_by": derived
                    .readiness
                    .as_ref()
                    .map(|readiness| {
                        readiness
                            .blocked_by
                            .iter()
                            .map(|id| id.as_str())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
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
        if let Some(readiness) = &derived.readiness {
            err_line(&format!(
                "  state       {}",
                if readiness.blocked {
                    "blocked"
                } else if readiness.ready {
                    "ready"
                } else if read.item.is_archived() {
                    "archived"
                } else {
                    "closed"
                }
            ));
            if !readiness.blocked_by.is_empty() {
                let ids: Vec<&str> = readiness.blocked_by.iter().map(|id| id.as_str()).collect();
                err_line(&format!("  blocked by  {}", ids.join(", ")));
            }
        } else {
            err_line("  state       unknown (the project could not be scanned)");
        }
        if !derived.children.is_empty() {
            let ids: Vec<&str> = derived.children.iter().map(|id| id.as_str()).collect();
            err_line(&format!("  children    {}", ids.join(", ")));
        }
        if !derived.dependents.is_empty() {
            let ids: Vec<&str> = derived.dependents.iter().map(|id| id.as_str()).collect();
            err_line(&format!("  dependents  {}", ids.join(", ")));
        }
        for (key, value) in &read.item.custom_fields {
            err_line(&format!("  {key:<11} {}", render_field_value(value)));
        }
        if let Some(archived_at) = read.item.archived_at {
            err_line(&format!("  archived    {archived_at}"));
        }
        for finding in &derived.diagnostics {
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
