//! Hierarchy: the single parent edge.
//!
//! The seams exercised here are the two operations (`set_parent` /
//! `clear_parent`) and the persisted document. The rules the spec pins: at most
//! one parent, parentage may cross categories, it neither blocks readiness nor
//! cascades lifecycle, a cycle is rejected at write time with
//! `CYCLE_DETECTED {kind: "parent"}`, and re-setting the same parent is a true
//! no-op (no write, no `updated_at` move).

use waymark_core::config::{Category, Config};
use waymark_core::error::Code;
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::parent::{clear_parent, set_parent};
use waymark_core::ops::{create, read, status, OpOptions};
use waymark_core::testing::fixtures;
use waymark_core::{CategoryKey, Project, Status};

fn options() -> OpOptions {
    OpOptions::new().with_clock(fixtures::shared_clock())
}

/// A project with two declared categories, so cross-category parentage is a real
/// choice rather than a config detail.
fn two_category_project() -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let mut config = Config::fresh();
    config.categories.insert(
        CategoryKey::parse("bug").unwrap(),
        Category {
            path: "work/bug".to_owned(),
            name: "Bugs".to_owned(),
        },
    );
    let project = Project::init(
        &root,
        waymark_core::store::InitOptions {
            config: Some(config),
            ..waymark_core::store::InitOptions::default()
        },
    )
    .unwrap()
    .with_clock(waymark_core::testing::shared_clock())
    .with_id_source(Box::new(waymark_core::testing::SequentialIds::new()));
    (dir, project)
}

