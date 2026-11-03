//! Git inspection tools over the in-process MCP client.
//!
//! Outside a repository the domain answers with an empty report plus the
//! informational `NOT_A_GIT_REPOSITORY` finding; the adapter must surface that
//! as a successful envelope, not turn it into a failure.

mod common;

use serde_json::json;

fn diagnostics_contain(envelope: &serde_json::Value, code: &str) -> bool {
    envelope["_meta"]["diagnostics"]
        .as_array()
        .expect("diagnostics is an array")
        .iter()
        .any(|finding| finding["code"] == code)
}

#[tokio::test]
async fn git_history_outside_a_repository_is_a_successful_informational_report() {
    let fixture = common::fixture(&["one"]);
    let client = common::Client::connect(fixture.root.clone()).await;

    // The storage-root scope.
    let result = client.call("waymark_git_history", json!({})).await;
    assert_eq!(result.is_error, Some(false), "not a failure");
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["scope"], "storage_root");
    assert_eq!(envelope["data"]["commits"], json!([]));
    assert_eq!(envelope["data"]["limit"], 20);
    assert!(envelope["data"]["repository_root"].is_null());
    assert!(
        diagnostics_contain(&envelope, "NOT_A_GIT_REPOSITORY"),
        "the absent repository is a finding: {}",
        envelope["_meta"]
    );
    let finding = envelope["_meta"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|finding| finding["code"] == "NOT_A_GIT_REPOSITORY")
        .unwrap();
    assert!(common::is_registered_code(&finding["code"]));

    // The item scope, which resolves the item's document without needing a
    // repository.
    let result = client
        .call("waymark_git_history", json!({"item": fixture.ids[0]}))
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["scope"], "item");
    assert_eq!(envelope["data"]["item"], fixture.ids[0]);
    assert!(diagnostics_contain(&envelope, "NOT_A_GIT_REPOSITORY"));

    // An explicit limit is honoured, including `0` meaning unlimited.
    let result = client
        .call("waymark_git_history", json!({"limit": 0}))
        .await;
    assert_eq!(common::envelope(&result)["data"]["limit"], 0);

    client.shutdown().await;
}

#[tokio::test]
async fn git_diff_outside_a_repository_is_a_successful_empty_patch() {
    let fixture = common::fixture(&["one"]);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client.call("waymark_git_diff", json!({})).await;
    assert_eq!(result.is_error, Some(false), "not a failure");
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["scope"], "storage_root");
    assert_eq!(envelope["data"]["baseline"], "working_tree");
    assert_eq!(envelope["data"]["empty"], true);
    assert_eq!(envelope["data"]["patch"], "");
    assert!(
        diagnostics_contain(&envelope, "NOT_A_GIT_REPOSITORY"),
        "the absent repository is a finding: {}",
        envelope["_meta"]
    );

    // A named commit changes the baseline but not the success semantics.
    let result = client
        .call("waymark_git_diff", json!({"commit": "HEAD"}))
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["baseline"], "commit");
    assert_eq!(envelope["data"]["empty"], true);

    client.shutdown().await;
}

#[tokio::test]
async fn git_scoping_errors_reuse_the_shared_codes() {
    let fixture = common::fixture(&["one"]);
    let client = common::Client::connect(fixture.root.clone()).await;

    // Item and path are mutually exclusive; the domain, not the adapter,
    // decides this.
    let result = client
        .call(
            "waymark_git_history",
            json!({"item": fixture.ids[0], "path": "work"}),
        )
        .await;
    let error = common::business_error(&result);
    assert_eq!(error["code"], "VALIDATION_ERROR");
    assert!(error["details"]["issues"].is_array());

    let result = client
        .call("waymark_git_history", json!({"item": "not-an-id"}))
        .await;
    let error = common::business_error(&result);
    assert_eq!(error["code"], "INVALID_ID");
    assert!(common::is_registered_code(&error["code"]));

    client.shutdown().await;
}
