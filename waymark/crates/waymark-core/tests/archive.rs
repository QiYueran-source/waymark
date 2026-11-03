//! Archive: irreversible-through-Waymark logical deletion.
//!
//! The seam is `waymark_core::ops::archive::archive` plus the collection reads.
//! Every case drives the public API against a temporary project and then reads
//! the documents back, because archive's whole promise is about which bytes
//! change and which must not.

use std::sync::Arc;

use camino::Utf8PathBuf;
use chrono::{DateTime, Utc};
use waymark_core::config::{Config, FieldDecl, FieldType};
use waymark_core::error::Code;
use waymark_core::lock::{with_project_lock, LockWait};
use waymark_core::model::FieldKey;
use waymark_core::ops::archive::{self, InboundRelation};
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::edit::FieldChangeEntry;
use waymark_core::ops::list::{self, Query};
use waymark_core::ops::tree::hierarchy_tree;
use waymark_core::ops::{create, edit, link, parent, read, relationship, status, OpOptions};
use waymark_core::query::ArchiveRange;
use waymark_core::testing::{fixtures, SequentialIds};
use waymark_core::{FieldValue, FixedClock, InitOptions, Project, Status, WorkItemId};

/// The instant the fixtures start at.
fn base() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-03T05:43:10Z")
        .expect("a valid instant")
        .with_timezone(&Utc)
}

/// Options whose clock is frozen `seconds` after the fixture instant.
fn options_at(seconds: i64) -> OpOptions {
    OpOptions::new().with_clock(Arc::new(FixedClock::new(
        base() + chrono::Duration::seconds(seconds),
    )))
}

fn options() -> OpOptions {
    options_at(0)
}

fn id(literal: &str) -> WorkItemId {
    WorkItemId::parse(literal).expect("a valid stable ID")
}

/// A project that declares one optional custom field, so a field write is a
/// legal request and only the archived rule can refuse it.
fn project() -> (tempfile::TempDir, Project) {
    let mut config = Config::fresh();
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
    .expect("a project with a declared field initialises")
    .with_clock(Arc::new(fixtures::clock()))
    .with_id_source(Box::new(SequentialIds::new()));
    (dir, project)
}

fn make(project: &Project, title: &str) -> String {
    create::create(
        project,
        CreateRequest::new(title, "task", "frank"),
        &options(),
    )
    .expect("creation succeeds")
    .item
    .id
    .as_str()
    .to_owned()
}

/// The canonical bytes of one document.
fn document(project: &Project, item: &str) -> String {
    let entry = project
        .locate(&id(item))
        .expect("the item is in the inventory");
    std::fs::read_to_string(&entry.path).expect("the document is readable")
}

fn parse(project: &Project, item: &str) -> waymark_core::WorkItem {
    let entry = project
        .locate(&id(item))
        .expect("the item is in the inventory");
    waymark_core::parse::parse_item_file(&entry.path).expect("the document parses")
}

fn titles(listing: &list::Listing) -> Vec<String> {
    listing
        .items
        .iter()
        .map(|row| row.item.title.clone())
        .collect()
}

fn code_of<T>(result: Result<T, waymark_core::WaymarkError>) -> Code {
    match result {
        Ok(_) => panic!("the operation was expected to fail"),
        Err(error) => error.code(),
    }
}

#[test]
fn archive_is_allowed_from_any_status_and_changes_neither_status_nor_closed_at() {
    for (index, (from, expected_closed_at)) in [
        (Status::Open, false),
        (Status::InProgress, false),
        (Status::Closed, true),
    ]
    .into_iter()
    .enumerate()
    {
        let (_dir, project) = project();
        let item = make(&project, "work");
        if from != Status::Open {
            status::set_status(&project, &item, from, &options_at(1)).expect("the status moves");
        }
        let before = parse(&project, &item);

        let outcome =
            archive::archive(&project, &item, None, &options_at(10)).expect("archive is allowed");

        assert_eq!(outcome.item.status, from, "archive never moves status");
        assert_eq!(outcome.item.closed_at, before.closed_at);
        assert_eq!(
            outcome.item.closed_at.is_some(),
            expected_closed_at,
            "case {index}"
        );
        assert!(outcome.written);
        let after = parse(&project, &item);
        assert_eq!(after.status, from);
        assert_eq!(after.closed_at, before.closed_at);
    }
}

