//! `INCOMPLETE_PROJECTION` names the shortfall instead of restating the
//! requirement.
//!
//! The old wording read "a complete, fresh projection is required but the
//! projection is scoped and fresh", which presents the requirement as if
//! freshness were the problem. The message now states what is missing (how many
//! documents could not be read and how many item-shaped paths are uncertain) or
//! why the answer cannot be trusted (staleness), and never both halves of the
//! requirement twice.

use camino::Utf8PathBuf;
use waymark_core::error::{Code, WaymarkError};
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::{create, OpOptions};
use waymark_core::projection::Projection;
use waymark_core::testing::fixtures;
use waymark_core::Project;

/// The requirement clause every message opens with, asserted once per case so a
/// message can never bury the requirement or repeat it mid-sentence.
const REQUIREMENT: &str = "a complete, fresh projection is required but ";

fn options() -> OpOptions {
    OpOptions::new().with_clock(fixtures::shared_clock())
}

fn project() -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let project = Project::init(&root, waymark_core::store::InitOptions::default())
        .unwrap()
        .with_clock(fixtures::shared_clock())
        .with_id_source(Box::new(waymark_core::testing::SequentialIds::new()));
    (dir, project)
}

fn make(project: &Project, title: &str) -> String {
    create::create(
        project,
        CreateRequest::new(title, "task", "frank"),
        &options(),
    )
    .unwrap()
    .item
    .id
    .as_str()
    .to_owned()
}

fn task_path(project: &Project, id: &str) -> Utf8PathBuf {
    project.root().join(format!("work/task/{id}.md"))
}

fn break_document(project: &Project, id: &str) {
    std::fs::write(task_path(project, id), "---\nid: not even close\n").unwrap();
}

#[test]
fn a_scoped_projection_names_the_documents_and_paths_that_blocked_it() {
    let (_dir, project) = project();
    make(&project, "readable");
    let broken = make(&project, "broken");
    break_document(&project, &broken);

    let projection = Projection::load(&project).unwrap();
    let error = projection
        .require_complete()
        .expect_err("a broken document blocks a global read");

    assert_eq!(error.code(), Code::IncompleteProjection);
    let message = error.message();
    assert!(
        message.starts_with(REQUIREMENT),
        "message does not open with the requirement: {message}"
    );
    assert!(
        message.contains("incomplete"),
        "a scoped projection must be called incomplete: {message}"
    );
    assert!(
        message.contains("1 document(s) could not be read"),
        "the message must name the one unreadable document: {message}"
    );
    assert!(
        message.contains("0 item-shaped path(s) are uncertain"),
        "the message must name the uncertain-path count: {message}"
    );
    assert!(
        !message.contains("freshness"),
        "a fresh projection has no freshness shortfall to report: {message}"
    );
    // The machine contract is untouched: the literals now live in the details,
    // not in the prose.
    assert_eq!(error.details()["completeness"], "scoped");
    assert_eq!(error.details()["freshness"], "fresh");
    assert!(error.details()["diagnostics"].is_array());
}

#[test]
fn a_stale_but_complete_projection_names_staleness_not_incompleteness() {
    let error = WaymarkError::incomplete_projection("complete", "stale", Vec::new());
    let message = error.message();

    assert!(message.starts_with(REQUIREMENT), "message: {message}");
    assert!(
        message.contains("stale"),
        "a stale projection must be called stale: {message}"
    );
    assert!(
        !message.contains("incomplete"),
        "a complete projection must not be called incomplete: {message}"
    );
    assert!(
        !message.contains("document(s)") && !message.contains("item-shaped"),
        "without a shortfall the message must not invent counts: {message}"
    );
}

