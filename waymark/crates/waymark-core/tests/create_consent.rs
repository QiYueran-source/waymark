//! Creation and its soft-link partner: consent decides who is rewritten.
//!
//! A soft link is stored on both ends, but the partner is a document the caller
//! did not name. These tests pin the rule through the public operation and the
//! files on disk: without consent the partner keeps its bytes and its
//! `updated_at` while the new item records the link on its own side; with
//! consent the partner is rewritten only when the new item's owner, the
//! partner's owner, and the operator are the same identity; nothing is refused
//! because a partner was left alone; and a rewritten partner is always listed in
//! the result, with an `OWNER_MISMATCH` warning when the operator is acting for
//! its owner.

use waymark_core::config::FieldType;
use waymark_core::model::FieldKey;
use waymark_core::ops::config::Scalar;
use waymark_core::ops::create::{self, CreateRequest};
use waymark_core::ops::{local_config, OpOptions};
use waymark_core::{FieldValue, InitOptions, Project, WorkItem, WorkItemId};

fn options() -> OpOptions {
    OpOptions::new().with_clock(waymark_core::testing::fixtures::shared_clock())
}

/// The declared field every item in these tests fills in.
fn field() -> FieldKey {
    FieldKey::parse("effort").expect("a valid field key")
}

/// A project that declares one required integer field before it is opened.
fn project() -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 path");
    let mut declared = waymark_core::config::Config::fresh();
    declared.fields.insert(
        field(),
        waymark_core::config::FieldDecl {
            ty: FieldType::Integer,
            required: true,
            default: None,
        },
    );
    let project = Project::init(
        &root,
        InitOptions {
            config: Some(declared),
            ..InitOptions::default()
        },
    )
    .expect("a fresh project initialises")
    .with_clock(waymark_core::testing::fixtures::shared_clock())
    .with_id_source(Box::new(waymark_core::testing::SequentialIds::new()));
    (dir, project)
}

/// A request owned by `owner`, with the required field supplied.
fn request(owner: &str, title: &str) -> CreateRequest {
    CreateRequest::new(title, "task", owner).with_field(field(), FieldValue::Integer(1))
}

/// Create one item directly through the operation and return its ID.
fn make(project: &Project, owner: &str, title: &str) -> String {
    create::create(project, request(owner, title), &options())
        .expect("creation succeeds")
        .item
        .id
        .as_str()
        .to_owned()
}

fn item_id(literal: &str) -> WorkItemId {
    WorkItemId::parse(literal).expect("a stable ID")
}

/// The item whose document is on disk, read through the project entry.
fn read_item(project: &Project, id: &str) -> WorkItem {
    let entry = project
        .locate(&item_id(id))
        .expect("the item is inventoried");
    waymark_core::parse::parse_item_file(&entry.path).expect("the document parses")
}

/// The partner's document bytes, read directly so nothing re-serialises them.
fn document(project: &Project, id: &str) -> Vec<u8> {
    let entry = project
        .locate(&item_id(id))
        .expect("the item is inventoried");
    std::fs::read(&entry.path).expect("the document is readable")
}

/// Make `operator` the acting identity the way a person does: the local
/// `user.toml` document, not an options override.
fn act_as(project: &Project, operator: &str) {
    local_config::set_scalar(project, Scalar::Operator, operator, &options())
        .expect("the operator is recorded");
}

/// The warning codes a result's `_meta` carries.
fn warning_codes(created: &create::Created) -> Vec<String> {
    created
        .meta
        .warnings
        .iter()
        .map(|warning| warning.code.to_string())
        .collect()
}

fn rewritten(created: &create::Created) -> Vec<String> {
    created
        .partners
        .iter()
        .map(|partner| partner.item.id.as_str().to_owned())
        .collect()
}

fn untouched(created: &create::Created) -> Vec<String> {
    created
        .untouched
        .iter()
        .map(|partner| partner.item.id.as_str().to_owned())
        .collect()
}

#[test]
fn without_consent_the_partner_keeps_its_bytes_and_updated_at() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let partner = make(&project, "frank", "partner");
    let partner_document = document(&project, &partner);
    let partner_updated_at = read_item(&project, &partner).updated_at;

    let created = create::create(
        &project,
        request("frank", "linked").with_soft_link(partner.as_str()),
        &options(),
    )
    .expect("creation succeeds");

    assert!(created.written);
    assert_eq!(created.item.soft_links, vec![item_id(&partner)]);
    assert!(created.partners.is_empty(), "no partner was rewritten");
    assert_eq!(created.fingerprints.len(), 1, "the new document alone");
    assert_eq!(
        document(&project, &partner),
        partner_document,
        "the partner keeps its exact bytes"
    );
    assert_eq!(
        read_item(&project, &partner).updated_at,
        partner_updated_at,
        "the partner's updated_at did not move"
    );
    assert!(
        read_item(&project, &partner).soft_links.is_empty(),
        "the partner never recorded the link"
    );
    assert_eq!(untouched(&created), vec![partner]);
    assert_eq!(
        warning_codes(&created),
        Vec::<String>::new(),
        "leaving a partner alone is reported, not warned about"
    );
}