#[test]
fn archive_adds_the_marker_and_reason_without_moving_updated_at() {
    let (_dir, project) = project();
    let item = make(&project, "work");
    let before = parse(&project, &item);

    let outcome = archive::archive(&project, &item, Some("superseded by wm-x"), &options_at(30))
        .expect("archive succeeds");

    let after = parse(&project, &item);
    assert_eq!(
        after.updated_at, before.updated_at,
        "updated_at must not move"
    );
    assert_eq!(
        after.archived_at,
        Some(waymark_core::Timestamp::new(
            base() + chrono::Duration::seconds(30)
        )),
        "archived_at comes from the operation clock"
    );
    assert_eq!(
        after.archived_at.map(|value| value.to_string()).as_deref(),
        Some("2026-11-03T05:43:40Z")
    );
    assert_eq!(after.archive_reason.as_deref(), Some("superseded by wm-x"));
    assert_eq!(after.title, before.title);
    assert_eq!(after.body, before.body);
    assert_eq!(after.custom_fields, before.custom_fields);
    assert_eq!(after.parent, before.parent);
    assert_eq!(after.dependencies, before.dependencies);
    assert_eq!(after.soft_links, before.soft_links);
    assert_eq!(
        outcome.archived_at.to_string(),
        "2026-11-03T05:43:40Z",
        "the outcome reports the marker it wrote"
    );
    assert!(outcome.written);
}

#[test]
fn a_blank_reason_is_no_reason_at_all() {
    let (_dir, project) = project();
    let item = make(&project, "work");

    archive::archive(&project, &item, Some("   \n "), &options()).expect("archive succeeds");

    let after = parse(&project, &item);
    assert!(after.archived_at.is_some());
    assert_eq!(after.archive_reason, None);
    assert!(!document(&project, &item).contains("archive_reason"));
}

#[test]
fn re_archiving_is_refused_and_replaces_neither_marker() {
    let (_dir, project) = project();
    let item = make(&project, "work");
    archive::archive(&project, &item, Some("first"), &options_at(10)).expect("archive succeeds");
    let after_first = document(&project, &item);

    let error = archive::archive(&project, &item, Some("second"), &options_at(20))
        .expect_err("re-archive is refused");

    assert_eq!(error.code(), Code::AlreadyArchived);
    assert_eq!(
        error.details().get("id").and_then(|value| value.as_str()),
        Some(item.as_str())
    );
    assert_eq!(
        document(&project, &item),
        after_first,
        "no marker was replaced"
    );
    let after = parse(&project, &item);
    assert_eq!(after.archive_reason.as_deref(), Some("first"));
    assert_eq!(
        after.archived_at.map(|value| value.to_string()).as_deref(),
        Some("2026-11-03T05:43:20Z")
    );
}

#[test]
fn an_unarchived_child_blocks_archive_and_archive_never_cascades() {
    let (_dir, project) = project();
    let parent_id = make(&project, "parent");
    let child = make(&project, "child");
    parent::set_parent(&project, &child, &parent_id, &options()).expect("the parent edge is set");
    let parent_before = document(&project, &parent_id);
    let child_before = document(&project, &child);

    let error = archive::archive(&project, &parent_id, None, &options_at(10))
        .expect_err("an active child blocks archive");

    assert_eq!(error.code(), Code::HasActiveChildren);
    assert_eq!(
        error.details().get("children"),
        Some(&serde_json::json!([child.as_str()]))
    );
    assert_eq!(
        document(&project, &parent_id),
        parent_before,
        "nothing was written"
    );
    assert_eq!(
        document(&project, &child),
        child_before,
        "the child was not cascaded"
    );

    // Not a cascade either: archiving the child first is the only route.
    archive::archive(&project, &child, None, &options_at(10)).expect("the child archives");
    archive::archive(&project, &parent_id, None, &options_at(20)).expect("the parent now archives");
    assert!(parse(&project, &child).is_archived());
    assert!(parse(&project, &parent_id).is_archived());
}

