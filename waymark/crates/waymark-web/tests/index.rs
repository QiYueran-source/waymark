//! The active-item index API: a paginated, read-only list of the work items
//! the shared projection holds, ordered by stable ID ascending.
//!
//! The index answers in every state the shared projection can be in — healthy,
//! incomplete, stale after a failed refresh, or never loaded — because it is a
//! diagnostic list. Incomplete or stale snapshots supply whatever rows parsed,
//! labeled by a `state` block that states the rows are not a complete/current
//! index; only the invalid-page adapter error is a non-200 answer.

mod support;

use support::{assets, fixture, get, item, seed, start, Fixture};
use waymark_core::model::Status;
use waymark_core::WorkItemId;

/// One root bug already on disk when the service starts.
fn seeded() -> Fixture {
    let fixture = fixture();
    seed(
        &fixture.project,
        &item("wm-000000000001", "bug", "First bug"),
    );
    fixture
}

/// Nine active items plus one archived: ids `…001`–`…009` active, `…00a`
/// archived (a Crockford body of the same length), so the archived row can
/// never be confused with an active one and the total is asserted exactly.
fn populated() -> Fixture {
    let fixture = fixture();
    for body in 1..=9 {
        let id = format!("wm-00000000000{body}");
        seed(
            &fixture.project,
            &item(
                &id,
                if body % 2 == 0 { "task" } else { "bug" },
                &format!("Item {body}"),
            ),
        );
    }
    let mut archived = item("wm-00000000000a", "bug", "Archived item");
    archived.archived_at = Some(waymark_core::testing::instant());
    seed(&fixture.project, &archived);
    fixture
}

/// The rows of one index response, in wire order.
fn rows(body: &serde_json::Value) -> Vec<serde_json::Value> {
    body["data"]["rows"]
        .as_array()
        .expect("rows is an array")
        .clone()
}

/// Assert the `state` block of a payload that labels its rows: `complete` and
/// `current` are false, `freshness` matches the lifecycle, and the notice
/// states the rows are not a complete/current index.
fn assert_state_block(body: &serde_json::Value, freshness: &str) {
    let state = &body["data"]["state"];
    assert_eq!(state["complete"], false);
    assert_eq!(state["current"], false);
    assert_eq!(state["freshness"], freshness);
    assert!(state["unreadable_documents"].is_number());
    assert!(state["uncertain_paths"].is_number());
    assert!(state["diagnostics"].is_array());
    assert!(
        state["notice"]
            .as_str()
            .expect("the notice is prose")
            .contains("not a complete, current index"),
        "notice: {}",
        state["notice"]
    );
}

