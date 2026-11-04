//! Moving one item between categories: one document, two paths, all or nothing.
//!
//! Every case drives `waymark_core::ops::r#move::move_item` against a temporary
//! project and then inspects the filesystem, because a move's whole promise is
//! about which file exists where and which bytes survive a refusal.

use std::sync::Arc;

use camino::Utf8PathBuf;
use chrono::{DateTime, Utc};
use waymark_core::config::{Category, Config};
use waymark_core::error::Code;
use waymark_core::model::CategoryKey;
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::r#move;
use waymark_core::ops::{create, OpOptions};
use waymark_core::testing::{fixtures, SequentialIds};
use waymark_core::{FixedClock, InitOptions, Project, WorkItemId};

fn base() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-03T05:43:10Z")
        .expect("a valid instant")
        .with_timezone(&Utc)
}

fn options_at(seconds: i64) -> OpOptions {
    OpOptions::new().with_clock(Arc::new(FixedClock::new(
        base() + chrono::Duration::seconds(seconds),
    )))
}

fn id(literal: &str) -> WorkItemId {
    WorkItemId::parse(literal).expect("a valid stable ID")
}

fn key(literal: &str) -> CategoryKey {
    CategoryKey::parse(literal).expect("a valid category key")
}

/// A project that declares both `task` and `bug`, so a move has a destination.
fn project() -> (tempfile::TempDir, Project) {
    let mut config = Config::fresh();
    config.categories.insert(
        key("bug"),
        Category {
            path: "work/bugs".to_owned(),
            name: "Bugs".to_owned(),
        },
    );
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 path");
    let project = Project::init(
        &root,
        InitOptions {
            config: Some(config),
            ..InitOptions::default()
        },
    )
    .expect("the project initialises")
    .with_clock(Arc::new(fixtures::clock()))
    .with_id_source(Box::new(SequentialIds::new()));
    (dir, project)
}

fn make(project: &Project, title: &str) -> String {
    create::create(
        project,
        CreateRequest::new(title, "task", "frank").with_body("original body\n"),
        &options_at(0).with_operator("alice"),
    )
    .expect("creation succeeds")
    .item
    .id
    .as_str()
    .to_owned()
}

fn document(project: &Project, item: &str) -> String {
    let entry = project
        .locate(&id(item))
        .expect("the item is in the inventory");
    std::fs::read_to_string(&entry.path).expect("the document is readable")
}

fn old_path(project: &Project, item: &str) -> Utf8PathBuf {
    project
        .path_for(&id(item), &key("task"))
        .expect("the task category is declared")
}

fn destination(project: &Project, item: &str) -> Utf8PathBuf {
    project
        .path_for(&id(item), &key("bug"))
        .expect("the bug category is declared")
}

fn archive(project: &Project, item: &str) {
    let entry = project
        .locate(&id(item))
        .expect("the item is in the inventory");
    let mut parsed =
        waymark_core::parse::parse_item_file(&entry.path).expect("the document parses");
    parsed.archived_at = Some(waymark_core::testing::instant());
    parsed.archive_reason = Some("retired".to_owned());
    std::fs::write(&entry.path, waymark_core::serialize::to_document(&parsed))
        .expect("the archive marker is writable");
}

fn downgrade_generation(project: &Project, item: &str) {
    let entry = project
        .locate(&id(item))
        .expect("the item is in the inventory");
    let text = std::fs::read_to_string(&entry.path).expect("the document is readable");
    let downgraded = text.replacen("schema: 1\n", "schema: 0\n", 1);
    std::fs::write(&entry.path, downgraded).expect("the document is writable");
}

