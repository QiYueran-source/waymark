//! Migration tools over the in-process MCP client.
//!
//! The domain owns every planning, revalidation, and write rule. These tests
//! pin the adapter's translation (one action per call), the report contents, and
//! the dry-run guarantee.

mod common;

use camino::Utf8Path;
use serde_json::json;
use waymark_core::config::FieldType;
use waymark_core::model::{FieldKey, FORMAT_GENERATION};
use waymark_core::ops::config;
use waymark_core::ops::edit::{self, FieldChangeEntry};
use waymark_core::FieldValue;

/// Rewrite one document's generation literal to the previous generation.
fn downgrade_item(root: &Utf8Path, id: &str) {
    let path = root.join("work/task").join(format!("{id}.md"));
    let text = common::read(&path);
    let downgraded = text.replacen(
        &format!("schema: {FORMAT_GENERATION}\n"),
        &format!("schema: {}\n", FORMAT_GENERATION - 1),
        1,
    );
    assert_ne!(text, downgraded, "the document declares a generation");
    std::fs::write(&path, downgraded).expect("the document is writable");
}

/// Rewrite the configuration's version literal by hand, so the migration has an
/// older generation to upgrade.
fn downgrade_config(root: &Utf8Path) {
    let path = root.join(".waymark/config.toml");
    let text = common::read(&path);
    let current = waymark_core::Project::discover(root)
        .expect("the project opens")
        .config()
        .version
        .to_string();
    let replaced = text.replacen(&format!("version = \"{current}\""), "version = \"0.2\"", 1);
    assert_ne!(text, replaced, "the version literal was found");
    std::fs::write(&path, replaced).expect("the configuration is writable");
}

