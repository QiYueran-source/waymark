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
