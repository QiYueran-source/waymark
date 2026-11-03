//! Composite creation: one atomic operation for the complete initial state.
//!
//! The seams are the public `create` operation and the documents on disk. The
//! acceptance rules exercised here are the ones the catalog promises: a creator
//! distinct from the owner (or defaulted to the operator), typed custom fields,
//! initial parent/dependency/soft-link edges, the same bytes as creating and
//! then applying each relation one by one, symmetric partner rewrites, and a
//! refused or failed creation that leaves the store exactly as it was.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use chrono::{DateTime, Utc};
use waymark_core::config::{Config, FieldDecl, FieldType};
use waymark_core::error::Code;
use waymark_core::model::FieldKey;
use waymark_core::ops::create::{self, CreateRequest};
use waymark_core::ops::{link, parent, read, relationship, OpOptions};
use waymark_core::testing::{fixtures, SequentialIds};
use waymark_core::time::FixedClock;
use waymark_core::{Clock, FieldValue, InitOptions, Project};

/// The next planned ID in a project whose items were minted sequentially.
const NEXT_AFTER_ONE: &str = "wm-000000000001";

fn options() -> OpOptions {
    OpOptions::new().with_clock(fixtures::shared_clock())
}

fn field_key(literal: &str) -> FieldKey {
    FieldKey::parse(literal).expect("a valid field key")
}

/// A project declaring one field of every shape; `effort` is required so the
/// required-field rule is exercised by the same fixture.
fn config() -> Config {
    let mut config = Config::fresh();
    for (key, ty, required) in [
        ("effort", FieldType::Integer, true),
        ("note", FieldType::String, false),
        ("done", FieldType::Boolean, false),
        ("tags", FieldType::List(Box::new(FieldType::String)), false),
    ] {
        config.fields.insert(
            field_key(key),
            FieldDecl {
                ty,
                required,
                default: None,
            },
        );
    }
    config
}

fn project() -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 path");
    let project = Project::init(
        &root,
        InitOptions {
            config: Some(config()),
            ..InitOptions::default()
        },
    )
    .expect("a project with declared fields initialises")
    .with_clock(fixtures::shared_clock())
    .with_id_source(Box::new(SequentialIds::new()));
    (dir, project)
}

/// A request for the fixture project: the required `effort` value is supplied.
fn request(title: &str) -> CreateRequest {
    CreateRequest::new(title, "task", "frank")
        .with_field(field_key("effort"), FieldValue::Integer(1))
}

/// Create one seeded item and return its ID.
fn make(project: &Project, title: &str) -> String {
    create::create(project, request(title), &options())
        .expect("creation succeeds")
        .item
        .id
        .as_str()
        .to_owned()
}

fn read_item(project: &Project, id: &str) -> waymark_core::WorkItem {
    read::read(project, id).expect("the item is readable").item
}

fn task_path(project: &Project, id: &str) -> Utf8PathBuf {
    project.root().join(format!("work/task/{id}.md"))
}

/// Rewrite one document by hand, the way an external editor or a stale writer
/// would.
fn overwrite(project: &Project, id: &str, item: &waymark_core::WorkItem) {
    std::fs::write(
        task_path(project, id),
        waymark_core::serialize::to_document(item),
    )
    .expect("the document is writable");
}

fn archive_by_hand(project: &Project, id: &str) {
    let mut item = read_item(project, id);
    item.archived_at = Some(fixtures::instant());
    overwrite(project, id, &item);
}

/// Every regular file under `root`, keyed by repository-relative path.
fn snapshot(root: &Utf8Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    collect(root, root, &mut files);
    files
}

fn collect(root: &Utf8Path, directory: &Utf8Path, files: &mut BTreeMap<String, Vec<u8>>) {
    for entry in std::fs::read_dir(directory).expect("a readable directory") {
        let entry = entry.expect("a readable directory entry");
        let path = Utf8PathBuf::from_path_buf(entry.path()).expect("a UTF-8 path");
        if path.is_dir() {
            collect(root, &path, files);
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .expect("the file is under the root")
                .to_string();
            files.insert(relative, std::fs::read(&path).expect("a readable file"));
        }
    }
}

