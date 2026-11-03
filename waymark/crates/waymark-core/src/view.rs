//! Shared JSON renderers for the machine forms adapters publish.
//!
//! The CLI and MCP adapters render the same domain values: one item, one tree
//! node, one flat dependency row, one configuration document, one collection
//! page, and one mediated-write result. Those payloads are part of the contract
//! (the CLI tests pin them byte for byte), so they must not be re-derived in
//! two places where one can drift from the other.
//!
//! This module is the single dependency-free home for that rendering. It only
//! reads the domain types and returns [`serde_json::Value`]; adapters decide
//! where to print, keep their own human-readable prose, and wrap these values in
//! their envelope. Nothing here reads a file, takes a lock, or decides whether
//! a write is allowed.
//!
//! # The shapes
//!
//! - [`item_fields`] — persisted fields, no path, no derived columns.
//! - [`item_json`] — [`item_fields`] plus the document path.
//! - [`tree_node_json`] — one nested tree node; the embedded item has no body.
//! - [`dependency_row_json`] — one flat dependency row.
//! - [`row_json`] — one projected list row (delegates to
//!   [`query::projected_row`](crate::query::projected_row)).
//! - [`read_json`] — one scoped item read (`show`).
//! - [`config_json`] — one configuration document.
//! - [`listing_summary_json`] — the window counts of a collection page.
//! - [`listing_json`] — the complete collection page (summary, rows, projection).
//! - [`write_result_json`] / [`write_result_json_with`] — the shared
//!   `{id, path, …, changed, written, item}` core of every write result.

use camino::Utf8Path;
use serde_json::{json, Map, Value};

use crate::config::Config;
use crate::model::WorkItem;
use crate::ops::list::Listing;
use crate::ops::tree::{DependencyRow, TreeNode};
use crate::projection::ScopedRead;
use crate::query::{ArchiveRange, ListedItem, Projection};

/// The persisted fields of one item, without the file that happens to hold it.
///
/// `show` names the document; a page of many items does not, because the path is
/// a fact about one file and a listing is not about files.
pub fn item_fields(item: &WorkItem) -> Value {
    json!({
        "schema": item.schema,
        "needs_migration": item.needs_migration(),
        "id": item.id.as_str(),
        "title": item.title,
        "category": item.category.as_str(),
        "status": item.status.as_str(),
        "owner": item.owner,
        "creator": item.creator,
        "created_at": item.created_at.to_string(),
        "updated_at": item.updated_at.to_string(),
        "closed_at": item.closed_at.map(|value| value.to_string()),
        "parent": item.parent.as_ref().map(|id| id.as_str().to_owned()),
        "dependencies": item.dependencies.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        "soft_links": item.soft_links.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        "custom_fields": item.custom_fields,
        "archived_at": item.archived_at.map(|value| value.to_string()),
        "archive_reason": item.archive_reason,
        "body": item.body,
    })
}

/// The persisted fields plus the document path, for a read that names one file.
pub fn item_json(item: &WorkItem, path: &Utf8Path) -> Value {
    let mut value = item_fields(item);
    if let Some(map) = value.as_object_mut() {
        map.insert("path".to_owned(), json!(path.to_string()));
    }
    value
}

/// The nested machine form of one tree node.
///
/// The embedded item keeps every persisted field except the body: a tree is a
/// relationship view, not a document view.
pub fn tree_node_json(node: &TreeNode) -> Value {
    json!({
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
        "children": node.children.iter().map(tree_node_json).collect::<Vec<_>>(),
    })
}

/// The machine form of one flat dependency row.
pub fn dependency_row_json(row: &DependencyRow) -> Value {
    json!({
        "id": row.id.as_str(),
        "depth": row.depth,
        "from": row.from.as_ref().map(|id| id.as_str()),
        "repeated": row.repeated,
        "resolved": row.item.is_some(),
    })
}

/// One projected list row: persisted fields under `item`, derived columns at the
/// top level.
///
/// This is the same value [`query::projected_row`](crate::query::projected_row)
/// returns; it lives here so an adapter has one import for every machine form.
pub fn row_json(row: &ListedItem, projection: &Projection) -> Value {
    crate::query::projected_row(row, projection)
}

