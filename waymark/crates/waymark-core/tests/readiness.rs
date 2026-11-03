//! Readiness: the derived `ready` / `blocked` / `blocked_by` view, and the
//! relationship diagnostics that come with it.
//!
//! The graph is built in memory from documents. Nothing in this file writes, and
//! the last test proves that readiness data never reaches the disk.

use waymark_core::error::Code;
use waymark_core::model::{Status, WorkItem};
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::{create, status, OpOptions};
use waymark_core::readiness::{Graph, Readiness};
use waymark_core::testing::fixtures;
use waymark_core::WorkItemId;

fn id(literal: &str) -> WorkItemId {
    WorkItemId::parse(literal).expect("a valid stable ID")
}

/// A minimal item in `status`, depending on `deps`.
fn item(literal: &str, status: Status, deps: &[&str]) -> WorkItem {
    let mut item = fixtures::minimal_item(literal);
    item.status = status;
    item.dependencies = deps.iter().map(|value| id(value)).collect();
    item
}

fn readiness(graph: &Graph, literal: &str) -> Readiness {
    graph
        .readiness(&id(literal))
        .expect("the graph knows this item")
}

#[test]
fn an_item_with_no_dependencies_is_ready() {
    let graph = Graph::from_items(&[item("wm-000000000001", Status::Open, &[])]);

    assert_eq!(
        readiness(&graph, "wm-000000000001"),
        Readiness {
            ready: true,
            blocked: false,
            blocked_by: Vec::new(),
        }
    );
}

#[test]
fn an_unfinished_dependency_blocks_with_the_target_listed() {
    let graph = Graph::from_items(&[
        item("wm-000000000001", Status::Open, &["wm-000000000002"]),
        item("wm-000000000002", Status::InProgress, &[]),
    ]);

    let readiness = readiness(&graph, "wm-000000000001");
    assert!(!readiness.ready);
    assert!(readiness.blocked);
    assert_eq!(readiness.blocked_by, vec![id("wm-000000000002")]);
    assert!(readiness_refs(&graph, "wm-000000000002").is_empty());
}

/// A helper that reads whether an item is blocked, so a test can say what it
/// means without repeating three assertions.
fn readiness_refs(graph: &Graph, literal: &str) -> Vec<WorkItemId> {
    readiness(graph, literal).blocked_by
}

#[test]
fn a_closed_dependency_blocks_nothing() {
    let graph = Graph::from_items(&[
        item("wm-000000000001", Status::Open, &["wm-000000000002"]),
        item("wm-000000000002", Status::Closed, &[]),
    ]);

    let readiness = readiness(&graph, "wm-000000000001");
    assert!(readiness.ready);
    assert!(!readiness.blocked);
    assert!(readiness.blocked_by.is_empty());
}

#[test]
fn a_closed_item_is_never_blocked_even_with_an_unfinished_dependency() {
    let graph = Graph::from_items(&[
        item("wm-000000000001", Status::Closed, &["wm-000000000002"]),
        item("wm-000000000002", Status::Open, &[]),
    ]);

    let readiness = readiness(&graph, "wm-000000000001");
    assert!(!readiness.ready, "a closed item is not ready work");
    assert!(!readiness.blocked, "a closed item is never blocked");
    assert!(readiness.blocked_by.is_empty());
}

#[test]
fn a_missing_dependency_blocks_and_is_listed() {
    let graph = Graph::from_items(&[item("wm-000000000001", Status::Open, &["wm-0000000000zz"])]);

    let readiness = readiness(&graph, "wm-000000000001");
    assert!(readiness.blocked);
    assert_eq!(readiness.blocked_by, vec![id("wm-0000000000zz")]);
}

#[test]
fn blocked_by_is_sorted_and_lists_only_unfinished_or_missing_targets() {
    let graph = Graph::from_items(&[
        item(
            "wm-000000000001",
            Status::Open,
            &[
                "wm-000000000004",
                "wm-000000000003",
                "wm-000000000002",
                "wm-0000000000zz",
            ],
        ),
        item("wm-000000000002", Status::Closed, &[]),
        item("wm-000000000003", Status::Open, &[]),
        item("wm-000000000004", Status::InProgress, &[]),
    ]);

    assert_eq!(
        readiness(&graph, "wm-000000000001").blocked_by,
        vec![
            id("wm-000000000003"),
            id("wm-000000000004"),
            id("wm-0000000000zz")
        ]
    );
}

#[test]
fn children_are_the_items_whose_parent_is_this_one() {
    let mut child_a = fixtures::minimal_item("wm-000000000002");
    child_a.parent = Some(id("wm-000000000001"));
    let mut child_b = fixtures::minimal_item("wm-000000000003");
    child_b.parent = Some(id("wm-000000000001"));
    let unrelated = fixtures::minimal_item("wm-000000000004");
    let graph = Graph::from_items(&[
        fixtures::minimal_item("wm-000000000001"),
        child_a,
        child_b,
        unrelated,
    ]);

    assert_eq!(
        graph.children(&id("wm-000000000001")),
        vec![id("wm-000000000002"), id("wm-000000000003")]
    );
    assert!(graph.children(&id("wm-000000000004")).is_empty());
}

#[test]
fn dependents_are_the_items_that_depend_on_this_one() {
    let graph = Graph::from_items(&[
        item("wm-000000000001", Status::Open, &["wm-000000000003"]),
        item(
            "wm-000000000002",
            Status::Open,
            &["wm-000000000003", "wm-000000000001"],
        ),
        item("wm-000000000003", Status::Open, &[]),
    ]);

    assert_eq!(
        graph.dependents(&id("wm-000000000003")),
        vec![id("wm-000000000001"), id("wm-000000000002")]
    );
    assert_eq!(
        graph.dependents(&id("wm-000000000001")),
        vec![id("wm-000000000002")]
    );
}