/// Assert a refused creation: the code matches, no document appears, and the
/// whole store is byte-identical.
fn assert_refused(project: &Project, candidate: CreateRequest, code: Code) {
    let before = snapshot(project.root());
    let entries = project
        .inventory()
        .expect("the inventory reads")
        .entries
        .len();
    let error =
        create::create(project, candidate, &options()).expect_err("the creation is refused");
    assert_eq!(error.code(), code, "{error}");
    assert_eq!(
        snapshot(project.root()),
        before,
        "a refused creation must leave the store byte-identical"
    );
    assert_eq!(
        project
            .inventory()
            .expect("the inventory reads")
            .entries
            .len(),
        entries,
        "no new document exists"
    );
}

#[test]
fn creation_accepts_a_creator_distinct_from_the_owner() {
    let (_dir, project) = project();
    let created = create::create(
        &project,
        request("Delegated").with_creator("alice"),
        &options(),
    )
    .expect("creation succeeds");

    assert_eq!(created.item.owner, "frank");
    assert_eq!(created.item.creator.as_deref(), Some("alice"));
    assert_eq!(
        read_item(&project, created.item.id.as_str())
            .creator
            .as_deref(),
        Some("alice"),
        "the creator is persisted, not just returned"
    );
}

#[test]
fn creation_defaults_the_creator_to_the_acting_operator() {
    let (_dir, mut project) = project();

    let anonymous = create::create(&project, request("Anonymous"), &options()).expect("creation");
    assert_eq!(anonymous.item.creator, None);
    assert_ne!(
        anonymous.item.creator.as_deref(),
        Some(anonymous.item.owner.as_str()),
        "the creator is never inferred from the owner"
    );

    let operator = create::create(
        &project,
        request("By operator").with_creator("  bob  "),
        &options().with_operator("alice"),
    )
    .expect("creation");
    assert_eq!(
        operator.item.creator.as_deref(),
        Some("bob"),
        "a supplied creator wins over the operator"
    );

    let defaulted = create::create(
        &project,
        request("Defaulted"),
        &options().with_operator("alice"),
    )
    .expect("creation");
    assert_eq!(defaulted.item.creator.as_deref(), Some("alice"));

    // The configured operator is the acting operator when no override is given.
    let mut config = project.config().clone();
    config.operator = Some("carol".to_owned());
    project.set_config(config);
    let configured = create::create(&project, request("Configured"), &options()).expect("creation");
    assert_eq!(configured.item.creator.as_deref(), Some("carol"));
}

#[test]
fn a_supplied_creator_is_validated_like_an_owner() {
    let (_dir, project) = project();
    for literal in ["   ", "line one\nline two"] {
        let error = create::create(
            &project,
            request("Bad creator").with_creator(literal),
            &options(),
        )
        .expect_err("the creator is not a valid identity");
        assert_eq!(error.code(), Code::ValidationError);
        let paths: Vec<&str> = error.details()["issues"]
            .as_array()
            .expect("the issue list")
            .iter()
            .filter_map(|issue| issue["path"].as_str())
            .collect();
        assert!(paths.contains(&"creator"), "{paths:?}");
    }
    assert!(project.inventory().expect("inventory").entries.is_empty());
}

#[test]
fn custom_fields_are_persisted_with_their_declared_types() {
    let (_dir, project) = project();
    let created = create::create(
        &project,
        CreateRequest::new("With fields", "task", "frank")
            .with_field(field_key("effort"), FieldValue::Integer(3))
            .with_field(field_key("note"), FieldValue::String("hello".to_owned()))
            .with_field(field_key("done"), FieldValue::Boolean(true))
            .with_field(
                field_key("tags"),
                FieldValue::List(vec![
                    FieldValue::String("a".to_owned()),
                    FieldValue::String("b".to_owned()),
                ]),
            ),
        &options(),
    )
    .expect("creation succeeds");

    let expected = created.item.custom_fields.clone();
    assert_eq!(expected.len(), 4);
    assert_eq!(expected["effort"], FieldValue::Integer(3));
    assert_eq!(
        read_item(&project, created.item.id.as_str()).custom_fields,
        expected
    );

    let document = std::fs::read_to_string(&created.path).expect("the document reads");
    assert!(document.contains("custom_fields:"), "{document}");
    assert!(document.contains("  effort: 3"), "{document}");
    assert!(document.contains("  note: hello"), "{document}");
    assert!(document.contains("  done: true"), "{document}");
}

