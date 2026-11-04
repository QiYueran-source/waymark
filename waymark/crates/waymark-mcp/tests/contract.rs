//! The MCP contract: catalog, shared envelopes, registry errors, protocol
//! separation, and the cross-phase batch invariant.

mod common;

use std::collections::BTreeSet;

use rmcp::model::ErrorCode;
use rmcp::service::ServiceError;
use serde_json::{json, Value};
use waymark_core::error::WaymarkError;
use waymark_core::WorkItemId;

fn tool_schema(tool: &rmcp::model::Tool) -> Value {
    Value::Object((*tool.input_schema).clone())
}

fn assert_success_envelope(envelope: &Value, tool: &str) {
    assert_eq!(envelope["v"], 1, "{tool}");
    assert_eq!(envelope["ok"], true, "{tool}");
    assert!(envelope["data"].is_object(), "{tool} data");
    let meta = &envelope["_meta"];
    assert!(
        matches!(
            meta["source"].as_str(),
            Some("direct_file" | "memory_projection")
        ),
        "{tool} source: {meta}"
    );
    assert!(
        matches!(meta["completeness"].as_str(), Some("scoped" | "complete")),
        "{tool} completeness: {meta}"
    );
    assert!(
        matches!(
            meta["freshness"].as_str(),
            Some("fresh" | "stale" | "unknown")
        ),
        "{tool} freshness: {meta}"
    );
    assert!(meta["diagnostics"].is_array(), "{tool} diagnostics");
    assert!(meta["warnings"].is_array(), "{tool} warnings");
}

#[tokio::test]
async fn the_catalog_exposes_the_advertised_capabilities() {
    let fixture = common::fixture(&["one"]);
    let client = common::Client::connect(fixture.root.clone()).await;
    let tools = client.tools().await;

    let names: BTreeSet<String> = tools.iter().map(|tool| tool.name.to_string()).collect();
    let expected: BTreeSet<String> = waymark_mcp::TOOLS
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    assert_eq!(
        names, expected,
        "the catalog is exactly the advertised list"
    );
    assert_eq!(
        tools.len(),
        waymark_mcp::TOOLS.len(),
        "no tool is served twice or left out"
    );
    assert!(
        waymark_mcp::TOOLS.starts_with(waymark_mcp::PHASE_A_TOOLS),
        "phase A's tools keep their places in the catalog"
    );

    for tool in &tools {
        let description = tool
            .description
            .as_deref()
            .expect("every tool is documented for an agent");
        assert!(!description.trim().is_empty(), "{}", tool.name);
        let schema = tool_schema(tool);
        assert_eq!(schema["type"], "object", "{}", tool.name);
        assert!(schema["properties"].is_object(), "{}", tool.name);
        assert!(
            waymark_mcp::invariants::batch_parameters(&schema).is_empty(),
            "{} accepts a batch parameter: {schema}",
            tool.name
        );
    }

    let list = tools
        .iter()
        .find(|tool| tool.name == "waymark_list_work_items")
        .expect("the list tool is catalogued");
    assert!(
        list.description.as_deref().unwrap().contains("50"),
        "the list description documents the MCP default limit"
    );

    client.shutdown().await;
}

