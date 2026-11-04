//! Bounded tree reads.
//!
//! The seams exercised here are `hierarchy_tree` and `dependency_tree` plus the
//! `_meta` they justify. The rules the spec pins: depth `None` is unlimited,
//! `Some(0)` is the root alone, `Some(1)` is one level; the dependency tree has a
//! flat form; and a reference to a target that no longer exists is reported as
//! written — persisted and diagnosed — never silently dropped from the tree.

use waymark_core::envelope::Completeness;
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::link::add_soft_link;
use waymark_core::ops::parent::set_parent;
use waymark_core::ops::relationship::add_dependency;
use waymark_core::ops::tree::{dependency_tree, hierarchy_tree, UnresolvedReason};
use waymark_core::ops::{create, read, OpOptions};
use waymark_core::testing::fixtures;
use waymark_core::Project;

fn options() -> OpOptions {
    OpOptions::new().with_clock(fixtures::shared_clock())
}

fn project() -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let project = Project::init(&root, waymark_core::store::InitOptions::default())
        .unwrap()
        .with_clock(waymark_core::testing::shared_clock())
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

fn child(project: &Project, title: &str, parent: &str) -> String {
    let id = make(project, title);
    set_parent(project, &id, parent, &options()).unwrap();
    id
}

/// A three-level chain: `root` -> `middle` -> `leaf`.
fn chain(project: &Project) -> (String, String, String) {
    let root = make(project, "root");
    let middle = child(project, "middle", &root);
    let leaf = child(project, "leaf", &middle);
    (root, middle, leaf)
}

fn ids(nodes: &[waymark_core::ops::tree::TreeNode]) -> Vec<String> {
    nodes
        .iter()
        .map(|node| node.id.as_str().to_owned())
        .collect()
}

#[test]
fn the_whole_hierarchy_lists_roots_in_stable_id_order() {
    let (_dir, project) = project();
    let (root, middle, leaf) = chain(&project);
    let other = make(&project, "other root");

    let tree = hierarchy_tree(&project, None, None).unwrap();

    let mut expected = vec![root.clone(), other.clone()];
    expected.sort();
    assert_eq!(ids(&tree.roots), expected);
    assert!(tree.unresolved.is_empty());

    let first = tree
        .roots
        .iter()
        .find(|node| node.id.as_str() == root)
        .unwrap();
    assert_eq!(ids(&first.children), vec![middle.clone()]);
    assert_eq!(ids(&first.children[0].children), vec![leaf.clone()]);
    assert_eq!(first.children[0].children[0].depth, 2);
    assert_eq!(tree.meta.completeness, Completeness::Complete);
}

#[test]
fn a_scoped_hierarchy_read_starts_at_the_requested_root() {
    let (_dir, project) = project();
    let (root, middle, leaf) = chain(&project);

    let tree = hierarchy_tree(&project, Some(&middle), None).unwrap();

    assert_eq!(tree.roots.len(), 1);
    assert_eq!(tree.roots[0].id.as_str(), middle);
    assert_eq!(tree.roots[0].depth, 0, "the start is depth zero");
    assert_eq!(ids(&tree.roots[0].children), vec![leaf]);
    assert!(
        !tree.roots[0]
            .walk()
            .iter()
            .any(|node| node.id.as_str() == root),
        "a scoped read never walks upward"
    );
}

#[test]
fn hierarchy_depth_zero_is_the_root_alone() {
    let (_dir, project) = project();
    let (root, _middle, _leaf) = chain(&project);

    let tree = hierarchy_tree(&project, Some(&root), Some(0)).unwrap();

    assert_eq!(ids(&tree.roots), vec![root]);
    assert!(tree.roots[0].children.is_empty());
}

#[test]
fn hierarchy_depth_one_is_the_children() {
    let (_dir, project) = project();
    let (root, middle, _leaf) = chain(&project);

    let tree = hierarchy_tree(&project, Some(&root), Some(1)).unwrap();

    assert_eq!(ids(&tree.roots[0].children), vec![middle.clone()]);
    assert!(
        tree.roots[0].children[0].children.is_empty(),
        "depth 1 stops below the child"
    );
}

