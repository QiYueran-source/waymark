//! Project lifecycle tools over the in-process MCP client.

mod common;

use serde_json::json;
use waymark_core::model::FORMAT_GENERATION;

#[tokio::test]
async fn project_init_creates_a_project_and_info_describes_it() {
    let (_dir, root) = common::empty_dir();
    let client = common::Client::connect(root.clone()).await;

    // Before init, a project tool is a business failure, not a panic and not a
    // JSON-RPC error. This is also the "missing project" startup case.
    let result = client.call("waymark_project_info", common::no_args()).await;
    let error = common::business_error(&result);
    assert_eq!(error["code"], "PROJECT_NOT_FOUND");
    assert!(error["retryable"].is_boolean());
    assert!(error["details"].is_object());

    let result = client
        .call(
            "waymark_project_init",
            json!({"operator": "frank", "timezone": "Europe/Berlin"}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["v"], 1);
    assert_eq!(envelope["data"]["storage_root"], "work");
    // The identity seeds are local settings: the summary never reports them as
    // part of the shared project.
    assert!(envelope["data"].get("operator").is_none());
    assert!(envelope["data"].get("timezone").is_none());
    assert_eq!(envelope["data"]["categories"]["task"]["path"], "work/task");
    assert_eq!(envelope["data"]["item_count"], 0);
    assert_eq!(envelope["_meta"]["source"], "direct_file");

    let local = std::fs::read_to_string(root.join(".waymark").join("user.toml"))
        .expect("init writes the local document");
    assert!(local.contains("operator = \"frank\""), "{local}");
    assert!(local.contains("timezone = \"Europe/Berlin\""), "{local}");

    let result = client.call("waymark_project_info", common::no_args()).await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["root"], root.as_str());
    assert_eq!(envelope["data"]["generation"], json!(FORMAT_GENERATION));
    assert!(envelope["data"]["revision"].is_number());
    assert_eq!(
        envelope["data"]["config_path"],
        format!("{root}/.waymark/config.toml")
    );
    assert_eq!(envelope["_meta"]["completeness"], "scoped");
    assert_eq!(envelope["_meta"]["freshness"], "fresh");

    client.shutdown().await;
}

#[tokio::test]
async fn project_init_reuses_an_existing_project_and_force_replaces_it() {
    let fixture = common::fixture(&["one"]);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call("waymark_project_init", json!({"storage_root": "elsewhere"}))
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(
        envelope["data"]["storage_root"], "work",
        "init reuses by default"
    );
    assert_eq!(envelope["data"]["item_count"], 1);

    let result = client
        .call(
            "waymark_project_init",
            json!({"storage_root": "elsewhere", "category": "issue", "force": true}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["storage_root"], "elsewhere");
    assert_eq!(
        envelope["data"]["categories"]["issue"]["path"],
        "elsewhere/issue"
    );

    client.shutdown().await;
}
