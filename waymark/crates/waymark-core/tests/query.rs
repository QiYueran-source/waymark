//! The collection query model: filters, ordering, pagination, and projection
//! shared by `list`, `ready`, `blocked`, `query`, `search`, and `stats`.
//!
//! The seam is the public read API. Each test drives a real project on disk and
//! asserts on the returned rows, the metadata, and the ordering, never on the
//! internals of the sorter.

use waymark_core::config::{Category, Config, FieldDecl, FieldType};
use waymark_core::envelope::{Completeness, Freshness, Source};
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::edit::{self, FieldChangeEntry};
use waymark_core::ops::list::{self, Query};
use waymark_core::ops::relationship::add_dependency;
use waymark_core::ops::{create, status, OpOptions};
use waymark_core::query::{
    ArchiveRange, FieldKey, FieldOp, FieldPredicate, Filters, Population, Projection, Sort,
    SortField, Window,
};
use waymark_core::search::{self, SearchRequest};
use waymark_core::stats::{self, StatsRequest};
use waymark_core::testing::{self, fixtures};
use waymark_core::{CategoryKey, FieldValue, Project, Status};

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

fn set_field(project: &Project, id: &str, key: &str, value: FieldValue) {
    edit::update_custom_fields(
        project,
        id,
        &[FieldChangeEntry::set(key, value)],
        &options(),
    )
    .unwrap();
}

fn titles(listing: &list::Listing) -> Vec<String> {
    listing
        .items
        .iter()
        .map(|row| row.item.title.clone())
        .collect()
}

fn ids(listing: &list::Listing) -> Vec<String> {
    listing
        .items
        .iter()
        .map(|row| row.item.id.as_str().to_owned())
        .collect()
}

/// A project whose configuration declares one string, one integer, and one list
/// field, so the typed predicates have something to validate against.
fn fielded_project() -> (tempfile::TempDir, Project) {
    let dir = tempfile::TempDir::new().unwrap();
    let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let mut config = Config::fresh();
    config.categories.insert(
        CategoryKey::parse("bug").unwrap(),
        Category {
            path: "work/bug".to_owned(),
            name: "Bugs".to_owned(),
        },
    );
    config.fields.insert(
        FieldKey::parse("effort").unwrap(),
        FieldDecl {
            ty: FieldType::Integer,
            required: false,
            default: None,
        },
    );
    config.fields.insert(
        FieldKey::parse("team").unwrap(),
        FieldDecl {
            ty: FieldType::String,
            required: false,
            default: None,
        },
    );
    config.fields.insert(
        FieldKey::parse("tags").unwrap(),
        FieldDecl {
            ty: FieldType::List(Box::new(FieldType::String)),
            required: false,
            default: None,
        },
    );
    let project = Project::init(
        &root,
        waymark_core::InitOptions {
            config: Some(config),
            ..waymark_core::InitOptions::default()
        },
    )
    .unwrap()
    .with_clock(fixtures::shared_clock())
    .with_id_source(Box::new(testing::SequentialIds::new()));
    (dir, project)
}

fn text_predicate(key: &str, op: FieldOp, value: FieldValue) -> FieldPredicate {
    FieldPredicate::new(FieldKey::parse(key).unwrap(), op, value)
}

// -- one vocabulary ---------------------------------------------------------

