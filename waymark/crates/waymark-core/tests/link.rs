//! Soft links: symmetric, non-blocking associations.
//!
//! The seams exercised here are the two operations (`add_soft_link` /
//! `remove_soft_link`) and the two persisted documents. The rules the spec pins:
//! the association is stored on both sides, sorted and deduplicated, both files
//! change in one all-or-nothing operation, a duplicate or self link is
//! `INVALID_RELATIONSHIP`, and a missing or archived target is
//! `ITEM_NOT_FOUND` / `ITEM_ARCHIVED`.
//!
//! The failure-path test is the important one: the batch engine's promise is
//! that a half-applied link never survives, and that promise is only real if a
//! test makes the second write fail.

use waymark_core::config::{Category, Config};
use waymark_core::error::Code;
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::link::{add_soft_link, remove_soft_link};
use waymark_core::ops::{create, read, OpOptions};
use waymark_core::testing::fixtures;
use waymark_core::{CategoryKey, Project};

fn options() -> OpOptions {
    OpOptions::new().with_clock(fixtures::shared_clock())
}

fn project() -> (tempfile::TempDir, Project) {
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

fn links(project: &Project, id: &str) -> Vec<String> {
    read::read(project, id)
        .unwrap()
        .item
        .soft_links
        .iter()
        .map(|id| id.as_str().to_owned())
        .collect()
}

fn document(project: &Project, id: &str) -> String {
    std::fs::read_to_string(read::read(project, id).unwrap().path).unwrap()
}

#[test]
fn adding_a_link_records_it_on_both_sides() {
    let (_dir, project) = project();
    let left = make(&project, "task", "left");
    let right = make(&project, "bug", "right");

    let outcome = add_soft_link(&project, &left, &right, &options()).unwrap();

    assert!(outcome.changed && outcome.written);
    assert_eq!(outcome.other_id.as_str(), right);
    assert_eq!(
        outcome.fingerprints.len(),
        2,
        "both documents were replaced"
    );
    assert_eq!(links(&project, &left), vec![right.clone()]);
    assert_eq!(links(&project, &right), vec![left.clone()]);
}

#[test]
fn both_ends_are_sorted_and_deduplicated() {
    let (_dir, project) = project();
    let a = make(&project, "task", "a");
    let b = make(&project, "task", "b");
    let c = make(&project, "task", "c");
    let d = make(&project, "task", "d");
    add_soft_link(&project, &a, &d, &options()).unwrap();
    add_soft_link(&project, &a, &c, &options()).unwrap();
    add_soft_link(&project, &a, &b, &options()).unwrap();

    let expected = vec![b.clone(), c.clone(), d.clone()];
    assert_eq!(links(&project, &a), expected);
    for other in [&b, &c, &d] {
        assert_eq!(links(&project, other), vec![a.clone()]);
    }

    // A link from the other side reaches the same state, not a second copy.
    remove_soft_link(&project, &d, &a, &options()).unwrap();
    assert!(links(&project, &d).is_empty());
    assert!(!links(&project, &a).contains(&d));
}

#[test]
fn adding_a_duplicate_link_is_an_invalid_relationship() {
    let (_dir, project) = project();
    let left = make(&project, "task", "left");
    let right = make(&project, "task", "right");
    add_soft_link(&project, &left, &right, &options()).unwrap();
    let before = (document(&project, &left), document(&project, &right));

    let error = add_soft_link(&project, &left, &right, &options()).unwrap_err();
    assert_eq!(error.code(), Code::InvalidRelationship);
    assert_eq!(error.details()["relation"], serde_json::json!("soft_links"));

    // From the far side it is the same existing association, not a new one.
    let error = add_soft_link(&project, &right, &left, &options()).unwrap_err();
    assert_eq!(error.code(), Code::InvalidRelationship);

    assert_eq!(
        (document(&project, &left), document(&project, &right)),
        before,
        "a rejected link writes nothing"
    );
}

#[test]
fn a_self_link_is_an_invalid_relationship() {
    let (_dir, project) = project();
    let item = make(&project, "task", "alone");
    let before = document(&project, &item);

    let error = add_soft_link(&project, &item, &item, &options()).unwrap_err();

    assert_eq!(error.code(), Code::InvalidRelationship);
    assert_eq!(error.details()["relation"], serde_json::json!("soft_links"));
    assert_eq!(document(&project, &item), before);
}

#[test]
fn a_missing_target_is_item_not_found() {
    let (_dir, project) = project();
    let item = make(&project, "task", "left");
    let before = document(&project, &item);

    let error = add_soft_link(&project, &item, "wm-0000000000zz", &options()).unwrap_err();

    assert_eq!(error.code(), Code::ItemNotFound);
    assert_eq!(error.details()["relation"], serde_json::json!("soft_links"));
    assert_eq!(document(&project, &item), before);
}

#[test]
fn an_archived_target_or_source_is_item_archived() {
    let (_dir, project) = project();
    let left = make(&project, "task", "left");
    let right = make(&project, "task", "right");
    archive_by_hand(&project, &right);

    let error = add_soft_link(&project, &left, &right, &options()).unwrap_err();
    assert_eq!(error.code(), Code::ItemArchived);
    assert_eq!(error.details()["operation"], serde_json::json!("link add"));

    archive_by_hand(&project, &left);
    let error = add_soft_link(&project, &left, &left, &options()).unwrap_err();
    assert_eq!(
        error.code(),
        Code::ItemArchived,
        "an archived source is refused before the self-link check"
    );
}

#[test]
fn removing_a_link_updates_both_sides() {
    let (_dir, project) = project();
    let left = make(&project, "task", "left");
    let right = make(&project, "bug", "right");
    add_soft_link(&project, &left, &right, &options()).unwrap();

    let outcome = remove_soft_link(&project, &left, &right, &options()).unwrap();

    assert!(outcome.changed && outcome.written);
    assert!(links(&project, &left).is_empty());
    assert!(links(&project, &right).is_empty());
    assert!(!document(&project, &left).contains("soft_links"));
}

#[test]
fn removing_a_link_that_does_not_exist_is_an_invalid_relationship() {
    let (_dir, project) = project();
    let left = make(&project, "task", "left");
    let right = make(&project, "task", "right");

    let error = remove_soft_link(&project, &left, &right, &options()).unwrap_err();

    assert_eq!(error.code(), Code::InvalidRelationship);
}

#[test]
fn removing_a_link_to_a_deleted_target_cleans_the_surviving_side() {
    let (_dir, project) = project();
    let left = make(&project, "task", "left");
    let right = make(&project, "task", "right");
    add_soft_link(&project, &left, &right, &options()).unwrap();

    // Someone deletes the other document behind our back.
    let path = read::read(&project, &right).unwrap().path;
    std::fs::remove_file(path).unwrap();

    let outcome = remove_soft_link(&project, &left, &right, &options()).unwrap();

    assert!(outcome.written);
    assert!(outcome.other.is_none(), "there was nothing to rewrite");
    assert!(links(&project, &left).is_empty());
}

#[test]
fn a_failed_second_write_leaves_both_sides_unchanged() {
    let (_dir, project) = project();
    let left = make(&project, "task", "left");
    let right = make(&project, "task", "right");
    let before = (document(&project, &left), document(&project, &right));

    // Make the *second* document unwritable by turning it into a directory whose
    // name is the file: the staged rename cannot replace a directory, so the
    // batch must fail after replacing the first document and roll it back.
    let right_path = read::read(&project, &right).unwrap().path;
    std::fs::remove_file(&right_path).unwrap();
    std::fs::create_dir(&right_path).unwrap();

    let error = add_soft_link(&project, &left, &right, &options()).unwrap_err();
    assert_ne!(error.code(), Code::InternalError, "{error}");

    std::fs::remove_dir(&right_path).unwrap();
    std::fs::write(&right_path, before.1.clone().as_bytes()).unwrap();

    assert_eq!(
        document(&project, &left),
        before.0,
        "the first document was rolled back"
    );
    assert_eq!(document(&project, &right), before.1);
}

/// Archive `id` by writing the marker by hand: archive's own operation arrives
/// with T7, but the link rules must already honour an archived item.
fn archive_by_hand(project: &Project, id: &str) {
    let path = read::read(project, id).unwrap().path;
    let mut item = waymark_core::parse::parse_item_file(&path).unwrap();
    item.archived_at = Some(fixtures::instant());
    std::fs::write(&path, waymark_core::serialize::to_document(&item)).unwrap();
}
