//! Soft-link refusals: consent names an option, a foreign owner ends the road.
//!
//! The seam is the two operations (`add_soft_link` / `remove_soft_link`). Every
//! case drives the public API against a temporary project and then reads both
//! documents back, because a refusal is only real if the bytes did not move.
//!
//! The caller is identified through the local `user.toml` document — the source
//! a real project configures — and through an options override only where the
//! test is about which source answers.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use waymark_core::error::{Code, ExitClass};
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::link::{add_soft_link, remove_soft_link};
use waymark_core::ops::{config::Scalar, create, local_config, read, OpOptions};
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

fn options() -> OpOptions {
    options_at(0)
}

fn project() -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 path");
    let project = Project::init(&root, InitOptions::default())
        .expect("the project initialises")
        .with_clock(Arc::new(fixtures::clock()))
        .with_id_source(Box::new(SequentialIds::new()));
    (dir, project)
}

/// Make `operator` the acting identity the way a person does: the local
/// `user.toml` document.
fn act_as(project: &Project, operator: &str) {
    local_config::set_scalar(project, Scalar::Operator, operator, &options())
        .expect("the operator is recorded");
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

fn make(project: &Project, title: &str) -> String {
    make_owned_by(project, title, "frank")
}

fn document(project: &Project, item: &str) -> String {
    std::fs::read_to_string(read::read(project, item).expect("the item reads").path)
        .expect("the document is readable")
}

fn links(project: &Project, item: &str) -> Vec<String> {
    read::read(project, item)
        .expect("the item reads")
        .item
        .soft_links
        .iter()
        .map(|id| id.as_str().to_owned())
        .collect()
}

fn user_document(project: &Project) -> camino::Utf8PathBuf {
    project
        .metadata_dir()
        .join(waymark_core::user_config::USER_CONFIG_FILE)
}

/// Record the link on both sides, the way a completed pair looks.
fn seed_pair(project: &Project, left: &str, right: &str) {
    seed_caller_side(project, left, right);
    seed_caller_side(project, right, left);
}

/// Write the half of a link the named document records, leaving the other
/// document exactly as it is.
///
/// The fixtures need a one-sided pair as a starting state, and after this ticket
/// a real `link add` is refused rather than half-written, so setting up that
/// state is a hand edit — the same half-state a `git pull` can deliver.
fn seed_caller_side(project: &Project, from: &str, to: &str) {
    let entry = project
        .locate(&WorkItemId::parse(from).expect("a valid stable ID"))
        .expect("the item is in the inventory");
    let mut item = waymark_core::parse::parse_item_file(&entry.path).expect("the item parses");
    item.soft_links
        .push(WorkItemId::parse(to).expect("a valid stable ID"));
    std::fs::write(&entry.path, waymark_core::serialize::to_document(&item))
        .expect("the document is writable");
}

#[test]
fn an_add_without_consent_stops_when_the_partner_is_my_own_and_records_nothing() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let left = make(&project, "left");
    let right = make(&project, "right");
    let before = (document(&project, &left), document(&project, &right));

    let error =
        add_soft_link(&project, &left, &right, &options()).expect_err("consent is asked for");

    assert_eq!(error.code(), Code::CascadeConsentRequired, "{error:?}");
    assert!(!error.retryable());
    assert_eq!(error.exit_class(), Some(ExitClass::Conflict));
    assert_eq!(error.details()["operation"], serde_json::json!("link add"));
    assert_eq!(error.details()["id"], serde_json::json!(left));
    assert_eq!(error.details()["other"], serde_json::json!(right));
    assert_eq!(error.details()["option"], serde_json::json!("cascade"));
    assert!(
        error.message().contains("--cascade"),
        "the refusal names the option to re-run with: {}",
        error.message()
    );
    assert!(!error.message().contains("will not change"), "{error}");
    assert_eq!(
        (document(&project, &left), document(&project, &right)),
        before,
        "a refusal writes nothing"
    );

    // The refusal is the same on a dry run: a preview never promises a write
    // the real run would refuse.
    let error = add_soft_link(&project, &left, &right, &options().dry_run())
        .expect_err("the preview refuses too");
    assert_eq!(error.code(), Code::CascadeConsentRequired);
    assert_eq!(
        (document(&project, &left), document(&project, &right)),
        before
    );
}

