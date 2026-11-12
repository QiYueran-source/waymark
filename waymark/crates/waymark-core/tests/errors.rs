//! The error contract: the location union, the typed details of every
//! constructor, the `retryable`/`exit_class` projection, and the versioned
//! envelopes.
//!
//! The registry table itself — every code's exit class, retryability, details
//! keys, location kinds, and finding severity — is pinned in `registry.rs`
//! against `tests/golden/error_registry.json`. This file pins the *behaviour*
//! around it.

use serde_json::json;
use waymark_core::envelope::{
    adapter_failure_envelope, failure_envelope, success_envelope, Finding, Meta, Warning,
};
use waymark_core::error::{Code, ExitClass, Severity, ValidationIssue, WaymarkError};
use waymark_core::id::WorkItemId;
use waymark_core::location::{Location, LocationKind};

fn id() -> WorkItemId {
    WorkItemId::parse("wm-000000000001").unwrap()
}

fn other() -> WorkItemId {
    WorkItemId::parse("wm-000000000002").unwrap()
}

#[test]
fn locations_are_a_tagged_union_over_the_five_kinds() {
    let item = id();
    let cases = [
        (
            Location::work_item(&item),
            json!({"kind": "work_item", "id": "wm-000000000001"}),
        ),
        (
            Location::file("/tmp/project/work/bugs/wm-000000000001.md"),
            json!({"kind": "file", "path": "/tmp/project/work/bugs/wm-000000000001.md"}),
        ),
        (
            Location::field("title"),
            json!({"kind": "field", "field": "title"}),
        ),
        (
            Location::config_key("categories"),
            json!({"kind": "config_key", "key": "categories"}),
        ),
        (
            Location::relationship(&item, "dependencies", &other()),
            json!({
                "kind": "relationship",
                "from": "wm-000000000001",
                "relation": "dependencies",
                "to": "wm-000000000002"
            }),
        ),
    ];
    for (location, expected) in cases {
        assert_eq!(serde_json::to_value(&location).unwrap(), expected);
        let round_trip: Location = serde_json::from_value(expected).unwrap();
        assert_eq!(round_trip, location);
    }
}

