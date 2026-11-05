//! Configuration and diagnostic tools over the in-process MCP client.

mod common;

use std::sync::mpsc;

use serde_json::json;
use waymark_core::lock::LockWait;

#[tokio::test]
async fn config_read_returns_the_effective_configuration() {
    let fixture = common::fixture(&["one"]);
    // Record an operator through the local configuration operation, so the read
    // proves it returns the effective values and not a stale copy.
    waymark_core::ops::local_config::set_scalar(
        &fixture.project(),
        waymark_core::ops::config::Scalar::parse("operator").unwrap(),
        "frank",
        &common::options(),
    )
    .expect("the operator is recorded");
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client.call("waymark_config_read", common::no_args()).await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["project"]["storage_root"], "work");
    assert_eq!(
        envelope["data"]["project"]["categories"]["task"]["path"],
        "work/task"
    );
    assert!(envelope["data"]["project"]["revision"].is_number());
    // The operator is a local setting: it appears in the local document and in
    // the resolved values, never in the shared project document.
    assert_eq!(envelope["data"]["local"]["operator"], "frank");
    assert_eq!(envelope["data"]["effective"]["operator"], "frank");
    assert_eq!(envelope["data"]["sources"]["operator"], "local_document");
    assert!(envelope["data"]["project"].get("operator").is_none());
    assert!(envelope["data"]["project"].get("timezone").is_none());
    assert!(envelope["data"]["project"].get("cache").is_none());
    assert_eq!(envelope["_meta"]["source"], "direct_file");

    client.shutdown().await;
}

#[tokio::test]
async fn config_validate_reports_findings_without_a_business_failure() {
    let fixture = common::fixture(&["one"]);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call("waymark_config_validate", common::no_args())
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["valid"], true);
    assert_eq!(envelope["data"]["findings"], json!([]));

    // Break the document on disk: validation must still answer with a report,
    // and the findings must reuse the shared registry.
    std::fs::write(fixture.root.join(".waymark/config.toml"), "not = [toml")
        .expect("the configuration is replaced");
    let result = client
        .call("waymark_config_validate", common::no_args())
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["valid"], false);
    let findings = envelope["data"]["findings"].as_array().expect("findings");
    assert!(!findings.is_empty());
    assert_eq!(findings[0]["code"], "CONFIG_INVALID");
    assert!(common::is_registered_code(&findings[0]["code"]));
    assert_eq!(envelope["_meta"]["diagnostics"], json!(findings));

    // The local document is validated alongside it, and a local finding reuses
    // the same registry and the same shape.
    std::fs::write(
        fixture.root.join(".waymark/config.toml"),
        waymark_core::config::Config::fresh().to_toml_string(),
    )
    .expect("the configuration is healthy again");
    std::fs::write(
        fixture.root.join(".waymark/user.toml"),
        "timezone = \"Mars/Olympus\"\n",
    )
    .expect("the local document is replaced");
    let result = client
        .call("waymark_config_validate", common::no_args())
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true, "{envelope}");
    let findings = envelope["data"]["findings"].as_array().expect("findings");
    assert!(
        findings.iter().any(|finding| finding["message"]
            .as_str()
            .unwrap_or_default()
            .contains("timezone")),
        "{findings:?}"
    );
    for finding in findings {
        assert_eq!(finding["code"], "CONFIG_INVALID");
        assert!(common::is_registered_code(&finding["code"]));
        // The location names the document that carries the problem, so a client
        // repairs the local file rather than the shared one.
        assert_eq!(finding["location"]["kind"], "file");
        assert!(
            finding["location"]["path"]
                .as_str()
                .unwrap_or_default()
                .ends_with("user.toml"),
            "{finding}"
        );
    }

    client.shutdown().await;
}

