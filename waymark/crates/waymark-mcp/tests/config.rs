//! Configuration and diagnostic tools over the in-process MCP client.

mod common;

use std::sync::mpsc;

use serde_json::json;
use waymark_core::lock::LockWait;

#[tokio::test]
async fn config_read_returns_the_effective_configuration() {
    let fixture = common::fixture(&["one"]);
    // Record an operator through the shared configuration operation, so the
    // read proves it returns the effective document and not a stale copy.
    waymark_core::ops::config::set_scalar(
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
    assert_eq!(envelope["data"]["config"]["storage_root"], "work");
    assert_eq!(
        envelope["data"]["config"]["categories"]["task"]["path"],
        "work/task"
    );
    assert_eq!(envelope["data"]["config"]["operator"], "frank");
    assert!(envelope["data"]["config"]["revision"].is_number());
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