/// The anti-drift test the brief asks for: every typed constructor produces
/// exactly the details keys its code declares, no more and no fewer.
///
/// A constructor that forgets a key, invents one, or leaves one out because the
/// argument was optional fails here instead of surfacing as a machine-contract
/// break in a consumer.
#[test]
fn every_typed_constructor_fills_exactly_the_registered_details_keys() {
    let mut cases: Vec<(&str, WaymarkError)> = vec![
        // The one code whose details are a superset depending on the caller:
        // a direct lookup names no relation.
        ("item_not_found", WaymarkError::item_not_found(&id())),
        (
            "referenced_item_not_found",
            WaymarkError::referenced_item_not_found(&other(), "dependencies"),
        ),
        (
            "category_not_found",
            WaymarkError::category_not_found("bug"),
        ),
        (
            "validation",
            WaymarkError::validation(
                "create",
                vec![ValidationIssue::new("title", "a title is required")],
            ),
        ),
        (
            "invalid_id",
            WaymarkError::invalid_id(
                "nope",
                &WorkItemId::parse("nope").expect_err("`nope` is not an ID"),
            ),
        ),
        (
            "parse",
            WaymarkError::parse("/p/wm-000000000001.md", "bad frontmatter"),
        ),
        (
            "config_invalid",
            WaymarkError::config_invalid("/p/.waymark/config.toml", vec!["version: bad".into()]),
        ),
        (
            "duplicate_id",
            WaymarkError::duplicate_id(
                &id(),
                vec![
                    "/p/a/wm-000000000001.md".into(),
                    "/p/b/wm-000000000001.md".into(),
                ],
            ),
        ),
        (
            "id_path_mismatch",
            WaymarkError::id_path_mismatch(
                &item_path_id(),
                camino::Utf8Path::new("/p/wm-000000000002.md"),
                "wm-000000000002",
            ),
        ),
        (
            "cycle_detected",
            WaymarkError::cycle_detected("dependency", vec![id(), other(), id()]),
        ),
        (
            "invalid_relationship",
            WaymarkError::invalid_relationship("dependencies", &id(), &id(), "self-reference"),
        ),
        (
            "incomplete_projection",
            WaymarkError::incomplete_projection("scoped", "stale", vec![json!({"code": "X"})]),
        ),
        ("web_not_included", WaymarkError::web_not_included()),
        (
            "write_conflict",
            WaymarkError::write_conflict(&id(), "/p/wm-000000000001.md", "aa", "bb"),
        ),
        (
            "item_archived",
            WaymarkError::item_archived(&id(), "set_title"),
        ),
        ("already_archived", WaymarkError::already_archived(&id())),
        (
            "has_active_children",
            WaymarkError::has_active_children(&id(), &[other()]),
        ),
        (
            "project_not_found",
            WaymarkError::project_not_found("/tmp/nowhere"),
        ),
        (
            "io",
            WaymarkError::io(
                "/p/wm-000000000001.md",
                "read",
                &std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"),
            ),
        ),
        (
            "git_error",
            WaymarkError::git_error("log", "not a repository"),
        ),
        (
            "lock_timeout",
            WaymarkError::lock_timeout("/p/.waymark/lock", 1500),
        ),
        (
            "lock_failed",
            WaymarkError::lock_failed("/p/.waymark/lock", "permission denied"),
        ),
        ("snapshot_unstable", WaymarkError::snapshot_unstable(3)),
        ("port_in_use", WaymarkError::port_in_use(7331)),
        (
            "unsupported_version",
            WaymarkError::unsupported_version("3", "1 and 2"),
        ),
        (
            "item_needs_migration",
            WaymarkError::item_needs_migration(&id(), 1, 2),
        ),
        (
            "action_requires_migration",
            WaymarkError::action_requires_migration("set_title", "waymark migrate"),
        ),
        (
            "migration_path_conflict",
            WaymarkError::migration_path_conflict("/p/work/bugs", "an existing directory"),
        ),
        (
            "migration_unsupported_generation",
            WaymarkError::migration_unsupported_generation(3, 2),
        ),
        (
            "migration_precondition_failed",
            WaymarkError::migration_precondition_failed(vec![json!({"code": "X"})]),
        ),
        (
            "migration_failed",
            WaymarkError::migration_failed(vec![waymark_core::error::TargetOutcome::new(
                "/p/a.md", "failed",
            )
            .with_error("boom")]),
        ),
        (
            "rollback_failed",
            WaymarkError::rollback_failed(
                vec!["/p/a.md".into()],
                vec!["/p/b.md".into()],
                vec!["/p/a.md".into()],
            ),
        ),
        (
            "id_generation_failed",
            WaymarkError::id_generation_failed(64),
        ),
        ("internal", WaymarkError::internal("boom")),
        (
            "owner_mismatch",
            WaymarkError::owner_mismatch(&id(), "frank", "alice"),
        ),
        ("lock_held", WaymarkError::lock_held("/p/.waymark/lock")),
        (
            "migration_available",
            WaymarkError::migration_available(&[id(), other()]),
        ),
        ("git_unavailable", WaymarkError::git_unavailable()),
        ("not_a_git_repository", WaymarkError::not_a_git_repository()),
        (
            "dangling_reference",
            WaymarkError::dangling_reference(&id(), "dependencies", &[other()]),
        ),
        (
            "timestamp_incoherent",
            WaymarkError::timestamp_incoherent(
                &id(),
                "updated_at",
                "2020-01-01T00:00:00Z",
                "before created_at",
            ),
        ),
        (
            "assets_version_mismatch",
            WaymarkError::assets_version_mismatch("version", "0.1.0", "0.2.0"),
        ),
        (
            "closed_parent",
            WaymarkError::closed_parent(&id(), &other()),
        ),
        (
            "one_sided_soft_link",
            WaymarkError::one_sided_soft_link(&id(), &other()),
        ),
        (
            "cascade_consent_required",
            WaymarkError::cascade_consent_required(
                "link add",
                &id(),
                &other(),
                "re-run with --cascade to also record the link there",
            ),
        ),
        (
            "owned_by_other",
            WaymarkError::owned_by_other(&other(), "alice", "link add"),
        ),
        (
            "missing_operator_source",
            WaymarkError::missing_operator_source("link add", &other(), "/p/.waymark/user.toml"),
        ),
    ];

    let mut seen: Vec<Code> = Vec::new();
    for (constructor, error) in cases.drain(..) {
        let code = error.code();
        let declared: Vec<&str> = code.details_keys().to_vec();
        let optional: &[&str] = code.optional_details_keys();
        let actual: Vec<&str> = error.details().keys().map(String::as_str).collect();

        // Never invent a key the registry does not declare.
        for key in &actual {
            assert!(
                declared.contains(key),
                "`{constructor}` invented the details key `{key}` for {}",
                code.as_str()
            );
        }
        // Never omit a key the registry requires.
        for key in &declared {
            if optional.contains(key) {
                continue;
            }
            assert!(
                actual.contains(key),
                "`{constructor}` omitted the required details key `{key}` for {}",
                code.as_str()
            );
        }
        // Every declared optional key is also declared, so the two lists agree.
        for key in optional {
            assert!(
                declared.contains(key),
                "{} declares `{key}` optional without declaring it required-or-optional",
                code.as_str()
            );
        }

        // A registered location kind must accept the location it produced.
        if let Some(location) = error.location() {
            assert!(
                code.location_kinds().contains(&location.kind()),
                "`{constructor}` located at {:?}, which {} may not carry",
                location.kind(),
                code.as_str()
            );
        }
        seen.push(code);
    }

    // Every code that declares details keys has a constructor above.
    for code in Code::ALL {
        if !code.details_keys().is_empty() {
            assert!(
                seen.contains(code),
                "{} declares details but no constructor exercises it",
                code.as_str()
            );
        }
    }
}