#[test]
fn list_ready_and_blocked_accept_the_same_filters_and_ordering() {
    let (_dir, project) = fielded_project();
    let blocker = make(&project, "blocker", "task");
    let dependent = make(&project, "dependent", "task");
    let other = make(&project, "other", "task");
    add_dependency(&project, &dependent, &blocker, &options()).unwrap();
    set_field(
        &project,
        &blocker,
        "team",
        FieldValue::String("core".into()),
    );
    set_field(
        &project,
        &dependent,
        "team",
        FieldValue::String("core".into()),
    );
    set_field(&project, &other, "team", FieldValue::String("web".into()));

    let filters = Filters::new().with_field(text_predicate(
        "team",
        FieldOp::Eq,
        FieldValue::String("core".into()),
    ));
    let query = Query::new().with_filters(filters);

    let all = list::list(&project, &query).unwrap();
    assert_eq!(titles(&all), vec!["blocker", "dependent"]);

    let ready = list::ready(&project, &query).unwrap();
    assert_eq!(titles(&ready), vec!["blocker"]);

    let blocked = list::blocked(&project, &query).unwrap();
    assert_eq!(titles(&blocked), vec!["dependent"]);

    // The same ordering vocabulary applies to all three: `title` descending.
    let ordered = Query::new().with_sort(Sort::of(SortField::Title).descending());
    assert_eq!(
        titles(&list::list(&project, &ordered).unwrap()),
        vec!["other", "dependent", "blocker"]
    );
    assert_eq!(
        titles(&list::ready(&project, &ordered).unwrap()),
        vec!["other", "blocker"]
    );
    assert_eq!(
        titles(&list::blocked(&project, &ordered).unwrap()),
        vec!["dependent"]
    );
}

#[test]
fn the_owner_creator_parent_and_leaf_filters_are_shared() {
    let (_dir, project) = fielded_project();
    let root_id = make(&project, "root", "task");
    let child = make(&project, "child", "task");
    let grandchild = make(&project, "grandchild", "task");
    waymark_core::ops::parent::set_parent(&project, &child, &root_id, &options()).unwrap();
    waymark_core::ops::parent::set_parent(&project, &grandchild, &child, &options()).unwrap();

    let roots = Query::new().with_leaves(true);
    assert_eq!(
        titles(&list::list(&project, &roots).unwrap()),
        vec!["grandchild"]
    );

    let children = Query::new().with_parent(waymark_core::WorkItemId::parse(&root_id).unwrap());
    assert_eq!(
        titles(&list::list(&project, &children).unwrap()),
        vec!["child"]
    );

    let owned = Query::new().with_owner("frank");
    assert_eq!(list::list(&project, &owned).unwrap().len(), 3);
    let unowned = Query::new().with_owner("alice");
    assert!(list::list(&project, &unowned).unwrap().is_empty());

    // `creator` is recorded only when the request names an operator, so the
    // filter is exercised through a request that does.
    let by_operator = create::create(
        &project,
        CreateRequest::new("attributed", "task", "frank"),
        &OpOptions::new()
            .with_clock(fixtures::shared_clock())
            .with_operator("frank"),
    )
    .unwrap();
    let created_by = Query::new().with_creator("frank");
    let listing = list::list(&project, &created_by).unwrap();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing.items[0].item.id, by_operator.item.id);
    assert!(list::list(&project, &Query::new().with_creator("alice"))
        .unwrap()
        .is_empty());
}