#[test]
fn a_consented_add_writes_both_sides_of_a_pair_i_own() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let left = make(&project, "left");
    let right = make(&project, "right");

    let outcome = add_soft_link(&project, &left, &right, &options().with_cascade())
        .expect("two documents, one owner, consent given");

    assert!(outcome.written);
    assert_eq!(outcome.fingerprints.len(), 2);
    assert_eq!(links(&project, &left), vec![right.clone()]);
    assert_eq!(links(&project, &right), vec![left.clone()]);
    assert!(
        outcome.meta.warnings.is_empty(),
        "owning both ends warns about nothing: {:?}",
        outcome.meta.warnings
    );
}

#[test]
fn a_consented_add_against_a_foreign_partner_is_owned_by_other_and_writes_nothing() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let mine = make(&project, "mine");
    let theirs = make_owned_by(&project, "theirs", "alice");
    let before = (document(&project, &mine), document(&project, &theirs));

    let error = add_soft_link(&project, &mine, &theirs, &options().with_cascade())
        .expect_err("consent does not grant access to another owner's document");

    assert_eq!(error.code(), Code::OwnedByOther, "{error:?}");
    assert_eq!(error.details()["id"], serde_json::json!(theirs));
    assert_eq!(error.details()["owner"], serde_json::json!("alice"));
    assert_eq!(error.details()["operation"], serde_json::json!("link add"));
    assert!(
        error.message().contains("alice"),
        "the refusal names the owner: {}",
        error.message()
    );
    assert!(
        error
            .message()
            .contains("re-running will not change the outcome"),
        "the refusal says retrying cannot help: {}",
        error.message()
    );
    assert_eq!(
        (document(&project, &mine), document(&project, &theirs)),
        before,
        "a foreign partner is never written"
    );
}

#[test]
fn an_add_without_consent_against_a_foreign_partner_asks_for_consent_first() {
    // Consent alone never decides — the ownership answer is the same with and
    // without it — but the caller is told the one thing it can fix by itself
    // first. The foreign owner is reported as soon as consent is given.
    let (_dir, project) = project();
    act_as(&project, "frank");
    let mine = make(&project, "mine");
    let theirs = make_owned_by(&project, "theirs", "alice");
    let before = (document(&project, &mine), document(&project, &theirs));

    let error = add_soft_link(&project, &mine, &theirs, &options())
        .expect_err("a foreign partner is refused without consent as well");
    assert_eq!(error.code(), Code::CascadeConsentRequired, "{error:?}");

    let error = add_soft_link(&project, &mine, &theirs, &options().with_cascade())
        .expect_err("and still refused once consent is given");
    assert_eq!(error.code(), Code::OwnedByOther, "{error:?}");
    assert_eq!(
        (document(&project, &mine), document(&project, &theirs)),
        before
    );
}

#[test]
fn the_ownership_refusal_waits_for_consent_when_both_conditions_fail() {
    // Both are wrong at once. The cheap, self-correcting refusal comes first, so
    // the caller is not sent looking for an owner before it has even consented.
    let (_dir, project) = project();
    act_as(&project, "frank");
    let mine = make(&project, "mine");
    let theirs = make_owned_by(&project, "theirs", "alice");

    let error = add_soft_link(&project, &mine, &theirs, &options()).expect_err("both fail");
    assert_eq!(error.code(), Code::CascadeConsentRequired);

    let error = add_soft_link(&project, &mine, &theirs, &options().with_cascade())
        .expect_err("the owner is still somebody else");
    assert_eq!(error.code(), Code::OwnedByOther);
}

