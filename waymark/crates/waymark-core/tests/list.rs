//! `list`, `ready`, and `blocked`: one population, three questions.
//!
//! The seam is the three core reads. They share [`Query`], and every one of
//! them answers from a complete project scan, so `_meta` says `complete`.

use waymark_core::envelope::{Completeness, Source};
use waymark_core::error::Code;
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::list::{self, Query};
use waymark_core::ops::relationship::add_dependency;
use waymark_core::ops::{create, read, status, OpOptions};
use waymark_core::testing::fixtures;
use waymark_core::{CategoryKey, Project, Status, WorkItemId};

fn options() -> OpOptions {
    OpOptions::new().with_clock(fixtures::shared_clock())
}

fn make(project: &Project, title: &str, category: &str) -> String {
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

fn titles(listing: &list::Listing) -> Vec<String> {
    listing
        .items
        .iter()
        .map(|row| row.item.title.clone())
        .collect()
}

fn id(literal: &str) -> WorkItemId {
    WorkItemId::parse(literal).unwrap()
}

#[test]
fn list_defaults_to_rank_order_with_stable_id_as_the_final_tie_break() {
    let (_dir, project) = fixtures::deterministic_project();
    let first = make(&project, "first", "task");
    let second = make(&project, "second", "task");

    let listing = list::list(&project, &Query::new()).unwrap();

    // Both are open with no dependencies, so the topological level and the
    // status group tie and the stable ID decides.
    assert_eq!(titles(&listing), vec!["first", "second"]);
    assert_eq!(listing.items[0].item.id.as_str(), first);
    assert_eq!(listing.items[1].item.id.as_str(), second);
    assert!(listing.items[0].item.id < listing.items[1].item.id);
}

#[test]
fn list_never_includes_archived_items() {
    let (_dir, project) = fixtures::deterministic_project();
    let kept = make(&project, "kept", "task");
    let archived = make(&project, "archived", "task");
    let path = read::read(&project, &archived).unwrap().path;
    let mut item = waymark_core::parse::parse_item_file(&path).unwrap();
    item.archived_at = Some(fixtures::instant());
    std::fs::write(&path, waymark_core::serialize::to_document(&item)).unwrap();

    let listing = list::list(&project, &Query::new()).unwrap();
    assert_eq!(titles(&listing), vec!["kept"]);
    assert_eq!(listing.items[0].item.id.as_str(), kept);
}

#[test]
fn list_includes_closed_items_but_ready_and_blocked_never_do() {
    let (_dir, project) = fixtures::deterministic_project();
    let closed = make(&project, "closed", "task");
    let open = make(&project, "open", "task");
    status::set_status(&project, &closed, Status::Closed, &options()).unwrap();

    // `rank` puts the open item before the closed one, whatever the IDs are.
    let all = list::list(&project, &Query::new()).unwrap();
    assert_eq!(titles(&all), vec!["open", "closed"]);

    let ready = list::ready(&project, &Query::new()).unwrap();
    assert_eq!(titles(&ready), vec!["open"]);
    assert_eq!(ready.items[0].item.id.as_str(), open);
    assert_eq!(all.items[1].item.id.as_str(), closed);

    let blocked = list::blocked(&project, &Query::new()).unwrap();
    assert!(blocked.is_empty(), "{:?}", titles(&blocked));
    assert_eq!(blocked.len(), 0);
}

#[test]
fn ready_lists_only_items_with_every_dependency_closed() {
    let (_dir, project) = fixtures::deterministic_project();
    let blocker = make(&project, "blocker", "task");
    let dependent = make(&project, "dependent", "task");
    let free = make(&project, "free", "task");
    add_dependency(&project, &dependent, &blocker, &options()).unwrap();

    let ready = list::ready(&project, &Query::new()).unwrap();
    assert_eq!(titles(&ready), vec!["blocker", "free"]);

    status::set_status(&project, &blocker, Status::Closed, &options()).unwrap();
    // `rank` is status group, then dependency level, then stable ID. The
    // dependency level is computed over the whole active graph, so `dependent`
    // stays one level below its — now closed — blocker and sorts after `free`.
    let ready = list::ready(&project, &Query::new()).unwrap();
    assert_eq!(titles(&ready), vec!["free", "dependent"]);
    assert!(!titles(&ready).contains(&"blocker".to_owned()));
    assert_eq!(free, ready.items[0].item.id.as_str());
}

#[test]
fn blocked_lists_only_items_with_an_unfinished_dependency() {
    let (_dir, project) = fixtures::deterministic_project();
    let blocker = make(&project, "blocker", "task");
    let dependent = make(&project, "dependent", "task");
    make(&project, "free", "task");
    add_dependency(&project, &dependent, &blocker, &options()).unwrap();

    let blocked = list::blocked(&project, &Query::new()).unwrap();
    assert_eq!(titles(&blocked), vec!["dependent"]);
    assert_eq!(blocked.items[0].readiness.blocked_by, vec![id(&blocker)]);
    assert!(blocked.items[0].readiness.blocked);
    assert!(!blocked.items[0].readiness.ready);
}

#[test]
fn a_dangling_dependency_makes_an_item_blocked_and_is_reported() {
    let (_dir, project) = fixtures::deterministic_project();
    let item = make(&project, "item", "task");
    let path = read::read(&project, &item).unwrap().path;
    let mut document = waymark_core::parse::parse_item_file(&path).unwrap();
    document.dependencies = vec![id("wm-00000000zzzz")];
    std::fs::write(&path, waymark_core::serialize::to_document(&document)).unwrap();

    let blocked = list::blocked(&project, &Query::new()).unwrap();
    assert_eq!(titles(&blocked), vec!["item"]);
    assert_eq!(
        blocked.items[0].readiness.blocked_by,
        vec![id("wm-00000000zzzz")]
    );

    let ready = list::ready(&project, &Query::new()).unwrap();
    assert!(ready.is_empty());

    // Both reads report the dangling reference, because both scanned the graph.
    let diagnostics = &blocked.meta.diagnostics;
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, Code::DanglingReference);
    assert!(diagnostics[0].message.contains("wm-00000000zzzz"));
    assert_eq!(ready.meta.diagnostics, blocked.meta.diagnostics);
}