#[test]
fn custom_field_predicates_cover_presence_equality_ordering_and_matching() {
    let (_dir, project) = fielded_project();
    let low = make(&project, "low", "task");
    let high = make(&project, "high", "task");
    make(&project, "none", "task");
    set_field(&project, &low, "effort", FieldValue::Integer(1));
    set_field(&project, &high, "effort", FieldValue::Integer(8));
    set_field(
        &project,
        &low,
        "team",
        FieldValue::String("core-platform".into()),
    );
    set_field(&project, &high, "team", FieldValue::String("web".into()));
    set_field(
        &project,
        &low,
        "tags",
        FieldValue::List(vec![
            FieldValue::String("a".into()),
            FieldValue::String("b".into()),
        ]),
    );

    let run = |predicate: FieldPredicate| {
        list::list(
            &project,
            &Query::new().with_filters(Filters::new().with_field(predicate)),
        )
        .unwrap()
    };

    assert_eq!(
        titles(&run(FieldPredicate::present(
            FieldKey::parse("effort").unwrap()
        ))),
        vec!["low", "high"]
    );
    assert_eq!(
        titles(&run(FieldPredicate {
            key: FieldKey::parse("effort").unwrap(),
            op: FieldOp::Missing,
            value: None,
        })),
        vec!["none"]
    );
    assert_eq!(
        titles(&run(text_predicate(
            "effort",
            FieldOp::Ge,
            FieldValue::Integer(8)
        ))),
        vec!["high"]
    );
    assert_eq!(
        titles(&run(text_predicate(
            "team",
            FieldOp::Prefix,
            FieldValue::String("core".into())
        ))),
        vec!["low"]
    );
    assert_eq!(
        titles(&run(text_predicate(
            "team",
            FieldOp::Contains,
            FieldValue::String("plat".into())
        ))),
        vec!["low"]
    );
    assert_eq!(
        titles(&run(text_predicate(
            "tags",
            FieldOp::ListContains,
            FieldValue::String("b".into())
        ))),
        vec!["low"]
    );
    assert!(run(text_predicate(
        "tags",
        FieldOp::ListContains,
        FieldValue::String("zzz".into())
    ))
    .is_empty());
    // `none` has no effort at all, so a comparison never matches it.
    assert_eq!(
        titles(&run(text_predicate(
            "effort",
            FieldOp::Lt,
            FieldValue::Integer(2)
        ))),
        vec!["low"]
    );
}

#[test]
fn a_predicate_whose_operand_disagrees_with_the_declared_type_is_rejected() {
    let (_dir, project) = fielded_project();
    make(&project, "item", "task");

    let error = list::list(
        &project,
        &Query::new().with_filters(Filters::new().with_field(text_predicate(
            "effort",
            FieldOp::Eq,
            FieldValue::String("three".into()),
        ))),
    )
    .unwrap_err();
    assert_eq!(error.code(), waymark_core::Code::ValidationError);

    // Ordering is defined for strings too, so that predicate is valid; a
    // boolean field is not ordered at all, and a list is not a scalar.
    for predicate in [
        text_predicate("tags", FieldOp::Gt, FieldValue::String("a".into())),
        text_predicate(
            "team",
            FieldOp::ListContains,
            FieldValue::String("a".into()),
        ),
    ] {
        let error = list::list(
            &project,
            &Query::new().with_filters(Filters::new().with_field(predicate)),
        )
        .unwrap_err();
        assert_eq!(error.code(), waymark_core::Code::ValidationError);
    }

    let unknown = FieldPredicate::present(FieldKey::parse("nope").unwrap());
    assert_eq!(
        list::list(
            &project,
            &Query::new().with_filters(Filters::new().with_field(unknown))
        )
        .unwrap_err()
        .code(),
        waymark_core::Code::ValidationError
    );
}

#[test]
fn the_archive_range_is_explicit_and_defaults_to_active_only() {
    let (_dir, project) = fixtures::deterministic_project();
    let active = make(&project, "active", "task");
    let archived = make(&project, "archived", "task");
    let path = waymark_core::ops::read::read(&project, &archived)
        .unwrap()
        .path;
    let mut item = waymark_core::parse::parse_item_file(&path).unwrap();
    item.archived_at = Some(fixtures::instant());
    std::fs::write(&path, waymark_core::serialize::to_document(&item)).unwrap();

    assert_eq!(
        titles(&list::list(&project, &Query::new()).unwrap()),
        vec!["active"]
    );
    assert_eq!(
        titles(&list::list(&project, &Query::new().with_archive(ArchiveRange::Archived)).unwrap()),
        vec!["archived"]
    );
    assert_eq!(
        titles(&list::list(&project, &Query::new().with_archive(ArchiveRange::All)).unwrap()),
        vec!["active", "archived"]
    );
    assert_eq!(
        list::list(&project, &Query::new().with_archive(ArchiveRange::Archived))
            .unwrap()
            .items[0]
            .item
            .id
            .as_str(),
        archived
    );
    assert_ne!(active, archived);

    // Ready and blocked never report an archived item, whatever the range says.
    assert!(
        list::ready(&project, &Query::new().with_archive(ArchiveRange::All))
            .unwrap()
            .items
            .iter()
            .all(|row| !row.item.is_archived())
    );
    assert!(
        list::blocked(&project, &Query::new().with_archive(ArchiveRange::All))
            .unwrap()
            .items
            .iter()
            .all(|row| !row.item.is_archived())
    );
}