/// The ID written to a file whose name says otherwise.
fn item_path_id() -> WorkItemId {
    WorkItemId::parse("wm-000000000001").unwrap()
}

#[test]
fn an_error_carries_a_registered_code_details_location_and_retryability() {
    let err = WaymarkError::write_conflict(&id(), "/p/wm-000000000001.md", "aa", "bb");

    assert_eq!(err.code(), Code::WriteConflict);
    assert_eq!(err.exit_class(), Some(ExitClass::Conflict));
    assert_eq!(err.location(), Some(&Location::work_item(&id())));
    assert!(!err.retryable());
    assert_eq!(err.details()["expected_fingerprint"], json!("aa"));
    assert_eq!(
        err.message(),
        "wm-000000000001 changed on disk since it was read; reread and retry"
    );
}

#[test]
fn a_missing_web_build_names_the_package_and_a_busy_port_names_the_recovery() {
    let web = WaymarkError::web_not_included();
    assert_eq!(web.code(), Code::WebNotIncluded);
    assert_eq!(web.exit_class(), Some(ExitClass::InvalidInput));
    assert!(
        web.message().contains("waymark-web"),
        "the message must name the Web package: {}",
        web.message()
    );

    let port = WaymarkError::port_in_use(7331);
    assert_eq!(port.code(), Code::PortInUse);
    assert_eq!(port.exit_class(), Some(ExitClass::Environment));
    assert_eq!(port.details()["port"], json!(7331));
    assert!(
        port.message().contains("7331"),
        "the message must name the port: {}",
        port.message()
    );
    assert!(
        port.message().contains("--port 0"),
        "the message must suggest --port 0: {}",
        port.message()
    );
}

