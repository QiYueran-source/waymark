//! Global reads require a complete, fresh projection.
//!
//! A document that cannot be parsed leaves a hole in the relationship world, so
//! `list`, readiness, `tree`, statistics, `search`, and archive refuse to answer
//! with a partial graph (`INCOMPLETE_PROJECTION`). Repairing the document
//! restores the complete world, because the projection is rebuilt from the files.

use camino::Utf8PathBuf;
use waymark_core::envelope::{Completeness, Freshness};
use waymark_core::error::{Code, WaymarkError};
use waymark_core::ops::archive;
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::tree::hierarchy_tree;
use waymark_core::ops::{create, list, OpOptions};
use waymark_core::query::Query;
use waymark_core::readiness::Graph;
use waymark_core::testing::fixtures;
use waymark_core::Project;

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
fn global_reads_fail_instead_of_returning_a_partial_graph() {
    let (_dir, project) = project();
    let good = make(&project, "readable");
    let broken = make(&project, "broken");
    break_document(&project, &broken);

    let expect_incomplete = |label: &str, result: Result<(), WaymarkError>| {
        let error = result.expect_err(label);
        assert_eq!(error.code(), Code::IncompleteProjection, "{label}");
    };

    expect_incomplete("list", list::list(&project, &Query::new()).map(|_| ()));
    expect_incomplete("readiness", Graph::from_project(&project).map(|_| ()));
    expect_incomplete("tree", hierarchy_tree(&project, None, None).map(|_| ()));
    expect_incomplete(
        "stats",
        waymark_core::stats::stats(&project, &waymark_core::stats::StatsRequest::new()).map(|_| ()),
    );
    expect_incomplete(
        "search",
        waymark_core::search::search(
            &project,
            &waymark_core::search::SearchRequest::new("readable"),
        )
        .map(|_| ()),
    );
    expect_incomplete(
        "archive",
        archive::archive(&project, &good, None, &options()).map(|_| ()),
    );

    // Repairing the file restores the complete world.
    let repaired = fixtures::work_item(&broken, "task", "repaired").build();
    std::fs::write(
        task_path(&project, &broken),
        waymark_core::serialize::to_document(&repaired),
    )
    .unwrap();
    let listing = list::list(&project, &Query::new()).unwrap();
    assert_eq!(listing.meta.completeness, Completeness::Complete);
    assert_eq!(listing.meta.freshness, Freshness::Fresh);
}