fn make(project: &Project, category: &str, title: &str) -> String {
    create::create(
        project,
        CreateRequest::new(title, category, "frank"),
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
fn a_parent_is_recorded_and_may_cross_categories() {
    let (_dir, project) = two_category_project();
    let task = make(&project, "task", "the epic");
    let bug = make(&project, "bug", "the child");

    let outcome = set_parent(&project, &bug, &task, &options()).unwrap();

    assert!(outcome.changed, "a new edge is a change");
    assert!(outcome.written, "a non-dry run writes");
    assert_eq!(
        outcome.parent.as_ref().map(|id| id.as_str()),
        Some(task.as_str())
    );
    assert_eq!(outcome.previous_parent, None, "there was no parent before");
    assert_eq!(outcome.item.id.as_str(), bug);

    let persisted = read::read(&project, &bug).unwrap().item;
    assert_eq!(
        persisted.parent.as_ref().map(|id| id.as_str()),
        Some(task.as_str()),
        "the child records exactly one parent"
    );
    assert_eq!(
        persisted.category.as_str(),
        "bug",
        "cross-category parentage does not move the item"
    );
    let bytes = document(&project, &bug);
    assert!(bytes.contains(&format!("parent: {task}")), "{bytes}");
}

#[test]
fn setting_a_second_parent_replaces_the_first() {
    let (_dir, project) = two_category_project();
    let first = make(&project, "task", "first");
    let second = make(&project, "task", "second");
    let child = make(&project, "bug", "the child");

    set_parent(&project, &child, &first, &options()).unwrap();
    let outcome = set_parent(&project, &child, &second, &options()).unwrap();

    assert!(outcome.changed);
    assert_eq!(outcome.previous_parent.as_ref().unwrap().as_str(), first);
    assert_eq!(
        read::read(&project, &child)
            .unwrap()
            .item
            .parent
            .unwrap()
            .as_str(),
        second,
        "a second parent replaces the first, it does not accumulate"
    );
    let bytes = document(&project, &child);
    assert!(!bytes.contains(&first), "the old parent is gone: {bytes}");
}

#[test]
fn setting_the_parent_it_already_has_writes_nothing() {
    let (_dir, project) = two_category_project();
    let parent = make(&project, "task", "the parent");
    let child = make(&project, "task", "the child");
    set_parent(&project, &child, &parent, &options()).unwrap();
    let before = document(&project, &child);

    let outcome = set_parent(&project, &child, &parent, &options()).unwrap();

    assert!(!outcome.changed, "the edge was already there");
    assert!(!outcome.written, "a repeat does not rewrite");
    assert_eq!(outcome.fingerprint, None);
    assert_eq!(
        document(&project, &child),
        before,
        "not even updated_at moves"
    );
}

#[test]
fn a_parent_cycle_is_rejected_with_a_closed_path() {
    let (_dir, project) = two_category_project();
    let a = make(&project, "task", "a");
    let b = make(&project, "task", "b");
    let c = make(&project, "task", "c");
    set_parent(&project, &b, &a, &options()).unwrap();
    set_parent(&project, &c, &b, &options()).unwrap();
    let before = document(&project, &a);

    let error = set_parent(&project, &a, &c, &options()).unwrap_err();

    assert_eq!(error.code(), Code::CycleDetected);
    assert_eq!(error.details()["kind"], serde_json::json!("parent"));
    let cycle: Vec<&str> = error.details()["cycle"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap())
        .collect();
    assert_eq!(cycle.first(), Some(&a.as_str()), "{cycle:?}");
    assert_eq!(cycle.last(), Some(&a.as_str()), "{cycle:?}");
    assert!(cycle.contains(&b.as_str()) && cycle.contains(&c.as_str()));
    assert_eq!(
        document(&project, &a),
        before,
        "a rejected cycle writes nothing"
    );
}

#[test]
fn a_self_parent_is_an_invalid_relationship() {
    let (_dir, project) = two_category_project();
    let a = make(&project, "task", "a");

    let error = set_parent(&project, &a, &a, &options()).unwrap_err();

    assert_eq!(error.code(), Code::InvalidRelationship);
    assert_eq!(error.details()["relation"], serde_json::json!("parent"));
}

#[test]
fn a_missing_parent_is_item_not_found() {
    let (_dir, project) = two_category_project();
    let a = make(&project, "task", "a");

    let error = set_parent(&project, &a, "wm-0000000000zz", &options()).unwrap_err();

    assert_eq!(error.code(), Code::ItemNotFound);
    assert_eq!(error.details()["relation"], serde_json::json!("parent"));
}

#[test]
fn an_archived_parent_or_child_is_rejected() {
    let (_dir, project) = two_category_project();
    let parent = make(&project, "task", "parent");
    let child = make(&project, "task", "child");
    archive_by_hand(&project, &parent);
    archive_by_hand(&project, &child);

    let error = set_parent(&project, &child, &parent, &options()).unwrap_err();
    assert_eq!(
        error.code(),
        Code::ItemArchived,
        "the child itself is archived"
    );

    let active_parent = make(&project, "task", "active parent");
    let error = set_parent(&project, "wm-0000000000zz", &active_parent, &options()).unwrap_err();
    assert_eq!(error.code(), Code::ItemNotFound, "the child does not exist");
}

#[test]
fn an_archived_parent_cannot_be_named() {
    let (_dir, project) = two_category_project();
    let parent = make(&project, "task", "parent");
    let child = make(&project, "task", "child");
    archive_by_hand(&project, &parent);

    let error = set_parent(&project, &child, &parent, &options()).unwrap_err();

    assert_eq!(error.code(), Code::ItemArchived);
    assert_eq!(
        error.details()["operation"],
        serde_json::json!("parent set")
    );
}

#[test]
fn clearing_a_parent_removes_the_edge_and_moves_updated_at() {
    let (_dir, project) = two_category_project();
    let parent = make(&project, "task", "parent");
    let child = make(&project, "task", "child");
    set_parent(&project, &child, &parent, &options()).unwrap();

    let outcome = clear_parent(&project, &child, &options()).unwrap();

    assert!(outcome.changed);
    assert!(outcome.written);
    assert_eq!(outcome.parent, None);
    assert_eq!(outcome.previous_parent.unwrap().as_str(), parent);
    assert_eq!(read::read(&project, &child).unwrap().item.parent, None);
    let bytes = document(&project, &child);
    assert!(!bytes.contains("parent:"), "{bytes}");
}

#[test]
fn clearing_a_parentless_item_is_a_no_op() {
    let (_dir, project) = two_category_project();
    let child = make(&project, "task", "child");
    let before = document(&project, &child);

    let outcome = clear_parent(&project, &child, &options()).unwrap();

    assert!(!outcome.changed);
    assert!(!outcome.written);
    assert_eq!(document(&project, &child), before);
}

#[test]
fn a_parent_neither_blocks_readiness_nor_cascades_lifecycle() {
    let (_dir, project) = two_category_project();
    let parent = make(&project, "task", "parent");
    let child = make(&project, "task", "child");
    set_parent(&project, &child, &parent, &options()).unwrap();

    // The child is ready even though its parent is open: a parent is not a
    // prerequisite.
    let listing =
        waymark_core::ops::list::ready(&project, &waymark_core::ops::list::Query::new()).unwrap();
    assert!(
        listing
            .items
            .iter()
            .any(|row| row.item.id.as_str() == child),
        "the child is ready"
    );

    // Closing the parent leaves the child's status alone.
    status::set_status(&project, &parent, Status::Closed, &options()).unwrap();
    assert_eq!(
        read::read(&project, &child).unwrap().item.status,
        Status::Open,
        "closing a parent does not cascade"
    );

    // Closing the child leaves the parent alone too.
    status::set_status(&project, &child, Status::Closed, &options()).unwrap();
    assert_eq!(
        read::read(&project, &parent).unwrap().item.status,
        Status::Closed
    );
}

#[test]
fn a_cycle_introduced_by_an_external_edit_is_diagnosed_and_never_repaired() {
    let (_dir, project) = two_category_project();
    let a = make(&project, "task", "a");
    let b = make(&project, "task", "b");
    set_parent(&project, &a, &b, &options()).unwrap();

    // An editor closes the loop behind our back.
    let path = read::read(&project, &b).unwrap().path;
    let mut item = waymark_core::parse::parse_item_file(&path).unwrap();
    item.parent = Some(waymark_core::WorkItemId::parse(&a).unwrap());
    std::fs::write(&path, waymark_core::serialize::to_document(&item)).unwrap();

    let graph = waymark_core::Graph::from_project(&project).unwrap();
    let cycles: Vec<_> = graph
        .diagnostics()
        .into_iter()
        .filter(|finding| finding.code == Code::CycleDetected)
        .collect();
    assert_eq!(cycles.len(), 1, "{cycles:?}");
    assert!(
        cycles[0].message.contains(&a) && cycles[0].message.contains(&b),
        "{}",
        cycles[0].message
    );

    // Nothing repaired it: the files still hold the loop.
    assert_eq!(
        read::read(&project, &a)
            .unwrap()
            .item
            .parent
            .unwrap()
            .as_str(),
        b
    );
    assert_eq!(
        read::read(&project, &b)
            .unwrap()
            .item
            .parent
            .unwrap()
            .as_str(),
        a
    );
}

/// Archive `id` by writing the marker by hand: archive's own operation arrives
/// with T7, but the parent rules must already honour an archived item.
fn archive_by_hand(project: &Project, id: &str) {
    let path = read::read(project, id).unwrap().path;
    let mut item = waymark_core::parse::parse_item_file(&path).unwrap();
    item.archived_at = Some(fixtures::instant());
    std::fs::write(&path, waymark_core::serialize::to_document(&item)).unwrap();
}
