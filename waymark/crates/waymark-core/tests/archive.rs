//! Archive: irreversible-through-Waymark logical deletion.
//!
//! The seam is `waymark_core::ops::archive::archive` plus the collection reads.
//! Every case drives the public API against a temporary project and then reads
//! the documents back, because archive's whole promise is about which bytes
//! change and which must not.
//!
//! The cross-item cases identify the caller through the local `user.toml`
//! document, the same way a real project does, rather than an options override:
//! whether an inbound document belongs to the operator is the whole question,
//! so the fixture that answers it must be the one the CLI reads.

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
use waymark_core::ops::{config::Scalar, OpOptions};
use waymark_core::ops::{create, edit, link, local_config, parent, read, relationship, status};
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

/// Make `operator` the acting identity the way a person does: the local
/// `user.toml` document. Git's `user.name` is only consulted when the document
/// says nothing, so a recorded operator is the deterministic choice here.
fn act_as(project: &Project, operator: &str) {
    local_config::set_scalar(project, Scalar::Operator, operator, &options())
        .expect("the operator is recorded");
}

fn make(project: &Project, title: &str) -> String {
    make_owned_by(project, title, "frank")
}

fn make_owned_by(project: &Project, title: &str, owner: &str) -> String {
    create::create(
        project,
        CreateRequest::new(title, "task", owner),
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

/// The active ready queue as the reader sees it.
fn ready_titles(project: &Project) -> Vec<String> {
    titles(&list::ready(project, &Query::new()).expect("ready reads"))
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

    // Consent does not reach children either: the hierarchy rule is untouched
    // by the cross-item consent rule.
    let error = archive::archive(&project, &parent_id, None, &options_at(10).with_cascade())
        .expect_err("consent does not detach a child");
    assert_eq!(error.code(), Code::HasActiveChildren);
    assert_eq!(document(&project, &parent_id), parent_before);
    assert_eq!(document(&project, &child), child_before);

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
fn archive_without_consent_rewrites_nothing_but_the_target() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let target = make(&project, "target");
    let dependent = make(&project, "dependent");
    let partner = make(&project, "partner");
    relationship::add_dependency(&project, &dependent, &target, &options())
        .expect("the edge is added");
    link::add_soft_link(&project, &partner, &target, &options()).expect("the link is added");
    let target_before = parse(&project, &target);
    let dependent_before = document(&project, &dependent);
    let partner_before = document(&project, &partner);
    assert!(
        dependent_before.contains(&format!("- {target}")),
        "the fixture gives the target an inbound dependency"
    );
    assert!(partner_before.contains(&target));

    let outcome =
        archive::archive(&project, &target, None, &options_at(10)).expect("archive succeeds");

    // The target is the one document that moved, and only its marker moved.
    let target_after = parse(&project, &target);
    assert!(target_after.is_archived());
    assert_eq!(
        target_after.updated_at, target_before.updated_at,
        "archive does not move the target's updated_at either"
    );
    assert_eq!(document(&project, &dependent), dependent_before);
    assert_eq!(document(&project, &partner), partner_before);

    assert!(outcome.removals.is_empty(), "no edge was stripped");
    assert!(outcome.affected.is_empty(), "no other document was written");
    assert!(outcome.written);
}

#[test]
fn a_foreign_reference_does_not_refuse_the_archive_and_is_named() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let target = make(&project, "target");
    let dependent = make_owned_by(&project, "dependent", "alice");
    let partner = make_owned_by(&project, "partner", "alice");
    let extra = make(&project, "mine");
    relationship::add_dependency(&project, &dependent, &target, &options())
        .expect("the edge is added");
    link::add_soft_link(&project, &partner, &target, &options()).expect("the link is added");
    relationship::add_dependency(&project, &extra, &target, &options()).expect("the edge is added");
    let dependent_before = document(&project, &dependent);
    let partner_before = document(&project, &partner);
    let extra_before = document(&project, &extra);

    let outcome =
        archive::archive(&project, &target, None, &options_at(10)).expect("archive succeeds");

    assert!(outcome.item.is_archived());
    assert_eq!(
        document(&project, &dependent),
        dependent_before,
        "another owner's document keeps its bytes"
    );
    assert_eq!(document(&project, &partner), partner_before);
    assert_eq!(document(&project, &extra), extra_before);
    assert!(outcome.removals.is_empty());
    assert!(outcome.affected.is_empty());

    // Every document it left alone, with the relation that still points at the
    // target, in stable-ID then relation order.
    assert_eq!(
        outcome
            .untouched
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec![dependent.as_str(), partner.as_str(), extra.as_str()]
    );
    assert!(outcome.untouched[0]
        .path
        .as_str()
        .ends_with(&format!("{dependent}.md")));
    assert_eq!(
        outcome.untouched_relations,
        vec![
            archive::UntouchedRelation {
                from: id(&dependent),
                relation: InboundRelation::Dependencies,
            },
            archive::UntouchedRelation {
                from: id(&partner),
                relation: InboundRelation::SoftLinks,
            },
            archive::UntouchedRelation {
                from: id(&extra),
                relation: InboundRelation::Dependencies,
            },
        ]
    );
}

#[test]
fn cascade_cleans_the_operators_own_inbound_edges_only() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let target = make(&project, "target");
    let mine = make(&project, "mine");
    let theirs = make_owned_by(&project, "theirs", "alice");
    relationship::add_dependency(&project, &mine, &target, &options()).expect("the edge is added");
    relationship::add_dependency(&project, &theirs, &target, &options())
        .expect("the edge is added");
    let theirs_before = document(&project, &theirs);
    let mine_before = parse(&project, &mine);

    let outcome = archive::archive(&project, &target, None, &options_at(10).with_cascade())
        .expect("archive succeeds");

    assert_eq!(
        outcome.removals,
        vec![archive::PlannedRemoval {
            from: id(&mine),
            relation: InboundRelation::Dependencies,
        }]
    );
    assert_eq!(
        outcome.affected.len(),
        1,
        "only the operator's document is rewritten"
    );
    assert_eq!(outcome.affected[0].id, id(&mine));
    let mine_after = parse(&project, &mine);
    assert!(mine_after.dependencies.is_empty());
    assert_ne!(
        mine_after.updated_at, mine_before.updated_at,
        "a cleaned document really changed, so it gets a new timestamp"
    );

    assert_eq!(
        document(&project, &theirs),
        theirs_before,
        "consent never reaches another owner's document"
    );
    assert_eq!(
        outcome
            .untouched
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec![theirs.as_str()]
    );
    assert_eq!(
        outcome.untouched_relations,
        vec![archive::UntouchedRelation {
            from: id(&theirs),
            relation: InboundRelation::Dependencies,
        }]
    );
}

