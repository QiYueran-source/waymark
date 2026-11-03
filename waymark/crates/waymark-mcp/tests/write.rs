//! Work-item write tools over the in-process MCP client.

mod common;

use serde_json::json;

#[tokio::test]
async fn create_work_item_mints_identity_and_writes_the_document() {
    let fixture = common::fixture(&["one"]);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_create_work_item",
            json!({
                "title": "New item",
                "category": "task",
                "owner": "frank",
                "body": "the body\n"
            }),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["written"], true);
    let id = envelope["data"]["id"].as_str().expect("the new id");
    let path = envelope["data"]["path"].as_str().expect("the new path");
    assert!(id.starts_with("wm-"));
    assert!(path.ends_with(&format!("{id}.md")));
    assert_eq!(envelope["data"]["item"]["title"], "New item");
    assert_eq!(envelope["data"]["item"]["category"], "task");
    assert_eq!(envelope["data"]["item"]["owner"], "frank");
    assert_eq!(envelope["data"]["item"]["status"], "open");
    assert_eq!(envelope["data"]["item"]["body"], "the body\n");
    assert_eq!(envelope["data"]["item"]["schema"], 1);
    assert_eq!(envelope["_meta"]["source"], "direct_file");
    assert!(std::path::Path::new(path).is_file());

    // The document on disk is the created item.
    let document = common::read(path.into());
    assert!(document.contains("title: New item"));
    assert!(document.contains("owner: frank"));
    assert!(document.contains("the body"));

    client.shutdown().await;
}

#[tokio::test]
async fn create_dry_run_reports_the_item_without_writing() {
    let fixture = common::fixture(&["one"]);
    let client = common::Client::connect(fixture.root.clone()).await;
    let before = common::snapshot(&fixture.root);

    let result = client
        .call(
            "waymark_create_work_item",
            json!({
                "title": "Would be",
                "category": "task",
                "owner": "frank",
                "dry_run": true
            }),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["written"], false);
    assert!(envelope["data"]["id"].is_string());
    assert_eq!(
        common::snapshot(&fixture.root),
        before,
        "nothing was written"
    );

    client.shutdown().await;
}

#[tokio::test]
async fn set_title_replaces_the_title_and_a_repeat_is_a_no_op() {
    let fixture = common::fixture(&["one"]);
    let id = fixture.ids[0].clone();
    let path = fixture.root.join("work/task").join(format!("{id}.md"));
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_set_title",
            json!({"id": id, "title": "  Renamed  "}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["item"]["title"], "Renamed");
    assert_eq!(envelope["data"]["changed"], true);
    assert_eq!(envelope["data"]["written"], true);
    assert!(common::read(&path).contains("title: Renamed"));

    let result = client
        .call("waymark_set_title", json!({"id": id, "title": "Renamed"}))
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["changed"], false);
    assert_eq!(envelope["data"]["written"], false);

    client.shutdown().await;
}

#[tokio::test]
async fn replace_body_canonicalizes_and_can_clear() {
    let fixture = common::fixture(&["one"]);
    let id = fixture.ids[0].clone();
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_replace_body",
            json!({"id": id, "body": "  paragraph\n\n"}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["item"]["body"], "  paragraph\n");
    assert_eq!(envelope["data"]["changed"], true);

    let result = client
        .call("waymark_replace_body", json!({"id": id, "body": ""}))
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["item"]["body"], "");
    assert_eq!(envelope["data"]["changed"], true);

    client.shutdown().await;
}