#[tokio::test]
async fn every_tool_routes_to_the_shared_library_and_returns_the_envelope() {
    let fixture = common::fixture(&["one", "two"]);
    let id0 = fixture.ids[0].clone();
    let id1 = fixture.ids[1].clone();
    let client = common::Client::connect(fixture.root.clone()).await;

    let calls: Vec<(&str, Value)> = vec![
        ("waymark_project_init", common::no_args()),
        ("waymark_project_info", common::no_args()),
        ("waymark_config_read", common::no_args()),
        ("waymark_config_validate", common::no_args()),
        ("waymark_validate", common::no_args()),
        ("waymark_doctor", common::no_args()),
        ("waymark_get_work_item", json!({"id": id0})),
        ("waymark_list_work_items", common::no_args()),
        ("waymark_search_work_items", json!({"text": "one"})),
        ("waymark_get_tree", common::no_args()),
        ("waymark_get_dependency_tree", json!({"id": id0})),
        ("waymark_get_ready_items", common::no_args()),
        ("waymark_get_blocked_items", common::no_args()),
        ("waymark_get_stats", common::no_args()),
        (
            "waymark_config_set",
            json!({"name": "operator", "value": "frank"}),
        ),
        (
            "waymark_config_add_category",
            json!({"key": "bug", "path": "work/bugs", "name": "Bugs"}),
        ),
        (
            "waymark_config_add_category",
            json!({"key": "chore", "path": "work/chore"}),
        ),
        (
            "waymark_config_update_category",
            json!({"key": "chore", "name": "Chores"}),
        ),
        ("waymark_config_remove_category", json!({"key": "chore"})),
        (
            "waymark_config_add_field",
            json!({"key": "note", "type": "string"}),
        ),
        (
            "waymark_config_add_field",
            json!({"key": "points", "type": "integer"}),
        ),
        (
            "waymark_config_update_field",
            json!({"key": "points", "attribute": "default", "value": "0"}),
        ),
        (
            "waymark_migrate_version",
            json!({"target_generation": waymark_core::model::FORMAT_GENERATION}),
        ),
        (
            "waymark_migrate_schema",
            json!({"action": {"kind": "remove_field", "key": "note"}}),
        ),
        (
            "waymark_migrate_storage",
            json!({"action": {"kind": "category_path", "key": "bug", "path": "work/bugs2"}}),
        ),
        ("waymark_git_history", common::no_args()),
        ("waymark_git_diff", common::no_args()),
        (
            "waymark_create_work_item",
            json!({"title": "three", "category": "task", "owner": "frank", "body": "b"}),
        ),
        (
            "waymark_set_title",
            json!({"id": id0, "title": "one renamed"}),
        ),
        (
            "waymark_replace_body",
            json!({"id": id0, "body": "the body"}),
        ),
        (
            "waymark_update_custom_fields",
            json!({"id": id0, "custom_fields": {"set": {"points": 5}}}),
        ),
        (
            "waymark_set_status",
            json!({"id": id0, "status": "in_progress"}),
        ),
        (
            "waymark_transfer_owner",
            json!({"id": id0, "owner": "carol"}),
        ),
        (
            "waymark_move_work_item",
            json!({"id": id0, "category": "bug"}),
        ),
        ("waymark_set_parent", json!({"id": id1, "parent": id0})),
        ("waymark_clear_parent", json!({"id": id1})),
        (
            "waymark_add_dependency",
            json!({"id": id1, "blocking": id0}),
        ),
        (
            "waymark_remove_dependency",
            json!({"id": id1, "blocking": id0}),
        ),
        ("waymark_add_soft_link", json!({"id": id0, "other": id1})),
        ("waymark_remove_soft_link", json!({"id": id0, "other": id1})),
        ("waymark_archive_work_item", json!({"id": id1})),
    ];

    for (tool, arguments) in calls {
        let result = client.call(tool, arguments).await;
        assert_eq!(
            result.is_error,
            Some(false),
            "{tool} is a successful result"
        );
        let envelope = common::envelope(&result);
        assert_success_envelope(&envelope, tool);
    }

    client.shutdown().await;
}

