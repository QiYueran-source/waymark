//! Every dry-run-capable write reports without touching a byte.
//!
//! The project is warmed (the lock file exists) before the first snapshot, so
//! "nothing written" can be a strict byte-for-byte comparison: the lock file is
//! the operation's mutual-exclusion artifact, not business data, and taking the
//! lock again changes nothing.

mod common;

use serde_json::json;
use waymark_core::config::FieldType;
use waymark_core::model::FieldKey;
use waymark_core::ops::{config, edit, link, parent, relationship};
use waymark_core::FieldValue;

#[tokio::test]
async fn every_dry_run_tool_leaves_the_project_byte_identical() {
    let fixture = common::fixture(&["one", "two", "three"]);
    let id0 = fixture.ids[0].clone();
    let id1 = fixture.ids[1].clone();
    let id2 = fixture.ids[2].clone();
    let project = fixture.project();

    // A state in which every dry-run request is valid: categories, a declared
    // field with a value, a parent, a dependency, and a soft link.
    let mut project = project;
    for (key, path) in [("bug", "work/bugs"), ("chore", "work/chore")] {
        config::add_category(
            &project,
            &waymark_core::CategoryKey::parse(key).unwrap(),
            path,
            None,
            &common::options(),
        )
        .expect("the category is declared");
    }
    project = fixture.project();
    for (key, ty) in [("note", FieldType::String), ("points", FieldType::Integer)] {
        config::add_field(
            &project,
            &FieldKey::parse(key).unwrap(),
            ty,
            false,
            None,
            &common::options(),
        )
        .expect("the field is declared");
    }
    let project = fixture.project();
    edit::update_custom_fields(
        &project,
        &id0,
        &[edit::FieldChangeEntry::set(
            "note",
            FieldValue::String("hello".to_owned()),
        )],
        &common::options(),
    )
    .expect("the field value is written");
    parent::set_parent(&project, &id1, &id0, &common::options()).expect("the parent is set");
    relationship::add_dependency(&project, &id1, &id0, &common::options())
        .expect("the dependency is added");
    link::add_soft_link(&project, &id0, &id1, &common::options()).expect("the link is added");

    common::warm_lock(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;

    let calls: Vec<(&str, serde_json::Value)> = vec![
        (
            "waymark_config_set",
            json!({"name": "cache.enabled", "value": "false", "dry_run": true}),
        ),
        (
            "waymark_config_add_category",
            json!({"key": "feature", "path": "work/feature", "dry_run": true}),
        ),
        (
            "waymark_config_update_category",
            json!({"key": "bug", "name": "Defects", "dry_run": true}),
        ),
        (
            "waymark_config_remove_category",
            json!({"key": "chore", "dry_run": true}),
        ),
        (
            "waymark_config_add_field",
            json!({"key": "extra", "type": "string", "dry_run": true}),
        ),
        (
            "waymark_config_update_field",
            json!({"key": "points", "attribute": "default", "value": "9", "dry_run": true}),
        ),
        (
            "waymark_migrate_version",
            json!({
                "target_generation": waymark_core::model::FORMAT_GENERATION,
                "dry_run": true
            }),
        ),
        (
            "waymark_migrate_schema",
            json!({"action": {"kind": "remove_field", "key": "note"}, "dry_run": true}),
        ),
        (
            "waymark_migrate_storage",
            json!({
                "action": {"kind": "category_path", "key": "bug", "path": "work/bugs2"},
                "dry_run": true
            }),
        ),
        (
            "waymark_create_work_item",
            json!({
                "title": "Would be created",
                "category": "task",
                "owner": "frank",
                "dry_run": true
            }),
        ),
        (
            "waymark_set_title",
            json!({"id": id0, "title": "Would be renamed", "dry_run": true}),
        ),
        (
            "waymark_replace_body",
            json!({"id": id0, "body": "would be replaced", "dry_run": true}),
        ),
        (
            "waymark_update_custom_fields",
            json!({"id": id0, "custom_fields": {"set": {"points": 11}}, "dry_run": true}),
        ),
        (
            "waymark_set_status",
            json!({"id": id0, "status": "closed", "dry_run": true}),
        ),
        (
            "waymark_transfer_owner",
            json!({"id": id0, "owner": "carol", "dry_run": true}),
        ),
        (
            "waymark_move_work_item",
            json!({"id": id0, "category": "bug", "dry_run": true}),
        ),
        (
            "waymark_set_parent",
            json!({"id": id2, "parent": id0, "dry_run": true}),
        ),
        ("waymark_clear_parent", json!({"id": id1, "dry_run": true})),
        (
            "waymark_add_dependency",
            json!({"id": id2, "blocking": id0, "dry_run": true}),
        ),
        (
            "waymark_remove_dependency",
            json!({"id": id1, "blocking": id0, "dry_run": true}),
        ),
        (
            "waymark_add_soft_link",
            json!({"id": id0, "other": id2, "dry_run": true}),
        ),
        (
            "waymark_remove_soft_link",
            json!({"id": id0, "other": id1, "dry_run": true}),
        ),
        (
            "waymark_archive_work_item",
            json!({"id": id2, "reason": "would retire", "dry_run": true}),
        ),
    ];

    for (tool, arguments) in calls {
        let before = common::snapshot(&fixture.root);
        let result = client.call(tool, arguments).await;
        assert_eq!(
            result.is_error,
            Some(false),
            "{tool} failed instead of reporting a dry run: {}",
            common::envelope(&result)
        );
        let envelope = common::envelope(&result);
        assert_eq!(envelope["ok"], true, "{tool}");
        if let Some(written) = envelope["data"]["written"].as_bool() {
            assert!(!written, "{tool} dry run claims to have written");
        }
        assert_eq!(
            common::snapshot(&fixture.root),
            before,
            "{tool} dry run changed the project on disk"
        );
    }

    client.shutdown().await;
}