#[tokio::test]
async fn update_custom_fields_takes_an_explicit_set_and_clear_map() {
    let fixture = common::fixture(&["one"]);
    let id = fixture.ids[0].clone();
    let client = common::Client::connect(fixture.root.clone()).await;

    for field in [
        json!({"key": "note", "type": "string"}),
        json!({"key": "points", "type": "integer"}),
    ] {
        let result = client.call("waymark_config_add_field", field).await;
        assert_eq!(common::envelope(&result)["ok"], true);
    }

    let result = client
        .call(
            "waymark_update_custom_fields",
            json!({
                "id": id,
                "custom_fields": {
                    "set": {"note": "hello", "points": 3}
                }
            }),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(
        envelope["data"]["changed_fields"],
        json!(["note", "points"])
    );
    assert_eq!(envelope["data"]["item"]["custom_fields"]["note"], "hello");
    assert_eq!(envelope["data"]["item"]["custom_fields"]["points"], 3);

    let result = client
        .call(
            "waymark_update_custom_fields",
            json!({"id": id, "custom_fields": {"clear": ["note"]}}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["changed_fields"], json!(["note"]));
    assert!(envelope["data"]["item"]["custom_fields"]["note"].is_null());

    // An explicit null is not a value and does not silently clear.
    let result = client
        .call(
            "waymark_update_custom_fields",
            json!({"id": id, "custom_fields": {"set": {"note": null}}}),
        )
        .await;
    let error = common::business_error(&result);
    assert_eq!(error["code"], "VALIDATION_ERROR");
    assert!(error["details"]["issues"].is_array());
    assert!(
        error["details"]["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["path"].as_str().unwrap_or_default().contains("note")),
        "{error}"
    );

    // Unknown keys and wrong value types are validation failures, not writes.
    let result = client
        .call(
            "waymark_update_custom_fields",
            json!({"id": id, "custom_fields": {"set": {"unknown": "x"}}}),
        )
        .await;
    assert_eq!(common::business_error(&result)["code"], "VALIDATION_ERROR");
    let result = client
        .call(
            "waymark_update_custom_fields",
            json!({"id": id, "custom_fields": {"set": {"points": "three"}}}),
        )
        .await;
    assert_eq!(common::business_error(&result)["code"], "VALIDATION_ERROR");

    // No editor path: the change map is required.
    let error = client
        .try_call("waymark_update_custom_fields", json!({"id": id}))
        .await
        .expect_err("the change map is required");
    assert!(matches!(
        error,
        rmcp::service::ServiceError::McpError(rmcp::model::ErrorData {
            code: rmcp::model::ErrorCode::INVALID_PARAMS,
            ..
        })
    ));

    client.shutdown().await;
}

#[tokio::test]
async fn set_status_moves_closed_at_with_the_transition() {
    let fixture = common::fixture(&["one"]);
    let id = fixture.ids[0].clone();
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_set_status",
            json!({"id": id, "status": "in_progress"}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["previous_status"], "open");
    assert_eq!(envelope["data"]["status"], "in_progress");
    assert!(envelope["data"]["closed_at"].is_null());
    assert_eq!(envelope["data"]["changed"], true);

    let result = client
        .call("waymark_set_status", json!({"id": id, "status": "closed"}))
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["status"], "closed");
    let closed_at = envelope["data"]["closed_at"].clone();
    assert!(closed_at.is_string(), "closing stamps closed_at");

    // Repeating the status writes nothing and leaves closed_at alone.
    let result = client
        .call("waymark_set_status", json!({"id": id, "status": "closed"}))
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["changed"], false);
    assert_eq!(envelope["data"]["written"], false);
    assert_eq!(envelope["data"]["closed_at"], closed_at);

    let result = client
        .call("waymark_set_status", json!({"id": id, "status": "open"}))
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["status"], "open");
    assert!(envelope["data"]["closed_at"].is_null());

    // A derived word is not a status.
    let result = client
        .call("waymark_set_status", json!({"id": id, "status": "blocked"}))
        .await;
    let error = common::business_error(&result);
    assert_eq!(error["code"], "VALIDATION_ERROR");

    client.shutdown().await;
}