#[test]
fn the_status_filter_selects_one_lifecycle_state() {
    let (_dir, project) = fixtures::deterministic_project();
    let open = make(&project, "open", "task");
    let progress = make(&project, "progress", "task");
    let closed = make(&project, "closed", "task");
    status::set_status(&project, &progress, Status::InProgress, &options()).unwrap();
    status::set_status(&project, &closed, Status::Closed, &options()).unwrap();

    assert_eq!(
        titles(&list::list(&project, &Query::new().with_status(Status::Open)).unwrap()),
        vec!["open"]
    );
    assert_eq!(
        titles(&list::list(&project, &Query::new().with_status(Status::InProgress)).unwrap()),
        vec!["progress"]
    );
    assert_eq!(
        titles(&list::list(&project, &Query::new().with_status(Status::Closed)).unwrap()),
        vec!["closed"]
    );
    assert_eq!(
        list::list(&project, &Query::new().with_status(Status::Open))
            .unwrap()
            .items[0]
            .item
            .id
            .as_str(),
        open
    );
}

/// A project with two declared categories, so a filter has something to choose
/// between.
fn two_category_project() -> (tempfile::TempDir, Project) {
    use tempfile::TempDir;
    use waymark_core::config::{Category, Config};
    use waymark_core::store::InitOptions;

    let dir = TempDir::new().unwrap();
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
        InitOptions {
            config: Some(config),
            ..InitOptions::default()
        },
    )
    .unwrap()
    .with_clock(fixtures::shared_clock())
    .with_id_source(Box::new(waymark_core::testing::SequentialIds::new()));
    (dir, project)
}

