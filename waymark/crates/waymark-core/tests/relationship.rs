//! Dependency edges: adding and removing directed hard dependencies.
//!
//! Two seams are exercised here: the operation (`add_dependency` /
//! `remove_dependency`) and the command bytes it leaves behind. The rules the
//! operation enforces are the ones the spec names — a self-reference, a
//! duplicate edge, or a wrong relation kind is `INVALID_RELATIONSHIP`; a cycle is
//! `CYCLE_DETECTED` carrying the path; a missing target is `ITEM_NOT_FOUND`; an
//! archived target is `ITEM_ARCHIVED` — and every accepted change moves
//! `updated_at`.

use waymark_core::error::Code;
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::relationship::{add_dependency, remove_dependency};
use waymark_core::ops::{create, read, status, OpOptions};
use waymark_core::testing::fixtures;
use waymark_core::{Project, Status};

fn options() -> OpOptions {
    OpOptions::new().with_clock(fixtures::shared_clock())
}

fn make(project: &Project, title: &str) -> String {
    create::create(
        project,
        CreateRequest::new(title, "task", "frank"),
        &options(),
    )
    .unwrap()
    .item
    .id
    .as_str()
    .to_owned()
}

fn document(project: &Project, id: &str) -> String {
    std::fs::read_to_string(read::read(project, id).unwrap().path).unwrap()
}

#[test]
fn adding_a_dependency_writes_a_sorted_deduplicated_list() {
    let (_dir, project) = fixtures::deterministic_project();
    let a = make(&project, "a");
    let b = make(&project, "b");
    let c = make(&project, "c");

    add_dependency(&project, &a, &c, &options()).unwrap();
    let outcome = add_dependency(&project, &a, &b, &options()).unwrap();

    assert!(outcome.changed);
    assert!(outcome.written);
    assert_eq!(outcome.dependency.as_str(), b);
    assert_eq!(
        outcome.item.dependencies,
        vec![
            waymark_core::WorkItemId::parse(&b).unwrap(),
            waymark_core::WorkItemId::parse(&c).unwrap()
        ]
    );
    let bytes = document(&project, &a);
    assert!(bytes.contains("dependencies:"), "{bytes}");
    assert!(
        bytes.find(&b).unwrap() < bytes.find(&c).unwrap(),
        "the canonical writer sorts:\n{bytes}"
    );
}

#[test]
fn a_new_edge_updates_updated_at() {
    let (_dir, project) = fixtures::deterministic_project();
    let a = make(&project, "a");
    let b = make(&project, "b");
    assert_eq!(
        read::read(&project, &a)
            .unwrap()
            .item
            .updated_at
            .to_string(),
        "2026-11-03T05:43:10Z"
    );

    let later = OpOptions::new().with_clock(std::sync::Arc::new(waymark_core::FixedClock::new(
        fixtures::instant().get() + chrono::Duration::minutes(5),
    )));
    add_dependency(&project, &a, &b, &later).unwrap();

    assert_eq!(
        read::read(&project, &a)
            .unwrap()
            .item
            .updated_at
            .to_string(),
        "2026-11-03T05:48:10Z"
    );
}

#[test]
fn a_self_reference_is_rejected_as_an_invalid_relationship() {
    let (_dir, project) = fixtures::deterministic_project();
    let a = make(&project, "a");

    let error = add_dependency(&project, &a, &a, &options()).unwrap_err();
    assert_eq!(error.code(), Code::InvalidRelationship);
    assert_eq!(
        error.details()["relation"],
        serde_json::json!("dependencies")
    );
    assert_eq!(error.details()["from"], serde_json::json!(a));
    assert_eq!(error.details()["to"], serde_json::json!(a));
    assert!(error.details()["reason"].as_str().unwrap().contains("self"));
}

#[test]
fn a_duplicate_edge_is_a_rejection_not_a_silent_no_op() {
    let (_dir, project) = fixtures::deterministic_project();
    let a = make(&project, "a");
    let b = make(&project, "b");
    add_dependency(&project, &a, &b, &options()).unwrap();
    let before = document(&project, &a);

    let error = add_dependency(&project, &a, &b, &options()).unwrap_err();
    assert_eq!(error.code(), Code::InvalidRelationship);
    assert!(error.details()["reason"]
        .as_str()
        .unwrap()
        .contains("already"));
    assert_eq!(document(&project, &a), before, "nothing was written");
}

#[test]
fn a_wrong_relation_kind_is_rejected() {
    let (_dir, project) = fixtures::deterministic_project();
    let a = make(&project, "a");
    let b = make(&project, "b");

    // `soft` is a real relationship on a work item but not a dependency, and
    // `parent` is a different edge again: neither may be requested through the
    // dependency operation.
    for relation in ["soft_links", "parent", "blocks", "child"] {
        let error = waymark_core::ops::relationship::add_relationship(
            &project,
            &a,
            relation,
            &b,
            &options(),
        )
        .expect_err("a wrong relation kind is refused");
        assert_eq!(
            error.code(),
            Code::InvalidRelationship,
            "relation {relation}"
        );
        assert_eq!(error.details()["relation"], serde_json::json!(relation));
    }
}