#[test]
fn re_running_an_add_after_a_refusal_asks_for_consent_and_writes_nothing() {
    // A refusal stops the write, and re-running the same command repeats the
    // same refusal. The one thing that changes it is the option the message
    // names, which completes the pair in a single step.
    let (_dir, project) = project();
    act_as(&project, "frank");
    let left = make(&project, "left");
    let right = make(&project, "right");

    for _ in 0..2 {
        let error = add_soft_link(&project, &left, &right, &options())
            .expect_err("a non-consenting add never writes the partner");
        assert_eq!(error.code(), Code::CascadeConsentRequired, "{error:?}");
        assert!(error.message().contains("--cascade"));
        assert!(links(&project, &left).is_empty(), "nothing was recorded");
        assert!(links(&project, &right).is_empty(), "nothing was recorded");
    }

    let outcome = add_soft_link(&project, &left, &right, &options().with_cascade())
        .expect("consent completes what the refusal asked for");
    assert!(outcome.written);
    assert!(!outcome.partner_unsynchronised);
    assert_eq!(links(&project, &right), vec![left.clone()]);

    // Now the pair is complete, and consent has nothing left to write.
    let error = add_soft_link(&project, &left, &right, &options().with_cascade())
        .expect_err("the pair already records the link");
    assert_eq!(error.code(), Code::InvalidRelationship);
}

#[test]
fn a_retry_of_a_one_sided_add_reports_the_state_and_consent_completes_it() {
    // A one-sided pair arrives through Git, and re-running the same command is
    // then the honest answer: success, no write, `partner_unsynchronised`. The
    // pair converges with one consented add.
    let (_dir, project) = project();
    act_as(&project, "frank");
    let left = make(&project, "left");
    let right = make(&project, "right");
    seed_caller_side(&project, &left, &right);
    let before = (document(&project, &left), document(&project, &right));

    let outcome = add_soft_link(&project, &left, &right, &options())
        .expect("a repeat of a one-sided add is not an error");
    assert!(!outcome.changed && !outcome.written);
    assert!(outcome.partner_unsynchronised);
    assert!(outcome.other.is_none());
    assert_eq!(
        (document(&project, &left), document(&project, &right)),
        before,
        "neither document was rewritten"
    );

    let outcome = add_soft_link(&project, &left, &right, &options().with_cascade())
        .expect("consent completes the pair");
    assert!(outcome.written);
    assert!(!outcome.partner_unsynchronised);
    assert_eq!(links(&project, &right), vec![left.clone()]);
}

#[test]
fn a_consented_repeat_of_a_complete_pair_is_the_duplicate() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let left = make(&project, "left");
    let right = make(&project, "right");
    add_soft_link(&project, &left, &right, &options().with_cascade())
        .expect("the first add is the pair the caller asked for");
    let before = (document(&project, &left), document(&project, &right));

    let error = add_soft_link(&project, &left, &right, &options().with_cascade())
        .expect_err("the pair already records the link");
    assert_eq!(error.code(), Code::InvalidRelationship, "{error:?}");
    assert_eq!(
        (document(&project, &left), document(&project, &right)),
        before,
        "a duplicate writes nothing"
    );
}

#[test]
fn an_add_as_the_far_side_of_a_one_sided_pair_records_only_its_own_half() {
    // The far side has not recorded the link, so this is a first add for that
    // document — and a first add needs consent like any other.
    let (_dir, project) = project();
    act_as(&project, "frank");
    let left = make(&project, "left");
    let right = make(&project, "right");
    seed_caller_side(&project, &left, &right);
    let left_before = document(&project, &left);

    let error = add_soft_link(&project, &right, &left, &options())
        .expect_err("the partner write is not consented to");
    assert_eq!(error.code(), Code::CascadeConsentRequired);
    assert_eq!(document(&project, &left), left_before);

    let outcome = add_soft_link(&project, &right, &left, &options().with_cascade())
        .expect("both ends are mine and consent was given");
    assert_eq!(outcome.fingerprints.len(), 2);
    assert_eq!(links(&project, &left), vec![right.clone()]);
    assert_eq!(links(&project, &right), vec![left.clone()]);
}