#[tokio::test]
async fn transfer_owner_replaces_the_owner_and_reports_the_mismatch_warning() {
    let fixture = common::fixture(&["one"]);
    let id = fixture.ids[0].clone();
    // The configured operator is the identity every mediated write warns about.
    waymark_core::ops::config::set_scalar(
        &fixture.project(),
        waymark_core::ops::config::Scalar::Operator,
        "alice",
        &common::options(),
    )
    .expect("the operator is recorded");
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_transfer_owner",
            json!({"id": id, "owner": "carol"}),
        )
        .await;
    assert_eq!(result.is_error, Some(false), "a mismatch never blocks");
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["previous_owner"], "frank");
    assert_eq!(envelope["data"]["owner"], "carol");
    assert_eq!(envelope["data"]["changed"], true);
    let warnings = envelope["_meta"]["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1, "one advisory warning: {warnings:?}");
    assert_eq!(warnings[0]["code"], "OWNER_MISMATCH");
    assert!(common::is_registered_code(&warnings[0]["code"]));
    assert!(warnings[0]["message"].as_str().unwrap().contains("alice"));

    // Repeating the owner is a true no-op, still carrying the advisory warning.
    let result = client
        .call(
            "waymark_transfer_owner",
            json!({"id": id, "owner": "carol"}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["changed"], false);
    assert_eq!(envelope["data"]["written"], false);

    // An invalid identity is a validation failure, not a write.
    let result = client
        .call("waymark_transfer_owner", json!({"id": id, "owner": " "}))
        .await;
    let error = common::business_error(&result);
    assert_eq!(error["code"], "VALIDATION_ERROR");
    assert_eq!(error["location"]["field"], "owner");

    client.shutdown().await;
}

#[tokio::test]
async fn move_work_item_relocates_the_document() {
    let fixture = common::fixture(&["one"]);
    let id = fixture.ids[0].clone();
    let client = common::Client::connect(fixture.root.clone()).await;
    client
        .call(
            "waymark_config_add_category",
            json!({"key": "bug", "path": "work/bugs"}),
        )
        .await;

    let result = client
        .call(
            "waymark_move_work_item",
            json!({"id": id, "category": "bug"}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["previous_category"], "task");
    assert_eq!(envelope["data"]["category"], "bug");
    assert_eq!(envelope["data"]["changed"], true);
    assert_eq!(envelope["data"]["written"], true);
    assert!(envelope["data"]["path"]
        .as_str()
        .unwrap()
        .ends_with(&format!("work/bugs/{id}.md")));
    assert!(!fixture
        .root
        .join("work/task")
        .join(format!("{id}.md"))
        .exists());
    assert!(fixture
        .root
        .join("work/bugs")
        .join(format!("{id}.md"))
        .is_file());

    // An undeclared category is the same validation issue creation reports.
    let result = client
        .call(
            "waymark_move_work_item",
            json!({"id": id, "category": "nope"}),
        )
        .await;
    let error = common::business_error(&result);
    assert_eq!(error["code"], "VALIDATION_ERROR");
    assert_eq!(error["location"]["field"], "category");

    client.shutdown().await;
}

#[tokio::test]
async fn set_and_clear_parent_manage_the_single_hierarchy_edge() {
    let fixture = common::fixture(&["one", "two"]);
    let child = fixture.ids[1].clone();
    let parent = fixture.ids[0].clone();
    let child_path = fixture.root.join("work/task").join(format!("{child}.md"));
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call("waymark_set_parent", json!({"id": child, "parent": parent}))
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["verb"], "set");
    assert_eq!(envelope["data"]["parent"], parent);
    assert_eq!(envelope["data"]["changed"], true);
    assert!(common::read(&child_path).contains(&format!("parent: {parent}")));

    let result = client
        .call("waymark_set_parent", json!({"id": child, "parent": parent}))
        .await;
    assert_eq!(common::envelope(&result)["data"]["changed"], false);

    let result = client
        .call("waymark_clear_parent", json!({"id": child}))
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["verb"], "clear");
    assert!(envelope["data"]["parent"].is_null());
    assert_eq!(envelope["data"]["previous_parent"], parent);
    assert_eq!(envelope["data"]["changed"], true);

    // Clearing again is a true no-op.
    let result = client
        .call("waymark_clear_parent", json!({"id": child}))
        .await;
    assert_eq!(common::envelope(&result)["data"]["changed"], false);

    // A self-parent is a relationship rejection.
    let result = client
        .call("waymark_set_parent", json!({"id": child, "parent": child}))
        .await;
    assert_eq!(
        common::business_error(&result)["code"],
        "INVALID_RELATIONSHIP"
    );

    client.shutdown().await;
}

