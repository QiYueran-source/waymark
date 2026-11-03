//! The error contract: stable codes, exit classes, retryability, the location
//! union, and the versioned failure envelope.

use serde_json::json;
use waymark_core::envelope::failure_envelope;
use waymark_core::error::{Code, ValidationIssue, WaymarkError};
use waymark_core::id::WorkItemId;
use waymark_core::location::Location;

/// Every code this ticket can produce, with its pinned class and retryability.
/// Kept here (not in the library) so a change to the table is a visible change.
const TABLE: &[(Code, &str, u8, bool)] = &[
    (Code::ItemNotFound, "ITEM_NOT_FOUND", 2, false),
    (Code::ValidationError, "VALIDATION_ERROR", 3, false),
    (Code::ParseError, "PARSE_ERROR", 3, false),
    (Code::ConfigInvalid, "CONFIG_INVALID", 3, false),
    (Code::InvalidId, "INVALID_ID", 3, false),
    (Code::WriteConflict, "WRITE_CONFLICT", 4, false),
    (Code::ProjectNotFound, "PROJECT_NOT_FOUND", 5, false),
    (Code::IoError, "IO_ERROR", 5, false),
    (Code::LockTimeout, "LOCK_TIMEOUT", 5, true),
    (Code::LockFailed, "LOCK_FAILED", 5, false),
    (Code::IdGenerationFailed, "ID_GENERATION_FAILED", 7, false),
    (Code::InternalError, "INTERNAL_ERROR", 7, false),
    (Code::UsageError, "USAGE_ERROR", 1, false),
];

#[test]
fn every_code_has_a_stable_literal_and_exit_class() {
    for (code, literal, exit, retryable) in TABLE {
        assert_eq!(code.as_str(), *literal, "literal for {code:?}");
        assert_eq!(code.exit_class(), *exit, "exit class for {literal}");
        assert_eq!(code.retryable(), *retryable, "retryable for {literal}");
        assert_eq!(serde_json::to_value(code).unwrap(), json!(literal));
    }
}

#[test]
fn the_enum_has_no_code_the_contract_forgot() {
    for code in Code::ALL {
        assert!(
            TABLE.iter().any(|(known, ..)| known == code),
            "{code:?} is missing from the pinned table"
        );
    }
    assert_eq!(Code::ALL.len(), TABLE.len());
}

#[test]
fn only_lock_timeout_is_retryable() {
    let retryable: Vec<Code> = Code::ALL
        .iter()
        .copied()
        .filter(|code| code.retryable())
        .collect();
    assert_eq!(retryable, vec![Code::LockTimeout]);
}

#[test]
fn locations_are_a_tagged_union_over_the_five_kinds() {
    let id = WorkItemId::parse("wm-000000000001").unwrap();
    let cases = [
        (
            Location::work_item(&id),
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
            Location::relationship(&id, "dependencies", &id),
            json!({
                "kind": "relationship",
                "from": "wm-000000000001",
                "relation": "dependencies",
                "to": "wm-000000000001"
            }),
        ),
    ];
    for (location, expected) in cases {
        assert_eq!(serde_json::to_value(&location).unwrap(), expected);
        let round_trip: Location = serde_json::from_value(expected).unwrap();
        assert_eq!(round_trip, location);
    }
}

#[test]
fn an_error_carries_a_registered_code_typed_details_location_and_retryability() {
    let id = WorkItemId::parse("wm-000000000001").unwrap();
    let err = WaymarkError::new(Code::WriteConflict, "the item changed on disk")
        .with_details(json!({
            "id": id.as_str(),
            "path": "/p/wm-000000000001.md",
            "expected_fingerprint": "aa",
            "actual_fingerprint": "bb"
        }))
        .with_location(Location::work_item(&id));

    assert_eq!(err.code(), Code::WriteConflict);
    assert_eq!(err.message(), "the item changed on disk");
    assert_eq!(err.location(), Some(&Location::work_item(&id)));
    assert!(!err.retryable());
    assert_eq!(err.details()["expected_fingerprint"], json!("aa"));
}

#[test]
fn a_failure_envelope_is_versioned_and_identical_in_shape_across_adapters() {
    let err = WaymarkError::validation(
        "work item",
        vec![ValidationIssue::new("title", "a title is required")],
    )
    .with_location(Location::field("title"));

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
fn a_success_envelope_carries_data_and_metadata() {
    use waymark_core::envelope::{success_envelope, Meta};

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
