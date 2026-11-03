//! `waymark tree` and `waymark dep tree`: bounded structural reads.
//!
//! Both commands are read-only. The walking, the depth semantics, and the
//! unresolved-node rules live in `waymark_core::ops::tree`; this module parses the
//! flags and renders the result. The human form indents by depth with a distinct
//! marker per edge kind — `-` for a parent edge, `->` for a dependency edge — so a
//! hierarchy line and a dependency line can never be confused on a terminal.

use clap::{Args, Subcommand};
use waymark_core::envelope::success_envelope;
use waymark_core::error::WaymarkError;
use waymark_core::ops::tree::{
    dependency_tree, hierarchy_tree, DependencyRow, DependencyTree, HierarchyTree, TreeNode,
};
use waymark_core::ops::OpOptions;
use waymark_core::Project;

use super::{emit, err_line, item_fields};

/// Arguments of `waymark tree`.
#[derive(Debug, Args)]
pub struct TreeArgs {
    /// Start at this item instead of listing the whole hierarchy.
    #[arg(long, value_name = "ID")]
    pub root: Option<String>,

    /// Stop this many levels below the start; `0` is the start alone.
    #[arg(long, value_name = "N")]
    pub depth: Option<usize>,
}

/// Arguments of `waymark dep`.
#[derive(Debug, Args)]
pub struct DepArgs {
    #[command(subcommand)]
    pub command: DepCommand,
}

/// The dependency family.
#[derive(Debug, Subcommand)]
pub enum DepCommand {
    /// Read the dependency tree of one item.
    Tree(DepTreeArgs),
}

/// Arguments of `waymark dep tree`.
#[derive(Debug, Args)]
pub struct DepTreeArgs {
    /// Stable ID of the item to start from.
    #[arg(value_name = "ID")]
    pub id: String,

    /// Stop this many levels below the start; `0` is the start alone.
    #[arg(long, value_name = "N")]
    pub depth: Option<usize>,

    /// Emit the pre-order row form instead of the nested tree.
    #[arg(long)]
    pub flat: bool,
}

/// Run `waymark tree`.
pub fn run(project: &Project, args: &TreeArgs, json: bool) -> Result<(), WaymarkError> {
    let tree = hierarchy_tree(project, args.root.as_deref(), args.depth)?;
    if json {
        emit(&success_envelope(
            serde_json::json!({
                "root": args.root,
                "depth": args.depth,
                "roots": tree.roots.iter().map(node_json).collect::<Vec<_>>(),
                "unresolved": tree.unresolved.iter().map(|entry| serde_json::json!({
                    "id": entry.id.as_str(),
                    "parent": entry.parent.as_str(),
                    "reason": entry.reason.as_str(),
                })).collect::<Vec<_>>(),
            }),
            tree.meta.clone(),
        ))?;
        return Ok(());
    }
    render_hierarchy(&tree);
    Ok(())
}

/// Run `waymark dep`.
pub fn run_dep(
    project: &Project,
    args: DepArgs,
    _options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    match args.command {
        DepCommand::Tree(args) => {
            let tree = dependency_tree(project, &args.id, args.depth, args.flat)?;
            if json {
                emit(&success_envelope(
                    serde_json::json!({
                        "id": args.id,
                        "depth": args.depth,
                        "flat": args.flat,
                        "tree": node_json(&tree.root),
                        "rows": tree.flat.iter().map(row_json).collect::<Vec<_>>(),
                    }),
                    tree.meta.clone(),
                ))?;
                return Ok(());
            }
            if args.flat {
                render_rows(&tree.flat);
            } else {
                render_dependency(&tree);
            }
            Ok(())
        }
    }
}

/// Render the hierarchy: roots flush left, children indented by parent edges.
fn render_hierarchy(tree: &HierarchyTree) {
    if tree.roots.is_empty() && tree.unresolved.is_empty() {
        err_line("no items");
        return;
    }
    for root in &tree.roots {
        render_node(root, "");
    }
    if !tree.unresolved.is_empty() {
        err_line("");
        err_line("unresolved relations:");
        for entry in &tree.unresolved {
            err_line(&format!(
                "  {} -> {} ({}) {}",
                entry.id,
                entry.parent,
                entry.reason.as_str(),
                entry.item.title
            ));
        }
    }
    render_findings(&tree.meta.diagnostics);
}

fn render_node(node: &TreeNode, indent: &str) {
    err_line(&format!("{indent}{}", describe(node)));
    for child in &node.children {
        render_node(child, &format!("{indent}  - "));
    }
}

/// Render the dependency tree with `->` edges, so it never reads as a hierarchy.
fn render_dependency(tree: &DependencyTree) {
    err_line(&describe(&tree.root));
    render_dependency_children(&tree.root, "");
    render_findings(&tree.meta.diagnostics);
}

fn render_dependency_children(node: &TreeNode, indent: &str) {
    for child in &node.children {
        err_line(&format!("{indent}  -> {}", describe(child)));
        render_dependency_children(child, &format!("{indent}  "));
    }
}

/// Render the flat dependency rows, one line each.
fn render_rows(rows: &[DependencyRow]) {
    for row in rows {
        let indent = "  ".repeat(row.depth);
        let item = row.item.as_ref();
        let label = match item {
            Some(item) => format!("{} {}", row.id, item.title),
            None => format!("{} (missing)", row.id),
        };
        let mut line = format!("{indent}{label}");
        if let Some(from) = &row.from {
            line.push_str(&format!("  <- {from}"));
        }
        if row.repeated {
            line.push_str("  (repeated)");
        }
        err_line(&line);
    }
}

/// One human line for a node.
fn describe(node: &TreeNode) -> String {
    match &node.item {
        Some(item) => format!("{} {} [{}]", node.id, item.title, item.status.as_str()),
        None => format!("{} (missing)", node.id),
    }
}

fn render_findings(findings: &[waymark_core::envelope::Finding]) {
    for finding in findings {
        err_line(&format!("  {}: {}", finding.code, finding.message));
    }
}

/// The nested machine form of one node.
fn node_json(node: &TreeNode) -> serde_json::Value {
    serde_json::json!({
        "id": node.id.as_str(),
        "depth": node.depth,
        "repeated": node.repeated,
        "resolved": node.item.is_some(),
        "item": node.item.as_ref().map(|item| {
            let mut fields = item_fields(item);
            if let Some(map) = fields.as_object_mut() {
                map.remove("body");
            }
            fields
        }),
        "children": node.children.iter().map(node_json).collect::<Vec<_>>(),
    })
}

/// The machine form of one flat row.
fn row_json(row: &DependencyRow) -> serde_json::Value {
    serde_json::json!({
        "id": row.id.as_str(),
        "depth": row.depth,
        "from": row.from.as_ref().map(|id| id.as_str()),
        "repeated": row.repeated,
        "resolved": row.item.is_some(),
    })
}