#[test]
fn a_consented_remove_against_a_foreign_partner_is_owned_by_other_and_keeps_both_sides() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let mine = make(&project, "mine");
    let theirs = make_owned_by(&project, "theirs", "alice");
    // Both sides record the link, so the removal has something to drop on the
    // partner and only the ownership rule can stop it.
    seed_pair(&project, &mine, &theirs);
    let before = (document(&project, &mine), document(&project, &theirs));

    let error = remove_soft_link(&project, &mine, &theirs, &options().with_cascade())
        .expect_err("a consented removal may not rewrite a foreign document");

    assert_eq!(error.code(), Code::OwnedByOther, "{error:?}");
    assert_eq!(error.details()["id"], serde_json::json!(theirs));
    assert_eq!(
        error.details()["operation"],
        serde_json::json!("link remove")
    );
    assert_eq!(
        (document(&project, &mine), document(&project, &theirs)),
        before
    );

    // Without consent the removal is one document and always succeeds: the
    // ownership question does not arise.
    let outcome = remove_soft_link(&project, &mine, &theirs, &options())
        .expect("dropping my own side never needs the owner's consent");
    assert!(outcome.written);
    assert_eq!(
        outcome.other_untouched.as_ref().map(|link| &link.id),
        Some(&WorkItemId::parse(&theirs).expect("a valid stable ID")),
        "the surviving partner is reported as left alone"
    );
    assert_eq!(
        document(&project, &theirs),
        before.1,
        "the partner keeps its bytes"
    );
    assert!(links(&project, &mine).is_empty());
}

#[test]
fn a_consented_remove_of_a_pair_i_own_drops_both_sides() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let left = make(&project, "left");
    let right = make(&project, "right");
    add_soft_link(&project, &left, &right, &options().with_cascade())
        .expect("the pair is recorded on both sides");
    let partner_before = document(&project, &right);

    let outcome = remove_soft_link(&project, &left, &right, &options().with_cascade())
        .expect("both ends are mine and consent was given");

    assert_eq!(outcome.fingerprints.len(), 2);
    assert_ne!(document(&project, &right), partner_before);
    assert!(links(&project, &left).is_empty());
    assert!(links(&project, &right).is_empty());
}

#[test]
fn a_broken_local_document_fails_a_cross_item_write_as_a_configuration_error() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let left = make(&project, "left");
    let right = make(&project, "right");
    std::fs::write(user_document(&project), "operator = [\n").expect("the document is broken");
    let before = (document(&project, &left), document(&project, &right));

    let error = add_soft_link(&project, &left, &right, &options().with_cascade())
        .expect_err("an unreadable identity is a configuration error, not \"no operator\"");

    assert_eq!(error.code(), Code::ConfigInvalid, "{error:?}");
    assert!(
        error.message().contains("user.toml"),
        "the message names the identity source: {}",
        error.message()
    );
    assert_eq!(
        (document(&project, &left), document(&project, &right)),
        before,
        "nothing was written"
    );
}

#[test]
fn a_project_with_no_operator_refuses_a_cross_item_write_and_names_the_source() {
    // No local document, no override, and an isolated environment: Git cannot
    // supply a fallback here, which is also what a CI checkout looks like.
    let (_dir, project) = project();
    let left = make(&project, "left");
    let right = make(&project, "right");
    let before = (document(&project, &left), document(&project, &right));
    let local = user_document(&project);
    assert!(
        !local.exists(),
        "the project starts with no operator document"
    );

    let error = add_soft_link(&project, &left, &right, &options().with_cascade())
        .expect_err("a cross-item write with nobody to own it is refused");

    if error.code() != Code::ConfigInvalid {
        // A machine whose Git `user.name` resolves has an operator after all:
        // then the pair is legitimately written or legitimately refused as
        // another owner's, and the missing-source path cannot be exercised here.
        assert!(
            matches!(error.code(), Code::OwnedByOther),
            "unexpected refusal: {error:?}"
        );
        return;
    }
    assert!(
        error
            .message()
            .contains("no operator identity is configured"),
        "{}",
        error.message()
    );
    assert!(
        error.message().contains("user.toml"),
        "the message names the document that would supply it: {}",
        error.message()
    );
    assert_eq!(
        (document(&project, &left), document(&project, &right)),
        before,
        "nothing was written"
    );
}