#[tokio::test]
async fn migrate_version_dry_run_reports_the_work_and_writes_nothing() {
    let fixture = common::fixture(&["one"]);
    let id = fixture.ids[0].clone();
    downgrade_item(&fixture.root, &id);
    downgrade_config(&fixture.root);
    let item_path = fixture.root.join("work/task").join(format!("{id}.md"));

    let project = common::warm_lock(&fixture.root);
    let before = common::snapshot(&fixture.root);
    assert_eq!(project.config().version.generation, 0);

    let client = common::Client::connect(fixture.root.clone()).await;
    let result = client
        .call(
            "waymark_migrate_version",
            json!({"target_generation": FORMAT_GENERATION, "dry_run": true}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["migration"], "version");
    assert_eq!(envelope["data"]["dry_run"], true);
    assert_eq!(envelope["data"]["written"], false);
    assert_eq!(envelope["data"]["count"], 1);
    assert_eq!(envelope["data"]["affected"][0]["id"], id);
    assert_eq!(envelope["data"]["targets"][0]["outcome"], "skipped");
    assert_eq!(envelope["data"]["config_changed"], true);
    assert_eq!(
        envelope["data"]["revision"],
        envelope["data"]["previous_revision"].as_u64().unwrap() + 1
    );
    assert_eq!(envelope["_meta"]["source"], "memory_projection");

    // The dry run performed the complete check and wrote nothing at all.
    assert_eq!(common::snapshot(&fixture.root), before);
    assert!(common::read(&item_path).contains(&format!("schema: {}", FORMAT_GENERATION - 1)));

    // A real run applies the plan.
    let result = client
        .call(
            "waymark_migrate_version",
            json!({"target_generation": FORMAT_GENERATION}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["dry_run"], false);
    assert_eq!(envelope["data"]["written"], true);
    assert_eq!(envelope["data"]["targets"][0]["outcome"], "applied");
    assert!(common::read(&item_path).contains(&format!("schema: {FORMAT_GENERATION}\n")));
    assert_eq!(
        fixture.project().config().version.generation,
        FORMAT_GENERATION
    );

    client.shutdown().await;
}

#[tokio::test]
async fn migrate_version_refuses_a_generation_this_release_does_not_write() {
    let fixture = common::fixture(&["one"]);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_migrate_version",
            json!({"target_generation": FORMAT_GENERATION + 1}),
        )
        .await;
    let error = common::business_error(&result);
    assert_eq!(error["code"], "MIGRATION_UNSUPPORTED_GENERATION");
    assert!(common::is_registered_code(&error["code"]));

    client.shutdown().await;
}

#[tokio::test]
async fn migrate_schema_removes_the_declaration_and_every_value() {
    let fixture = common::fixture(&["one"]);
    let id = fixture.ids[0].clone();
    let project = fixture.project();
    let key = FieldKey::parse("note").unwrap();
    config::add_field(
        &project,
        &key,
        FieldType::String,
        false,
        None,
        &common::options(),
    )
    .expect("the field is declared");
    let project = fixture.project();
    edit::update_custom_fields(
        &project,
        &id,
        &[FieldChangeEntry::set(
            "note",
            FieldValue::String("hello".to_owned()),
        )],
        &common::options(),
    )
    .expect("the value is written");
    let item_path = fixture.root.join("work/task").join(format!("{id}.md"));
    assert!(common::read(&item_path).contains("note"));

    common::warm_lock(&fixture.root);
    let before = common::snapshot(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_migrate_schema",
            json!({"action": {"kind": "remove_field", "key": "note"}, "dry_run": true}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["migration"], "schema");
    assert_eq!(envelope["data"]["dry_run"], true);
    assert_eq!(envelope["data"]["written"], false);
    assert_eq!(envelope["data"]["count"], 1);
    assert_eq!(envelope["data"]["affected"][0]["id"], id);
    assert_eq!(envelope["data"]["targets"][0]["outcome"], "skipped");
    assert_eq!(common::snapshot(&fixture.root), before);

    let result = client
        .call(
            "waymark_migrate_schema",
            json!({"action": {"kind": "remove_field", "key": "note"}}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["written"], true);
    assert_eq!(envelope["data"]["targets"][0]["outcome"], "applied");
    assert!(!common::read(&item_path).contains("note"));
    assert!(!fixture
        .project()
        .config()
        .fields
        .contains_key(&FieldKey::parse("note").unwrap()));

    client.shutdown().await;
}

#[tokio::test]
async fn migrate_schema_adds_a_required_field_with_its_default() {
    let fixture = common::fixture(&["one", "two"]);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_migrate_schema",
            json!({
                "action": {
                    "kind": "add_required_field",
                    "key": "points",
                    "type": "integer",
                    "default": "0"
                },
                "dry_run": true
            }),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["count"], 2, "both items get the default");
    assert_eq!(envelope["data"]["written"], false);
    assert_eq!(envelope["data"]["config_changed"], true);
    assert!(
        !fixture
            .project()
            .config()
            .fields
            .contains_key(&FieldKey::parse("points").unwrap()),
        "a dry run declares nothing"
    );

    let result = client
        .call(
            "waymark_migrate_schema",
            json!({
                "action": {
                    "kind": "add_required_field",
                    "key": "points",
                    "type": "integer",
                    "default": "0"
                }
            }),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["written"], true);
    let project = fixture.project();
    assert!(project.config().fields[&FieldKey::parse("points").unwrap()].required);
    for id in &fixture.ids {
        let path = fixture.root.join("work/task").join(format!("{id}.md"));
        let text = common::read(&path);
        assert!(text.contains("points: 0"), "{text}");
    }

    client.shutdown().await;
}

#[tokio::test]
async fn migrate_storage_moves_one_category_and_rewrites_the_key() {
    let fixture = common::fixture(&["one"]);
    let id = fixture.ids[0].clone();
    // Hand-construct the second category so the fixture stays minimal.
    let project = fixture.project();
    config::add_category(
        &project,
        &waymark_core::CategoryKey::parse("bug").unwrap(),
        "work/bugs",
        None,
        &common::options(),
    )
    .expect("the category is declared");

    common::warm_lock(&fixture.root);
    let before = common::snapshot(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;

    // A dry run reports the planned directory move and the rewritten value.
    let result = client
        .call(
            "waymark_migrate_storage",
            json!({
                "action": {"kind": "category_key", "from": "task", "to": "todo"},
                "dry_run": true
            }),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["migration"], "storage");
    assert_eq!(envelope["data"]["dry_run"], true);
    assert_eq!(envelope["data"]["written"], false);
    assert_eq!(envelope["data"]["moves"][0]["outcome"], "skipped");
    assert_eq!(envelope["data"]["rewritten"][0]["id"], id);
    assert_eq!(envelope["data"]["rewritten"][0]["from"], "task");
    assert_eq!(envelope["data"]["rewritten"][0]["to"], "todo");
    assert_eq!(common::snapshot(&fixture.root), before);

    let result = client
        .call(
            "waymark_migrate_storage",
            json!({"action": {"kind": "category_key", "from": "task", "to": "todo"}}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["written"], true);
    assert!(fixture.root.join("work/todo").is_dir());
    assert!(!fixture.root.join("work/task").exists());
    let moved = fixture.root.join("work/todo").join(format!("{id}.md"));
    assert!(common::read(&moved).contains("category: todo"));

    client.shutdown().await;
}

#[tokio::test]
async fn migration_tools_accept_exactly_one_tagged_action() {
    let fixture = common::fixture(&["one"]);
    let client = common::Client::connect(fixture.root.clone()).await;

    // No action at all is missing `action`, a protocol error, not a business
    // failure and not a silently chosen default.
    let error = client
        .try_call("waymark_migrate_schema", json!({"dry_run": true}))
        .await
        .expect_err("a missing action is a protocol error");
    assert!(matches!(
        error,
        rmcp::service::ServiceError::McpError(rmcp::model::ErrorData {
            code: rmcp::model::ErrorCode::INVALID_PARAMS,
            ..
        })
    ));

    // An unknown action tag is likewise rejected by the schema/deserializer.
    let error = client
        .try_call(
            "waymark_migrate_schema",
            json!({"action": {"kind": "rename_field", "key": "note"}}),
        )
        .await
        .expect_err("an unknown action is a protocol error");
    assert!(matches!(
        error,
        rmcp::service::ServiceError::McpError(rmcp::model::ErrorData {
            code: rmcp::model::ErrorCode::INVALID_PARAMS,
            ..
        })
    ));

    // The tag is required in storage actions too.
    let error = client
        .try_call(
            "waymark_migrate_storage",
            json!({"action": {"from": "task", "to": "todo"}}),
        )
        .await
        .expect_err("an untagged storage action is a protocol error");
    assert!(matches!(
        error,
        rmcp::service::ServiceError::McpError(rmcp::model::ErrorData {
            code: rmcp::model::ErrorCode::INVALID_PARAMS,
            ..
        })
    ));

    client.shutdown().await;
}