#[test]
fn move_relocates_the_document_and_rewrites_the_persisted_category() {
    let (_dir, project) = project();
    let item = make(&project, "Fix the thing");
    let source = old_path(&project, &item);
    let target = destination(&project, &item);
    assert!(source.exists());
    assert!(!target.exists());
    let before = waymark_core::parse::parse_item_file(&source).expect("the document parses");

    let outcome = r#move::move_item(&project, &item, &key("bug"), &options_at(60))
        .expect("the move succeeds");

    assert!(outcome.changed);
    assert!(outcome.written);
    assert_eq!(outcome.previous_path, source);
    assert_eq!(outcome.path, target);
    assert_eq!(outcome.previous_category, key("task"));
    assert_eq!(outcome.item.category, key("bug"));
    assert_eq!(
        outcome.item.updated_at.to_string(),
        "2026-11-03T05:44:10Z",
        "the persisted category changed, so updated_at moves"
    );
    assert_eq!(outcome.item.created_at, before.created_at);

    assert!(!source.exists(), "the old document is removed");
    assert!(target.exists(), "the document lives at the new path");
    assert_eq!(
        outcome.fingerprint,
        Some(project.fingerprint(&target).unwrap())
    );
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        waymark_core::serialize::to_document(&outcome.item)
    );

    // Every field except category and updated_at is untouched.
    let mut expected = before;
    expected.category = key("bug");
    expected.updated_at = outcome.item.updated_at;
    assert_eq!(outcome.item, expected);

    // The inventory sees one document, at the new path.
    let inventory = project.inventory().unwrap();
    assert_eq!(inventory.entries.len(), 1);
    assert_eq!(inventory.entries[0].path, target);
    assert_eq!(
        project.category_for_path(&target),
        Some(key("bug")),
        "the new location classifies as the target category"
    );
}

#[test]
fn a_failed_move_leaves_the_old_document_byte_identical_and_creates_nothing_new() {
    let (_dir, project) = project();
    let item = make(&project, "Fix the thing");
    let source = old_path(&project, &item);
    let target = destination(&project, &item);
    let before = document(&project, &item);
    // A directory is not a work-item document, so the inventory skips it and
    // the move reaches its own destination check; the refusal is `WRITE_CONFLICT`
    // and nothing is staged or removed.
    std::fs::create_dir(&target).expect("the destination directory is creatable");

    let error = r#move::move_item(&project, &item, &key("bug"), &options_at(60))
        .expect_err("an occupied destination is never overwritten");

    assert_eq!(error.code(), Code::WriteConflict);
    assert_eq!(error.details()["path"], serde_json::json!(target.as_str()));
    assert_eq!(
        error.details()["expected_fingerprint"],
        serde_json::json!("absent")
    );
    assert_eq!(
        error.details()["actual_fingerprint"],
        serde_json::json!("present")
    );

    assert_eq!(
        document(&project, &item),
        before,
        "the old file is untouched"
    );
    assert!(source.exists());
    assert!(target.is_dir(), "the occupied destination is untouched");
}

#[test]
fn an_existing_document_at_the_destination_is_already_a_duplicate_id() {
    let (_dir, project) = project();
    let item = make(&project, "Fix the thing");
    let source = old_path(&project, &item);
    let target = destination(&project, &item);
    let before = document(&project, &item);
    // Copying the document to the target path makes two files claim one ID; the
    // inventory diagnoses that before any move rule runs.
    std::fs::copy(&source, &target).expect("the document is copyable");

    let error = r#move::move_item(&project, &item, &key("bug"), &options_at(60))
        .expect_err("two files for one ID are refused");

    assert_eq!(error.code(), Code::DuplicateId);
    assert_eq!(
        std::fs::read_to_string(&source).unwrap(),
        before,
        "the original file is untouched"
    );
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        before,
        "the duplicate file is untouched"
    );
}

#[test]
fn moving_to_the_same_category_is_a_true_no_op() {
    let (_dir, project) = project();
    let item = make(&project, "Fix the thing");
    let source = old_path(&project, &item);
    let before = document(&project, &item);
    let before_updated = waymark_core::parse::parse_item_file(&source)
        .unwrap()
        .updated_at;

    let outcome = r#move::move_item(&project, &item, &key("task"), &options_at(600))
        .expect("asking for the current category succeeds");

    assert!(!outcome.changed);
    assert!(!outcome.written);
    assert_eq!(outcome.fingerprint, None);
    assert_eq!(outcome.path, source);
    assert_eq!(outcome.previous_path, source);
    assert_eq!(outcome.item.updated_at, before_updated);
    assert_eq!(document(&project, &item), before);
}

#[test]
fn an_undeclared_category_is_refused_with_the_creation_error() {
    let (_dir, project) = fixtures::deterministic_project();
    let created = create::create(
        &project,
        CreateRequest::new("Fix the thing", "task", "frank"),
        &options_at(0),
    )
    .expect("creation succeeds");
    let item = created.item.id.as_str().to_owned();
    let before = document(&project, &item);

    let error = r#move::move_item(&project, &item, &key("bug"), &options_at(60))
        .expect_err("an undeclared category is refused");

    assert_eq!(error.code(), Code::ValidationError);
    assert!(error.message().contains("not a category"), "{error}");
    assert_eq!(document(&project, &item), before);
}