#[test]
fn an_archived_child_does_not_block_and_is_never_rewritten() {
    let (_dir, project) = project();
    let parent_id = make(&project, "parent");
    let child = make(&project, "child");
    parent::set_parent(&project, &child, &parent_id, &options()).expect("the parent edge is set");
    archive::archive(&project, &child, None, &options_at(5)).expect("the child archives");
    let child_before = document(&project, &child);

    let outcome = archive::archive(&project, &parent_id, None, &options_at(10))
        .expect("an archived child does not block its parent");

    assert!(outcome.removals.is_empty(), "the archived side is history");
    assert!(outcome.affected.is_empty());
    assert_eq!(
        document(&project, &child),
        child_before,
        "history stays history"
    );
    assert_eq!(
        parse(&project, &child)
            .parent
            .as_ref()
            .map(|id| id.as_str()),
        Some(parent_id.as_str()),
        "the archived document keeps its own parent edge"
    );
}

#[test]
fn every_mediated_write_family_rejects_an_archived_item() {
    let (_dir, project) = project();
    let item = make(&project, "work");
    let other = make(&project, "other");
    archive::archive(&project, &item, None, &options_at(10)).expect("archive succeeds");

    let cases: Vec<(&str, Code)> = vec![
        (
            "title",
            code_of(edit::set_title(&project, &item, "new", &options_at(20))),
        ),
        (
            "body",
            code_of(edit::replace_body(
                &project,
                &item,
                Some("text"),
                &options_at(20),
            )),
        ),
        (
            "custom fields",
            code_of(edit::update_custom_fields(
                &project,
                &item,
                &[FieldChangeEntry::set(
                    "note",
                    FieldValue::String("x".into()),
                )],
                &options_at(20),
            )),
        ),
        (
            "status",
            code_of(status::set_status(
                &project,
                &item,
                Status::Closed,
                &options_at(20),
            )),
        ),
        (
            "parent set",
            code_of(parent::set_parent(&project, &item, &other, &options_at(20))),
        ),
        (
            "parent clear",
            code_of(parent::clear_parent(&project, &item, &options_at(20))),
        ),
        (
            "dependency add",
            code_of(relationship::add_dependency(
                &project,
                &item,
                &other,
                &options_at(20),
            )),
        ),
        (
            "dependency remove",
            code_of(relationship::remove_dependency(
                &project,
                &item,
                &other,
                &options_at(20),
            )),
        ),
        (
            "link add",
            code_of(link::add_soft_link(
                &project,
                &item,
                &other,
                &options_at(20),
            )),
        ),
    ];
    for (operation, code) in cases {
        assert_eq!(
            code,
            Code::ItemArchived,
            "{operation} must reject an archived item"
        );
    }

    // The refusal names the attempted operation, so an adapter can say which.
    let error = edit::set_title(&project, &item, "new", &options_at(20))
        .expect_err("title write is refused");
    assert_eq!(
        error
            .details()
            .get("operation")
            .and_then(|value| value.as_str()),
        Some("title")
    );
    assert!(error.message().contains("is archived"));
}

#[test]
fn inbound_dependencies_moves_updated_at_on_the_dependent_only() {
    let (_dir, project) = project();
    let target = make(&project, "target");
    let dependent = make(&project, "dependent");
    relationship::add_dependency(&project, &dependent, &target, &options())
        .expect("the edge is added");
    let target_before = parse(&project, &target);
    let dependent_before = parse(&project, &dependent);

    let outcome =
        archive::archive(&project, &target, None, &options_at(10)).expect("archive succeeds");

    assert_eq!(
        outcome.removals,
        vec![archive::PlannedRemoval {
            from: id(&dependent),
            relation: InboundRelation::Dependencies,
        }]
    );
    let after = parse(&project, &dependent);
    assert!(after.dependencies.is_empty());
    assert_eq!(after.updated_at.to_string(), "2026-11-03T05:43:20Z");
    assert_ne!(after.updated_at, dependent_before.updated_at);
    assert_eq!(
        parse(&project, &target).updated_at,
        target_before.updated_at
    );
}