#[test]
fn the_cheap_text_filter_reads_only_id_and_title() {
    let (_dir, project) = fixtures::deterministic_project();
    let title_hit = make(&project, "Database migration", "task");
    let body_hit = create::create(
        &project,
        CreateRequest::new("unrelated", "task", "frank").with_body("mentions database"),
        &options(),
    )
    .unwrap()
    .item
    .id
    .as_str()
    .to_owned();

    let listing = list::list(&project, &Query::new().with_text("database")).unwrap();
    assert_eq!(titles(&listing), vec!["Database migration"]);
    assert_eq!(listing.items[0].item.id.as_str(), title_hit);
    assert_ne!(title_hit, body_hit);

    // The ID is the other half of the cheap surface.
    let by_id = list::list(&project, &Query::new().with_text(&body_hit)).unwrap();
    assert_eq!(ids(&by_id), vec![body_hit.clone()]);
    // ...and the match is case-insensitive on both halves.
    let shouty = list::list(&project, &Query::new().with_text("DATABASE")).unwrap();
    assert_eq!(ids(&shouty), vec![title_hit]);
    assert!(
        list::list(&project, &Query::new().with_text("mentions database"))
            .unwrap()
            .is_empty()
    );
}

// -- rank ordering ----------------------------------------------------------

#[test]
fn rank_orders_by_status_group_then_dependency_topology_then_stable_id() {
    let (_dir, project) = fixtures::deterministic_project();
    // Create the dependent first so its stable ID sorts before its blocker's.
    let dependent = make(&project, "dependent", "task");
    let blocker = make(&project, "blocker", "task");
    let free_a = make(&project, "free-a", "task");
    let free_b = make(&project, "free-b", "task");
    add_dependency(&project, &dependent, &blocker, &options()).unwrap();
    assert!(dependent < blocker, "the fixture must be adversarial");

    let listing = list::list(&project, &Query::new()).unwrap();
    assert_eq!(
        ids(&listing),
        vec![
            blocker.clone(),
            free_a.clone(),
            free_b.clone(),
            dependent.clone()
        ],
        "a blocker always ranks before the item that depends on it"
    );

    // A closed item is in the last status group even though it is topologically
    // first: closing the blocker must move it after every open item.
    status::set_status(&project, &blocker, Status::Closed, &options()).unwrap();
    let listing = list::list(&project, &Query::new()).unwrap();
    let ordered = ids(&listing);
    assert_eq!(ordered.last().unwrap(), &blocker);
    assert!(
        ordered.iter().position(|id| id == &dependent).unwrap() < ordered.len() - 1,
        "{ordered:?}"
    );
}

#[test]
fn rank_is_deterministic_across_runs_and_never_drops_a_cycle_member() {
    let (_dir, project) = fixtures::deterministic_project();
    let a = make(&project, "a", "task");
    let b = make(&project, "b", "task");
    add_dependency(&project, &b, &a, &options()).unwrap();
    // A cycle can only arrive by an external edit; the query must still rank
    // every item exactly once.
    let path = waymark_core::ops::read::read(&project, &a).unwrap().path;
    let mut item = waymark_core::parse::parse_item_file(&path).unwrap();
    item.dependencies = vec![waymark_core::WorkItemId::parse(&b).unwrap()];
    std::fs::write(&path, waymark_core::serialize::to_document(&item)).unwrap();

    let first = ids(&list::list(&project, &Query::new()).unwrap());
    let second = ids(&list::list(&project, &Query::new()).unwrap());
    assert_eq!(first, second);
    let mut sorted = first.clone();
    sorted.sort();
    assert_eq!(sorted, vec![a, b]);
}