#[test]
fn only_the_two_retryable_constructors_report_retryable() {
    assert!(WaymarkError::lock_timeout("/p/lock", 5).retryable());
    assert!(WaymarkError::snapshot_unstable(2).retryable());
    assert!(!WaymarkError::lock_failed("/p/lock", "busy").retryable());
    assert!(!WaymarkError::item_not_found(&id()).retryable());
}

#[test]
fn a_failure_envelope_is_versioned_and_identical_in_shape_across_adapters() {
    let err = WaymarkError::validation(
        "work item",
        vec![ValidationIssue::new("title", "a title is required")],
    )
    .at_field("title");

    let envelope = failure_envelope(&err);
    assert_eq!(envelope["v"], json!(1));
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["v"], json!(1));
    assert_eq!(envelope["error"]["code"], json!("VALIDATION_ERROR"));
    assert_eq!(envelope["error"]["retryable"], json!(false));
    assert_eq!(
        envelope["error"]["details"],
        json!({
            "target": "work item",
            "issues": [{"path": "title", "reason": "a title is required"}]
        })
    );
    assert_eq!(
        envelope["error"]["location"],
        json!({"kind": "field", "field": "title"})
    );
}

#[test]
fn an_adapter_owned_code_uses_the_same_failure_envelope() {
    let envelope = adapter_failure_envelope(
        "USAGE_ERROR",
        "unexpected argument '--nonsense' found",
        json!({}),
        None,
        false,
    );
    assert_eq!(envelope["v"], json!(1));
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("USAGE_ERROR"));
    assert_eq!(envelope["error"]["retryable"], json!(false));
    assert_eq!(envelope["error"]["details"], json!({}));
    assert!(envelope["error"]["location"].is_null());
}

#[test]
fn a_success_envelope_carries_data_and_metadata() {
    let envelope = success_envelope(json!({"id": "wm-000000000001"}), Meta::direct_file_scoped());
    assert_eq!(envelope["v"], json!(1));
    assert_eq!(envelope["ok"], json!(true));
    assert_eq!(envelope["data"]["id"], json!("wm-000000000001"));
    assert_eq!(envelope["_meta"]["source"], json!("direct_file"));
    assert_eq!(envelope["_meta"]["completeness"], json!("scoped"));
    assert_eq!(envelope["_meta"]["freshness"], json!("fresh"));
    assert_eq!(envelope["_meta"]["diagnostics"], json!([]));
    assert_eq!(envelope["_meta"]["warnings"], json!([]));
}

#[test]
fn a_warning_reuses_the_registry_and_never_carries_an_exit_class() {
    let error = WaymarkError::owner_mismatch(&id(), "frank", "alice");
    assert_eq!(
        error.code().exit_class(),
        None,
        "a warning is not a failure"
    );

    let warning = Warning::from_error(&error);
    assert_eq!(warning.code, Code::OwnerMismatch);
    assert_eq!(warning.location, Some(Location::work_item(&id())));
    assert_eq!(
        serde_json::to_value(&warning).unwrap(),
        json!({
            "code": "OWNER_MISMATCH",
            "message": "operator frank is not the owner alice of wm-000000000001",
            "location": {"kind": "work_item", "id": "wm-000000000001"},
        })
    );
}

#[test]
fn a_finding_carries_its_registry_severity_and_fixability() {
    let error = WaymarkError::dangling_reference(&id(), "dependencies", &[other()]);
    let finding = Finding::from_error(&error).fixable(false);
    assert_eq!(finding.severity, Severity::Error);
    assert_eq!(finding.code, Code::DanglingReference);
    assert!(!finding.fixable);

    let marked_fixable = Finding::from_error(&WaymarkError::lock_held("/p/lock")).fixable(true);
    assert_eq!(marked_fixable.severity, Severity::Info);
    assert!(marked_fixable.fixable);
}