#[test]
fn the_category_filter_selects_one_category_and_composes_with_status() {
    let (_dir, project) = two_category_project();
    let task = make(&project, "task", "task");
    let bug = make(&project, "bug", "bug");
    status::set_status(&project, &bug, Status::Closed, &options()).unwrap();

    let only_tasks = list::list(
        &project,
        &Query::new().with_category(CategoryKey::parse("task").unwrap()),
    )
    .unwrap();
    assert_eq!(titles(&only_tasks), vec!["task"]);
    assert_eq!(only_tasks.items[0].item.id.as_str(), task);

    let only_bugs = list::list(
        &project,
        &Query::new().with_category(CategoryKey::parse("bug").unwrap()),
    )
    .unwrap();
    assert_eq!(titles(&only_bugs), vec!["bug"]);

    let closed_tasks = list::list(
        &project,
        &Query::new()
            .with_category(CategoryKey::parse("task").unwrap())
            .with_status(Status::Closed),
    )
    .unwrap();
    assert!(
        closed_tasks.is_empty(),
        "the filters compose by conjunction"
    );
    assert_ne!(task, bug);
}

#[test]
fn an_empty_project_lists_nothing_but_still_reports_a_complete_scan() {
    let (_dir, project) = fixtures::deterministic_project();

    for listing in [
        list::list(&project, &Query::new()).unwrap(),
        list::ready(&project, &Query::new()).unwrap(),
        list::blocked(&project, &Query::new()).unwrap(),
    ] {
        assert!(listing.is_empty());
        assert_eq!(listing.meta.source, Source::MemoryProjection);
        assert_eq!(listing.meta.completeness, Completeness::Complete);
        assert!(listing.meta.diagnostics.is_empty());
    }
}

#[test]
fn every_read_projects_completeness_and_source_into_meta() {
    let (_dir, project) = fixtures::deterministic_project();
    make(&project, "item", "task");

    for listing in [
        list::list(&project, &Query::new()).unwrap(),
        list::ready(&project, &Query::new()).unwrap(),
        list::blocked(&project, &Query::new()).unwrap(),
    ] {
        assert_eq!(listing.meta.source, Source::MemoryProjection);
        assert_eq!(listing.meta.completeness, Completeness::Complete);
        assert_eq!(listing.meta.completeness.as_str(), "complete");
    }
}

#[test]
fn a_listed_row_carries_children_dependents_and_readiness() {
    let (_dir, project) = fixtures::deterministic_project();
    let parent = make(&project, "parent", "task");
    let child = make(&project, "child", "task");
    let dependent = make(&project, "dependent", "task");

    // Give `child` a parent and make `dependent` wait for `parent`.
    let child_path = read::read(&project, &child).unwrap().path;
    let mut child_item = waymark_core::parse::parse_item_file(&child_path).unwrap();
    child_item.parent = Some(id(&parent));
    std::fs::write(
        &child_path,
        waymark_core::serialize::to_document(&child_item),
    )
    .unwrap();
    add_dependency(&project, &dependent, &parent, &options()).unwrap();

    let listing = list::list(&project, &Query::new()).unwrap();
    let row = listing
        .items
        .iter()
        .find(|row| row.item.id.as_str() == parent)
        .expect("the parent is listed");

    assert_eq!(row.children, vec![id(&child)]);
    assert_eq!(row.dependents, vec![id(&dependent)]);
    assert!(row.readiness.ready);
    assert!(!row.readiness.blocked);
}

#[test]
fn the_derived_columns_are_never_written_back() {
    let (_dir, project) = fixtures::deterministic_project();
    let blocker = make(&project, "blocker", "task");
    let dependent = make(&project, "dependent", "task");
    add_dependency(&project, &dependent, &blocker, &options()).unwrap();

    let before: Vec<String> = [&blocker, &dependent]
        .iter()
        .map(|item| std::fs::read_to_string(read::read(&project, item).unwrap().path).unwrap())
        .collect();
    let _ = list::ready(&project, &Query::new()).unwrap();
    let _ = list::blocked(&project, &Query::new()).unwrap();
    let _ = list::list(&project, &Query::new()).unwrap();
    let after: Vec<String> = [&blocker, &dependent]
        .iter()
        .map(|item| std::fs::read_to_string(read::read(&project, item).unwrap().path).unwrap())
        .collect();

    assert_eq!(before, after);
}