#[test]
fn an_undeclared_field_fails_the_whole_create() {
    let (_dir, project) = project();
    let error = create::create(
        &project,
        request("Undeclared").with_field(field_key("nope"), FieldValue::Integer(1)),
        &options(),
    )
    .expect_err("the key is not declared");
    assert_eq!(error.code(), Code::ValidationError);
    assert_eq!(
        error.details()["issues"][0]["path"],
        serde_json::json!("custom_fields.nope")
    );
    assert!(project.inventory().expect("inventory").entries.is_empty());
}

#[test]
fn a_wrong_field_type_fails_the_whole_create() {
    let (_dir, project) = project();
    let error = create::create(
        &project,
        CreateRequest::new("Wrong type", "task", "frank")
            .with_field(field_key("effort"), FieldValue::String("lots".to_owned())),
        &options(),
    )
    .expect_err("the value does not fit the declaration");
    assert_eq!(error.code(), Code::ValidationError);
    assert_eq!(
        error.details()["issues"][0]["path"],
        serde_json::json!("custom_fields.effort")
    );
    assert!(project.inventory().expect("inventory").entries.is_empty());
}

#[test]
fn a_missing_required_field_fails_the_whole_create() {
    let (_dir, project) = project();
    let error = create::create(
        &project,
        CreateRequest::new("No effort", "task", "frank"),
        &options(),
    )
    .expect_err("the required field is absent");
    assert_eq!(error.code(), Code::ValidationError);
    assert_eq!(
        error.details()["issues"][0]["path"],
        serde_json::json!("custom_fields.effort")
    );
    assert!(project.inventory().expect("inventory").entries.is_empty());
}

#[test]
fn a_duplicate_field_key_fails_the_whole_create() {
    let (_dir, project) = project();
    let error = create::create(
        &project,
        CreateRequest::new("Twice", "task", "frank")
            .with_field(field_key("effort"), FieldValue::Integer(2))
            .with_field(field_key("note"), FieldValue::String("a".to_owned()))
            .with_field(field_key("note"), FieldValue::String("b".to_owned())),
        &options(),
    )
    .expect_err("the key is named twice");
    assert_eq!(error.code(), Code::ValidationError);
    assert_eq!(
        error.details()["issues"][0]["path"],
        serde_json::json!("custom_fields.note")
    );
    assert!(project.inventory().expect("inventory").entries.is_empty());
}

#[test]
fn soft_link_partners_gain_the_new_id_symmetrically() {
    let (_dir, project) = project();
    let first = make(&project, "first");
    let second = make(&project, "second");

    let created = create::create(
        &project,
        request("linked")
            .with_soft_link(first.as_str())
            .with_soft_link(second.as_str()),
        &options(),
    )
    .expect("creation succeeds");

    assert_eq!(created.partners.len(), 2);
    assert_eq!(
        created.fingerprints.len(),
        3,
        "new document plus two partners"
    );
    assert!(created
        .partners
        .iter()
        .all(|partner| partner.item.soft_links.contains(&created.item.id)));

    for other in [&first, &second] {
        let partner = read_item(&project, other);
        assert!(
            partner.soft_links.contains(&created.item.id),
            "the partner document records the new ID symmetrically"
        );
        assert_eq!(
            partner.updated_at, created.item.updated_at,
            "both sides carry the same operation instant"
        );
    }
}

#[test]
fn a_create_with_parent_dependencies_and_links_matches_applying_them_one_by_one() {
    let (_composite_dir, composite) = project();
    let (_sequential_dir, sequential) = project();

    let composite_ids: Vec<String> = ["a", "b", "c"]
        .iter()
        .map(|title| make(&composite, title))
        .collect();
    let sequential_ids: Vec<String> = ["a", "b", "c"]
        .iter()
        .map(|title| make(&sequential, title))
        .collect();
    assert_eq!(
        composite_ids, sequential_ids,
        "both fixtures mint the same IDs"
    );

    let composite_created = create::create(
        &composite,
        request("work")
            .with_field(field_key("note"), FieldValue::String("n".to_owned()))
            .with_parent(composite_ids[1].as_str())
            .with_dependency(composite_ids[0].as_str())
            .with_soft_link(composite_ids[0].as_str())
            .with_soft_link(composite_ids[2].as_str()),
        &options(),
    )
    .expect("composite creation succeeds");

    let sequential_created = create::create(
        &sequential,
        request("work").with_field(field_key("note"), FieldValue::String("n".to_owned())),
        &options(),
    )
    .expect("plain creation succeeds");
    assert_eq!(composite_created.item.id, sequential_created.item.id);

    parent::set_parent(
        &sequential,
        sequential_created.item.id.as_str(),
        &sequential_ids[1],
        &options(),
    )
    .expect("parent");
    relationship::add_dependency(
        &sequential,
        sequential_created.item.id.as_str(),
        &sequential_ids[0],
        &options(),
    )
    .expect("dependency");
    link::add_soft_link(
        &sequential,
        sequential_created.item.id.as_str(),
        &sequential_ids[0],
        &options(),
    )
    .expect("link");
    link::add_soft_link(
        &sequential,
        sequential_created.item.id.as_str(),
        &sequential_ids[2],
        &options(),
    )
    .expect("link");

    assert_eq!(
        snapshot(composite.root()),
        snapshot(sequential.root()),
        "composite creation must produce the same documents as the separate writes"
    );
}