#[test]
fn warnings_and_findings_do_not_change_what_the_envelope_reports_as_ok() {
    let meta = Meta::direct_file_scoped()
        .with_warnings(vec![Warning::from_error(&WaymarkError::owner_mismatch(
            &id(),
            "frank",
            "alice",
        ))])
        .with_diagnostics(vec![Finding::from_error(
            &WaymarkError::timestamp_incoherent(
                &id(),
                "updated_at",
                "2020-01-01T00:00:00Z",
                "before created_at",
            ),
        )]);
    let envelope = success_envelope(json!({"id": "wm-000000000001"}), meta);
    assert_eq!(envelope["ok"], json!(true));
    assert_eq!(
        envelope["_meta"]["warnings"][0]["code"],
        json!("OWNER_MISMATCH")
    );
    assert_eq!(
        envelope["_meta"]["diagnostics"][0]["severity"],
        json!("warning")
    );
    assert_eq!(
        envelope["_meta"]["diagnostics"][0]["code"],
        json!("TIMESTAMP_INCOHERENT")
    );
}

#[test]
fn a_meta_round_trips_through_json() {
    let meta = Meta::direct_file_scoped().with_warnings(vec![Warning::new(
        Code::OwnerMismatch,
        "operator is not the owner",
    )]);
    let json = serde_json::to_value(&meta).unwrap();
    let back: Meta = serde_json::from_value(json).unwrap();
    assert_eq!(back, meta);
}

#[test]
fn exit_classes_are_two_through_seven() {
    assert_eq!(ExitClass::NotFound.as_i32(), 2);
    assert_eq!(ExitClass::InvalidInput.as_i32(), 3);
    assert_eq!(ExitClass::Conflict.as_i32(), 4);
    assert_eq!(ExitClass::Environment.as_i32(), 5);
    assert_eq!(ExitClass::Version.as_i32(), 6);
    assert_eq!(ExitClass::Internal.as_i32(), 7);
}

#[test]
fn usage_error_is_not_a_registry_code() {
    // The adapter-level code lives in the CLI. Nothing in the registry may
    // resolve to it, so a domain caller can never claim an exit-1 failure.
    assert!(Code::from_str("USAGE_ERROR").is_err());
    assert!(!Code::ALL.iter().any(|code| code.as_str() == "USAGE_ERROR"));
}

#[test]
#[should_panic(expected = "is a failure code and cannot be a warning")]
fn a_failure_cannot_be_reported_as_a_warning() {
    // The type-level guard on the "warnings never change the exit code" rule:
    // a code with an exit class cannot be smuggled into `_meta.warnings`, where
    // it would look informational beside a zero exit.
    let _ = Warning::from_error(&WaymarkError::item_not_found(&id()));
}

#[test]
#[should_panic(expected = "is not a finding code")]
fn a_failure_only_code_cannot_be_reported_as_a_finding() {
    let _ = Finding::from_error(&WaymarkError::item_not_found(&id()));
}

#[test]
fn every_location_kind_literal_matches_its_location_discriminator() {
    // The registry declares kinds as `LocationKind`; the union declares them as
    // serde discriminators. They must be the same five strings, or a registry row
    // would claim a kind the wire never produces.
    let item = id();
    let samples = [
        (LocationKind::WorkItem, Location::work_item(&item)),
        (LocationKind::File, Location::file("/p/wm-000000000001.md")),
        (LocationKind::Field, Location::field("title")),
        (LocationKind::ConfigKey, Location::config_key("operator")),
        (
            LocationKind::Relationship,
            Location::relationship(&item, "dependencies", &other()),
        ),
    ];
    assert_eq!(samples.len(), LocationKind::ALL.len());
    for (kind, location) in samples {
        assert_eq!(location.kind(), kind);
        let wire = serde_json::to_value(&location).unwrap();
        assert_eq!(wire["kind"], json!(kind.as_str()));
    }
}