#[tokio::test]
async fn validate_and_doctor_return_the_shared_diagnostic_report() {
    let fixture = common::fixture(&["one"]);
    // A document the parser cannot read produces a diagnostic rather than a
    // failure, which is the whole point of `validate` and `doctor`.
    std::fs::write(
        fixture.root.join("work/task/broken.md"),
        "---\nnot frontmatter",
    )
    .expect("the broken document is written");
    let client = common::Client::connect(fixture.root.clone()).await;

    for tool in ["waymark_validate", "waymark_doctor"] {
        let result = client.call(tool, common::no_args()).await;
        let envelope = common::envelope(&result);
        assert_eq!(envelope["ok"], true, "{tool} reports instead of failing");
        assert_eq!(envelope["data"]["valid"], false, "{tool}");
        assert_eq!(envelope["data"]["healthy"], false, "{tool}");
        assert!(
            envelope["data"]["counts"]["errors"].as_u64().unwrap() >= 1,
            "{tool}"
        );
        let findings = envelope["data"]["findings"].as_array().unwrap();
        assert!(!findings.is_empty(), "{tool}");
        assert!(common::is_registered_code(&findings[0]["code"]), "{tool}");
        // The unreadable document makes the diagnostic scan scoped, and the
        // report says so instead of passing a partial scan off as complete.
        assert_eq!(envelope["_meta"]["completeness"], "scoped", "{tool}");
        assert_eq!(envelope["_meta"]["source"], "memory_projection", "{tool}");
        assert!(
            !envelope["data"]["scope"].as_array().unwrap().is_empty(),
            "{tool}"
        );
        // The fixture is a temporary project outside any repository, so the
        // absent-repository cause travels through the same envelope.
        let git = findings
            .iter()
            .find(|finding| finding["code"] == "NOT_A_GIT_REPOSITORY")
            .unwrap_or_else(|| panic!("{tool} reports the Git environment: {findings:?}"));
        assert_eq!(git["severity"], "info", "{tool}");
        assert_eq!(git["fixable"], false, "{tool}");
        assert!(git["location"].is_null(), "{tool}");
    }

    client.shutdown().await;
}

#[tokio::test]
async fn doctor_includes_the_held_lock_probe() {
    let fixture = common::fixture(&["one"]);
    let project = fixture.project();
    let lock_path = project.lock_path();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();

    // Hold the real store lock on a thread while the diagnostic runs. The probe
    // must observe it without waiting.
    let holder = std::thread::spawn(move || {
        waymark_core::lock::with_write_lock(&lock_path, LockWait::immediate(), |_guard| {
            ready_tx.send(()).expect("the probe test is waiting");
            release_rx.recv().expect("the probe test releases the lock");
            Ok::<(), waymark_core::error::WaymarkError>(())
        })
        .expect("the lock is acquired");
    });
    ready_rx.recv().expect("the lock is held");

    let client = common::Client::connect(fixture.root.clone()).await;
    let result = client.call("waymark_doctor", common::no_args()).await;
    let envelope = common::envelope(&result);
    let findings = envelope["data"]["findings"].as_array().unwrap();
    assert!(
        findings
            .iter()
            .any(|finding| finding["code"] == "LOCK_HELD"),
        "the held-lock probe reported: {findings:?}"
    );
    client.shutdown().await;

    release_tx.send(()).expect("the holder is released");
    holder.join().expect("the holder thread ends");
}