/// The machine form of one configuration document.
pub fn config_json(config: &Config) -> Value {
    json!({
        "version": config.version.to_string(),
        "generation": config.version.generation,
        "revision": config.version.revision,
        "storage_root": config.storage_root,
        "operator": config.operator,
        "timezone": config.timezone.name(),
        "categories": config
            .categories
            .iter()
            .map(|(key, category)| {
                (
                    key.as_str().to_owned(),
                    json!({
                        "key": key.as_str(),
                        "path": category.path,
                        "name": category.name,
                    }),
                )
            })
            .collect::<Map<String, Value>>(),
        "fields": config
            .fields
            .iter()
            .map(|(key, declaration)| {
                (
                    key.as_str().to_owned(),
                    json!({
                        "key": key.as_str(),
                        "type": declaration.ty.as_str(),
                        "required": declaration.required,
                        "default": declaration.default,
                    }),
                )
            })
            .collect::<Map<String, Value>>(),
        "cache": {"enabled": config.cache.enabled, "capacity": config.cache.capacity},
    })
}

/// The window counts of one collection page.
///
/// Counts only: the caller merges these into the page it is building, or uses
/// [`listing_json`] for the complete document the CLI and MCP return.
pub fn listing_summary_json(listing: &Listing) -> Value {
    json!({
        "count": listing.len(),
        "total": listing.total,
        "offset": listing.offset,
        "limit": listing.limit,
    })
}

/// The complete machine form of one collection page.
///
/// This is the CLI's `list`/`ready`/`blocked` data block and the MCP listing
/// data block: the summary counts, the projected rows, the archive slice that
/// was read, and the projection the rows were rendered through.
pub fn listing_json(listing: &Listing, projection: &Projection, archive: ArchiveRange) -> Value {
    let mut value = listing_summary_json(listing);
    if let Some(map) = value.as_object_mut() {
        map.insert(
            "items".to_owned(),
            Value::Array(
                listing
                    .items
                    .iter()
                    .map(|row| row_json(row, projection))
                    .collect(),
            ),
        );
        map.insert("archive".to_owned(), json!(archive.as_str()));
        map.insert(
            "fields".to_owned(),
            match &projection.fields {
                Some(fields) => json!(fields
                    .iter()
                    .map(|field| field.as_str())
                    .collect::<Vec<_>>()),
                None => Value::Null,
            },
        );
        map.insert("compact".to_owned(), json!(projection.compact));
    }
    value
}

/// The machine form of one scoped item read, matching `show --json`.
pub fn read_json(read: &ScopedRead) -> Value {
    json!({
        "id": read.item.id.as_str(),
        "path": read.path.to_string(),
        "category": read.category.as_str(),
        "fingerprint": read.fingerprint.as_str(),
        "item": item_json(&read.item, &read.path),
        "needs_migration": read.item.needs_migration(),
        "archived": read.item.is_archived(),
        "relationships": {
            // An archived item's own edges are history: they no longer
            // participate in the active graph, so the payload says so rather
            // than letting a reader mistake them for live ones.
            "historical": read.item.is_archived(),
            "parent": read.item.parent.as_ref().map(|id| id.as_str().to_owned()),
            "dependencies": read.item.dependencies.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
            "soft_links": read.item.soft_links.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
            "children": read.children.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
            "dependents": read.dependents.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        },
        "ready": read.readiness.as_ref().map(|readiness| readiness.ready),
        "blocked": read.readiness.as_ref().map(|readiness| readiness.blocked),
        "blocked_by": read
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
    })
}

/// The core of every mediated-write result: `{id, path, changed, written, item}`.
///
/// An adapter adds the fields its operation owns (`previous_status`, `verb`,
/// `fingerprints`, …) with [`write_result_json_with`]. `written` is the
/// caller's fact, not something this function can recompute; the domain
/// constructors are what keep it equal to the presence of a fingerprint.
pub fn write_result_json(item: &WorkItem, path: &Utf8Path, changed: bool, written: bool) -> Value {
    write_result_json_with(item, path, changed, written, Map::new())
}