#[test]
fn a_missing_dependency_target_creates_nothing() {
    let (_dir, project) = project();
    make(&project, "seed");
    assert_refused(
        &project,
        request("bad").with_dependency("wm-0000000000zz"),
        Code::ItemNotFound,
    );
}

#[test]
fn a_missing_parent_or_link_target_creates_nothing() {
    let (_dir, project) = project();
    make(&project, "seed");
    assert_refused(
        &project,
        request("bad").with_parent("wm-0000000000zz"),
        Code::ItemNotFound,
    );
    assert_refused(
        &project,
        request("bad").with_soft_link("wm-0000000000zz"),
        Code::ItemNotFound,
    );
}

#[test]
fn an_archived_target_creates_nothing() {
    let (_dir, project) = project();
    let retired = make(&project, "retired");
    archive_by_hand(&project, &retired);
    assert_refused(
        &project,
        request("bad").with_soft_link(retired.as_str()),
        Code::ItemArchived,
    );
}

#[test]
fn a_dry_run_applies_the_same_mediated_checks_to_soft_link_partners() {
    let (_dir, project) = project();

    // An archived partner is refused by both the preview and the write.
    let retired = make(&project, "retired");
    archive_by_hand(&project, &retired);
    let before = snapshot(project.root());
    let preview = create::create(
        &project,
        request("plan").with_soft_link(retired.as_str()),
        &options().dry_run(),
    )
    .expect_err("the preview refuses an archived partner");
    let write = create::create(
        &project,
        request("plan").with_soft_link(retired.as_str()),
        &options(),
    )
    .expect_err("the write refuses an archived partner");
    assert_eq!(preview.code(), Code::ItemArchived);
    assert_eq!(preview.code(), write.code());
    assert_eq!(snapshot(project.root()), before);

    // A partner whose generation predates this release is only caught by the
    // mediated read: the graph still sees an active node. The preview must catch
    // it exactly like the write.
    let stale = make(&project, "stale");
    let path = task_path(&project, &stale);
    let text = std::fs::read_to_string(&path).expect("the document is readable");
    let downgraded = text.replacen("schema: 1\n", "schema: 0\n", 1);
    assert_ne!(text, downgraded);
    std::fs::write(&path, downgraded).expect("the document is writable");
    let before = snapshot(project.root());

    let preview = create::create(
        &project,
        request("plan").with_soft_link(stale.as_str()),
        &options().dry_run(),
    )
    .expect_err("the preview refuses a stale partner");
    let write = create::create(
        &project,
        request("plan").with_soft_link(stale.as_str()),
        &options(),
    )
    .expect_err("the write refuses a stale partner");
    assert_eq!(preview.code(), Code::ItemNeedsMigration);
    assert_eq!(preview.code(), write.code());
    assert_eq!(snapshot(project.root()), before);
}

#[test]
fn a_duplicate_reference_creates_nothing() {
    let (_dir, project) = project();
    let seed = make(&project, "seed");
    assert_refused(
        &project,
        request("bad")
            .with_dependency(seed.as_str())
            .with_dependency(seed.to_uppercase()),
        Code::InvalidRelationship,
    );
}

#[test]
fn a_self_reference_creates_nothing() {
    let (_dir, project) = project();
    make(&project, "seed");
    assert_refused(
        &project,
        request("bad").with_parent(NEXT_AFTER_ONE),
        Code::InvalidRelationship,
    );
}