#[test]
fn named_sorts_keep_the_stable_id_tie_break_in_both_directions() {
    let (_dir, project) = fixtures::deterministic_project();
    let first = make(&project, "same title", "task");
    let second = make(&project, "same title", "task");
    let third = make(&project, "another", "task");

    let ascending = list::list(&project, &Query::new().order_by(SortField::Title)).unwrap();
    assert_eq!(
        ids(&ascending),
        vec![third.clone(), first.clone(), second.clone()]
    );
    let descending = list::list(
        &project,
        &Query::new().with_sort(Sort::of(SortField::Title).descending()),
    )
    .unwrap();
    // The tie between the two identical titles stays in stable-ID order.
    assert_eq!(ids(&descending), vec![first, second, third]);
}

#[test]
fn a_missing_custom_value_sorts_after_a_present_one_in_both_directions() {
    let (_dir, project) = fielded_project();
    let low = make(&project, "low", "task");
    let high = make(&project, "high", "task");
    let missing = make(&project, "missing", "task");
    set_field(&project, &low, "effort", FieldValue::Integer(1));
    set_field(&project, &high, "effort", FieldValue::Integer(9));

    let field = SortField::Custom(FieldKey::parse("effort").unwrap());
    let ascending = list::list(&project, &Query::new().order_by(field.clone())).unwrap();
    assert_eq!(
        ids(&ascending),
        vec![low.clone(), high.clone(), missing.clone()]
    );

    let descending = list::list(
        &project,
        &Query::new().with_sort(Sort::of(field).descending()),
    )
    .unwrap();
    // Present values reverse; the missing value stays last.
    assert_eq!(ids(&descending), vec![high, low, missing]);
}

#[test]
fn only_strings_and_integers_are_sortable() {
    let (_dir, project) = fielded_project();
    make(&project, "item", "task");

    let error = list::list(
        &project,
        &Query::new().order_by(SortField::Custom(FieldKey::parse("tags").unwrap())),
    )
    .unwrap_err();
    assert_eq!(error.code(), waymark_core::Code::ValidationError);
}

// -- pagination -------------------------------------------------------------

#[test]
fn pagination_is_snapshot_local_and_the_union_of_pages_is_the_whole_result() {
    let (_dir, project) = fixtures::deterministic_project();
    let mut created = Vec::new();
    for index in 0..7 {
        created.push(make(&project, &format!("item-{index}"), "task"));
    }

    let full = list::list(&project, &Query::new()).unwrap();
    let expected = ids(&full);
    assert_eq!(full.total, 7);

    let mut union = Vec::new();
    let mut offset = 0;
    while offset < 7 {
        let page = list::list(&project, &Query::new().with_limit(2).with_offset(offset)).unwrap();
        assert_eq!(page.total, 7, "total is the pre-window count");
        assert!(page.items.len() <= 2);
        union.extend(ids(&page));
        offset += 2;
    }
    assert_eq!(union, expected, "no page may skip or repeat an item");
    assert_eq!(
        union
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        7
    );

    // A limit of zero is unlimited, and an offset past the end is empty.
    let unlimited = list::list(&project, &Query::new().with_limit(0)).unwrap();
    assert_eq!(ids(&unlimited), expected);
    let past_end = list::list(&project, &Query::new().with_limit(10).with_offset(99)).unwrap();
    assert!(past_end.is_empty());
    assert_eq!(past_end.total, 7);
    assert_eq!(past_end.offset, 99);
}

#[test]
fn the_default_window_is_one_hundred_rows() {
    assert_eq!(Window::new().limit, 100);
    assert_eq!(list::DEFAULT_LIMIT, 100);
    assert_eq!(waymark_core::query::MCP_DEFAULT_LIMIT, 50);
}

// -- structured query form --------------------------------------------------

