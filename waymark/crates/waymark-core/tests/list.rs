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

#[test]
fn an_active_child_of_a_closed_parent_is_labelled_in_list_and_ready() {
    let (_dir, project) = fixtures::deterministic_project();
    let parent = make(&project, "parent", "task");
    let child = make(&project, "child", "task");
    let grandchild = make(&project, "grandchild", "task");
    waymark_core::ops::parent::set_parent(&project, &child, &parent, &options()).unwrap();
    waymark_core::ops::parent::set_parent(&project, &grandchild, &child, &options()).unwrap();
    let paths: Vec<camino::Utf8PathBuf> = [&parent, &child, &grandchild]
        .iter()
        .map(|item| read::read(&project, item).unwrap().path)
        .collect();
    let bytes = |paths: &[camino::Utf8PathBuf]| -> Vec<String> {
        paths
            .iter()
            .map(|path| std::fs::read_to_string(path).unwrap())
            .collect()
    };

    let row_of = |listing: &list::Listing, id: &str| {
        listing
            .items
            .iter()
            .find(|row| row.item.id.as_str() == id)
            .unwrap_or_else(|| panic!("{id} is listed"))
            .clone()
    };

    // The parent is open: nobody carries a closed-parent signal.
    let listing = list::list(&project, &Query::new()).unwrap();
    assert_eq!(row_of(&listing, &child).closed_parent, None);
    assert_eq!(row_of(&listing, &grandchild).closed_parent, None);
    let before_close = bytes(&paths);

    status::set_status(&project, &parent, Status::Closed, &options()).unwrap();
    // Closing the parent writes its own document and nothing else.
    let after_close = bytes(&paths);
    assert_ne!(before_close[0], after_close[0], "the parent itself moved");
    assert_eq!(
        before_close[1..],
        after_close[1..],
        "no child document moved"
    );

    let listing = list::list(&project, &Query::new()).unwrap();
    let child_row = row_of(&listing, &child);
    assert_eq!(
        child_row.closed_parent.as_ref().map(|id| id.as_str()),
        Some(parent.as_str())
    );
    // Only the direct parent is inspected: the grandchild's parent is open.
    assert_eq!(row_of(&listing, &grandchild).closed_parent, None);
    // Closing a parent changes no readiness and no lifecycle state.
    assert!(child_row.readiness.ready);
    assert!(!child_row.readiness.blocked);
    assert_eq!(child_row.item.status, Status::Open);

    let ready = list::ready(&project, &Query::new()).unwrap();
    assert_eq!(
        row_of(&ready, &child)
            .closed_parent
            .as_ref()
            .map(|id| id.as_str()),
        Some(parent.as_str())
    );

    // The same rows render the signal through the shared view layer, under the
    // query column's own name.
    let projection = waymark_core::query::Projection::all();
    let row = waymark_core::view::row_json(&child_row, &projection);
    assert_eq!(row["closed_parent"], serde_json::json!(parent));
    assert_eq!(row["ready"], serde_json::json!(true));
    assert!(row.get("item").is_some(), "a row is still a row");
    // Reading it back through that vocabulary selects it and nothing else.
    let selected = watcher_field(&project);
    assert_eq!(selected["closed_parent"], serde_json::json!(parent));
    assert!(selected.get("ready").is_none(), "{selected}");

    // Four reads wrote nothing.
    assert_eq!(after_close, bytes(&paths));
}

/// One row projected to the single `closed_parent` column, fetched by that name.
///
/// The filter selects the child by the derived column's own query name; the
/// projection then renders only that column and the ID, which is what a caller
/// choosing a field projection asks for.
fn watcher_field(project: &Project) -> serde_json::Value {
    let filters = waymark_core::query::Filters::default();
    let listing = list::list(project, &Query::new().with_filters(filters)).unwrap();
    let row = listing
        .items
        .iter()
        .find(|row| row.item.title == "child")
        .expect("the child is listed");
    waymark_core::view::row_json(
        row,
        &waymark_core::query::Projection::of([
            waymark_core::query::FieldName::Id,
            waymark_core::query::FieldName::ClosedParent,
        ]),
    )
}

#[test]
fn a_closed_child_of_a_closed_parent_and_an_archived_parent_carry_no_signal() {
    let (_dir, project) = fixtures::deterministic_project();
    let closed_parent = make(&project, "closed parent", "task");
    let closed_child = make(&project, "closed child", "task");
    let orphan = make(&project, "orphan", "task");
    waymark_core::ops::parent::set_parent(&project, &closed_child, &closed_parent, &options())
        .unwrap();
    let archived_parent = make(&project, "archived parent", "task");
    waymark_core::ops::parent::set_parent(&project, &orphan, &archived_parent, &options()).unwrap();
    status::set_status(&project, &closed_parent, Status::Closed, &options()).unwrap();
    status::set_status(&project, &closed_child, Status::Closed, &options()).unwrap();
    let path = read::read(&project, &archived_parent).unwrap().path;
    let mut archived = waymark_core::parse::parse_item_file(&path).unwrap();
    archived.archived_at = Some(fixtures::instant());
    std::fs::write(&path, waymark_core::serialize::to_document(&archived)).unwrap();

    let listing = list::list(&project, &Query::new()).unwrap();
    let row_of = |id: &str| {
        listing
            .items
            .iter()
            .find(|row| row.item.id.as_str() == id)
            .unwrap()
            .clone()
    };
    // A closed child produces no signal, whether or not its parent is closed.
    assert_eq!(row_of(&closed_child).closed_parent, None);
    // "Closed" is the lifecycle state; an archived parent is not it.
    assert_eq!(row_of(&orphan).closed_parent, None);
}
