//! The frozen error registry, pinned five ways:
//!
//! 1. a golden snapshot of every code's exit class, retryability, details keys,
//!    location kinds, and finding severity;
//! 2. a consistency test that the snapshot and [`Code::ALL`] name the same codes
//!    exactly once each;
//! 3. a string and serde round-trip for every code;
//! 4. the classifier invariants the spec states in prose (only two retryable
//!    codes, seven finding-only codes with no exit class);
//! 5. a JSON-schema enum of the wire literals, in registry/table order.
//!
//! Adding, removing, or reclassifying a code therefore requires updating
//! `tests/golden/error_registry.json` and the schema's `enum` in the same
//! change. Regenerate the snapshot with
//!
//! ```sh
//! UPDATE_ERROR_REGISTRY=1 cargo test -p waymark-core --test registry
//! ```
//!
//! and then review the diff: the snapshot is the contract, not an artifact.

use std::collections::BTreeSet;

use serde_json::{json, Value};
use waymark_core::error::{Code, ExitClass, Severity};

const GOLDEN: &str = include_str!("golden/error_registry.json");

const SCHEMA: &str = include_str!("golden/error_codes.schema.json");

const GOLDEN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/golden/error_registry.json"
);

/// The finding-only codes: they never fail an operation and therefore have no
/// exit class.
const FINDING_ONLY: &[Code] = &[
    Code::OwnerMismatch,
    Code::LockHeld,
    Code::MigrationAvailable,
    Code::GitUnavailable,
    Code::NotAGitRepository,
    Code::DanglingReference,
    Code::TimestampIncoherent,
];

fn row(code: Code) -> Value {
    json!({
        "code": code.as_str(),
        "exit": code.exit_class().map(ExitClass::as_i32),
        "retryable": code.retryable(),
        "details": code.details_keys(),
        "location_kinds": code
            .location_kinds()
            .iter()
            .map(|kind| kind.as_str())
            .collect::<Vec<_>>(),
        "finding": code.finding().map(Severity::as_str),
    })
}

fn snapshot() -> Value {
    let mut rows: Vec<Value> = Code::ALL.iter().copied().map(row).collect();
    rows.sort_by(|left, right| {
        left["code"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["code"].as_str().unwrap_or_default())
    });
    Value::Array(rows)
}

fn rendered_snapshot() -> String {
    let mut text = serde_json::to_string_pretty(&snapshot()).expect("the snapshot is JSON");
    text.push('\n');
    text
}

#[test]
fn the_golden_snapshot_matches_the_registry() {
    if std::env::var_os("UPDATE_ERROR_REGISTRY").is_some() {
        std::fs::write(GOLDEN_PATH, rendered_snapshot()).expect("the golden file is writable");
        return;
    }
    let expected: Value = serde_json::from_str(GOLDEN).expect("the golden file is valid JSON");
    assert_eq!(
        expected,
        snapshot(),
        "the error registry changed; review the diff and regenerate \
         `tests/golden/error_registry.json` with UPDATE_ERROR_REGISTRY=1"
    );
}

#[test]
fn the_json_schema_enum_matches_the_registry_in_table_order() {
    let schema: Value = serde_json::from_str(SCHEMA).expect("the schema is valid JSON");
    assert_eq!(schema["type"], "string", "a wire code is a string");
    let choices = schema["enum"]
        .as_array()
        .expect("the schema declares an enum of codes");

    let expected: Vec<&str> = Code::ALL.iter().map(|code| code.as_str()).collect();
    let actual: Vec<&str> = choices
        .iter()
        .map(|choice| choice.as_str().expect("every enum value is a string"))
        .collect();
    assert_eq!(
        actual, expected,
        "the schema enum must be the registry, in table order"
    );
    assert!(
        actual.contains(&"GIT_UNAVAILABLE"),
        "the schema pins the Git availability finding too"
    );
}

#[test]
fn every_registry_code_appears_in_the_snapshot_exactly_once() {
    let golden: Value = serde_json::from_str(GOLDEN).expect("the golden file is valid JSON");
    let rows = golden.as_array().expect("the snapshot is an array");

    let mut seen = BTreeSet::new();
    for row in rows {
        let code = row["code"].as_str().expect("every row names a code");
        assert!(seen.insert(code.to_owned()), "`{code}` appears twice");
    }

    let expected: BTreeSet<String> = Code::ALL
        .iter()
        .map(|code| code.as_str().to_owned())
        .collect();
    assert_eq!(
        seen, expected,
        "the snapshot and `Code::ALL` must name the same codes"
    );
    assert_eq!(rows.len(), Code::ALL.len(), "one row per code");
}