#[test]
fn inbound_soft_links_are_removed_from_the_active_side_only() {
    let (_dir, project) = project();
    let target = make(&project, "target");
    let partner = make(&project, "partner");
    link::add_soft_link(&project, &target, &partner, &options()).expect("the link is added");
    let target_before = parse(&project, &target);

    let outcome =
        archive::archive(&project, &target, None, &options_at(10)).expect("archive succeeds");

    assert_eq!(
        outcome.removals_for(&id(&partner)),
        vec![InboundRelation::SoftLinks]
    );
    assert!(parse(&project, &partner).soft_links.is_empty());
    let archived = parse(&project, &target);
    assert_eq!(
        archived.soft_links, target_before.soft_links,
        "the archived document keeps its historical relations"
    );
    assert_eq!(archived.updated_at, target_before.updated_at);
}

#[test]
fn one_active_file_losing_several_relations_is_planned_once_per_relation() {
    let (_dir, project) = project();
    let target = make(&project, "target");
    let both = make(&project, "both");
    relationship::add_dependency(&project, &both, &target, &options()).expect("the edge is added");
    link::add_soft_link(&project, &both, &target, &options()).expect("the link is added");

    let outcome =
        archive::archive(&project, &target, None, &options_at(10)).expect("archive succeeds");

    assert_eq!(
        outcome.removals_for(&id(&both)),
        vec![InboundRelation::Dependencies, InboundRelation::SoftLinks]
    );
    assert_eq!(
        outcome.affected.len(),
        1,
        "one document, two stripped fields"
    );
    let after = parse(&project, &both);
    assert!(after.dependencies.is_empty());
    assert!(after.soft_links.is_empty());
}

#[test]
fn targeted_cleanup_leaves_body_and_custom_fields_alone() {
    let (_dir, project) = project();
    let target = make(&project, "target");
    let dependent = create::create(
        &project,
        CreateRequest::new("dependent", "task", "frank").with_body("keep this prose\n"),
        &options(),
    )
    .expect("creation succeeds")
    .item
    .id
    .as_str()
    .to_owned();
    edit::update_custom_fields(
        &project,
        &dependent,
        &[FieldChangeEntry::set(
            "note",
            FieldValue::String("keep".into()),
        )],
        &options(),
    )
    .expect("the field is set");
    relationship::add_dependency(&project, &dependent, &target, &options())
        .expect("the edge is added");
    let before = parse(&project, &dependent);

    archive::archive(&project, &target, None, &options_at(10)).expect("archive succeeds");

    let after = parse(&project, &dependent);
    assert_eq!(after.body, before.body);
    assert_eq!(after.custom_fields, before.custom_fields);
    assert_eq!(after.title, before.title);
    assert_eq!(after.status, before.status);
}

#[test]
fn a_broken_sibling_document_stops_the_archive_before_anything_is_written() {
    let (_dir, project) = project();
    let target = make(&project, "target");
    let dependent = make(&project, "dependent");
    relationship::add_dependency(&project, &dependent, &target, &options())
        .expect("the edge is added");
    let target_before = document(&project, &target);

    // A document that does not parse could be hiding an inbound edge, so the
    // complete scan fails and the archive is refused before the batch.
    let entry = project
        .locate(&id(&dependent))
        .expect("the dependent is located");
    std::fs::write(&entry.path, "---\nid: nope\n---\n").expect("the document is broken");

    let error = archive::archive(&project, &target, None, &options_at(10))
        .expect_err("the scan cannot be completed");

    assert_eq!(
        error.code(),
        Code::IncompleteProjection,
        "an unreadable document leaves no complete projection: {error:?}"
    );
    assert_eq!(
        document(&project, &target),
        target_before,
        "nothing was written"
    );
    assert!(!parse(&project, &target).is_archived());
}

#[test]
fn a_document_the_inventory_cannot_read_is_never_archived_through_a_link() {
    let (_dir, project) = project();
    let target = make(&project, "target");
    let entry = project.locate(&id(&target)).expect("the target is located");

    // A symlinked document is not a document the store will read or write, so
    // archive refuses rather than writing through the link.
    let real = entry.path.with_extension("real.md");
    std::fs::rename(&entry.path, &real).expect("the document moves aside");
    std::os::unix::fs::symlink(&real, &entry.path).expect("a symlink replaces it");

    let error = archive::archive(&project, &target, None, &options_at(10))
        .expect_err("the link is not an item");

    assert_eq!(error.code(), Code::ItemNotFound);
    let archived_bytes = std::fs::read_to_string(&real).expect("the real document is readable");
    assert!(
        !archived_bytes.contains("archived_at"),
        "the target was not archived"
    );
}

