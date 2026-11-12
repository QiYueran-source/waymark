//! Soft links: symmetric, non-blocking associations.
//!
//! The seams exercised here are the two operations (`add_soft_link` /
//! `remove_soft_link`) and the two persisted documents. The rules the spec pins:
//! the association is stored on both sides, sorted and deduplicated; with
//! consent both files change in one all-or-nothing operation; without consent
//! only the caller's file changes and the partner keeps its exact bytes; a
//! duplicate, self, or consented repeat is `INVALID_RELATIONSHIP`; and a
//! missing or archived target is `ITEM_NOT_FOUND` / `ITEM_ARCHIVED`.
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

/// Make `operator` the acting identity the way a person does: the local
/// `user.toml` document, not an options override.
fn act_as(project: &Project, operator: &str) {
    waymark_core::ops::local_config::set_scalar(
        project,
        waymark_core::ops::config::Scalar::Operator,
        operator,
        &options(),
    )
    .expect("the operator is recorded");
}

/// Record a soft link by writing **one** document, the way an external edit or
/// an older half-write leaves one behind.
///
/// `link add` now refuses a non-consented partner write, and a consented add of
/// a pair where only one side records the link writes the missing half, so a
/// deliberately one-sided pair has to be set up directly.
fn set_soft_links(project: &Project, id: &str, targets: &[&str]) {
    let path = read::read(project, id).unwrap().path;
    let mut item = waymark_core::parse::parse_item_file(&path).unwrap();
    item.soft_links = targets
        .iter()
        .map(|target| waymark_core::WorkItemId::parse(target).unwrap())
        .collect();
    std::fs::write(&path, waymark_core::serialize::to_document(&item)).unwrap();
}

/// Options whose clock is frozen `seconds` after the fixture instant.
fn options_at(seconds: i64) -> OpOptions {
    let base = chrono::DateTime::parse_from_rfc3339(waymark_core::testing::FIXTURE_INSTANT)
        .expect("a valid instant")
        .with_timezone(&chrono::Utc);
    OpOptions::new().with_clock(std::sync::Arc::new(waymark_core::FixedClock::new(
        base + chrono::Duration::seconds(seconds),
    )))
}

#[test]
fn adding_a_link_without_consent_records_it_on_the_callers_side_only() {
    // Leaving a document alone is never a refusal: the partner is reported as
    // unsynchronised and keeps its exact bytes and timestamp.
    let (_dir, project) = project();
    act_as(&project, "frank");
    let left = make(&project, "task", "left");
    let right = make(&project, "bug", "right");
    let right_document = document(&project, &right);
    let right_updated_at = read::read(&project, &right).unwrap().item.updated_at;

    let outcome = add_soft_link(&project, &left, &right, &options_at(60)).unwrap();

    assert_eq!(links(&project, &left), vec![right.to_owned()]);
    assert!(outcome.partner_unsynchronised);
    assert_eq!(outcome.other_id.as_str(), right);
    assert_eq!(
        outcome.fingerprints.len(),
        1,
        "only the caller's document was replaced"
    );
    assert!(outcome.other.is_none());
    assert!(links(&project, &right).is_empty());
    assert_eq!(
        document(&project, &right),
        right_document,
        "the partner keeps its exact bytes"
    );
    assert_eq!(
        read::read(&project, &right).unwrap().item.updated_at,
        right_updated_at,
        "the partner's updated_at did not move"
    );
}

#[test]
fn repeating_a_link_add_without_consent_changes_nothing() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let left = make(&project, "task", "left");
    let right = make(&project, "bug", "right");

    add_soft_link(&project, &left, &right, &options_at(30)).unwrap();
    let left_document = document(&project, &left);
    let right_document = document(&project, &right);

    // The caller's half already exists and the partner was told nothing, so a
    // retry answers with that state instead of writing a second timestamp.
    let again = add_soft_link(&project, &left, &right, &options_at(60)).unwrap();
    assert!(!again.changed && !again.written);
    assert!(again.partner_unsynchronised);
    assert!(again.fingerprints.is_empty());
    assert_eq!(document(&project, &left), left_document);
    assert_eq!(document(&project, &right), right_document);
}