#[test]
fn an_archived_item_does_not_participate_in_the_active_graph() {
    let mut archived = fixtures::minimal_item("wm-000000000002");
    archived.archived_at = Some(fixtures::instant());
    let graph = Graph::from_items(&[
        item("wm-000000000001", Status::Open, &["wm-000000000002"]),
        archived,
    ]);

    // The archived target is not part of the active graph, so it cannot unblock
    // anything; it is reported as dangling rather than silently ignored.
    assert!(readiness(&graph, "wm-000000000001").blocked);
    assert_eq!(
        readiness(&graph, "wm-000000000001").blocked_by,
        vec![id("wm-000000000002")]
    );
    assert!(graph.get(&id("wm-000000000002")).is_none());
}

#[test]
fn a_missing_target_is_reported_as_a_dangling_reference() {
    let graph = Graph::from_items(&[item("wm-000000000001", Status::Open, &["wm-0000000000zz"])]);

    let diagnostics = graph.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, Code::DanglingReference);
    assert_eq!(diagnostics[0].severity, waymark_core::Severity::Error);
    assert!(diagnostics[0].message.contains("wm-0000000000zz"));
}

#[test]
fn a_dangling_soft_link_or_parent_is_reported_too() {
    let mut item = fixtures::minimal_item("wm-000000000001");
    item.parent = Some(id("wm-0000000000yy"));
    item.soft_links = vec![id("wm-0000000000zz")];
    let graph = Graph::from_items(&[item]);

    let relations: Vec<String> = graph
        .diagnostics()
        .iter()
        .map(|finding| finding.message.clone())
        .collect();
    assert_eq!(relations.len(), 2, "{relations:?}");
    assert!(
        relations.iter().any(|message| message.contains("parent")),
        "{relations:?}"
    );
    assert!(
        relations
            .iter()
            .any(|message| message.contains("soft_links")),
        "{relations:?}"
    );
}

#[test]
fn a_persisted_dependency_cycle_is_diagnosed_with_its_path() {
    let graph = Graph::from_items(&[
        item("wm-000000000001", Status::Open, &["wm-000000000002"]),
        item("wm-000000000002", Status::Open, &["wm-000000000003"]),
        item("wm-000000000003", Status::Open, &["wm-000000000001"]),
    ]);

    let cycles: Vec<_> = graph
        .diagnostics()
        .into_iter()
        .filter(|finding| finding.code == Code::CycleDetected)
        .collect();
    assert_eq!(cycles.len(), 1, "{cycles:?}");
    assert_eq!(cycles[0].severity, waymark_core::Severity::Error);
    for expected in ["wm-000000000001", "wm-000000000002", "wm-000000000003"] {
        assert!(
            cycles[0].message.contains(expected),
            "{}",
            cycles[0].message
        );
    }
}

#[test]
fn the_graph_is_built_from_the_project_inventory() {
    let (_dir, project) = fixtures::deterministic_project();
    let first = create::create(
        &project,
        CreateRequest::new("one", "task", "frank"),
        &OpOptions::new().with_clock(fixtures::shared_clock()),
    )
    .unwrap();
    let second = create::create(
        &project,
        CreateRequest::new("two", "task", "frank"),
        &OpOptions::new().with_clock(fixtures::shared_clock()),
    )
    .unwrap();

    let graph = Graph::from_project(&project).expect("the project scans");
    assert_eq!(graph.len(), 2);
    assert!(graph.get(&first.item.id).is_some());
    assert!(graph.get(&second.item.id).is_some());
}

#[test]
fn readiness_is_never_persisted() {
    let (_dir, project) = fixtures::deterministic_project();
    let first = create::create(
        &project,
        CreateRequest::new("one", "task", "frank"),
        &options(),
    )
    .unwrap();
    let second = create::create(
        &project,
        CreateRequest::new("two", "task", "frank"),
        &options(),
    )
    .unwrap();
    // `second` depends on `first`, so while `first` is open `second` is blocked.
    waymark_core::ops::relationship::add_dependency(
        &project,
        second.item.id.as_str(),
        first.item.id.as_str(),
        &options(),
    )
    .unwrap();

    let graph = Graph::from_project(&project).expect("the project scans");
    assert!(readiness(&graph, second.item.id.as_str()).blocked);

    status::set_status(&project, first.item.id.as_str(), Status::Closed, &options()).unwrap();
    let graph = Graph::from_project(&project).expect("the project scans");
    assert!(readiness(&graph, second.item.id.as_str()).ready);

    let path = waymark_core::ops::read::read(&project, first.item.id.as_str())
        .unwrap()
        .path;
    let bytes = std::fs::read_to_string(path).unwrap();
    for absent in ["ready", "blocked", "blocked_by", "children", "dependents"] {
        assert!(!bytes.contains(absent), "{absent} was persisted:\n{bytes}");
    }

    let dependency_path = waymark_core::ops::read::read(&project, second.item.id.as_str())
        .unwrap()
        .path;
    let dependency_bytes = std::fs::read_to_string(dependency_path).unwrap();
    assert!(
        dependency_bytes.contains("dependencies:"),
        "the edge itself is persisted:\n{dependency_bytes}"
    );
    assert!(!dependency_bytes.contains("blocked_by"));
}

fn options() -> OpOptions {
    OpOptions::new().with_clock(fixtures::shared_clock())
}