#[test]
fn a_cycle_is_rejected_with_its_path() {
    let (_dir, project) = fixtures::deterministic_project();
    let a = make(&project, "a");
    let b = make(&project, "b");
    let c = make(&project, "c");
    add_dependency(&project, &b, &a, &options()).unwrap();
    add_dependency(&project, &c, &b, &options()).unwrap();
    let before = document(&project, &a);

    let error = add_dependency(&project, &a, &c, &options()).unwrap_err();
    assert_eq!(error.code(), Code::CycleDetected);
    assert_eq!(error.details()["kind"], serde_json::json!("dependency"));
    let cycle: Vec<String> = error.details()["cycle"]
        .as_array()
        .expect("a cycle path")
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect();
    // The path is closed (it starts and ends at the same item) and names every
    // item on the loop.
    assert_eq!(cycle.first(), cycle.last(), "{cycle:?}");
    for expected in [&a, &b, &c] {
        assert!(cycle.contains(expected), "{cycle:?} misses {expected}");
    }
    assert_eq!(
        document(&project, &a),
        before,
        "a rejected edge writes nothing"
    );
    assert!(!document(&project, &a).contains(&c), "no partial edge");
}

#[test]
fn a_missing_target_is_item_not_found_when_adding() {
    let (_dir, project) = fixtures::deterministic_project();
    let a = make(&project, "a");

    let error = add_dependency(&project, &a, "wm-00000000zzzz", &options()).unwrap_err();
    assert_eq!(error.code(), Code::ItemNotFound);
    assert_eq!(error.details()["id"], serde_json::json!("wm-00000000zzzz"));
    assert_eq!(
        error.details()["relation"],
        serde_json::json!("dependencies")
    );
}

#[test]
fn an_archived_target_is_rejected_when_adding() {
    let (_dir, project) = fixtures::deterministic_project();
    let a = make(&project, "a");
    let b = make(&project, "b");
    let path = read::read(&project, &b).unwrap().path;
    let mut item = waymark_core::parse::parse_item_file(&path).unwrap();
    item.archived_at = Some(fixtures::instant());
    std::fs::write(&path, waymark_core::serialize::to_document(&item)).unwrap();

    let error = add_dependency(&project, &a, &b, &options()).unwrap_err();
    assert_eq!(error.code(), Code::ItemArchived);
    assert_eq!(
        error.details()["operation"],
        serde_json::json!("dependency add")
    );
}

#[test]
fn an_archived_source_rejects_the_write() {
    let (_dir, project) = fixtures::deterministic_project();
    let a = make(&project, "a");
    let b = make(&project, "b");
    let path = read::read(&project, &a).unwrap().path;
    let mut item = waymark_core::parse::parse_item_file(&path).unwrap();
    item.archived_at = Some(fixtures::instant());
    std::fs::write(&path, waymark_core::serialize::to_document(&item)).unwrap();

    let error = add_dependency(&project, &a, &b, &options()).unwrap_err();
    assert_eq!(error.code(), Code::ItemArchived);
}

#[test]
fn removing_an_edge_updates_the_document_and_updated_at() {
    let (_dir, project) = fixtures::deterministic_project();
    let a = make(&project, "a");
    let b = make(&project, "b");
    add_dependency(&project, &a, &b, &options()).unwrap();

    let later = OpOptions::new().with_clock(std::sync::Arc::new(waymark_core::FixedClock::new(
        fixtures::instant().get() + chrono::Duration::minutes(5),
    )));
    let outcome = remove_dependency(&project, &a, &b, &later).unwrap();

    assert!(outcome.changed);
    assert!(outcome.item.dependencies.is_empty());
    assert_eq!(outcome.item.updated_at.to_string(), "2026-11-03T05:48:10Z");
    assert!(!document(&project, &a).contains("dependencies:"));
}

#[test]
fn removing_an_absent_edge_is_an_invalid_relationship() {
    let (_dir, project) = fixtures::deterministic_project();
    let a = make(&project, "a");
    let b = make(&project, "b");

    let error = remove_dependency(&project, &a, &b, &options()).unwrap_err();
    assert_eq!(error.code(), Code::InvalidRelationship);
    assert!(error.details()["reason"]
        .as_str()
        .unwrap()
        .contains("does not depend"));
}

#[test]
fn removing_a_dangling_edge_is_allowed_so_a_dangling_pointer_can_be_deleted() {
    let (_dir, project) = fixtures::deterministic_project();
    let a = make(&project, "a");
    let dangling = "wm-00000000zzzz";
    let path = read::read(&project, &a).unwrap().path;
    let mut item = waymark_core::parse::parse_item_file(&path).unwrap();
    item.dependencies = vec![waymark_core::WorkItemId::parse(dangling).unwrap()];
    std::fs::write(&path, waymark_core::serialize::to_document(&item)).unwrap();

    let outcome = remove_dependency(&project, &a, dangling, &options()).unwrap();
    assert!(outcome.item.dependencies.is_empty());
}

#[test]
fn closing_a_dependency_makes_its_dependents_ready() {
    let (_dir, project) = fixtures::deterministic_project();
    let dependent = make(&project, "dependent");
    let blocker = make(&project, "blocker");
    add_dependency(&project, &dependent, &blocker, &options()).unwrap();

    let graph = waymark_core::readiness::Graph::from_project(&project).unwrap();
    let before = graph
        .readiness(&waymark_core::WorkItemId::parse(&dependent).unwrap())
        .unwrap();
    assert!(before.blocked);

    status::set_status(&project, &blocker, Status::Closed, &options()).unwrap();

    let graph = waymark_core::readiness::Graph::from_project(&project).unwrap();
    let after = graph
        .readiness(&waymark_core::WorkItemId::parse(&dependent).unwrap())
        .unwrap();
    assert!(after.ready);
    assert!(!after.blocked);
    assert!(after.blocked_by.is_empty());
}