#[test]
fn archived_items_leave_the_active_collections_and_the_tree() {
    let (_dir, project) = project();
    let retired = make(&project, "retired");
    let _kept = make(&project, "kept");
    let blocker = make(&project, "blocker");
    relationship::add_dependency(&project, &retired, &blocker, &options()).expect("blocked once");
    let retired_root = make(&project, "retired root");
    let child = make(&project, "child of retired");
    parent::set_parent(&project, &child, &retired_root, &options()).expect("the child is placed");

    archive::archive(&project, &child, None, &options_at(5)).expect("the child archives");
    archive::archive(&project, &retired_root, None, &options_at(6)).expect("the root archives");
    archive::archive(&project, &retired, None, &options_at(7)).expect("the dependent archives");

    let listed = list::list(&project, &Query::new()).expect("list succeeds");
    assert_eq!(titles(&listed), vec!["kept", "blocker"]);
    let ready = list::ready(&project, &Query::new()).expect("ready succeeds");
    assert!(!titles(&ready).contains(&"retired".to_owned()));
    let blocked = list::blocked(&project, &Query::new()).expect("blocked succeeds");
    assert!(titles(&blocked).is_empty(), "{:?}", titles(&blocked));

    let tree = hierarchy_tree(&project, None, None).expect("the tree reads");
    let ids: Vec<String> = tree
        .roots
        .iter()
        .flat_map(|node| node.walk().into_iter().map(|node| node.id.to_string()))
        .collect();
    assert!(!ids.contains(&retired_root));
    assert!(
        !ids.contains(&child),
        "an archived child is not in the active tree"
    );
    assert!(tree.unresolved.is_empty(), "{:?}", tree.unresolved);

    // Reading one by stable ID still works and still says what it is.
    let read_back = read::read(&project, &retired_root).expect("show still reads an archived item");
    assert!(read_back.item.is_archived());
    assert_eq!(read_back.item.archive_reason, None);
}

#[test]
fn the_archived_only_range_selects_only_archived_items() {
    let (_dir, project) = project();
    let retired = make(&project, "retired");
    let kept = make(&project, "kept");
    archive::archive(&project, &retired, Some("done"), &options_at(10)).expect("archive succeeds");

    let archived = list::list(&project, &Query::new().with_archive(ArchiveRange::Archived))
        .expect("list succeeds");
    assert_eq!(titles(&archived), vec!["retired"]);
    assert_eq!(archived.items[0].item.id.as_str(), retired);
    assert!(archived.items[0].item.is_archived());
    assert!(!archived.items[0].readiness.ready);
    assert!(!archived.items[0].readiness.blocked);

    let active = list::list(&project, &Query::new()).expect("list succeeds");
    assert_eq!(titles(&active), vec!["kept"]);
    assert_eq!(active.items[0].item.id.as_str(), kept);

    // Readiness is about active work: an archived range never claims either.
    assert!(
        list::ready(&project, &Query::new().with_archive(ArchiveRange::Archived))
            .expect("ready succeeds")
            .is_empty()
    );
    assert!(
        list::blocked(&project, &Query::new().with_archive(ArchiveRange::Archived))
            .expect("blocked succeeds")
            .is_empty()
    );

    // The archive range composes with the status filter.
    assert!(list::list(
        &project,
        &Query::new()
            .with_archive(ArchiveRange::Archived)
            .with_status(Status::Closed)
    )
    .expect("list succeeds")
    .is_empty());
    assert_eq!(
        titles(
            &list::list(
                &project,
                &Query::new()
                    .with_archive(ArchiveRange::Archived)
                    .with_status(Status::Open)
            )
            .expect("list succeeds")
        ),
        vec!["retired"]
    );
}