#[tokio::test]
async fn business_failures_reuse_the_shared_registry_and_typed_details() {
    let fixture = common::fixture(&["one"]);
    let client = common::Client::connect(fixture.root.clone()).await;

    // A missing ID is exactly the shared `ITEM_NOT_FOUND`, byte for byte: same
    // code, message, typed details, location, and retryability.
    let missing = WorkItemId::parse("wm-00000000000z").unwrap();
    let result = client
        .call("waymark_get_work_item", json!({"id": missing}))
        .await;
    assert_eq!(result.is_error, Some(true), "a business failure is isError");
    let error = common::business_error(&result);
    assert_eq!(
        error,
        common::shared_failure(&WaymarkError::item_not_found(&missing))
    );
    assert!(common::is_registered_code(&error["code"]));
    assert_eq!(error["code"], "ITEM_NOT_FOUND");
    assert_eq!(error["retryable"], false);
    assert!(error["location"].is_object());

    // A bad filter is the shared `VALIDATION_ERROR` with a per-field issue list,
    // produced by `waymark_core::query::Filters::from_json`.
    let result = client
        .call(
            "waymark_list_work_items",
            json!({"filter": {"status": "doing"}}),
        )
        .await;
    let error = common::business_error(&result);
    assert!(common::is_registered_code(&error["code"]));
    assert_eq!(error["code"], "VALIDATION_ERROR");
    assert_eq!(error["retryable"], false);
    assert!(error["details"]["issues"].is_array(), "{error}");
    assert_eq!(error["location"]["kind"], "field");
    assert_eq!(error["location"]["field"], "filter");

    // There is no second taxonomy: the error object carries only the shared
    // envelope's keys, with no adapter-specific classification field.
    let keys: BTreeSet<&str> = error
        .as_object()
        .expect("the failure body is an object")
        .keys()
        .map(String::as_str)
        .collect();
    for key in &keys {
        assert!(
            matches!(
                *key,
                "v" | "code" | "message" | "details" | "location" | "retryable"
            ),
            "unexpected error field {key:?}"
        );
    }
    assert_eq!(error["v"], 1);

    client.shutdown().await;
}

#[tokio::test]
async fn protocol_errors_stay_separate_from_business_envelopes() {
    let fixture = common::fixture(&["one"]);
    let client = common::Client::connect(fixture.root.clone()).await;

    // An unknown tool is a JSON-RPC error, not a failure envelope.
    let error = client
        .try_call("waymark_not_a_tool", common::no_args())
        .await
        .expect_err("an unknown tool is a protocol error");
    let error = match error {
        ServiceError::McpError(error) => error,
        other => panic!("expected an MCP protocol error, got {other:?}"),
    };
    assert_eq!(error.code, ErrorCode::INVALID_PARAMS);

    // A malformed request (wrong parameter type for a known tool) is rejected
    // by the tool layer as invalid params, again without a business envelope.
    let error = client
        .try_call("waymark_get_work_item", json!({"id": 42}))
        .await
        .expect_err("a malformed parameter is a protocol error");
    let error = match error {
        ServiceError::McpError(error) => error,
        other => panic!("expected an MCP protocol error, got {other:?}"),
    };
    assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
    assert!(
        error.message.contains("parameters"),
        "deserialization failure: {}",
        error.message
    );

    // The same tool with a well-formed but absent ID stays a tool result.
    let result = client
        .call("waymark_get_work_item", json!({"id": "wm-00000000000z"}))
        .await;
    assert_eq!(result.is_error, Some(true));
    assert_eq!(common::envelope(&result)["error"]["code"], "ITEM_NOT_FOUND");

    client.shutdown().await;
}

#[tokio::test]
async fn no_advertised_tool_accepts_a_batch_array() {
    // The shared helper is what phase B must apply to every write tool too; the
    // catalog pass here already covers every tool the server advertises.
    let fixture = common::fixture(&["one"]);
    let client = common::Client::connect(fixture.root.clone()).await;
    let tools = client.tools().await;
    assert!(!tools.is_empty());
    for tool in &tools {
        let offenders = waymark_mcp::invariants::batch_parameters(&tool_schema(tool));
        assert!(
            offenders.is_empty(),
            "{} accepts {offenders:?}; writes are single-target",
            tool.name
        );
    }
    client.shutdown().await;
}