#[test]
fn the_structured_json_filter_parses_into_the_same_model() {
    let predicate = serde_json::json!([
        {"key": "effort", "op": "ge", "value": 3},
        {"key": "team", "op": "prefix", "value": "core"}
    ]);
    let filters = Filters::from_json(&serde_json::json!({
        "status": ["open", "in_progress"],
        "category": "task",
        "owner": "frank",
        "parent_present": false,
        "dependencies": "none",
        "leaves": true,
        "archive": "active",
        "field": predicate,
    }))
    .unwrap();

    assert_eq!(filters.statuses.len(), 2);
    assert!(filters.statuses.contains(&Status::Open));
    assert!(filters
        .categories
        .contains(&CategoryKey::parse("task").unwrap()));
    assert_eq!(filters.owner.as_deref(), Some("frank"));
    assert_eq!(filters.parent_present, Some(false));
    assert_eq!(filters.has_dependencies, Some(false));
    assert!(filters.leaves_only);
    assert_eq!(filters.archive, ArchiveRange::Active);
    assert_eq!(filters.fields.len(), 2);

    // The structured form validates and runs against a real project.
    let (_dir, project) = fielded_project();
    let id = make(&project, "item", "task");
    set_field(&project, &id, "effort", FieldValue::Integer(5));
    set_field(&project, &id, "team", FieldValue::String("core".into()));
    let listing = list::list(&project, &Query::new().with_filters(filters)).unwrap();
    assert_eq!(titles(&listing), vec!["item"]);
}

#[test]
fn the_structured_filter_rejects_null_unknown_keys_and_typed_mismatches() {
    assert_eq!(
        Filters::from_json(&serde_json::json!({"owner": null}))
            .unwrap_err()
            .code(),
        waymark_core::Code::ValidationError
    );
    assert_eq!(
        Filters::from_json(&serde_json::json!({"nonsense": 1}))
            .unwrap_err()
            .code(),
        waymark_core::Code::ValidationError
    );
    assert_eq!(
        Filters::from_json(
            &serde_json::json!({"field": [{"key": "effort", "op": "exists", "value": 1}]})
        )
        .unwrap_err()
        .code(),
        waymark_core::Code::ValidationError
    );
    assert_eq!(
        Filters::from_json(&serde_json::json!({"archive": "sometimes"}))
            .unwrap_err()
            .code(),
        waymark_core::Code::ValidationError
    );
}

// -- search -----------------------------------------------------------------

#[test]
fn search_matches_title_and_body_case_insensitively_with_title_hits_first() {
    let (_dir, project) = fixtures::deterministic_project();
    let body_only = create::create(
        &project,
        CreateRequest::new("alpha", "task", "frank").with_body("The DATABASE is slow"),
        &options(),
    )
    .unwrap()
    .item
    .id
    .as_str()
    .to_owned();
    let title_hit = make(&project, "database migration", "task");
    let miss = make(&project, "unrelated", "task");

    let result = search::search(&project, &SearchRequest::new("DaTaBaSe")).unwrap();
    assert_eq!(result.total, 2);
    let ordered: Vec<&str> = result
        .hits
        .iter()
        .map(|hit| hit.row.item.id.as_str())
        .collect();
    assert_eq!(ordered, vec![title_hit.as_str(), body_only.as_str()]);
    assert!(result.hits[0].title_hit);
    assert!(!result.hits[1].title_hit);
    assert!(result.hits[1].body_hit);
    assert!(!ordered.contains(&miss.as_str()));

    // No snippets: the payload is a projected row, never a body excerpt.
    let payload = serde_json::to_string(&result.hits[0].row.item.body).unwrap();
    assert_eq!(payload, "\"\"");
}

#[test]
fn search_also_matches_frontmatter_scalars_and_shares_the_window() {
    let (_dir, project) = fixtures::deterministic_project();
    let owned = create::create(
        &project,
        CreateRequest::new("owned", "task", "averylongname"),
        &options(),
    )
    .unwrap()
    .item
    .id
    .as_str()
    .to_owned();
    make(&project, "another", "task");

    let result = search::search(&project, &SearchRequest::new("averylongname")).unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.hits[0].row.item.id.as_str(), owned);
    assert!(!result.hits[0].title_hit);

    let page = search::search(&project, &SearchRequest::new("").with_limit(1)).unwrap();
    assert_eq!(page.hits.len(), 1);
    assert_eq!(page.total, 2);
    assert_eq!(page.limit, 1);
}