#[test]
fn an_archived_item_rejects_the_move() {
    let (_dir, project) = project();
    let item = make(&project, "Fix the thing");
    archive(&project, &item);
    let before = document(&project, &item);

    let error = r#move::move_item(&project, &item, &key("bug"), &options_at(60))
        .expect_err("an archived item rejects a move");

    assert_eq!(error.code(), Code::ItemArchived);
    assert_eq!(error.details()["operation"], serde_json::json!("move"));
    assert_eq!(document(&project, &item), before);
    assert!(!destination(&project, &item).exists());
}

#[test]
fn a_migration_pending_item_rejects_the_move() {
    let (_dir, project) = project();
    let item = make(&project, "Fix the thing");
    downgrade_generation(&project, &item);
    let before = document(&project, &item);

    let error = r#move::move_item(&project, &item, &key("bug"), &options_at(60))
        .expect_err("an older generation rejects mediated writes");

    assert_eq!(error.code(), Code::ItemNeedsMigration);
    assert_eq!(document(&project, &item), before);
    assert!(!destination(&project, &item).exists());
}

#[test]
fn a_dry_run_reports_the_destination_without_writing() {
    let (_dir, project) = project();
    let item = make(&project, "Fix the thing");
    let source = old_path(&project, &item);
    let target = destination(&project, &item);
    let before = document(&project, &item);

    let outcome = r#move::move_item(&project, &item, &key("bug"), &options_at(60).dry_run())
        .expect("a dry run still validates");

    assert!(outcome.changed);
    assert!(!outcome.written);
    assert_eq!(outcome.fingerprint, None);
    assert_eq!(outcome.path, target);
    assert_eq!(outcome.previous_path, source);
    assert_eq!(outcome.item.category, key("bug"));
    assert!(source.exists(), "the source still exists");
    assert!(!target.exists(), "the destination was not created");
    assert_eq!(document(&project, &item), before);
}

#[test]
fn a_document_already_at_the_destination_path_rewrites_the_category_in_place() {
    let (_dir, project) = project();
    let item = make(&project, "Fix the thing");
    let source = old_path(&project, &item);
    let target = destination(&project, &item);
    // A hand-edit moved the file without rewriting the persisted category.
    std::fs::rename(&source, &target).expect("the document is movable");

    let outcome = r#move::move_item(&project, &item, &key("bug"), &options_at(60))
        .expect("the category is rewritten where the document already is");

    assert!(outcome.changed);
    assert!(outcome.written);
    assert_eq!(outcome.previous_category, key("task"));
    assert_eq!(outcome.item.category, key("bug"));
    assert_eq!(outcome.previous_path, target);
    assert_eq!(outcome.path, target);
    assert!(target.exists());
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        waymark_core::serialize::to_document(&outcome.item)
    );
    let reparsed = waymark_core::parse::parse_item_file(&target).expect("the document parses");
    assert_eq!(reparsed.category, key("bug"));
}

#[test]
fn the_written_flag_is_exactly_the_presence_of_a_fingerprint() {
    fn pin(outcome: &r#move::MoveOutcome) {
        assert_eq!(
            outcome.written,
            outcome.fingerprint.is_some(),
            "written and the fingerprint must agree: {outcome:?}"
        );
    }

    let (_dir, project) = project();
    let item = make(&project, "Fix the thing");

    // No-op: same category.
    let no_op = r#move::move_item(&project, &item, &key("task"), &options_at(60))
        .expect("the current category is a no-op");
    assert!(!no_op.changed);
    pin(&no_op);

    // Dry-run relocation: changed, but nothing written.
    let dry = r#move::move_item(&project, &item, &key("bug"), &options_at(60).dry_run())
        .expect("a dry run validates");
    assert!(dry.changed);
    pin(&dry);

    // Committed relocation: written, with a fingerprint.
    let moved = r#move::move_item(&project, &item, &key("bug"), &options_at(60))
        .expect("the move succeeds");
    assert!(moved.changed);
    pin(&moved);

    // In-place rewrite: the document already sits at the destination path.
    let in_place = make(&project, "Rewrite in place");
    let source = old_path(&project, &in_place);
    let target = destination(&project, &in_place);
    std::fs::rename(&source, &target).expect("the document is movable");
    let rewritten = r#move::move_item(&project, &in_place, &key("bug"), &options_at(60))
        .expect("the category is rewritten in place");
    assert!(rewritten.changed);
    pin(&rewritten);
}