#[test]
fn a_missing_parent_is_reported_unresolved_not_promoted_to_a_root() {
    let (_dir, project) = project();
    let child_id = make(&project, "orphan");
    let missing = "wm-0000000000zz";
    set_parent_by_hand(&project, &child_id, Some(missing));

    let tree = hierarchy_tree(&project, None, None).unwrap();

    assert!(
        !tree.roots.iter().any(|node| node.id.as_str() == child_id),
        "an orphan is not an ordinary root"
    );
    assert_eq!(tree.unresolved.len(), 1);
    let entry = &tree.unresolved[0];
    assert_eq!(entry.id.as_str(), child_id);
    assert_eq!(entry.parent.as_str(), missing);
    assert_eq!(entry.reason, UnresolvedReason::ParentMissing);
    // The relationship is still on disk exactly as written.
    assert_eq!(
        read::read(&project, &child_id)
            .unwrap()
            .item
            .parent
            .unwrap()
            .as_str(),
        missing
    );
    // And the read diagnoses it rather than failing.
    assert!(
        tree.meta
            .diagnostics
            .iter()
            .any(|finding| finding.code == waymark_core::error::Code::DanglingReference),
        "{:?}",
        tree.meta.diagnostics
    );
}

#[test]
fn a_parent_loop_leaves_its_members_unresolved() {
    let (_dir, project) = project();
    let a = make(&project, "a");
    let b = make(&project, "b");
    set_parent_by_hand(&project, &b, Some(&a));
    set_parent_by_hand(&project, &a, Some(&b));

    let tree = hierarchy_tree(&project, None, None).unwrap();

    assert!(tree.roots.is_empty(), "a loop has no root");
    assert_eq!(tree.unresolved.len(), 2);
    for entry in &tree.unresolved {
        assert_eq!(entry.reason, UnresolvedReason::ParentCycle);
    }
}

#[test]
fn an_archived_parent_is_reported_archived_and_its_child_stays_out() {
    let (_dir, project) = project();
    let parent = make(&project, "parent");
    let kid = child(&project, "kid", &parent);
    archive_by_hand(&project, &parent);

    let tree = hierarchy_tree(&project, None, None).unwrap();

    assert!(
        !tree.roots.iter().any(|node| node.id.as_str() == parent),
        "an archived item is not a root"
    );
    assert_eq!(tree.unresolved.len(), 1);
    assert_eq!(tree.unresolved[0].id.as_str(), kid);
    assert_eq!(tree.unresolved[0].reason, UnresolvedReason::ParentArchived);
}

#[test]
fn an_unresolved_dependency_is_a_node_in_the_dependency_tree() {
    let (_dir, project) = project();
    let item = make(&project, "item");
    let missing = "wm-0000000000zz";
    add_dependency_by_hand(&project, &item, &[missing]);

    let tree = dependency_tree(&project, &item, None, false).unwrap();

    assert_eq!(tree.root.id.as_str(), item);
    assert_eq!(tree.root.children.len(), 1);
    let dangling = &tree.root.children[0];
    assert!(dangling.is_unresolved(), "the missing target keeps a node");
    assert_eq!(dangling.id.as_str(), missing);
    assert!(dangling.children.is_empty());
    assert!(tree.flat.is_empty(), "a flat form was not requested");
}

#[test]
fn the_dependency_tree_is_bounded_by_depth() {
    let (_dir, project) = project();
    let top = make(&project, "top");
    let middle = make(&project, "middle");
    let bottom = make(&project, "bottom");
    add_dependency(&project, &top, &middle, &options()).unwrap();
    add_dependency(&project, &middle, &bottom, &options()).unwrap();

    let none = dependency_tree(&project, &top, None, false).unwrap();
    assert_eq!(none.root.children[0].children[0].id.as_str(), bottom);

    let zero = dependency_tree(&project, &top, Some(0), false).unwrap();
    assert!(zero.root.children.is_empty(), "depth 0 is the root alone");

    let one = dependency_tree(&project, &top, Some(1), false).unwrap();
    assert_eq!(ids(&one.root.children), vec![middle.clone()]);
    assert!(
        one.root.children[0].children.is_empty(),
        "depth 1 stops below the first level"
    );
}

