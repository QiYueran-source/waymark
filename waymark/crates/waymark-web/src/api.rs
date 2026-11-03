//! The read model behind the JSON API.
//!
//! Everything here is derived from one core read per request:
//!
//! - `GET /api/health` touches nothing.
//! - `GET /api/tree` loads one [`Graph`] and builds the hierarchy from it, so the
//!   tree shape and every node's readiness come from the same scan.
//! - `GET /api/item/:id` reads one document and also loads the graph for
//!   readiness and relationships. A graph that cannot be built is reported as a
//!   read failure, never as a detail with made-up readiness.
//!
//! Archived items never appear in the tree, because [`Graph::from_project`] puts
//! them aside. The item route still answers for an archived ID (the flag says
//! so); there is no archive browser and no list route.

use axum::extract::{Path, RawQuery, State};
use axum::response::Response;
use serde::Serialize;
use serde_json::{json, Value};

use waymark_core::envelope::Meta;
use waymark_core::error::WaymarkError;
use waymark_core::model::{CategoryKey, Status, WorkItem};
use waymark_core::ops::list::complete_meta;
use waymark_core::ops::read;
use waymark_core::ops::tree::{self, HierarchyTree, TreeNode, UnresolvedItem};
use waymark_core::readiness::{Graph, Readiness};
use waymark_core::store::Project;

use crate::render;
use crate::respond;
use crate::server::AppState;

/// The fixed, ordered shape vocabulary for categories.
///
/// The shape of a category is its index in the configured category order modulo
/// this list's length. Adding a shape at the end never changes earlier
/// assignments; inserting or reordering does, so the list is a contract.
pub const SHAPES: [&str; 6] = ["circle", "square", "triangle", "diamond", "hexagon", "star"];

/// One category as the API exposes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CategoryView {
    /// The stable category key.
    pub key: String,
    /// The configured human-readable name.
    pub name: String,
    /// The stable shape index into [`SHAPES`].
    pub shape: usize,
}

/// The tree view selected by `?view=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TreeView {
    Combined,
    Grouped,
}

impl TreeView {
    fn as_str(self) -> &'static str {
        match self {
            Self::Combined => "combined",
            Self::Grouped => "grouped",
        }
    }
}

/// `GET /api/health`: liveness, the read-only fact, and the project root.
///
/// `project.root` exists so the browser can namespace its local preferences by
/// the normalized project path without a second endpoint. It is a canonical
/// absolute path when the filesystem can provide one; preferences are never
/// written into the project or sent anywhere but this read.
pub(crate) async fn health(State(state): State<AppState>) -> Response {
    respond::success(
        json!({
            "status": "ok",
            "service": "waymark-web",
            "version": env!("CARGO_PKG_VERSION"),
            "read_only": true,
            "project": {
                "root": project_root(&state.project),
            },
        }),
        Meta::direct_file_scoped(),
    )
}

/// The project root as the browser should key its local preferences.
///
/// Canonicalizing resolves symlinks and `.`/`..` on a path that must already
/// exist (the project was opened), which keeps two spellings of one project
/// together. If canonicalization fails the opened root is still a usable
/// absolute path, so the answer is never missing.
fn project_root(project: &Project) -> String {
    std::fs::canonicalize(project.root())
        .ok()
        .and_then(|path| path.to_str().map(str::to_owned))
        .unwrap_or_else(|| project.root().to_string())
}

/// `GET /api/tree?view=combined|grouped`.
pub(crate) async fn tree(State(state): State<AppState>, RawQuery(raw): RawQuery) -> Response {
    let view = match tree_view(raw.as_deref()) {
        Ok(view) => view,
        Err(message) => return respond::bad_request("INVALID_QUERY", message),
    };
    match tree_data(&state.project, view) {
        Ok((data, meta)) => respond::success(data, meta),
        Err(error) => respond::domain_error(&error),
    }
}

/// `GET /api/item/:id`.
pub(crate) async fn item(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match item_data(&state.project, &id) {
        Ok(data) => respond::success(data, Meta::direct_file_scoped()),
        Err(error) => respond::domain_error(&error),
    }
}