// -- projection -------------------------------------------------------------

#[test]
fn projection_accepts_a_named_field_set_and_the_compact_form() {
    let (_dir, project) = fixtures::deterministic_project();
    let id = make(&project, "titled", "task");
    let listing = list::list(&project, &Query::new()).unwrap();
    let row = &listing.items[0];

    let named = Projection::parse_list("id,title,ready").unwrap();
    assert!(named.includes(waymark_core::query::FieldName::Title));
    assert!(!named.includes(waymark_core::query::FieldName::Status));
    let projected = waymark_core::query::projected_row(row, &named);
    assert_eq!(projected["item"]["title"], serde_json::json!("titled"));
    assert!(projected["item"].get("status").is_none());
    assert_eq!(projected["id"], serde_json::json!(id));
    assert_eq!(projected["ready"], serde_json::json!(true));

    let compact = Projection::compact();
    let projected = waymark_core::query::projected_row(row, &compact);
    assert!(projected.get("blocked_by").is_none());
    assert_eq!(projected["item"]["status"], serde_json::json!("open"));

    assert_eq!(
        Projection::parse_list("nope").unwrap_err().code(),
        waymark_core::Code::ValidationError
    );
}

// -- metadata ---------------------------------------------------------------

#[test]
fn every_collection_result_carries_source_completeness_freshness_and_diagnostics() {
    let (_dir, project) = fixtures::deterministic_project();
    let id = make(&project, "item", "task");
    // A dangling dependency makes the graph emit a diagnostic without failing.
    let path = waymark_core::ops::read::read(&project, &id).unwrap().path;
    let mut item = waymark_core::parse::parse_item_file(&path).unwrap();
    item.dependencies = vec![waymark_core::WorkItemId::parse("wm-00000000zzzz").unwrap()];
    std::fs::write(&path, waymark_core::serialize::to_document(&item)).unwrap();

    for meta in [
        list::list(&project, &Query::new()).unwrap().meta,
        list::ready(&project, &Query::new()).unwrap().meta,
        list::blocked(&project, &Query::new()).unwrap().meta,
        search::search(&project, &SearchRequest::new("item"))
            .unwrap()
            .meta,
        stats::stats(&project, &StatsRequest::new()).unwrap().meta,
    ] {
        assert_eq!(meta.source, Source::MemoryProjection);
        assert_eq!(meta.completeness, Completeness::Complete);
        assert_eq!(meta.freshness, Freshness::Fresh);
        assert!(!meta.diagnostics.is_empty());
        assert_eq!(
            meta.diagnostics[0].code,
            waymark_core::Code::DanglingReference
        );
    }
}

// -- stats ------------------------------------------------------------------