/// [`write_result_json`] plus adapter-specific fields.
///
/// `extra` is merged first, so an extra key can never replace the shared
/// `id`, `path`, `changed`, `written`, or `item` value.
pub fn write_result_json_with(
    item: &WorkItem,
    path: &Utf8Path,
    changed: bool,
    written: bool,
    extra: Map<String, Value>,
) -> Value {
    let mut value = extra;
    value.insert("id".to_owned(), json!(item.id.as_str()));
    value.insert("path".to_owned(), json!(path.to_string()));
    value.insert("changed".to_owned(), json!(changed));
    value.insert("written".to_owned(), json!(written));
    value.insert("item".to_owned(), item_json(item, path));
    Value::Object(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Category, FieldDecl, FieldType};
    use crate::envelope::Meta;
    use crate::model::{FieldKey, FieldValue};
    use crate::ops::tree::TreeNode;
    use crate::query::FieldName;
    use crate::readiness::Readiness;
    use crate::store::Fingerprint;
    use crate::testing::fixtures;
    use crate::WorkItemId;

    fn id(literal: &str) -> WorkItemId {
        WorkItemId::parse(literal).expect("a valid stable ID")
    }

    fn item() -> WorkItem {
        let mut item = fixtures::work_item("wm-000000000001", "task", "Fix the thing")
            .with_body("details\n")
            .build();
        item.creator = Some("alice".to_owned());
        item.parent = Some(id("wm-000000000002"));
        item.dependencies = vec![id("wm-000000000003")];
        item.soft_links = vec![id("wm-000000000004")];
        item.custom_fields
            .insert("effort".to_owned(), FieldValue::Integer(3));
        item
    }

    fn listed() -> ListedItem {
        ListedItem {
            item: item(),
            readiness: Readiness {
                ready: false,
                blocked: true,
                blocked_by: vec![id("wm-000000000003")],
            },
            children: vec![id("wm-000000000005")],
            dependents: vec![],
            needs_migration: false,
        }
    }

    #[test]
    fn item_fields_has_the_established_persisted_shape() {
        let value = item_fields(&item());
        let object = value.as_object().expect("an object");
        assert_eq!(object.len(), 18, "one field per persisted column + body");
        for key in [
            "schema",
            "needs_migration",
            "id",
            "title",
            "category",
            "status",
            "owner",
            "creator",
            "created_at",
            "updated_at",
            "closed_at",
            "parent",
            "dependencies",
            "soft_links",
            "custom_fields",
            "archived_at",
            "archive_reason",
            "body",
        ] {
            assert!(object.contains_key(key), "missing {key}: {value}");
        }
        assert_eq!(value["schema"], json!(1));
        assert_eq!(value["needs_migration"], json!(false));
        assert_eq!(value["id"], json!("wm-000000000001"));
        assert_eq!(value["title"], json!("Fix the thing"));
        assert_eq!(value["category"], json!("task"));
        assert_eq!(value["status"], json!("open"));
        assert_eq!(value["owner"], json!("frank"));
        assert_eq!(value["creator"], json!("alice"));
        assert_eq!(value["parent"], json!("wm-000000000002"));
        assert_eq!(value["dependencies"], json!(["wm-000000000003"]));
        assert_eq!(value["soft_links"], json!(["wm-000000000004"]));
        assert_eq!(value["custom_fields"], json!({"effort": 3}));
        assert_eq!(value["closed_at"], Value::Null);
        assert_eq!(value["body"], json!("details\n"));
    }

    #[test]
    fn item_json_adds_the_path_to_the_persisted_fields() {
        let path = Utf8Path::new("work/task/wm-000000000001.md");
        let value = item_json(&item(), path);
        assert_eq!(value["path"], json!("work/task/wm-000000000001.md"));
        assert_eq!(value["body"], json!("details\n"));
        let mut without_path = item_fields(&item());
        without_path.as_object_mut().unwrap().remove("body");
        let mut with_path = value;
        with_path.as_object_mut().unwrap().remove("path");
        with_path.as_object_mut().unwrap().remove("body");
        assert_eq!(
            with_path, without_path,
            "item_json differs from item_fields only by path"
        );
    }

    #[test]
    fn tree_node_json_keeps_the_nested_shape_and_drops_the_body() {
        let node = TreeNode {
            id: id("wm-000000000001"),
            item: Some(item()),
            depth: 0,
            repeated: false,
            children: vec![TreeNode {
                id: id("wm-000000000009"),
                item: None,
                depth: 1,
                repeated: true,
                children: Vec::new(),
            }],
        };
        let value = tree_node_json(&node);
        assert_eq!(value["id"], json!("wm-000000000001"));
        assert_eq!(value["depth"], json!(0));
        assert_eq!(value["repeated"], json!(false));
        assert_eq!(value["resolved"], json!(true));
        assert_eq!(value["item"]["title"], json!("Fix the thing"));
        assert!(
            value["item"].get("body").is_none(),
            "a tree node never carries the body"
        );
        assert_eq!(value["children"][0]["id"], json!("wm-000000000009"));
        assert_eq!(value["children"][0]["depth"], json!(1));
        assert_eq!(value["children"][0]["repeated"], json!(true));
        assert_eq!(value["children"][0]["resolved"], json!(false));
        assert_eq!(value["children"][0]["item"], Value::Null);
        assert_eq!(value["children"][0]["children"], json!([]));
    }

    #[test]
    fn dependency_row_json_is_the_flat_shape() {
        let row = DependencyRow {
            id: id("wm-000000000003"),
            item: Some(item()),
            depth: 2,
            from: Some(id("wm-000000000001")),
            repeated: true,
        };
        let value = dependency_row_json(&row);
        assert_eq!(
            value,
            json!({
                "id": "wm-000000000003",
                "depth": 2,
                "from": "wm-000000000001",
                "repeated": true,
                "resolved": true,
            })
        );

        let missing = DependencyRow {
            id: id("wm-000000000009"),
            item: None,
            depth: 0,
            from: None,
            repeated: false,
        };
        let value = dependency_row_json(&missing);
        assert_eq!(value["from"], Value::Null);
        assert_eq!(value["resolved"], json!(false));
        assert!(value.get("item").is_none(), "a flat row carries no item");
    }

    #[test]
    fn row_json_is_the_projected_list_row() {
        let listed = listed();
        let all = Projection::all();
        assert_eq!(
            row_json(&listed, &all),
            crate::query::projected_row(&listed, &all)
        );
        let value = row_json(&listed, &all);
        assert_eq!(value["id"], json!("wm-000000000001"));
        assert_eq!(value["status"], json!("open"));
        assert_eq!(value["item"]["title"], json!("Fix the thing"));
        assert_eq!(value["ready"], json!(false));
        assert_eq!(value["blocked"], json!(true));
        assert_eq!(value["blocked_by"], json!(["wm-000000000003"]));
        assert!(
            value["item"].get("body").is_none(),
            "a listing row never carries the body"
        );

        // The compact projection keeps only the identity columns plus the
        // always-present derived readiness columns that are non-null.
        let compact = row_json(&listed, &Projection::compact());
        assert_eq!(compact["item"]["title"], json!("Fix the thing"));
        assert!(compact.get("children").is_none());

        // An explicit projection names exactly the columns it includes.
        let named = Projection::of([FieldName::Id, FieldName::Title]);
        let value = row_json(&listed, &named);
        assert_eq!(
            value["item"],
            json!({"id": "wm-000000000001", "title": "Fix the thing"})
        );
        assert!(value.get("blocked").is_none());
    }

    #[test]
    fn config_json_renders_the_document_the_adapters_publish() {
        let mut config = Config::fresh();
        config.operator = Some("frank".to_owned());
        config.categories.insert(
            crate::model::CategoryKey::parse("bug").unwrap(),
            Category {
                path: "work/bugs".to_owned(),
                name: "Bugs".to_owned(),
            },
        );
        config.fields.insert(
            FieldKey::parse("effort").unwrap(),
            FieldDecl {
                ty: FieldType::Integer,
                required: true,
                default: Some(FieldValue::Integer(3)),
            },
        );

        let value = config_json(&config);
        assert_eq!(value["version"], json!(config.version.to_string()));
        assert_eq!(value["generation"], json!(1));
        assert_eq!(value["revision"], json!(config.version.revision));
        assert_eq!(value["storage_root"], json!("work"));
        assert_eq!(value["operator"], json!("frank"));
        assert_eq!(value["timezone"], json!("UTC"));
        assert_eq!(value["categories"]["task"]["key"], json!("task"));
        assert_eq!(value["categories"]["task"]["path"], json!("work/task"));
        assert_eq!(value["categories"]["bug"]["name"], json!("Bugs"));
        assert_eq!(value["fields"]["effort"]["key"], json!("effort"));
        assert_eq!(value["fields"]["effort"]["type"], json!("integer"));
        assert_eq!(value["fields"]["effort"]["required"], json!(true));
        assert_eq!(value["fields"]["effort"]["default"], json!(3));
        assert_eq!(value["cache"]["enabled"], json!(true));
        assert_eq!(value["cache"]["capacity"], json!(config.cache.capacity));
    }

    fn listing() -> Listing {
        Listing {
            items: vec![listed()],
            total: 7,
            offset: 2,
            limit: 25,
            meta: Meta::memory_projection_complete(),
        }
    }

    #[test]
    fn listing_summary_json_counts_the_window() {
        let value = listing_summary_json(&listing());
        assert_eq!(
            value,
            json!({"count": 1, "total": 7, "offset": 2, "limit": 25})
        );
    }

    #[test]
    fn listing_json_is_the_complete_collection_page() {
        let listing = listing();
        let all = Projection::all();
        let value = listing_json(&listing, &all, ArchiveRange::Active);
        assert_eq!(value["count"], json!(1));
        assert_eq!(value["total"], json!(7));
        assert_eq!(value["offset"], json!(2));
        assert_eq!(value["limit"], json!(25));
        assert_eq!(value["archive"], json!("active"));
        assert_eq!(value["compact"], json!(false));
        assert_eq!(value["fields"], Value::Null);
        assert_eq!(value["items"].as_array().unwrap().len(), 1);
        assert_eq!(value["items"][0]["id"], json!("wm-000000000001"));

        let named = Projection::of([FieldName::Id, FieldName::Title]);
        let value = listing_json(&listing, &named, ArchiveRange::Archived);
        assert_eq!(value["archive"], json!("archived"));
        assert_eq!(value["fields"], json!(["id", "title"]));

        let empty = Listing {
            items: vec![],
            total: 0,
            offset: 0,
            limit: 0,
            meta: Meta::memory_projection_complete(),
        };
        let value = listing_json(&empty, &all, ArchiveRange::All);
        assert_eq!(value["items"], json!([]));
        assert_eq!(value["archive"], json!("all"));
    }

    #[test]
    fn read_json_is_the_scoped_show_shape() {
        let path = Utf8Path::new("work/task/wm-000000000001.md");
        let fingerprint = Fingerprint::of(b"document");
        let read = ScopedRead {
            item: item(),
            path: path.to_path_buf(),
            category: crate::model::CategoryKey::parse("task").unwrap(),
            fingerprint: fingerprint.clone(),
            readiness: None,
            children: vec![id("wm-000000000005")],
            dependents: vec![id("wm-000000000006")],
            meta: Meta::direct_file_scoped(),
        };
        let value = read_json(&read);
        assert_eq!(value["id"], json!("wm-000000000001"));
        assert_eq!(value["path"], json!("work/task/wm-000000000001.md"));
        assert_eq!(value["category"], json!("task"));
        assert_eq!(value["fingerprint"], json!(fingerprint.as_str()));
        assert_eq!(value["item"]["body"], json!("details\n"));
        assert_eq!(value["needs_migration"], json!(false));
        assert_eq!(value["archived"], json!(false));
        assert_eq!(value["relationships"]["historical"], json!(false));
        assert_eq!(value["relationships"]["parent"], json!("wm-000000000002"));
        assert_eq!(
            value["relationships"]["children"],
            json!(["wm-000000000005"])
        );
        assert_eq!(
            value["relationships"]["dependents"],
            json!(["wm-000000000006"])
        );
        assert_eq!(value["ready"], Value::Null);
        assert_eq!(value["blocked"], Value::Null);
        assert_eq!(value["blocked_by"], json!([]));
    }

    #[test]
    fn write_result_json_is_the_shared_write_core_plus_extras() {
        let path = Utf8Path::new("work/task/wm-000000000001.md");
        let value = write_result_json(&item(), path, true, false);
        assert_eq!(value["id"], json!("wm-000000000001"));
        assert_eq!(value["path"], json!("work/task/wm-000000000001.md"));
        assert_eq!(value["changed"], json!(true));
        assert_eq!(value["written"], json!(false));
        assert_eq!(value["item"]["title"], json!("Fix the thing"));

        let mut extra = Map::new();
        extra.insert("previous_status".to_owned(), json!("open"));
        extra.insert("written".to_owned(), json!(true));
        let value = write_result_json_with(&item(), path, true, false, extra);
        assert_eq!(value["previous_status"], json!("open"));
        assert_eq!(
            value["written"],
            json!(false),
            "an extra key never overrides the shared written flag"
        );
        assert_eq!(value["changed"], json!(true));
    }
}
