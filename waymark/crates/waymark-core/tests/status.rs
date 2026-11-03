//! Lifecycle: the status operation.
//!
//! These tests drive the public `waymark_core` API against a temporary project
//! and assert both the returned outcome and the bytes on disk: a status change
//! is only real if the document says so.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use waymark_core::error::Code;
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::{create, status, OpOptions};
use waymark_core::testing::fixtures;
use waymark_core::{FixedClock, Project, Status};

/// The instant the fixtures start at.
fn base() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-03T05:43:10Z")
        .expect("a valid instant")
        .with_timezone(&Utc)
}

/// Options whose clock is frozen `seconds` after the fixture instant, so a test
/// can observe a timestamp move without a sleeping thread.
fn options_at(seconds: i64) -> OpOptions {
    OpOptions::new().with_clock(Arc::new(FixedClock::new(
        base() + chrono::Duration::seconds(seconds),
    )))
}

/// Create one item and return its stable ID.
fn make(project: &Project, title: &str) -> String {
    create::create(
        project,
        CreateRequest::new(title, "task", "frank"),
        &options_at(0),
    )
    .expect("creation succeeds")
    .item
    .id
    .as_str()
    .to_owned()
}

fn document(project: &Project, id: &str) -> String {
    let entry = project
        .locate(&waymark_core::WorkItemId::parse(id).unwrap())
        .expect("the item exists");
    std::fs::read_to_string(&entry.path).expect("the document is readable")
}

#[test]
fn entering_closed_sets_closed_at_from_the_operation_clock() {
    let (_dir, project) = fixtures::deterministic_project();
    let id = make(&project, "Fix the thing");

    let outcome = status::set_status(&project, &id, Status::Closed, &options_at(60))
        .expect("a closed transition is allowed");

    assert_eq!(outcome.item.status, Status::Closed);
    assert_eq!(outcome.previous, Status::Open);
    assert!(outcome.changed);
    assert_eq!(
        outcome.item.closed_at.map(|value| value.to_string()),
        Some("2026-11-03T05:44:10Z".to_owned())
    );
    assert_eq!(
        outcome.item.updated_at.to_string(),
        "2026-11-03T05:44:10Z",
        "a real status change updates updated_at"
    );

    let on_disk = document(&project, &id);
    assert!(on_disk.contains("\nstatus: closed\n"), "{on_disk}");
    assert!(
        on_disk.contains("\nclosed_at: 2026-11-03T05:44:10Z\n"),
        "{on_disk}"
    );
}

#[test]
fn leaving_closed_clears_closed_at() {
    let (_dir, project) = fixtures::deterministic_project();
    let id = make(&project, "Fix the thing");
    status::set_status(&project, &id, Status::Closed, &options_at(60)).unwrap();

    let outcome = status::set_status(&project, &id, Status::InProgress, &options_at(120))
        .expect("reopening is allowed");

    assert_eq!(outcome.item.status, Status::InProgress);
    assert_eq!(outcome.previous, Status::Closed);
    assert_eq!(outcome.item.closed_at, None);
    assert_eq!(outcome.item.updated_at.to_string(), "2026-11-03T05:45:10Z");

    let on_disk = document(&project, &id);
    assert!(on_disk.contains("\nstatus: in_progress\n"), "{on_disk}");
    assert!(!on_disk.contains("closed_at:"), "{on_disk}");
}

#[test]
fn repeating_the_current_status_changes_nothing() {
    let (_dir, project) = fixtures::deterministic_project();
    let id = make(&project, "Fix the thing");
    status::set_status(&project, &id, Status::InProgress, &options_at(60)).unwrap();
    let before = document(&project, &id);

    let outcome = status::set_status(&project, &id, Status::InProgress, &options_at(600))
        .expect("a repeat is not a failure");

    assert!(!outcome.changed);
    assert_eq!(
        outcome.item.updated_at.to_string(),
        "2026-11-03T05:44:10Z",
        "a no-op leaves updated_at alone"
    );
    assert_eq!(document(&project, &id), before, "a no-op performs no write");
}

