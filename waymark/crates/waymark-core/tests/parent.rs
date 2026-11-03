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
        read::read(&project, &child).unwrap().item.parent.unwrap().as_str(),
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