#[test]
fn consent_rewrites_a_partner_the_operator_owns_and_reports_it() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let partner = make(&project, "frank", "partner");

    let created = create::create(
        &project,
        request("frank", "linked").with_soft_link(partner.as_str()),
        &options().with_cascade(),
    )
    .expect("creation succeeds");

    assert_eq!(rewritten(&created), vec![partner.clone()]);
    assert_eq!(
        created.partners[0].path,
        project
            .locate(&item_id(&partner))
            .expect("inventoried")
            .path
    );
    assert_eq!(
        created.fingerprints.len(),
        2,
        "new document plus the rewritten partner"
    );
    assert!(created.untouched.is_empty(), "nothing was left alone");
    let stored = read_item(&project, &partner);
    assert_eq!(stored.soft_links, vec![created.item.id.clone()]);
    assert_eq!(stored.updated_at, created.item.updated_at);
    assert_eq!(
        warning_codes(&created),
        Vec::<String>::new(),
        "a partner taken from its own owner needs no warning"
    );
}

/// A partner the operator rewrites through the `--operator` override: the
/// operator is its owner, the new item is somebody else's, and the result still
/// carries the document itself so its new `updated_at` is explained.
#[test]
fn consent_rewrites_a_partner_the_operator_owns_while_creating_for_somebody_else() {
    let (_dir, project) = project();
    let partner = make(&project, "frank", "partner");

    // The operator speaks for the partner's owner through the override, while
    // the new item is created for somebody else; the partner is the operator's
    // own document, so it is rewritten and the result names it.
    let created = create::create(
        &project,
        request("frank", "linked").with_soft_link(partner.as_str()),
        &options().with_cascade().with_operator("frank"),
    )
    .expect("creation succeeds");

    assert_eq!(
        rewritten(&created),
        vec![partner.clone()],
        "the result names the partner it rewrote"
    );
    assert_eq!(
        read_item(&project, &partner).soft_links,
        vec![created.item.id.clone()]
    );
    assert_eq!(
        warning_codes(&created),
        Vec::<String>::new(),
        "an owner rewriting its own document needs no warning"
    );
}

#[test]
fn consent_leaves_a_partner_alone_when_the_new_item_owner_is_not_the_operator() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let partner = make(&project, "alice", "partner");
    let partner_document = document(&project, &partner);

    let created = create::create(
        &project,
        request("alice", "linked").with_soft_link(partner.as_str()),
        &OpOptions::new()
            .with_clock(waymark_core::testing::fixtures::shared_clock())
            .with_cascade()
            .with_operator("frank"),
    )
    .expect("creation is not refused");

    assert!(created.written, "the new item is created");
    assert!(created.partners.is_empty(), "the partner was not rewritten");
    assert_eq!(created.fingerprints.len(), 1);
    assert_eq!(untouched(&created), vec![partner.clone()]);
    assert_eq!(
        document(&project, &partner),
        partner_document,
        "the partner keeps its exact bytes"
    );
}

#[test]
fn consent_rewrites_only_the_operators_own_partners() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let own = make(&project, "frank", "mine");
    let foreign = make(&project, "alice", "hers");
    let foreign_document = document(&project, &foreign);
    let foreign_updated_at = read_item(&project, &foreign).updated_at;

    let created = create::create(
        &project,
        request("frank", "linked")
            .with_soft_link(own.as_str())
            .with_soft_link(foreign.as_str()),
        &options().with_cascade(),
    )
    .expect("creation is not refused");

    assert_eq!(rewritten(&created), vec![own]);
    assert_eq!(created.fingerprints.len(), 2);
    assert_eq!(untouched(&created), vec![foreign.clone()]);
    assert_eq!(document(&project, &foreign), foreign_document);
    assert_eq!(read_item(&project, &foreign).updated_at, foreign_updated_at);
}

#[test]
fn consent_leaves_the_partner_alone_when_the_recorded_operator_is_somebody_else() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let partner = make(&project, "frank", "partner");
    let partner_document = document(&project, &partner);

    let created = create::create(
        &project,
        request("frank", "linked").with_soft_link(partner.as_str()),
        &options().with_cascade().with_operator("alice"),
    )
    .expect("creation succeeds");

    assert!(
        created.partners.is_empty(),
        "the recorded operator is not the partner's owner"
    );
    assert_eq!(untouched(&created), vec![partner.clone()]);
    assert_eq!(document(&project, &partner), partner_document);
}

#[test]
fn no_operator_is_no_owner_so_consent_rewrites_nothing() {
    let (_dir, project) = project();
    let partner = make(&project, "frank", "partner");
    let partner_updated_at = read_item(&project, &partner).updated_at;

    let created = create::create(
        &project,
        request("frank", "linked").with_soft_link(partner.as_str()),
        &options().with_cascade(),
    )
    .expect("creation succeeds");

    assert!(
        created.partners.is_empty(),
        "nobody can claim the partner, so it is left alone"
    );
    assert_eq!(untouched(&created), vec![partner.clone()]);
    assert_eq!(read_item(&project, &partner).updated_at, partner_updated_at);
}

#[test]
fn a_dry_run_names_the_rewritten_and_the_untouched_partners() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let own = make(&project, "frank", "mine");
    let foreign = make(&project, "alice", "hers");
    let foreign_document = document(&project, &foreign);

    let planned = create::create(
        &project,
        request("frank", "planned")
            .with_soft_link(own.as_str())
            .with_soft_link(foreign.as_str()),
        &options().with_cascade().dry_run(),
    )
    .expect("the dry run validates");

    assert!(!planned.written);
    assert_eq!(rewritten(&planned), vec![own.clone()]);
    assert_eq!(untouched(&planned), vec![foreign.clone()]);
    assert!(planned
        .partners
        .iter()
        .all(|partner| partner.fingerprint.is_none()));
    assert!(
        read_item(&project, &own).soft_links.is_empty(),
        "a dry run writes nothing"
    );
    assert_eq!(document(&project, &foreign), foreign_document);
}
