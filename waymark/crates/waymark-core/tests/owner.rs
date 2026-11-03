//! Owner transfer: the accountable identity changes and nothing else does.
//!
//! Every case drives `waymark_core::ops::owner::transfer_owner` against a
//! temporary project and then reads the document back, because a transfer is only
//! real when the bytes say so — and a no-op is only real when they do not.

use std::sync::Arc;

use camino::Utf8PathBuf;
use chrono::{DateTime, Utc};
use waymark_core::config::{Category, Config, FieldDecl, FieldType};
use waymark_core::error::Code;
use waymark_core::model::{CategoryKey, FieldKey};
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::edit::FieldChangeEntry;
use waymark_core::ops::owner;
use waymark_core::ops::{create, edit, link, parent, relationship, status, OpOptions};
use waymark_core::testing::{fixtures, SequentialIds};
use waymark_core::{FieldValue, FixedClock, InitOptions, Project, Status, WorkItem, WorkItemId};

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

/// A project with two categories and one declared custom field, so a transfer
/// has other columns that must survive it.
fn project() -> (tempfile::TempDir, Project) {
    let mut config = Config::fresh();
    config.categories.insert(
        CategoryKey::parse("bug").expect("a valid key"),
        Category {
            path: "work/bugs".to_owned(),
            name: "Bugs".to_owned(),
        },
    );
    config.fields.insert(
        FieldKey::parse("note").expect("a valid field key"),
        FieldDecl {
            ty: FieldType::String,
            required: false,
            default: None,
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

fn parsed(project: &Project, item: &str) -> WorkItem {
    let entry = project
        .locate(&id(item))
        .expect("the item is in the inventory");
    waymark_core::parse::parse_item_file(&entry.path).expect("the document parses")
}

/// Give the item every other column the transfer must not touch.
fn decorate(project: &Project, item: &str, other: &str) {
    status::set_status(project, item, Status::InProgress, &options_at(10)).expect("status changes");
    edit::update_custom_fields(
        project,
        item,
        &[FieldChangeEntry::set(
            "note",
            FieldValue::String("keep me".to_owned()),
        )],
        &options_at(20),
    )
    .expect("the custom field is set");
    parent::set_parent(project, item, other, &options_at(40)).expect("the parent is set");
    relationship::add_dependency(project, item, other, &options_at(50))
        .expect("the dependency is set");
    link::add_soft_link(project, item, other, &options_at(60)).expect("the soft link is set");
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
fn transfer_replaces_the_owner_moves_updated_at_and_leaves_every_other_field_alone() {
    let (_dir, project) = project();
    let item = make(&project, "Fix the thing");
    let other = make(&project, "The other thing");
    decorate(&project, &item, &other);

    let before = parsed(&project, &item);
    let before_doc = document(&project, &item);
    assert_eq!(before.owner, "frank");
    assert_eq!(before.creator.as_deref(), Some("alice"));

    let outcome = owner::transfer_owner(&project, &item, "  bob  ", &options_at(120))
        .expect("a valid identity is accepted");

    assert!(outcome.changed);
    assert!(outcome.written);
    assert_eq!(outcome.previous_owner, "frank");
    assert_eq!(outcome.item.owner, "bob");
    assert_eq!(
        outcome.path,
        project.path_for(&id(&item), &before.category).unwrap()
    );
    assert_eq!(
        outcome.fingerprint,
        Some(project.fingerprint(&outcome.path).unwrap())
    );
    assert_eq!(
        outcome.item.updated_at.to_string(),
        "2026-11-03T05:45:10Z",
        "a real transfer moves updated_at from the operation clock"
    );
    assert_eq!(
        outcome.item.created_at, before.created_at,
        "created_at never moves"
    );

    // Everything except owner and updated_at is identical, field by field.
    let mut expected = before.clone();
    expected.owner = "bob".to_owned();
    expected.updated_at = outcome.item.updated_at;
    assert_eq!(outcome.item, expected);

    // And byte for byte, only those two frontmatter lines changed.
    let expected_doc = before_doc
        .replacen("owner: frank", "owner: bob", 1)
        .replacen(
            &format!("updated_at: {}", before.updated_at),
            &format!("updated_at: {}", outcome.item.updated_at),
            1,
        );
    assert_eq!(document(&project, &item), expected_doc);
    assert_eq!(parsed(&project, &item), outcome.item);
}

#[test]
fn transferring_to_the_same_owner_writes_nothing_and_does_not_move_updated_at() {
    let (_dir, project) = project();
    let item = make(&project, "Fix the thing");
    let before_doc = document(&project, &item);
    let before = parsed(&project, &item);

    let outcome = owner::transfer_owner(&project, &item, "  frank  ", &options_at(600))
        .expect("the current owner is accepted");

    assert!(!outcome.changed);
    assert!(!outcome.written);
    assert_eq!(outcome.fingerprint, None);
    assert_eq!(outcome.previous_owner, "frank");
    assert_eq!(outcome.item.owner, "frank");
    assert_eq!(
        outcome.item.updated_at, before.updated_at,
        "an unchanged request does not move the timestamp"
    );
    assert_eq!(document(&project, &item), before_doc);
}

#[test]
fn a_mismatched_operator_warns_without_blocking_the_transfer() {
    let (_dir, project) = project();
    let item = make(&project, "Fix the thing");

    let outcome = owner::transfer_owner(
        &project,
        &item,
        "bob",
        &options_at(120).with_operator("alice"),
    )
    .expect("a mismatch never blocks");

    assert!(outcome.changed);
    assert!(outcome.written);
    assert_eq!(outcome.item.owner, "bob");
    assert_eq!(
        outcome.meta.warnings.len(),
        1,
        "{:?}",
        outcome.meta.warnings
    );
    assert_eq!(outcome.meta.warnings[0].code, Code::OwnerMismatch);
    assert!(outcome.meta.warnings[0].message.contains("alice"));
    assert_eq!(parsed(&project, &item).owner, "bob");
}

#[test]
fn the_outgoing_owner_is_not_warned_after_handing_the_item_over() {
    let (_dir, project) = project();
    let item = make(&project, "Fix the thing");

    // `frank` owns the item and is the operator, so the transfer is legitimate;
    // comparing the operator with the *new* owner would warn for no reason.
    let outcome = owner::transfer_owner(
        &project,
        &item,
        "bob",
        &options_at(120).with_operator("frank"),
    )
    .expect("the current owner may transfer");

    assert!(outcome.changed);
    assert!(
        outcome.meta.warnings.is_empty(),
        "{:?}",
        outcome.meta.warnings
    );
}

#[test]
fn an_archived_item_rejects_the_transfer() {
    let (_dir, project) = project();
    let item = make(&project, "Fix the thing");
    archive(&project, &item);
    let before = document(&project, &item);

    let error = owner::transfer_owner(&project, &item, "bob", &options_at(120))
        .expect_err("an archived item rejects a transfer");

    assert_eq!(error.code(), Code::ItemArchived);
    assert_eq!(
        error.details()["operation"],
        serde_json::json!("owner transfer")
    );
    assert_eq!(document(&project, &item), before);
}

#[test]
fn a_migration_pending_item_rejects_the_transfer() {
    let (_dir, project) = project();
    let item = make(&project, "Fix the thing");
    downgrade_generation(&project, &item);
    let before = document(&project, &item);

    let error = owner::transfer_owner(&project, &item, "bob", &options_at(120))
        .expect_err("an older generation rejects mediated writes");

    assert_eq!(error.code(), Code::ItemNeedsMigration);
    assert_eq!(document(&project, &item), before);
}

#[test]
fn an_empty_or_multi_line_owner_is_rejected_before_anything_is_written() {
    let (_dir, project) = project();
    let item = make(&project, "Fix the thing");
    let before = document(&project, &item);

    let empty = owner::transfer_owner(&project, &item, "   ", &options_at(120))
        .expect_err("a whitespace-only identity is not an owner");
    assert_eq!(empty.code(), Code::ValidationError);
    assert!(empty.message().contains("owner"), "{empty}");

    let multi_line = owner::transfer_owner(&project, &item, "bob\nbob", &options_at(120))
        .expect_err("an identity is a single line");
    assert_eq!(multi_line.code(), Code::ValidationError);
    assert!(multi_line.message().contains("single line"), "{multi_line}");

    assert_eq!(document(&project, &item), before);
}

#[test]
fn a_dry_run_reports_the_transfer_without_writing() {
    let (_dir, project) = project();
    let item = make(&project, "Fix the thing");
    let before = document(&project, &item);

    let outcome = owner::transfer_owner(&project, &item, "bob", &options_at(120).dry_run())
        .expect("a dry run still validates");

    assert!(outcome.changed);
    assert!(!outcome.written);
    assert_eq!(outcome.fingerprint, None);
    assert_eq!(outcome.item.owner, "bob");
    assert_eq!(document(&project, &item), before);
}