#[tokio::test]
async fn write_schemas_are_single_target_and_migration_actions_are_tagged_unions() {
    let fixture = common::fixture(&["one"]);
    let client = common::Client::connect(fixture.root.clone()).await;
    let tools = client.tools().await;

    let tool = |name: &str| {
        tools
            .iter()
            .find(|tool| tool.name == name)
            .unwrap_or_else(|| panic!("{name} is advertised"))
    };

    // Every write tool is a single object with no `items`-style array. The
    // catalog-wide batch check covers the shared helper; this pins the shape of
    // the write family specifically.
    for name in waymark_mcp::TOOLS.iter().filter(|name| {
        name.starts_with("waymark_create")
            || name.starts_with("waymark_set_")
            || name.starts_with("waymark_replace_")
            || name.starts_with("waymark_update_")
            || name.starts_with("waymark_transfer_")
            || name.starts_with("waymark_move_")
            || name.starts_with("waymark_clear_")
            || name.starts_with("waymark_add_")
            || name.starts_with("waymark_remove_")
            || name.starts_with("waymark_archive_")
            || name.starts_with("waymark_config_set")
            || name.starts_with("waymark_config_add_")
            || name.starts_with("waymark_config_update_")
            || name.starts_with("waymark_config_remove_")
            || name.starts_with("waymark_migrate_")
    }) {
        let schema = tool_schema(tool(name));
        assert_eq!(schema["type"], "object", "{name}");
        assert!(
            schema["properties"].get("items").is_none(),
            "{name} must not take an items array"
        );
        assert!(
            waymark_mcp::invariants::batch_parameters(&schema).is_empty(),
            "{name} must be single-target: {schema}"
        );
    }

    // The custom-field change map is required and has explicit `set`/`clear`
    // halves; there is no editor path and no optional "maybe change" form.
    let fields = tool_schema(tool("waymark_update_custom_fields"));
    let required: Vec<&str> = fields["required"]
        .as_array()
        .expect("required parameters")
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert!(required.contains(&"id"));
    assert!(required.contains(&"custom_fields"));
    assert_eq!(
        fields["properties"]["custom_fields"]["type"], "object",
        "the change map is an object"
    );
    let change_map = &fields["properties"]["custom_fields"];
    assert!(change_map["properties"]["set"].is_object());
    assert_eq!(change_map["properties"]["set"]["type"], "object");
    assert_eq!(change_map["properties"]["clear"]["type"], "array");
    assert_eq!(
        change_map["properties"]["clear"]["items"]["type"], "string",
        "a clear is a list of keys, not a list of target objects"
    );

    // A migration action is a tagged union: one action per call, enforced by
    // the schema rather than by handler-side flag reconciliation.
    for name in ["waymark_migrate_schema", "waymark_migrate_storage"] {
        let schema = tool_schema(tool(name));
        assert_eq!(schema["type"], "object", "{name}");
        let action = &schema["properties"]["action"];
        assert!(
            action["oneOf"].is_array(),
            "{name} action must be a oneOf union: {action}"
        );
        assert_eq!(
            schema["required"],
            json!(["action"]),
            "{name} requires one action"
        );
    }

    client.shutdown().await;
}

/// A tool that can write exposes the same per-operation lock override as the
/// CLI's `--lock-timeout-ms`; a future write tool cannot silently omit it.
#[tokio::test]
async fn every_write_tool_exposes_the_shared_lock_override() {
    let fixture = common::fixture(&[]);
    let client = common::Client::connect(fixture.root.clone()).await;
    for tool in client.tools().await {
        let schema = tool_schema(&tool);
        if schema["properties"].get("dry_run").is_some() {
            assert!(
                schema["properties"].get("lock_timeout_ms").is_some(),
                "{} writes but has no lock_timeout_ms",
                tool.name
            );
            let description = tool.description.as_deref().unwrap_or_default();
            assert!(
                description.contains("lock_timeout_ms"),
                "{} does not document lock_timeout_ms",
                tool.name
            );
        }
    }
    client.shutdown().await;
}