#[test]
fn a_dry_run_performs_every_check_and_reports_every_removal() {
    let (_dir, project) = project();
    let target = make(&project, "target");
    let dependent = make(&project, "dependent");
    let partner = make(&project, "partner");
    relationship::add_dependency(&project, &dependent, &target, &options())
        .expect("the edge is added");
    link::add_soft_link(&project, &partner, &target, &options()).expect("the link is added");
    let target_before = document(&project, &target);
    let dependent_before = document(&project, &dependent);
    let partner_before = document(&project, &partner);

    let outcome = archive::archive(&project, &target, Some("dry"), &options_at(10).dry_run())
        .expect("the dry run succeeds");

    assert!(!outcome.written);
    assert!(outcome.fingerprints.is_empty());
    assert!(
        outcome.item.is_archived(),
        "the outcome still describes the archive"
    );
    assert_eq!(outcome.archive_reason.as_deref(), Some("dry"));
    assert_eq!(outcome.removals.len(), 2);
    assert_eq!(
        outcome.removals_for(&id(&dependent)),
        vec![InboundRelation::Dependencies]
    );
    assert_eq!(
        outcome.removals_for(&id(&partner)),
        vec![InboundRelation::SoftLinks]
    );
    assert_eq!(outcome.affected.len(), 2);
    assert_eq!(
        document(&project, &target),
        target_before,
        "nothing was written"
    );
    assert_eq!(document(&project, &dependent), dependent_before);
    assert_eq!(document(&project, &partner), partner_before);
}

#[test]
fn a_dry_run_does_not_wait_for_the_project_lock() {
    let (_dir, project) = project();
    let target = make(&project, "target");
    let dependent = make(&project, "dependent");
    relationship::add_dependency(&project, &dependent, &target, &options())
        .expect("the edge is added");

    // Hold the one write lock, then preview under an immediate wait: a dry run
    // that took the lock would answer LOCK_TIMEOUT instead of its report.
    with_project_lock(&project, LockWait::default_policy(), || {
        let outcome = archive::archive(
            &project,
            &target,
            Some("preview"),
            &options().with_lock_wait(LockWait::immediate()).dry_run(),
        )
        .expect("a dry run never waits on the write lock");
        assert!(!outcome.written);
        assert!(outcome.fingerprints.is_empty());
        assert_eq!(outcome.removals.len(), 1);
        Ok(())
    })
    .expect("the lock is released");
}

#[test]
fn a_dry_run_still_runs_the_child_check() {
    let (_dir, project) = project();
    let parent_id = make(&project, "parent");
    let child = make(&project, "child");
    parent::set_parent(&project, &child, &parent_id, &options()).expect("the parent edge is set");

    let error = archive::archive(&project, &parent_id, None, &options().dry_run())
        .expect_err("the dry run refuses too");

    assert_eq!(error.code(), Code::HasActiveChildren);
    assert!(!parse(&project, &parent_id).is_archived());
}

#[test]
fn a_dry_run_still_runs_the_already_archived_check() {
    let (_dir, project) = project();
    let item = make(&project, "work");
    archive::archive(&project, &item, None, &options_at(10)).expect("archive succeeds");

    let error = archive::archive(&project, &item, None, &options().dry_run())
        .expect_err("the dry run refuses too");

    assert_eq!(error.code(), Code::AlreadyArchived);
}

#[test]
fn a_missing_target_is_not_found_rather_than_a_silent_success() {
    let (_dir, project) = project();

    let error = archive::archive(&project, "wm-00000000zzzz", None, &options())
        .expect_err("a missing target is refused");

    assert_eq!(error.code(), Code::ItemNotFound);
}

#[test]
fn an_operator_who_is_not_the_owner_gets_a_warning_not_a_refusal() {
    let (_dir, project) = fixtures::deterministic_project();
    let item = create::create(
        &project,
        CreateRequest::new("work", "task", "alice"),
        &OpOptions::new()
            .with_clock(fixtures::shared_clock())
            .with_operator("frank"),
    )
    .expect("creation succeeds")
    .item
    .id
    .as_str()
    .to_owned();
    let mut options = options_at(10);
    options.operator = Some("frank".to_owned());

    let outcome =
        archive::archive(&project, &item, None, &options).expect("archive still proceeds");

    assert_eq!(outcome.meta.warnings.len(), 1);
    assert_eq!(outcome.meta.warnings[0].code, Code::OwnerMismatch);
}