#[test]
fn only_the_three_statuses_are_accepted() {
    let (_dir, project) = fixtures::deterministic_project();
    let id = make(&project, "Fix the thing");

    for literal in [
        "open",
        "in_progress",
        "closed",
        "in progress",
        "in-progress",
    ] {
        assert!(
            status::parse_status(literal).is_ok(),
            "{literal} is a status"
        );
    }
    // The literal is case-insensitive and accepts the two human spellings of
    // `in_progress`, exactly like the rest of the CLI's status vocabulary; it is
    // never a wildcard for a status-shaped word.
    assert_eq!(status::parse_status("CLOSED").unwrap(), Status::Closed);
    for literal in ["blocked", "ready", "done", "", "wip", "open-ish"] {
        let error = status::parse_status(literal).expect_err(literal);
        assert_eq!(error.code(), Code::ValidationError, "literal {literal:?}");
        assert!(error.message().contains("status"), "{literal:?}: {error}");
    }

    // A blocked item is derived, never a writable status: the operation cannot
    // express one because the type it takes has exactly three variants.
    assert!(
        !status::set_status(&project, &id, Status::Open, &options_at(60))
            .unwrap()
            .changed
    );
}

#[test]
fn an_archived_item_rejects_a_status_change() {
    let (_dir, project) = fixtures::deterministic_project();
    let id = make(&project, "Fix the thing");
    let entry = project
        .locate(&waymark_core::WorkItemId::parse(&id).unwrap())
        .unwrap();
    let mut item = waymark_core::parse::parse_item_file(&entry.path).unwrap();
    item.archived_at = Some(waymark_core::testing::instant());
    std::fs::write(&entry.path, waymark_core::serialize::to_document(&item)).unwrap();
    let before = document(&project, &id);

    let error = status::set_status(&project, &id, Status::Closed, &options_at(60))
        .expect_err("an archived item rejects writes");

    assert_eq!(error.code(), Code::ItemArchived);
    assert_eq!(error.details()["operation"], serde_json::json!("status"));
    assert_eq!(document(&project, &id), before, "nothing was written");
}

#[test]
fn an_owner_mismatch_is_a_warning_not_a_refusal() {
    let (_dir, project) = fixtures::deterministic_project();
    let id = make(&project, "Fix the thing");
    let options = options_at(60).with_operator("alice");

    let outcome = status::set_status(&project, &id, Status::Closed, &options)
        .expect("a mismatch never blocks the write");

    assert_eq!(outcome.item.status, Status::Closed);
    assert_eq!(
        outcome.meta.warnings.len(),
        1,
        "{:?}",
        outcome.meta.warnings
    );
    assert_eq!(outcome.meta.warnings[0].code, Code::OwnerMismatch);
    assert!(outcome.meta.warnings[0].message.contains("alice"));
}

#[test]
fn a_matching_operator_emits_no_warning() {
    let (_dir, project) = fixtures::deterministic_project();
    let id = make(&project, "Fix the thing");
    let outcome = status::set_status(
        &project,
        &id,
        Status::InProgress,
        &options_at(60).with_operator("frank"),
    )
    .unwrap();
    assert!(outcome.meta.warnings.is_empty());
}

#[test]
fn a_status_change_of_an_absent_item_is_item_not_found() {
    let (_dir, project) = fixtures::deterministic_project();
    let error = status::set_status(&project, "wm-000000000000", Status::Closed, &options_at(0))
        .expect_err("there is no such item");
    assert_eq!(error.code(), Code::ItemNotFound);
}

#[test]
fn a_dry_run_reports_the_transition_without_writing() {
    let (_dir, project) = fixtures::deterministic_project();
    let id = make(&project, "Fix the thing");
    let before = document(&project, &id);

    let outcome =
        status::set_status(&project, &id, Status::Closed, &options_at(60).dry_run()).unwrap();

    assert!(outcome.changed);
    assert!(!outcome.written);
    assert_eq!(outcome.item.status, Status::Closed);
    assert_eq!(document(&project, &id), before);
}