#[tokio::test]
async fn config_set_writes_a_scalar_and_reports_the_revision() {
    let fixture = common::fixture(&["one"]);
    let before = fixture.project().config().version.revision;
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_config_set",
            json!({"name": "operator", "value": "frank"}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["local"]["operator"], "frank");
    assert_eq!(envelope["data"]["changed"], true);
    assert_eq!(envelope["data"]["changed_keys"], json!(["operator"]));
    assert_eq!(envelope["data"]["previous_revision"], json!(before));
    assert_eq!(
        envelope["data"]["revision"],
        json!(before),
        "a local write never moves the shared revision"
    );
    assert_eq!(envelope["_meta"]["source"], "direct_file");

    // The write is on disk, not only in the response.
    let project = fixture.project();
    let local = waymark_core::user_config::UserConfig::load(&project.user_config_path())
        .expect("the local document is valid");
    assert_eq!(local.operator.as_deref(), Some("frank"));
    assert_eq!(project.config().version.revision, before);

    // Repeating the same value is a true no-op: no revision move, no write.
    let result = client
        .call(
            "waymark_config_set",
            json!({"name": "operator", "value": "frank"}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["changed"], false);
    assert_eq!(envelope["data"]["changed_keys"], json!([]));
    assert_eq!(envelope["data"]["revision"], json!(before));

    client.shutdown().await;
}

#[tokio::test]
async fn config_category_and_field_writes_use_the_domain() {
    let fixture = common::fixture(&["one"]);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_config_add_category",
            json!({"key": "bug", "path": "work/bugs", "name": "Bugs"}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(
        envelope["data"]["config"]["categories"]["bug"]["path"],
        "work/bugs"
    );
    assert_eq!(
        envelope["data"]["config"]["categories"]["bug"]["name"],
        "Bugs"
    );
    assert!(
        fixture.root.join("work/bugs").is_dir(),
        "a real add creates the category directory"
    );

    let result = client
        .call(
            "waymark_config_update_category",
            json!({"key": "bug", "name": "Defects"}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(
        envelope["data"]["config"]["categories"]["bug"]["name"],
        "Defects"
    );
    assert_eq!(
        envelope["data"]["changed_keys"],
        json!(["categories.bug.name"])
    );

    let result = client
        .call(
            "waymark_config_add_field",
            json!({"key": "points", "type": "integer", "default": "0"}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(
        envelope["data"]["config"]["fields"]["points"]["type"],
        "integer"
    );
    assert_eq!(
        envelope["data"]["config"]["fields"]["points"]["required"],
        false
    );
    assert_eq!(envelope["data"]["config"]["fields"]["points"]["default"], 0);

    let result = client
        .call(
            "waymark_config_update_field",
            json!({"key": "points", "attribute": "default", "value": "5"}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["config"]["fields"]["points"]["default"], 5);
    assert_eq!(envelope["data"]["changed"], true);

    // Relaxing an already-optional field changes no key and writes nothing.
    let result = client
        .call(
            "waymark_config_update_field",
            json!({"key": "points", "attribute": "required", "value": "false"}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["changed"], false);

    let result = client
        .call("waymark_config_remove_category", json!({"key": "bug"}))
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert!(envelope["data"]["config"]["categories"]["bug"].is_null());
    assert_eq!(envelope["data"]["changed_keys"], json!(["categories.bug"]));

    client.shutdown().await;
}

#[tokio::test]
async fn config_writes_reuse_the_shared_codes() {
    let fixture = common::fixture(&["one"]);
    let client = common::Client::connect(fixture.root.clone()).await;

    // A required field is migration territory, not a configuration write.
    let result = client
        .call(
            "waymark_config_add_field",
            json!({"key": "points", "type": "integer", "required": true}),
        )
        .await;
    let error = common::business_error(&result);
    assert_eq!(error["code"], "ACTION_REQUIRES_MIGRATION");
    assert!(common::is_registered_code(&error["code"]));

    // The same for changing a declared type.
    client
        .call(
            "waymark_config_add_field",
            json!({"key": "note", "type": "string"}),
        )
        .await;
    let result = client
        .call(
            "waymark_config_update_field",
            json!({"key": "note", "attribute": "type", "value": "integer"}),
        )
        .await;
    let error = common::business_error(&result);
    assert_eq!(error["code"], "ACTION_REQUIRES_MIGRATION");

    // A category that is still in use is refused with the shared validation
    // error naming every item that uses it.
    let result = client
        .call("waymark_config_remove_category", json!({"key": "task"}))
        .await;
    let error = common::business_error(&result);
    assert_eq!(error["code"], "VALIDATION_ERROR");
    assert!(error["details"]["issues"].is_array());

    // An undeclared category is `CATEGORY_NOT_FOUND`.
    let result = client
        .call(
            "waymark_config_update_category",
            json!({"key": "nope", "name": "Nope"}),
        )
        .await;
    let error = common::business_error(&result);
    assert_eq!(error["code"], "CATEGORY_NOT_FOUND");

    client.shutdown().await;
}