/// The project-relative document path the server must report for (`id`,
/// `category`): the category directory plus the ID's file name.
fn relative_path(fixture: &Fixture, id: &str, category: &str) -> String {
    let key = waymark_core::CategoryKey::parse(category).expect("a valid category key");
    let absolute = fixture
        .project
        .path_for(&WorkItemId::parse(id).expect("a valid stable ID"), &key)
        .expect("the category is configured");
    absolute
        .strip_prefix(fixture.project.root())
        .expect("the document path starts with the project root")
        .to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn healthy_index_returns_active_rows_sorted_by_id_with_only_the_five_fields() {
    let fixture = populated();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let response = get(running.addr(), "/api/index?page=1");
    assert_eq!(response.status, 200);
    let body = response.json();
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["page"], 1);
    assert_eq!(body["data"]["page_size"], 50);
    assert_eq!(body["data"]["total"], 9);
    assert_eq!(body["data"]["has_previous"], false);
    assert_eq!(body["data"]["has_next"], false);

    let rows = rows(&body);
    assert_eq!(rows.len(), 9);
    let ids: Vec<&str> = rows.iter().map(|row| row["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        (1..=9)
            .map(|n| format!("wm-00000000000{n}"))
            .collect::<Vec<_>>(),
        "the rows are ordered by stable ID ascending"
    );
    // The archived row never appears, in rows or the total.
    assert!(!ids.contains(&"wm-00000000000a"));
    assert!(!response.text().contains("Archived item"));

    // Rows carry exactly id/title/category/status/path — nothing else.
    let row = &rows[0];
    assert_eq!(row["id"], "wm-000000000001");
    assert_eq!(row["title"], "Item 1");
    assert_eq!(row["category"]["key"], "bug");
    assert_eq!(row["category"]["name"], "Bugs");
    assert_eq!(row["category"]["shape"], 0);
    assert_eq!(row["status"], "open");
    assert_eq!(
        row["path"],
        relative_path(&fixture, "wm-000000000001", "bug")
    );
    let mut fields: Vec<&str> = row
        .as_object()
        .expect("a row object")
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    assert_eq!(fields, ["category", "id", "path", "status", "title"]);

    // A complete, fresh snapshot labels its rows as a current index: the state
    // block is present, `complete`/`current` are true, and there is no notice.
    let state = &body["data"]["state"];
    assert_eq!(state["complete"], true);
    assert_eq!(state["current"], true);
    assert_eq!(state["freshness"], "fresh");
    assert_eq!(state["unreadable_documents"], 0);
    assert_eq!(state["uncertain_paths"], 0);
    assert_eq!(state["diagnostics"].as_array().unwrap().len(), 0);
    assert_eq!(state["notice"], "");

    // The `_meta` mirrors a complete, fresh memory projection read.
    assert_eq!(body["_meta"]["source"], "memory_projection");
    assert_eq!(body["_meta"]["completeness"], "complete");
    assert_eq!(body["_meta"]["freshness"], "fresh");
    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn rows_keep_project_relative_paths_and_statuses() {
    let fixture = seeded();
    let mut second = item("wm-000000000002", "task", "In progress task");
    second.status = Status::InProgress;
    seed(&fixture.project, &second);
    let mut third = item("wm-000000000003", "bug", "Closed bug");
    third.status = Status::Closed;
    seed(&fixture.project, &third);
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let body = get(running.addr(), "/api/index").json();
    let rows = rows(&body);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1]["status"], "in_progress");
    assert_eq!(
        rows[1]["path"],
        relative_path(&fixture, "wm-000000000002", "task")
    );
    assert_eq!(rows[2]["status"], "closed");
    assert_eq!(
        rows[2]["path"],
        relative_path(&fixture, "wm-000000000003", "bug")
    );
    // Paths are project-relative: the absolute fixture root never appears.
    assert!(!body.to_string().contains(fixture.root().to_str().unwrap()));
    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn pagination_window_and_navigation_are_driven_by_the_api() {
    let fixture = fixture();
    for body in 1..=60 {
        let id = format!("wm-0000000000{:02}", body);
        seed(&fixture.project, &item(&id, "bug", &format!("Item {body}")));
    }
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let first = get(running.addr(), "/api/index?page=1").json();
    assert_eq!(first["data"]["page"], 1);
    assert_eq!(first["data"]["total"], 60);
    assert_eq!(first["data"]["has_previous"], false);
    assert_eq!(first["data"]["has_next"], true);
    let page_one = rows(&first);
    assert_eq!(page_one.len(), 50);
    assert_eq!(page_one[0]["id"], "wm-000000000001");
    assert_eq!(page_one[49]["id"], "wm-000000000050");

    let second = get(running.addr(), "/api/index?page=2").json();
    assert_eq!(second["data"]["page"], 2);
    assert_eq!(second["data"]["has_previous"], true);
    assert_eq!(second["data"]["has_next"], false);
    let page_two = rows(&second);
    assert_eq!(page_two.len(), 10);
    assert_eq!(page_two[0]["id"], "wm-000000000051");
    assert_eq!(page_two[9]["id"], "wm-000000000060");

    // A page beyond the last page is an empty window with navigation, not an
    // error: the client can never push the API past its bound.
    let beyond = get(running.addr(), "/api/index?page=3").json();
    assert_eq!(beyond["ok"], true);
    assert_eq!(beyond["data"]["page"], 3);
    assert_eq!(
        beyond["data"]["has_previous"], true,
        "an empty page still has a previous page"
    );
    assert_eq!(beyond["data"]["has_next"], false);
    assert!(beyond["data"]["rows"].as_array().unwrap().is_empty());
    assert_eq!(beyond["data"]["total"], 60);
    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_absent_or_blank_page_defaults_to_one() {
    let fixture = seeded();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let body = get(running.addr(), "/api/index").json();
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["page"], 1);

    let blank = get(running.addr(), "/api/index?page=").json();
    assert_eq!(blank["data"]["page"], 1);

    // An unknown parameter is tolerated, exactly like the tree route.
    let ignored = get(running.addr(), "/api/index?page=1&cache_buster=abc").json();
    assert_eq!(ignored["ok"], true);
    assert_eq!(ignored["data"]["page"], 1);
    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_non_positive_or_non_numeric_page_is_an_invalid_query() {
    let fixture = seeded();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    for path in ["/api/index?page=0", "/api/index?page=abc"] {
        let response = get(running.addr(), path);
        assert_eq!(response.status, 400, "{path}");
        assert_eq!(response.json()["error"]["code"], "INVALID_QUERY", "{path}");
    }
    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stale_snapshot_still_answers_labeled_rows() {
    let fixture = seeded();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    // Warm the snapshot, then break the bug category so the next inventory
    // walk and refresh fail: the retained snapshot becomes stale.
    let warm = get(running.addr(), "/api/index");
    assert_eq!(warm.status, 200);
    let category = fixture.root().join("work").join("bugs");
    std::fs::remove_dir_all(&category).expect("the category directory is removable");
    let file = std::fs::File::create(&category).expect("a file can replace the directory");
    file.sync_all().expect("the replacement file is durable");

    let response = get(running.addr(), "/api/index?page=1");
    assert_eq!(response.status, 200);
    let body = response.json();
    assert_eq!(body["ok"], true);
    // The stale snapshot still labels its rows: id, title, category, status,
    // and a project-relative path.
    let row = &rows(&body)[0];
    assert_eq!(row["id"], "wm-000000000001");
    assert_eq!(row["title"], "First bug");
    assert_eq!(
        row["path"],
        relative_path(&fixture, "wm-000000000001", "bug")
    );
    assert_eq!(body["data"]["total"], 1);
    // The state block labels the rows as not a complete/current index.
    assert_state_block(&body, "stale");
    // The `_meta` mirrors the lifecycle: stale, with diagnostics.
    assert_eq!(body["_meta"]["source"], "memory_projection");
    assert_eq!(body["_meta"]["freshness"], "stale");
    assert!(body["_meta"]["diagnostics"].is_array());

    std::fs::remove_file(&category).expect("the replacement file is removable");
    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_incomplete_snapshot_still_answers_labeled_rows() {
    let fixture = seeded();
    // A document that parses as a failure makes the projection scoped.
    std::fs::write(
        fixture
            .root()
            .join("work")
            .join("bugs")
            .join("wm-0000000000aa.md"),
        "this is not a work-item document\n",
    )
    .expect("the broken document is writable");
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let response = get(running.addr(), "/api/index?page=1");
    assert_eq!(response.status, 200);
    let body = response.json();
    assert_eq!(body["data"]["total"], 1);
    let row = &rows(&body)[0];
    assert_eq!(row["id"], "wm-000000000001");
    assert_eq!(row["title"], "First bug");
    let state = &body["data"]["state"];
    assert_eq!(state["complete"], false);
    assert_eq!(state["current"], false);
    assert_eq!(state["freshness"], "fresh");
    assert_eq!(state["unreadable_documents"], 1);
    assert!(state["notice"]
        .as_str()
        .expect("the notice is prose")
        .contains("not a complete, current index"));
    assert_eq!(body["_meta"]["completeness"], "scoped");
    assert_eq!(body["_meta"]["freshness"], "fresh");
    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_never_loaded_projection_answers_empty_rows_with_state() {
    // Break the bug category before the service starts: the startup load fails,
    // and the first request's sync fails again, so the index reports rows `[]`
    // with the state block — exactly the status route's failed-startup setup.
    let fixture = seeded();
    let category = fixture.root().join("work").join("bugs");
    std::fs::remove_dir_all(&category).expect("the category directory is removable");
    let file = std::fs::File::create(&category).expect("a file can replace the directory");
    file.sync_all().expect("the replacement file is durable");
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let response = get(running.addr(), "/api/index?page=1");
    assert_eq!(
        response.status, 200,
        "a failed startup never prevents the index from answering"
    );
    let body = response.json();
    assert_eq!(body["ok"], true);
    assert!(body["data"]["rows"].as_array().unwrap().is_empty());
    assert_eq!(body["data"]["total"], 0);
    assert_eq!(body["data"]["has_previous"], false);
    assert_eq!(body["data"]["has_next"], false);
    assert_state_block(&body, "unknown");
    assert_eq!(body["_meta"]["completeness"], "scoped");
    assert_eq!(body["_meta"]["freshness"], "unknown");
    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn reading_the_index_never_changes_a_byte_of_the_project() {
    let fixture = populated();
    let before = support::snapshot(fixture.root());
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    for path in ["/api/index", "/api/index?page=1", "/api/index?page=2"] {
        assert_eq!(get(running.addr(), path).status, 200, "{path}");
    }
    assert_eq!(
        before,
        support::snapshot(fixture.root()),
        "the index read must not change one byte on disk"
    );
    running.shutdown().await.unwrap();
}