#[test]
fn a_broken_operator_document_fails_consented_cleanup_as_a_configuration_error() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let target = make(&project, "target");
    let mine = make(&project, "mine");
    relationship::add_dependency(&project, &mine, &target, &options()).expect("the edge is added");
    let local = project
        .metadata_dir()
        .join(waymark_core::user_config::USER_CONFIG_FILE);
    std::fs::write(&local, "operator = [\n").expect("the document is broken");
    let target_before = document(&project, &target);
    let mine_before = document(&project, &mine);

    // Consent asks "is this document mine?", and a document nobody can read
    // does not answer it: the failure is the configuration, not a silent
    // "no operator" that would quietly strip nothing.
    let error = archive::archive(&project, &target, None, &options_at(10).with_cascade())
        .expect_err("a broken local document is a configuration error");

    assert_eq!(error.code(), Code::ConfigInvalid, "{error:?}");
    assert_eq!(document(&project, &target), target_before);
    assert_eq!(document(&project, &mine), mine_before);
}

#[test]
fn the_archived_items_own_relationships_are_preserved_exactly() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let target = make(&project, "target");
    let prerequisite = make(&project, "prerequisite");
    let parent_id = make(&project, "parent");
    let linked = make(&project, "linked");
    relationship::add_dependency(&project, &target, &prerequisite, &options())
        .expect("the outbound edge is added");
    parent::set_parent(&project, &target, &parent_id, &options()).expect("the parent is set");
    link::add_soft_link(&project, &target, &linked, &options()).expect("the link is added");
    let before = parse(&project, &target);
    assert!(
        !before.dependencies.is_empty() && before.parent.is_some() && !before.soft_links.is_empty(),
        "the fixture gives the target something to preserve"
    );

    let outcome = archive::archive(&project, &target, None, &options_at(10).with_cascade())
        .expect("archive succeeds");

    let after = parse(&project, &target);
    assert_eq!(after.dependencies, before.dependencies);
    assert_eq!(after.parent, before.parent);
    assert_eq!(after.soft_links, before.soft_links);
    assert_eq!(outcome.item.dependencies, before.dependencies);
    assert_eq!(outcome.item.parent, before.parent);
}