#[test]
fn adding_a_link_records_it_on_both_sides() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let left = make(&project, "task", "left");
    let right = make(&project, "bug", "right");

    let outcome = add_soft_link(&project, &left, &right, &options().with_cascade()).unwrap();

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
    act_as(&project, "frank");
    add_soft_link(&project, &a, &d, &options().with_cascade()).unwrap();
    add_soft_link(&project, &a, &c, &options().with_cascade()).unwrap();
    add_soft_link(&project, &a, &b, &options().with_cascade()).unwrap();

    let expected = vec![b.clone(), c.clone(), d.clone()];
    assert_eq!(links(&project, &a), expected);
    for other in [&b, &c, &d] {
        assert_eq!(links(&project, other), vec![a.clone()]);
    }

    // A link from the other side reaches the same state, not a second copy.
    remove_soft_link(&project, &d, &a, &options().with_cascade()).unwrap();
    assert!(links(&project, &d).is_empty());
    assert!(!links(&project, &a).contains(&d));
}

#[test]
fn adding_a_duplicate_link_is_an_invalid_relationship() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let left = make(&project, "task", "left");
    let right = make(&project, "task", "right");
    add_soft_link(&project, &left, &right, &options().with_cascade()).unwrap();
    let before = (document(&project, &left), document(&project, &right));

    let error = add_soft_link(&project, &left, &right, &options().with_cascade()).unwrap_err();
    assert_eq!(error.code(), Code::InvalidRelationship);
    assert_eq!(error.details()["relation"], serde_json::json!("soft_links"));

    // From the far side it is the same existing association, not a new one,
    // and consent does not turn it into a second one.
    let error = add_soft_link(&project, &right, &left, &options().with_cascade()).unwrap_err();
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
fn removing_a_link_without_consent_leaves_the_partner_byte_identical() {
    let (_dir, project) = project();
    let left = make(&project, "task", "left");
    let right = make(&project, "bug", "right");
    act_as(&project, "frank");
    add_soft_link(&project, &left, &right, &options().with_cascade()).unwrap();
    let partner = document(&project, &right);

    let outcome = remove_soft_link(&project, &left, &right, &options()).unwrap();

    assert!(outcome.changed && outcome.written);
    assert_eq!(outcome.other_id.as_str(), right);
    assert_eq!(
        outcome.fingerprints.len(),
        1,
        "only the caller's document was replaced"
    );
    assert!(
        outcome.other_untouched.is_some(),
        "the partner is reported as left alone"
    );
    let untouched = outcome
        .other_untouched
        .as_ref()
        .expect("the partner is reported as left alone");
    assert_eq!(untouched.id.as_str(), right);
    assert_eq!(untouched.path, read::read(&project, &right).unwrap().path);
    assert_eq!(untouched.owner, "frank");
    assert!(
        untouched.owned,
        "the partner belongs to the operator, and a one-sided removal still does not ask"
    );
    assert!(outcome.other.is_none(), "the partner was not rewritten");
    assert!(outcome.other_path.is_none());
    assert!(!document(&project, &left).contains("soft_links"));
    assert!(links(&project, &left).is_empty());
    assert_eq!(
        document(&project, &right),
        partner,
        "the partner keeps its bytes and its updated_at"
    );
    assert_eq!(
        links(&project, &right),
        vec![left.clone()],
        "the partner still lists the link"
    );
}