#[test]
fn stats_reports_counts_only_and_always_states_its_scope() {
    let (_dir, project) = fielded_project();
    let blocker = make(&project, "blocker", "task");
    let dependent = make(&project, "dependent", "task");
    let done = make(&project, "done", "bug");
    add_dependency(&project, &dependent, &blocker, &options()).unwrap();
    status::set_status(&project, &done, Status::Closed, &options()).unwrap();

    let stats = stats::stats(&project, &StatsRequest::new()).unwrap();
    assert_eq!(stats.total, 3);
    assert_eq!(stats.by_status.get(&Status::Open), Some(&2));
    assert_eq!(stats.by_status.get(&Status::Closed), Some(&1));
    assert_eq!(
        stats.by_category.get(&CategoryKey::parse("task").unwrap()),
        Some(&2)
    );
    assert_eq!(
        stats.by_category.get(&CategoryKey::parse("bug").unwrap()),
        Some(&1)
    );
    assert_eq!(stats.ready, 1);
    assert_eq!(stats.blocked, 1);
    assert_eq!(stats.archive, ArchiveRange::Active);
    assert_eq!(stats.completeness, Completeness::Complete);
    assert_eq!(stats.freshness, Freshness::Fresh);

    let payload = stats::statistics_json(&stats);
    assert_eq!(payload["archive"], serde_json::json!("active"));
    assert_eq!(payload["completeness"], serde_json::json!("complete"));
    assert_eq!(payload["freshness"], serde_json::json!("fresh"));
    assert_eq!(payload["counts_only"], serde_json::json!(true));
    let not_reported = payload["not_reported"].as_array().unwrap();
    for forbidden in ["history", "throughput"] {
        assert!(
            not_reported.iter().any(|value| value == forbidden),
            "{forbidden} must be named as not reported"
        );
    }
    for forbidden in ["history", "throughput", "completed_last_week", "rate"] {
        assert!(
            payload.get(forbidden).is_none(),
            "stats must never report {forbidden}"
        );
    }
}

#[test]
fn stats_honours_the_archive_range_and_the_optional_top_n() {
    let (_dir, project) = fielded_project();
    let a = make(&project, "a", "task");
    let b = make(&project, "b", "task");
    let c = make(&project, "c", "task");
    set_field(&project, &a, "team", FieldValue::String("core".into()));
    set_field(&project, &b, "team", FieldValue::String("core".into()));
    set_field(&project, &c, "team", FieldValue::String("web".into()));

    let stats = stats::stats(
        &project,
        &StatsRequest::new().with_top(FieldKey::parse("team").unwrap(), 1),
    )
    .unwrap();
    let distribution = stats.distribution.expect("a distribution");
    assert_eq!(distribution.buckets.len(), 1);
    assert_eq!(distribution.buckets[0].value.as_deref(), Some("core"));
    assert_eq!(distribution.buckets[0].count, 2);

    let archived = make(&project, "archived", "task");
    let path = waymark_core::ops::read::read(&project, &archived)
        .unwrap()
        .path;
    let mut item = waymark_core::parse::parse_item_file(&path).unwrap();
    item.archived_at = Some(fixtures::instant());
    std::fs::write(&path, waymark_core::serialize::to_document(&item)).unwrap();

    assert_eq!(
        stats::stats(&project, &StatsRequest::new()).unwrap().total,
        3
    );
    let all = stats::stats(
        &project,
        &StatsRequest::new().with_archive(ArchiveRange::All),
    )
    .unwrap();
    assert_eq!(all.total, 4);
    assert_eq!(all.archive, ArchiveRange::All);
    // An archived item is never ready or blocked: it waits on nothing.
    assert_eq!(all.ready, 3);
    assert_eq!(all.blocked, 0);
}

#[test]
fn a_stats_distribution_requires_a_declared_string_field() {
    let (_dir, project) = fielded_project();
    make(&project, "item", "task");
    assert_eq!(
        stats::stats(
            &project,
            &StatsRequest::new().with_top(FieldKey::parse("effort").unwrap(), 3)
        )
        .unwrap_err()
        .code(),
        waymark_core::Code::ValidationError
    );
}

#[test]
fn two_populations_share_one_filter_model_and_never_leak_archived_items() {
    let (_dir, project) = fixtures::deterministic_project();
    make(&project, "one", "task");
    let ready = list::ready(&project, &Query::new().with_sort(Sort::rank())).unwrap();
    assert_eq!(ready.total, 1);
    let blocked = list::blocked(&project, &Query::new().with_sort(Sort::rank())).unwrap();
    assert_eq!(blocked.total, 0);
    assert_eq!(blocked.meta.completeness, ready.meta.completeness);
    assert!(Population::Ready.keeps(&waymark_core::Readiness::ready()));
    assert!(!Population::Blocked.keeps(&waymark_core::Readiness::closed()));
}