#[test]
fn a_parent_cycle_creates_nothing() {
    let (_dir, project) = project();
    let seed = make(&project, "seed");
    // An external edit points the seed at the ID the next create will mint.
    let mut item = read_item(&project, &seed);
    item.parent = Some(waymark_core::WorkItemId::parse(NEXT_AFTER_ONE).unwrap());
    overwrite(&project, &seed, &item);

    assert_refused(
        &project,
        request("bad").with_parent(seed.as_str()),
        Code::CycleDetected,
    );
}

#[test]
fn a_dependency_cycle_creates_nothing() {
    let (_dir, project) = project();
    let seed = make(&project, "seed");
    // A dangling reference to the ID the next create will mint closes the cycle.
    let mut item = read_item(&project, &seed);
    item.dependencies
        .push(waymark_core::WorkItemId::parse(NEXT_AFTER_ONE).unwrap());
    overwrite(&project, &seed, &item);

    assert_refused(
        &project,
        request("bad").with_dependency(seed.as_str()),
        Code::CycleDetected,
    );
}

/// A clock that turns a soft-link partner into a directory on its first call.
///
/// The operation reads the partner (and its fingerprint) before it asks for the
/// time, so this forces the partner's batch write to fail after the complete
/// plan — including the new document — exists.
struct SabotageClock {
    inner: FixedClock,
    partner: Utf8PathBuf,
    fired: AtomicBool,
}

impl Clock for SabotageClock {
    fn now(&self) -> DateTime<Utc> {
        if !self.fired.swap(true, Ordering::SeqCst) {
            std::fs::remove_file(&self.partner).expect("the partner document is removed");
            std::fs::create_dir(&self.partner).expect("a directory takes its place");
        }
        self.inner.now()
    }
}

#[test]
fn a_failed_partner_write_creates_nothing_and_leaves_the_store_byte_identical() {
    let (_dir, project) = project();
    let partner_id = make(&project, "partner");
    let partner_path = task_path(&project, &partner_id);
    let original = std::fs::read(&partner_path).expect("the partner document reads");

    let before = snapshot(project.root());
    let options = OpOptions::new().with_clock(Arc::new(SabotageClock {
        inner: fixtures::clock(),
        partner: partner_path.clone(),
        fired: AtomicBool::new(false),
    }));

    let error = create::create(
        &project,
        request("doomed").with_soft_link(partner_id.as_str()),
        &options,
    )
    .expect_err("the partner cannot be written");
    assert_ne!(error.code(), Code::RollbackFailed, "{error}");

    // Undo the test's own interference, then assert creation wrote nothing at
    // all: no new document, no changed partner, no debris.
    std::fs::remove_dir(&partner_path).expect("the sabotage directory is removed");
    std::fs::write(&partner_path, &original).expect("the partner is restored");

    assert_eq!(
        snapshot(project.root()),
        before,
        "a failed creation leaves the store exactly as it was"
    );
    assert_eq!(project.inventory().expect("inventory").entries.len(), 1);
    assert!(
        !project
            .root()
            .join(format!("work/task/{NEXT_AFTER_ONE}.md"))
            .exists(),
        "the new document never appeared"
    );
}

#[test]
fn a_dry_run_performs_every_check_and_writes_nothing() {
    let (_dir, project) = project();
    let first = make(&project, "first");
    let second = make(&project, "second");
    let before = snapshot(project.root());

    let planned = create::create(
        &project,
        request("planned")
            .with_field(field_key("note"), FieldValue::String("n".to_owned()))
            .with_soft_link(first.as_str())
            .with_soft_link(second.as_str()),
        &options().dry_run(),
    )
    .expect("the dry run validates");

    assert!(!planned.written);
    assert!(planned.fingerprint.is_none());
    assert!(planned.fingerprints.is_empty());
    assert!(!planned.path.exists());
    assert_eq!(planned.partners.len(), 2);
    assert!(planned
        .partners
        .iter()
        .all(|partner| partner.fingerprint.is_none()));
    assert_eq!(snapshot(project.root()), before, "a dry run writes nothing");

    // The dry run still performs the relationship checks.
    let error = create::create(
        &project,
        request("planned").with_soft_link("wm-0000000000zz"),
        &options().dry_run(),
    )
    .expect_err("the dry run refuses a missing target");
    assert_eq!(error.code(), Code::ItemNotFound);
    assert_eq!(snapshot(project.root()), before);
}