#[tokio::test]
async fn dependency_edges_are_added_and_removed_through_the_domain() {
    let fixture = common::fixture(&["one", "two"]);
    let waiting = fixture.ids[1].clone();
    let blocking = fixture.ids[0].clone();
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_add_dependency",
            json!({"id": waiting, "blocking": blocking}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["dependency"], blocking);
    assert_eq!(envelope["data"]["dependencies"], json!([blocking]));
    assert_eq!(envelope["data"]["changed"], true);

    // A duplicate edge is a rejection, not a silent no-op.
    let result = client
        .call(
            "waymark_add_dependency",
            json!({"id": waiting, "blocking": blocking}),
        )
        .await;
    assert_eq!(
        common::business_error(&result)["code"],
        "INVALID_RELATIONSHIP"
    );

    let result = client
        .call(
            "waymark_remove_dependency",
            json!({"id": waiting, "blocking": blocking}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["dependencies"], json!([]));
    assert_eq!(envelope["data"]["changed"], true);

    // Removing an edge that is not there is rejected.
    let result = client
        .call(
            "waymark_remove_dependency",
            json!({"id": waiting, "blocking": blocking}),
        )
        .await;
    assert_eq!(
        common::business_error(&result)["code"],
        "INVALID_RELATIONSHIP"
    );

    // A missing target is `ITEM_NOT_FOUND`.
    let result = client
        .call(
            "waymark_add_dependency",
            json!({"id": waiting, "blocking": "wm-00000000000z"}),
        )
        .await;
    assert_eq!(common::business_error(&result)["code"], "ITEM_NOT_FOUND");

    client.shutdown().await;
}

#[tokio::test]
async fn soft_links_are_written_on_both_sides_as_one_batch() {
    let fixture = common::fixture(&["one", "two"]);
    let left = fixture.ids[0].clone();
    let right = fixture.ids[1].clone();
    let left_path = fixture.root.join("work/task").join(format!("{left}.md"));
    let right_path = fixture.root.join("work/task").join(format!("{right}.md"));
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call("waymark_add_soft_link", json!({"id": left, "other": right}))
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["verb"], "add");
    assert_eq!(envelope["data"]["other"], right);
    assert_eq!(envelope["data"]["soft_links"], json!([right]));
    assert_eq!(envelope["data"]["other_item"]["soft_links"], json!([left]));
    assert_eq!(
        envelope["data"]["fingerprints"].as_array().unwrap().len(),
        2
    );
    assert!(common::read(&left_path).contains(&format!("soft_links:")));
    assert!(common::read(&right_path).contains(&format!("soft_links:")));

    let result = client
        .call(
            "waymark_remove_soft_link",
            json!({"id": left, "other": right}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["verb"], "remove");
    assert_eq!(envelope["data"]["soft_links"], json!([]));
    assert_eq!(envelope["data"]["changed"], true);
    assert!(!common::read(&right_path).contains(&format!("- {left}")));

    client.shutdown().await;
}

#[tokio::test]
async fn archive_writes_a_tombstone_and_cleans_inbound_edges() {
    let fixture = common::fixture(&["one", "two"]);
    let target = fixture.ids[0].clone();
    let dependent = fixture.ids[1].clone();
    common::depend(&fixture.project(), &dependent, &target);
    let target_path = fixture.root.join("work/task").join(format!("{target}.md"));
    let dependent_path = fixture
        .root
        .join("work/task")
        .join(format!("{dependent}.md"));
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_archive_work_item",
            json!({"id": target, "reason": "  retired  "}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["written"], true);
    assert_eq!(envelope["data"]["archive_reason"], "retired");
    assert!(envelope["data"]["archived_at"].is_string());
    assert_eq!(
        envelope["data"]["status"], "open",
        "archive is not a status"
    );
    assert_eq!(
        envelope["data"]["removals"],
        json!([{"from": dependent, "relation": "dependencies"}])
    );
    let affected: Vec<&str> = envelope["data"]["affected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(affected, vec![dependent.as_str()]);
    assert!(common::read(&target_path).contains("archived_at:"));
    assert!(!common::read(&dependent_path).contains(&format!("- {target}")));

    // Re-archiving is refused and replaces no marker.
    let result = client
        .call("waymark_archive_work_item", json!({"id": target}))
        .await;
    assert_eq!(common::business_error(&result)["code"], "ALREADY_ARCHIVED");

    // Archiving the dependent is now clean; a dry run of it writes nothing.
    let before = common::snapshot(&fixture.root);
    let result = client
        .call(
            "waymark_archive_work_item",
            json!({"id": dependent, "dry_run": true}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["written"], false);
    assert!(envelope["data"]["archived_at"].is_string());
    assert_eq!(common::snapshot(&fixture.root), before);

    client.shutdown().await;
}

#[tokio::test]
async fn write_failures_carry_the_shared_codes_and_typed_details() {
    let fixture = common::fixture(&["one"]);
    let id = fixture.ids[0].clone();
    let client = common::Client::connect(fixture.root.clone()).await;

    // ITEM_NOT_FOUND is byte-for-byte the shared business failure.
    let missing = waymark_core::WorkItemId::parse("wm-00000000000z").unwrap();
    let result = client
        .call("waymark_set_title", json!({"id": missing, "title": "x"}))
        .await;
    let error = common::business_error(&result);
    assert_eq!(
        error,
        common::shared_failure(&waymark_core::error::WaymarkError::item_not_found(&missing))
    );

    // VALIDATION_ERROR carries the per-field issue list.
    let result = client
        .call("waymark_set_title", json!({"id": id, "title": " "}))
        .await;
    let error = common::business_error(&result);
    assert_eq!(error["code"], "VALIDATION_ERROR");
    assert!(error["details"]["issues"].is_array());
    assert!(error["location"].is_object());

    // ITEM_ARCHIVED is the shared error with the same operation name.
    client
        .call("waymark_archive_work_item", json!({"id": id}))
        .await;
    let archived = waymark_core::WorkItemId::parse(&id).unwrap();
    let result = client
        .call("waymark_set_title", json!({"id": id, "title": "changed"}))
        .await;
    let error = common::business_error(&result);
    assert_eq!(
        error,
        common::shared_failure(&waymark_core::error::WaymarkError::item_archived(
            &archived, "title"
        ))
    );

    client.shutdown().await;
}

#[tokio::test]
async fn write_conflict_and_stale_generation_reuse_the_shared_errors() {
    let fixture = common::fixture(&["one", "two"]);
    let id = fixture.ids[0].clone();
    let occupied = fixture.ids[1].clone();
    let client = common::Client::connect(fixture.root.clone()).await;
    client
        .call(
            "waymark_config_add_category",
            json!({"key": "bug", "path": "work/bugs"}),
        )
        .await;

    // A directory is not a work-item document, so the inventory skips it and
    // the move reaches its destination check: WRITE_CONFLICT, typed exactly as
    // the shared write engine reports it.
    std::fs::create_dir(
        fixture
            .root
            .join("work/bugs")
            .join(format!("{occupied}.md")),
    )
    .expect("the occupied destination is creatable");
    let result = client
        .call(
            "waymark_move_work_item",
            json!({"id": occupied, "category": "bug"}),
        )
        .await;
    let error = common::business_error(&result);
    assert_eq!(error["code"], "WRITE_CONFLICT");
    assert_eq!(
        error["details"]["path"],
        format!("{}/work/bugs/{occupied}.md", fixture.root)
    );
    assert_eq!(error["details"]["expected_fingerprint"], "absent");
    assert_eq!(error["details"]["actual_fingerprint"], "present");
    assert_eq!(error["details"]["id"], occupied);

    // An older generated document rejects ordinary writes until the version
    // migration runs.
    let path = fixture.root.join("work/task").join(format!("{id}.md"));
    let text = common::read(&path);
    let downgraded = text.replacen(
        &format!("schema: {}\n", waymark_core::model::FORMAT_GENERATION),
        &format!("schema: {}\n", waymark_core::model::FORMAT_GENERATION - 1),
        1,
    );
    assert_ne!(text, downgraded);
    std::fs::write(&path, downgraded).expect("the document is writable");
    let result = client
        .call("waymark_set_title", json!({"id": id, "title": "changed"}))
        .await;
    let error = common::business_error(&result);
    let stale_id = waymark_core::WorkItemId::parse(&id).unwrap();
    assert_eq!(
        error,
        common::shared_failure(&waymark_core::error::WaymarkError::item_needs_migration(
            &stale_id,
            waymark_core::model::FORMAT_GENERATION - 1,
            waymark_core::model::FORMAT_GENERATION
        ))
    );

    client.shutdown().await;
}