#[test]
fn the_snapshot_has_no_row_the_enum_lacks() {
    let golden: Value = serde_json::from_str(GOLDEN).expect("the golden file is valid JSON");
    for row in golden.as_array().expect("the snapshot is an array") {
        let literal = row["code"].as_str().expect("every row names a code");
        let code = Code::from_str(literal).unwrap_or_else(|_| panic!("`{literal}` is not a Code"));
        assert_eq!(code.as_str(), literal, "the literal is canonical");
    }
}

#[test]
fn a_code_round_trips_through_its_string_and_serde_forms() {
    for code in Code::ALL {
        let literal = code.as_str();
        assert_eq!(Code::from_str(literal).unwrap(), *code, "string round-trip");
        assert_eq!(
            serde_json::to_value(code).unwrap(),
            json!(literal),
            "serde uses the canonical literal"
        );
        let back: Code = serde_json::from_value(json!(literal)).unwrap();
        assert_eq!(&back, code, "serde round-trip");
        assert!(
            Code::from_str(&literal.to_lowercase()).is_err(),
            "`{literal}` is matched exactly, not case-insensitively"
        );
    }
}

#[test]
fn git_unavailable_is_a_finding_only_info_code() {
    let code = Code::from_str("GIT_UNAVAILABLE").expect("GIT_UNAVAILABLE is a registered code");
    assert_eq!(code.exit_class(), None, "a finding never fails a command");
    assert!(!code.retryable());
    assert_eq!(code.finding(), Some(Severity::Info));
    assert!(code.details_keys().is_empty(), "no typed details");
    assert!(
        code.location_kinds().is_empty(),
        "a missing binary is an environment fact, not a document location"
    );
    assert_eq!(Code::ALL.len(), 38, "the registry grew by exactly one code");
}

#[test]
fn only_lock_timeout_and_snapshot_unstable_are_retryable() {
    let retryable: Vec<Code> = Code::ALL
        .iter()
        .copied()
        .filter(|code| code.retryable())
        .collect();
    assert_eq!(retryable, vec![Code::LockTimeout, Code::SnapshotUnstable]);
}

#[test]
fn the_finding_only_codes_have_no_exit_class() {
    for code in Code::ALL {
        let finding_only = FINDING_ONLY.contains(code);
        assert_eq!(
            code.exit_class().is_none(),
            finding_only,
            "{} is a finding-only code",
            code.as_str()
        );
        if finding_only {
            assert!(
                code.finding().is_some(),
                "{} is a finding-only code, so it declares a severity",
                code.as_str()
            );
        }
    }
}

#[test]
fn a_failure_code_may_or_may_not_also_be_a_finding() {
    // Failure codes that the spec also lists as diagnostic findings.
    let also_findings = [
        Code::ValidationError,
        Code::ParseError,
        Code::ConfigInvalid,
        Code::DuplicateId,
        Code::IdPathMismatch,
        Code::CycleDetected,
        Code::IncompleteProjection,
        Code::IoError,
        Code::UnsupportedVersion,
        Code::ItemNeedsMigration,
    ];
    for code in also_findings {
        assert!(
            code.exit_class().is_some(),
            "{} fails operations",
            code.as_str()
        );
        assert!(
            code.finding().is_some(),
            "{} is also a diagnostic finding",
            code.as_str()
        );
    }
    // The rest of the failure codes are failure-only.
    for code in Code::ALL {
        if code.exit_class().is_some() && !also_findings.contains(code) {
            assert!(code.finding().is_none(), "{}", code.as_str());
        }
    }
}

#[test]
fn the_finding_only_codes_keep_their_pinned_severity() {
    let expected = [
        (Code::OwnerMismatch, Severity::Warning),
        (Code::LockHeld, Severity::Info),
        (Code::MigrationAvailable, Severity::Info),
        (Code::GitUnavailable, Severity::Info),
        (Code::NotAGitRepository, Severity::Info),
        (Code::DanglingReference, Severity::Error),
        (Code::TimestampIncoherent, Severity::Warning),
    ];
    for (code, severity) in expected {
        assert_eq!(code.finding(), Some(severity), "{}", code.as_str());
    }
}

#[test]
fn exit_classes_cover_two_through_seven() {
    let mut classes: Vec<i32> = ExitClass::ALL.iter().map(|class| class.as_i32()).collect();
    classes.sort_unstable();
    assert_eq!(classes, vec![2, 3, 4, 5, 6, 7]);
}