#[test]
fn a_dependency_on_an_archived_item_leaves_ready_and_blocked_alone() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let target = make(&project, "target");
    let dependent = make_owned_by(&project, "dependent", "alice");
    relationship::add_dependency(&project, &dependent, &target, &options())
        .expect("the edge is added");
    // The target is open work that blocks the dependent before the archive.
    assert_eq!(ready_titles(&project), vec!["target".to_owned()]);
    let blocked_before = titles(&list::blocked(&project, &Query::new()).expect("blocked reads"));
    assert_eq!(blocked_before, vec!["dependent".to_owned()]);

    archive::archive(&project, &target, None, &options_at(10).with_cascade())
        .expect("archive succeeds");

    // Only the archived target left the queue. The dependent is exactly as it
    // was: still active, still where it was in its own lifecycle, and its
    // dependency on an archived item does not turn into a block verdict.
    assert_eq!(ready_titles(&project), Vec::<String>::new());
    assert_eq!(
        titles(&list::blocked(&project, &Query::new()).expect("blocked reads")),
        blocked_before,
        "somebody else's archive does not change my queue"
    );
    let shown = read::read(&project, &dependent).expect("show reads the dependent");
    assert!(!shown.item.is_archived());
    assert_eq!(shown.item.dependencies, vec![id(&target)]);
}

#[test]
fn a_dry_run_plans_the_child_check_and_every_removal() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let target = make(&project, "target");
    let dependent = make(&project, "dependent");
    let partner = make(&project, "partner");
    relationship::add_dependency(&project, &dependent, &target, &options())
        .expect("the edge is added");
    link::add_soft_link(&project, &partner, &target, &options()).expect("the link is added");
    let target_before = parse(&project, &target);
    let dependent_before = parse(&project, &dependent);
    let partner_before = parse(&project, &partner);

    let outcome = archive::archive(
        &project,
        &target,
        Some("dry"),
        &options_at(10).with_cascade().dry_run(),
    )
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
        parse(&project, &target).updated_at,
        target_before.updated_at,
        "nothing was written"
    );
    assert_eq!(
        parse(&project, &dependent).updated_at,
        dependent_before.updated_at
    );
    assert_eq!(
        parse(&project, &partner).updated_at,
        partner_before.updated_at
    );
}

#[test]
fn a_dry_run_without_consent_predicts_exactly_what_the_real_run_does() {
    for cascade in [false, true] {
        let (_dir, project) = project();
        act_as(&project, "frank");
        let target = make(&project, "target");
        let mine = make(&project, "mine");
        let theirs = make_owned_by(&project, "theirs", "alice");
        relationship::add_dependency(&project, &mine, &target, &options())
            .expect("the edge is added");
        link::add_soft_link(&project, &theirs, &target, &options()).expect("the link is added");
        let snapshot = |project: &Project, item: &str| document(project, item);
        let before = [
            snapshot(&project, &target),
            snapshot(&project, &mine),
            snapshot(&project, &theirs),
        ];

        let preview = archive::archive(&project, &target, Some("later"), &{
            let mut options = options_at(10).dry_run();
            options.cascade = cascade;
            options
        })
        .expect("the preview succeeds");
        assert_eq!(
            [
                snapshot(&project, &target),
                snapshot(&project, &mine),
                snapshot(&project, &theirs),
            ],
            before,
            "a preview writes nothing (cascade={cascade})"
        );

        let real = archive::archive(&project, &target, Some("later"), &{
            let mut options = options_at(10);
            options.cascade = cascade;
            options
        })
        .expect("the real run succeeds");

        assert_eq!(preview.removals, real.removals, "cascade={cascade}");
        assert_eq!(preview.affected, real.affected, "cascade={cascade}");
        assert_eq!(preview.untouched, real.untouched, "cascade={cascade}");
        assert_eq!(
            preview.untouched_relations, real.untouched_relations,
            "cascade={cascade}"
        );
        assert!(!preview.written);
        assert!(real.written);
    }
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
        assert!(outcome.removals.is_empty());
        assert_eq!(outcome.untouched.len(), 1);
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