#[test]
fn a_one_sided_removal_of_a_partner_i_own_is_reported_as_mine() {
    let (_dir, project) = project();
    waymark_core::ops::local_config::set_scalar(
        &project,
        waymark_core::ops::config::Scalar::Operator,
        "frank",
        &options(),
    )
    .unwrap();
    let left = make(&project, "task", "left");
    let right = make(&project, "bug", "right");
    add_soft_link(&project, &left, &right, &options().with_cascade()).unwrap();
    let partner = document(&project, &right);

    let outcome = remove_soft_link(&project, &left, &right, &options()).unwrap();

    let untouched = outcome.other_untouched.as_ref().unwrap();
    assert!(
        untouched.owned,
        "the partner belongs to the effective operator"
    );
    assert_eq!(document(&project, &right), partner);
}

#[test]
fn removing_a_link_with_consent_updates_both_owned_sides() {
    let (_dir, project) = project();
    act_as(&project, "frank");
    let left = make(&project, "task", "left");
    let right = make(&project, "bug", "right");
    add_soft_link(&project, &left, &right, &options().with_cascade()).unwrap();
    let consented = options().with_cascade();

    let outcome = remove_soft_link(&project, &left, &right, &consented).unwrap();

    assert_eq!(outcome.fingerprints.len(), 2);
    assert_eq!(
        outcome.other.as_ref().map(|item| &item.id),
        Some(&outcome.other_id)
    );
    assert!(links(&project, &left).is_empty());
    assert!(links(&project, &right).is_empty());
}

#[test]
fn removing_a_link_that_does_not_exist_is_an_invalid_relationship() {
    let (_dir, project) = project();
    let left = make(&project, "task", "left");
    let right = make(&project, "task", "right");

    let error = remove_soft_link(&project, &left, &right, &options()).unwrap_err();

    assert_eq!(error.code(), Code::InvalidRelationship);

    let consented = options().with_cascade();
    let error = remove_soft_link(&project, &left, &right, &consented).unwrap_err();

    assert_eq!(
        error.code(),
        Code::InvalidRelationship,
        "consent does not make an absent link removable"
    );
}

#[test]
fn removing_a_link_to_a_deleted_target_cleans_the_surviving_side() {
    for consented in [false, true] {
        let (_dir, project) = project();
        act_as(&project, "frank");
        let left = make(&project, "task", "left");
        let right = make(&project, "task", "right");
        add_soft_link(&project, &left, &right, &options().with_cascade()).unwrap();
        let options = if consented {
            options().with_cascade()
        } else {
            options()
        };

        // Someone deletes the other document behind our back.
        let path = read::read(&project, &right).unwrap().path;
        std::fs::remove_file(path).unwrap();

        let outcome = remove_soft_link(&project, &left, &right, &options).unwrap();

        assert!(outcome.written);
        assert!(outcome.other.is_none(), "there was nothing to rewrite");
        assert!(
            outcome.other_untouched.is_none(),
            "a missing partner is not reported as left alone"
        );
        assert!(links(&project, &left).is_empty());
    }
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

    act_as(&project, "frank");
    let error = add_soft_link(&project, &left, &right, &options().with_cascade()).unwrap_err();
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

#[test]
fn a_soft_link_to_a_deleted_target_stays_written_and_is_diagnosed() {
    let (_dir, project) = project();
    let left = make(&project, "task", "left");
    let right = make(&project, "task", "right");
    set_soft_links(&project, &left, &[&right]);

    // The target document disappears; the surviving half-link is data, not a
    // reason to rewrite anything on a read.
    let path = read::read(&project, &right).unwrap().path;
    std::fs::remove_file(path).unwrap();
    let before = document(&project, &left);

    let graph = waymark_core::Graph::from_project(&project).unwrap();
    let findings: Vec<_> = graph
        .diagnostics()
        .into_iter()
        .filter(|finding| {
            finding.code == Code::DanglingReference && finding.message.contains("soft_links")
        })
        .collect();

    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].message.contains(&left));
    assert!(findings[0].message.contains(&right));
    assert_eq!(
        document(&project, &left),
        before,
        "reading never rewrites the surviving side"
    );
}