/// Parse the only query parameter the tree accepts.
///
/// Unknown parameters are ignored (they let a caller add a cache-buster); a
/// repeated or unknown `view` is a `400`. Hand parsing keeps the rejection a JSON
/// envelope instead of axum's plain-text extractor rejection.
fn tree_view(raw: Option<&str>) -> Result<TreeView, String> {
    let Some(raw) = raw else {
        return Ok(TreeView::Combined);
    };
    let mut selected = None;
    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = percent_decode(key).ok_or("the query string is not valid UTF-8")?;
        let value = percent_decode(value).ok_or("the query string is not valid UTF-8")?;
        if key != "view" {
            continue;
        }
        if selected.is_some() {
            return Err("view is given more than once".to_owned());
        }
        selected = Some(match value.as_str() {
            "combined" => TreeView::Combined,
            "grouped" => TreeView::Grouped,
            other => return Err(format!("view must be combined or grouped, found {other:?}")),
        });
    }
    Ok(selected.unwrap_or(TreeView::Combined))
}

/// One tree read: a complete snapshot or an error with diagnostics.
fn tree_data(project: &Project, view: TreeView) -> Result<(Value, Meta), WaymarkError> {
    // One scan: the same graph answers both shape and readiness. A damaged
    // document fails the scan and therefore the request — never a partial tree.
    let graph = Graph::from_project(project)?;
    let hierarchy = tree::hierarchy_from_graph(&graph, None);
    let categories = category_views(project);
    let unresolved: Vec<Value> = hierarchy
        .unresolved
        .iter()
        .map(|item| unresolved_json(item, &categories))
        .collect();

    let data = match view {
        TreeView::Combined => json!({
            "view": view.as_str(),
            "roots": roots_json(&hierarchy, &graph, &categories),
            "unresolved": unresolved,
            "categories": categories,
        }),
        TreeView::Grouped => json!({
            "view": view.as_str(),
            "groups": groups_json(&hierarchy, &graph, &categories),
            "unresolved": unresolved,
            "categories": categories,
        }),
    };
    Ok((data, complete_meta(graph.diagnostics())))
}

fn roots_json(hierarchy: &HierarchyTree, graph: &Graph, categories: &[CategoryView]) -> Vec<Value> {
    hierarchy
        .roots
        .iter()
        .map(|node| node_json(node, graph, categories))
        .collect()
}

/// One group per configured category that actually has roots, in configured
/// (key-sorted) order. A cross-category child stays inside its parent's subtree
/// because the subtree was built before grouping.
fn groups_json(
    hierarchy: &HierarchyTree,
    graph: &Graph,
    categories: &[CategoryView],
) -> Vec<Value> {
    categories
        .iter()
        .filter_map(|category| {
            let roots: Vec<Value> = hierarchy
                .roots
                .iter()
                .filter(|node| {
                    node.item
                        .as_ref()
                        .is_some_and(|item| item.category.as_str() == category.key)
                })
                .map(|node| node_json(node, graph, categories))
                .collect();
            (!roots.is_empty()).then(|| {
                json!({
                    "category": category,
                    "roots": roots,
                })
            })
        })
        .collect()
}

fn unresolved_json(item: &UnresolvedItem, categories: &[CategoryView]) -> Value {
    json!({
        "id": item.id.as_str(),
        "title": item.item.title,
        "category": category_view(&item.item.category, categories),
        "parent": item.parent.as_str(),
        "reason": item.reason.as_str(),
    })
}

/// One hierarchy node with its derived state and relationships.
fn node_json(node: &TreeNode, graph: &Graph, categories: &[CategoryView]) -> Value {
    let Some(item) = node.item.as_ref() else {
        // The hierarchy builder only creates nodes for active IDs, so this is a
        // defensive shape rather than a reachable state.
        return json!({
            "id": node.id.as_str(),
            "title": Value::Null,
            "category": Value::Null,
            "unresolved": true,
            "children": [],
        });
    };
    let readiness = graph.readiness(&item.id);
    json!({
        "id": item.id.as_str(),
        "title": item.title,
        "category": category_view(&item.category, categories),
        "status": item.status.as_str(),
        "state": item_state(item, readiness.as_ref()),
        "ready": readiness.as_ref().is_some_and(|readiness| readiness.ready),
        "blocked": readiness.as_ref().is_some_and(|readiness| readiness.blocked),
        "blocked_reason": readiness
            .as_ref()
            .and_then(|readiness| blocked_reason(graph, readiness)),
        "parent": item.parent.as_ref().map(|parent| parent.as_str()),
        "dependencies": id_list(&item.dependencies),
        "children": node
            .children
            .iter()
            .map(|child| node_json(child, graph, categories))
            .collect::<Vec<_>>(),
    })
}