#[test]
fn a_scoped_and_stale_projection_names_both_shortfalls_once() {
    let error = WaymarkError::incomplete_projection_shortfall("scoped", "stale", 2, 1, Vec::new());
    let message = error.message();

    assert_eq!(
        message.matches(REQUIREMENT.trim_end()).count(),
        1,
        "the requirement is stated exactly once: {message}"
    );
    assert!(
        message.starts_with(REQUIREMENT),
        "message does not open with the requirement: {message}"
    );
    assert!(
        message.contains("2 document(s) could not be read"),
        "the message must name the unreadable documents: {message}"
    );
    assert!(
        message.contains("1 item-shaped path(s) are uncertain"),
        "the message must name the uncertain paths: {message}"
    );
    assert!(
        message.contains("incomplete") && message.contains("stale"),
        "both shortfalls must be named: {message}"
    );
}

#[test]
fn the_three_argument_constructor_still_states_the_requirement_without_counts() {
    let error = WaymarkError::incomplete_projection("scoped", "fresh", Vec::new());
    let message = error.message();

    assert!(message.starts_with(REQUIREMENT), "message: {message}");
    assert!(
        !message.contains("0 document(s)"),
        "an uncounted shortfall must not fabricate a zero: {message}"
    );
    assert!(
        !message.contains("0 item-shaped"),
        "an uncounted shortfall must not fabricate a zero: {message}"
    );
    assert_eq!(error.details().len(), 3);
    assert_eq!(error.details()["completeness"], "scoped");
    assert_eq!(error.details()["freshness"], "fresh");
    assert_eq!(error.details()["diagnostics"], serde_json::json!([]));
}

#[test]
fn a_complete_and_fresh_projection_is_never_called_incomplete() {
    // The public constructor accepts arbitrary literals, so a caller can hand
    // it a projection that satisfies the requirement. That is an inconsistent
    // request, but the message must still not describe a complete, fresh
    // projection as incomplete or stale.
    let error = WaymarkError::incomplete_projection("complete", "fresh", Vec::new());
    let message = error.message();

    assert!(message.starts_with(REQUIREMENT), "message: {message}");
    assert!(
        !message.contains("incomplete") && !message.contains("stale"),
        "a complete, fresh projection must be described neutrally: {message}"
    );
    assert_eq!(error.details()["completeness"], "complete");
    assert_eq!(error.details()["freshness"], "fresh");
}

#[test]
fn a_validation_failure_names_both_the_document_and_the_field() {
    let (_dir, project) = project();
    let id = make(&project, "broken field");
    // A valid document path and ID, but one field the schema rejects: this is
    // exactly the failure the Web read has to lead the reader back to.
    std::fs::write(
        task_path(&project, &id),
        format!(
            "---\nschema: 1\nid: {id}\ntitle: Broken\ncategory: task\nstatus: started\nowner: frank\ncreated_at: 2026-11-03T05:43:10Z\nupdated_at: 2026-11-03T05:43:10Z\n---\n"
        ),
    )
    .unwrap();

    let projection = Projection::load(&project).unwrap();
    let finding = projection
        .diagnostics()
        .into_iter()
        .find(|finding| finding.code == Code::ValidationError)
        .expect("the invalid field must produce a validation finding");
    let location = finding
        .location
        .expect("a validation finding must be locatable");
    let wire = serde_json::to_value(&location).unwrap();

    assert_eq!(wire["kind"], "field");
    assert_eq!(wire["field"], "status");
    assert_eq!(
        wire["path"],
        task_path(&project, &id).as_str(),
        "the field location must name its document: {wire}"
    );
}

#[test]
fn the_diagnostics_summary_and_the_error_use_one_sentence() {
    let (_dir, project) = project();
    let broken = make(&project, "broken");
    break_document(&project, &broken);

    let projection = Projection::load(&project).unwrap();
    let summary = projection
        .diagnostics()
        .into_iter()
        .find(|finding| finding.code == Code::IncompleteProjection && finding.location.is_none())
        .expect("a scoped projection carries one count-bearing summary");
    let error = projection
        .require_complete()
        .expect_err("a broken document blocks a global read");

    assert_eq!(
        summary.message,
        error.message(),
        "the summary finding and the error must be one sentence"
    );
    assert!(
        summary.message.contains("1 document(s) could not be read"),
        "the shared sentence must name the shortfall: {}",
        summary.message
    );
}