#[test]
fn the_flat_dependency_form_is_preorder_with_depths_and_edges() {
    let (_dir, project) = project();
    let top = make(&project, "top");
    let left = make(&project, "left");
    let right = make(&project, "right");
    let shared = make(&project, "shared");
    add_dependency(&project, &top, &left, &options()).unwrap();
    add_dependency(&project, &top, &right, &options()).unwrap();
    add_dependency(&project, &left, &shared, &options()).unwrap();
    add_dependency(&project, &right, &shared, &options()).unwrap();

    let tree = dependency_tree(&project, &top, None, true).unwrap();

    let rows: Vec<(String, usize, Option<String>)> = tree
        .flat
        .iter()
        .map(|row| {
            (
                row.id.as_str().to_owned(),
                row.depth,
                row.from.as_ref().map(|id| id.as_str().to_owned()),
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            (top.clone(), 0, None),
            (left.clone(), 1, Some(top.clone())),
            (shared.clone(), 2, Some(left.clone())),
            (right.clone(), 1, Some(top.clone())),
            (shared.clone(), 2, Some(right.clone())),
        ]
    );
    let repeated: Vec<bool> = tree.flat.iter().map(|row| row.repeated).collect();
    assert_eq!(
        repeated,
        vec![false, false, false, false, true],
        "a shared prerequisite is a repeated leaf the second time it appears"
    );
}

#[test]
fn a_dependency_cycle_is_rendered_once_and_does_not_loop_forever() {
    let (_dir, project) = project();
    let a = make(&project, "a");
    let b = make(&project, "b");
    // Two edits that each pass validation but together close a loop.
    add_dependency_by_hand(&project, &a, &[b.as_str()]);
    add_dependency_by_hand(&project, &b, &[a.as_str()]);

    let tree = dependency_tree(&project, &a, None, false).unwrap();

    assert_eq!(tree.root.id.as_str(), a);
    assert_eq!(ids(&tree.root.children), vec![b.clone()]);
    assert_eq!(tree.root.children[0].children.len(), 1);
    assert!(
        tree.root.children[0].children[0].repeated,
        "the loop closes with a repeated leaf"
    );
    assert!(
        tree.meta
            .diagnostics
            .iter()
            .any(|finding| finding.code == waymark_core::error::Code::CycleDetected),
        "{:?}",
        tree.meta.diagnostics
    );
}

#[test]
fn a_missing_or_archived_starting_item_is_reported() {
    let (_dir, project) = project();
    let archived = make(&project, "archived");
    archive_by_hand(&project, &archived);

    let error = dependency_tree(&project, "wm-0000000000zz", None, false).unwrap_err();
    assert_eq!(error.code(), waymark_core::error::Code::ItemNotFound);

    let error = hierarchy_tree(&project, Some("wm-0000000000zz"), None).unwrap_err();
    assert_eq!(error.code(), waymark_core::error::Code::ItemNotFound);

    let error = dependency_tree(&project, &archived, None, false).unwrap_err();
    assert_eq!(error.code(), waymark_core::error::Code::ItemArchived);

    let error = hierarchy_tree(&project, Some(&archived), None).unwrap_err();
    assert_eq!(error.code(), waymark_core::error::Code::ItemArchived);
}

#[test]
fn soft_links_do_not_appear_in_the_hierarchy_or_dependency_trees() {
    let (_dir, project) = project();
    let left = make(&project, "left");
    let right = make(&project, "right");
    add_soft_link(&project, &left, &right, &options()).unwrap();

    let hierarchy = hierarchy_tree(&project, None, None).unwrap();
    assert_eq!(hierarchy.roots.len(), 2, "both are roots, neither nests");

    let dependency = dependency_tree(&project, &left, None, false).unwrap();
    assert!(
        dependency.root.children.is_empty(),
        "a soft link is not a dependency"
    );
}

/// Set a parent directly, bypassing the operation's validation, so an external
/// edit can be simulated: the tree must still cope with what is on disk.
fn set_parent_by_hand(project: &Project, id: &str, parent: Option<&str>) {
    let path = read::read(project, id).unwrap().path;
    let mut item = waymark_core::parse::parse_item_file(&path).unwrap();
    item.parent = parent.map(|value| waymark_core::WorkItemId::parse(value).unwrap());
    std::fs::write(&path, waymark_core::serialize::to_document(&item)).unwrap();
}

fn add_dependency_by_hand(project: &Project, id: &str, targets: &[&str]) {
    let path = read::read(project, id).unwrap().path;
    let mut item = waymark_core::parse::parse_item_file(&path).unwrap();
    item.dependencies = targets
        .iter()
        .map(|value| waymark_core::WorkItemId::parse(value).unwrap())
        .collect();
    std::fs::write(&path, waymark_core::serialize::to_document(&item)).unwrap();
}

fn archive_by_hand(project: &Project, id: &str) {
    let path = read::read(project, id).unwrap().path;
    let mut item = waymark_core::parse::parse_item_file(&path).unwrap();
    item.archived_at = Some(fixtures::instant());
    std::fs::write(&path, waymark_core::serialize::to_document(&item)).unwrap();
}