fn item_data(project: &Project, id: &str) -> Result<Value, WaymarkError> {
    let read = read::read(project, id)?;
    // Detail needs readiness and relationships, which are whole-project facts.
    let graph = Graph::from_project(project)?;
    let item = read.item;
    let readiness = graph.readiness_any(&item.id);
    let child_ids = graph.children(&item.id);
    let dependent_ids = graph.dependents(&item.id);
    let custom_fields = serde_json::to_value(&item.custom_fields).unwrap_or_else(|_| json!({}));
    Ok(json!({
        "id": item.id.as_str(),
        "title": item.title,
        "body_html": render::markdown_to_html(&item.body),
        "category": category_view(&item.category, &category_views(project)),
        "status": item.status.as_str(),
        "state": item_state(&item, readiness.as_ref()),
        "ready": readiness.as_ref().is_some_and(|readiness| readiness.ready),
        "blocked": readiness.as_ref().is_some_and(|readiness| readiness.blocked),
        "blocked_reason": readiness
            .as_ref()
            .and_then(|readiness| blocked_reason(&graph, readiness)),
        "owner": item.owner,
        "creator": item.creator,
        "created_at": item.created_at,
        "updated_at": item.updated_at,
        "closed_at": item.closed_at,
        "archived": item.is_archived(),
        "custom_fields": custom_fields,
        "relationships": {
            "parent": item.parent.as_ref().map(|parent| parent.as_str()),
            "children": id_list(&child_ids),
            "dependencies": id_list(&item.dependencies),
            "dependents": id_list(&dependent_ids),
            "soft_links": id_list(&item.soft_links),
        },
    }))
}

/// The derived, renderable state of one node.
///
/// Precedence is `closed` → `blocked` → `in_progress` → `ready`: a closed item is
/// never blocked, and a blocked in-progress item is primarily blocked (the
/// actual `status` is sent separately, so the frontend can still show
/// "in progress" as a label if it wants).
fn item_state(item: &WorkItem, readiness: Option<&Readiness>) -> &'static str {
    if item.is_archived() || item.status.is_closed() {
        "closed"
    } else if readiness.is_some_and(|readiness| readiness.blocked) {
        "blocked"
    } else if item.status == Status::InProgress {
        "in_progress"
    } else {
        "ready"
    }
}

/// Human-readable reason for a derived block, or `None` when not blocked.
fn blocked_reason(graph: &Graph, readiness: &Readiness) -> Option<String> {
    if !readiness.blocked {
        return None;
    }
    let parts: Vec<String> = readiness
        .blocked_by
        .iter()
        .map(|id| {
            let label = if graph.get(id).is_some() {
                "unfinished"
            } else if graph.is_archived(id) {
                "archived"
            } else {
                "missing"
            };
            format!("{} ({label})", id.as_str())
        })
        .collect();
    Some(format!("waiting on {}", parts.join(", ")))
}

fn id_list(ids: &[waymark_core::WorkItemId]) -> Vec<&str> {
    ids.iter().map(|id| id.as_str()).collect()
}

/// Every configured category with its stable shape, in configured order.
pub(crate) fn category_views(project: &Project) -> Vec<CategoryView> {
    project
        .config()
        .categories
        .iter()
        .enumerate()
        .map(|(index, (key, category))| CategoryView {
            key: key.as_str().to_owned(),
            name: category.name.clone(),
            shape: index % SHAPES.len(),
        })
        .collect()
}

fn category_view(key: &CategoryKey, categories: &[CategoryView]) -> Value {
    match categories
        .iter()
        .find(|category| category.key == key.as_str())
    {
        Some(category) => json!(category),
        // Defensive: a parsed document always names a configured category, but
        // an unknown key must still render legibly rather than disappear.
        None => json!({
            "key": key.as_str(),
            "name": key.as_str(),
            "shape": 0,
        }),
    }
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' => {
                let high = hex(*bytes.get(index + 1)?)?;
                let low = hex(*bytes.get(index + 2)?)?;
                decoded.push(high * 16 + low);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
