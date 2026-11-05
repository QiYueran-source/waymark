//! The read API over real HTTP: health, the combined and grouped trees, item
//! detail, the empty project, and the unresolved-relations area.
//!
//! Fixtures:
//! - `wm-000000000001` bug root, has cross-category child `…002`
//! - `wm-000000000002` task child of `…001`
//! - `wm-000000000003` bug, depends on open `…004` → blocked
//! - `wm-000000000004` task, open, no dependencies
//! - `wm-000000000005` task, in progress
//! - `wm-000000000006` bug, closed
//! - `wm-000000000007` bug, archived (must never appear in the tree)
//! - `wm-000000000008` task, names missing parent `wm-0000000000ff`
//! - `wm-000000000009` bug root with a body

mod support;

use support::{assets, fixture, get, id, item, seed, start, Fixture};
use waymark_core::model::Status;

fn populated() -> Fixture {
    let fixture = fixture();

    let root = item("wm-000000000001", "bug", "Root bug");
    let mut child = item("wm-000000000002", "task", "Cross-category child");
    child.parent = Some(id("wm-000000000001"));
    let mut blocked = item("wm-000000000003", "bug", "Blocked bug");
    blocked.dependencies = vec![id("wm-000000000004")];
    let open = item("wm-000000000004", "task", "Open dependency");
    let mut in_progress = item("wm-000000000005", "task", "In progress");
    in_progress.status = Status::InProgress;
    let mut closed = item("wm-000000000006", "bug", "Closed bug");
    closed.status = Status::Closed;
    let mut archived = item("wm-000000000007", "bug", "Archived bug");
    archived.archived_at = Some(waymark_core::testing::instant());
    let mut orphan = item("wm-000000000008", "task", "Orphan");
    orphan.parent = Some(id("wm-0000000000ff"));
    let mut with_body = item("wm-000000000009", "bug", "Documented bug");
    with_body.body = "## Steps\n\n1. reproduce\n".to_owned();

    for document in [
        &root,
        &child,
        &blocked,
        &open,
        &in_progress,
        &closed,
        &archived,
        &orphan,
        &with_body,
    ] {
        seed(&fixture.project, document);
    }
    fixture
}

fn root_ids(tree: &serde_json::Value) -> Vec<String> {
    tree["data"]["roots"]
        .as_array()
        .expect("roots is an array")
        .iter()
        .map(|node| node["id"].as_str().expect("a node id").to_owned())
        .collect()
}

fn node<'a>(nodes: &'a [serde_json::Value], id: &str) -> Option<&'a serde_json::Value> {
    nodes.iter().find(|node| node["id"] == id)
}

#[tokio::test(flavor = "multi_thread")]
async fn health_reports_a_read_only_service() {
    let fixture = fixture();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let response = get(running.addr(), "/api/health");
    assert_eq!(response.status, 200);
    let body = response.json();
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["status"], "ok");
    assert_eq!(body["data"]["service"], "waymark-web");
    assert_eq!(body["data"]["read_only"], true);
    // `project.root` is the normalized absolute root the browser keys its local
    // preferences by; no second endpoint, and nothing is written to the project.
    let root = body["data"]["project"]["root"]
        .as_str()
        .expect("project.root is a string");
    let canonical = std::fs::canonicalize(fixture.root()).expect("the fixture root canonicalizes");
    assert_eq!(root, canonical.to_str().expect("a UTF-8 fixture root"));
    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn combined_tree_is_top_down_and_excludes_archived_items() {
    let fixture = populated();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let response = get(running.addr(), "/api/tree");
    assert_eq!(response.status, 200);
    let tree = response.json();
    assert_eq!(tree["ok"], true);
    assert_eq!(tree["data"]["view"], "combined");
    let roots = tree["data"]["roots"].as_array().unwrap();
    let roots_ids = root_ids(&tree);
    // Roots are the parentless active items, and the archived item is nowhere.
    assert_eq!(
        roots_ids,
        vec![
            "wm-000000000001",
            "wm-000000000003",
            "wm-000000000004",
            "wm-000000000005",
            "wm-000000000006",
            "wm-000000000009",
        ]
    );
    assert!(!tree.to_string().contains("wm-000000000007"));
    assert!(!tree.to_string().contains("Archived bug"));

    // The cross-category child stays under its real parent.
    let parent = node(roots, "wm-000000000001").unwrap();
    let children = parent["children"].as_array().unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0]["id"], "wm-000000000002");
    assert_eq!(children[0]["category"]["key"], "task");
    assert_eq!(children[0]["parent"], "wm-000000000001");

    // Derived state: open/ready, blocked with a reason, in progress, closed.
    let open_node = node(roots, "wm-000000000004").unwrap();
    assert_eq!(open_node["status"], "open");
    assert_eq!(open_node["state"], "ready");
    assert_eq!(open_node["ready"], true);
    assert_eq!(open_node["blocked_reason"], serde_json::Value::Null);

    let blocked_node = node(roots, "wm-000000000003").unwrap();
    assert_eq!(blocked_node["state"], "blocked");
    assert_eq!(blocked_node["blocked"], true);
    let reason = blocked_node["blocked_reason"].as_str().unwrap();
    assert!(reason.contains("wm-000000000004"), "reason: {reason}");
    assert!(reason.contains("unfinished"), "reason: {reason}");
    assert_eq!(blocked_node["dependencies"][0], "wm-000000000004");

    let in_progress_node = node(roots, "wm-000000000005").unwrap();
    assert_eq!(in_progress_node["state"], "in_progress");
    let closed_node = node(roots, "wm-000000000006").unwrap();
    assert_eq!(closed_node["state"], "closed");

    // Category identity: stable key, configured name, stable shape index.
    assert_eq!(open_node["category"]["key"], "task");
    assert_eq!(open_node["category"]["name"], "Tasks");
    assert_eq!(open_node["category"]["shape"], 1);
    assert_eq!(parent["category"]["shape"], 0);
    assert_eq!(tree["data"]["categories"].as_array().unwrap().len(), 2);

    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn grouped_tree_groups_by_root_category_and_keeps_cross_category_children() {
    let fixture = populated();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let tree = get(running.addr(), "/api/tree?view=grouped").json();
    assert_eq!(tree["data"]["view"], "grouped");
    let groups = tree["data"]["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 2);
    let bugs = groups
        .iter()
        .find(|group| group["category"]["key"] == "bug")
        .unwrap();
    let tasks = groups
        .iter()
        .find(|group| group["category"]["key"] == "task")
        .unwrap();

    let bug_roots = bugs["roots"].as_array().unwrap();
    let bug_ids: Vec<_> = bug_roots
        .iter()
        .map(|node| node["id"].as_str().unwrap())
        .collect();
    assert!(bug_ids.contains(&"wm-000000000001"));
    assert!(bug_ids.contains(&"wm-000000000003"));
    // The task-category child is not promoted into the task group...
    let task_roots = tasks["roots"].as_array().unwrap();
    let task_ids: Vec<_> = task_roots
        .iter()
        .map(|node| node["id"].as_str().unwrap())
        .collect();
    assert!(!task_ids.contains(&"wm-000000000002"));
    // ...it remains under its bug-category parent.
    let parent = node(bug_roots, "wm-000000000001").unwrap();
    assert_eq!(parent["children"][0]["id"], "wm-000000000002");
    assert_eq!(parent["children"][0]["category"]["key"], "task");

    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn item_detail_has_body_status_owner_fields_and_relationships() {
    let fixture = populated();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let response = get(running.addr(), "/api/item/wm-000000000009");
    assert_eq!(response.status, 200);
    let detail = response.json();
    assert_eq!(detail["ok"], true);
    let data = &detail["data"];
    assert_eq!(data["id"], "wm-000000000009");
    assert_eq!(data["title"], "Documented bug");
    assert_eq!(data["category"]["key"], "bug");
    assert_eq!(data["status"], "open");
    assert_eq!(data["owner"], "frank");
    assert_eq!(data["archived"], false);
    assert!(data["body_html"].as_str().unwrap().contains("<h2>"));
    assert!(data["relationships"]["parent"].is_null());
    assert_eq!(data["custom_fields"].as_object().unwrap().len(), 0);

    let child = get(running.addr(), "/api/item/wm-000000000002").json();
    assert_eq!(child["data"]["relationships"]["parent"], "wm-000000000001");

    let blocked = get(running.addr(), "/api/item/wm-000000000003").json();
    assert_eq!(blocked["data"]["state"], "blocked");
    assert!(blocked["data"]["blocked_reason"]
        .as_str()
        .unwrap()
        .contains("wm-000000000004"));
    assert_eq!(
        blocked["data"]["relationships"]["dependencies"][0],
        "wm-000000000004"
    );

    let archived = get(running.addr(), "/api/item/wm-000000000007").json();
    assert_eq!(archived["data"]["archived"], true);
    assert_eq!(archived["data"]["state"], "closed");

    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_project_is_a_legible_empty_state() {
    let fixture = fixture();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    for view in ["combined", "grouped"] {
        let tree = get(running.addr(), &format!("/api/tree?view={view}")).json();
        assert_eq!(tree["ok"], true);
        assert_eq!(tree["data"]["view"], view);
        assert!(tree["data"]["unresolved"].as_array().unwrap().is_empty());
        if view == "combined" {
            assert!(tree["data"]["roots"].as_array().unwrap().is_empty());
        } else {
            assert!(tree["data"]["groups"].as_array().unwrap().is_empty());
        }
    }
    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn unresolved_parent_does_not_suppress_valid_roots() {
    let fixture = populated();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let tree = get(running.addr(), "/api/tree").json();
    let unresolved = tree["data"]["unresolved"].as_array().unwrap();
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0]["id"], "wm-000000000008");
    assert_eq!(unresolved[0]["parent"], "wm-0000000000ff");
    assert_eq!(unresolved[0]["reason"], "parent_missing");
    assert!(root_ids(&tree).contains(&"wm-000000000001".to_owned()));
    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn damaged_document_fails_the_tree_with_code_and_diagnostics() {
    let fixture = populated();
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

    let response = get(running.addr(), "/api/tree");
    assert_eq!(response.status, 500);
    let failure = response.json();
    assert_eq!(failure["ok"], false);
    assert!(
        failure.get("data").is_none(),
        "a failed tree carries no partial data"
    );
    // A damaged file makes the projection incomplete; the tree read refuses to
    // answer from a partial graph rather than presenting it as complete.
    assert_eq!(failure["error"]["code"], "INCOMPLETE_PROJECTION");
    assert!(
        failure["error"]["message"]
            .as_str()
            .unwrap()
            .contains("complete"),
        "message: {}",
        failure["error"]["message"]
    );
    assert!(failure.get("_meta").is_some());

    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn assets_are_served_from_the_configured_directory() {
    let fixture = fixture();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let index = get(running.addr(), "/");
    assert_eq!(index.status, 200);
    assert!(index
        .header("content-type")
        .unwrap()
        .starts_with("text/html"));
    assert!(index.text().contains("Waymark"));

    let script = get(running.addr(), "/assets/_app/app.js");
    assert_eq!(script.status, 200);
    assert!(script
        .header("content-type")
        .unwrap()
        .starts_with("text/javascript"));
    assert_eq!(script.text(), "console.log('waymark');");

    // SvelteKit's generated shell references `/_app/...` directly; that prefix
    // maps to `<assets>/_app/...` rather than to `<assets>/...`.
    let direct = get(running.addr(), "/_app/app.js");
    assert_eq!(direct.status, 200);
    assert!(direct
        .header("content-type")
        .unwrap()
        .starts_with("text/javascript"));
    assert_eq!(direct.text(), "console.log('waymark');");

    running.shutdown().await.unwrap();
}

/// Guard against accidental absolute-path or dot-segment asset escapes.
#[tokio::test(flavor = "multi_thread")]
async fn missing_assets_return_a_clear_unavailable_page_not_a_placeholder() {
    let fixture = fixture();
    let empty = tempfile::tempdir().unwrap();
    let running = start(&fixture, empty.path()).await;

    let response = get(running.addr(), "/");
    assert_eq!(response.status, 503);
    assert!(response.text().contains("assets are missing"));
    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn item_times_use_the_effective_zone_from_the_shared_boundary() {
    let fixture = populated();
    // The Web adapter parses no local document: the zone and the rendered
    // values it reports are exactly what the shared boundary resolves.
    let fallback = waymark_core::ops::local_config::effective_lenient(&fixture.project).timezone;
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let detail = get(running.addr(), "/api/item/wm-000000000009").json();
    let data = &detail["data"];
    assert_eq!(data["display_times"]["timezone"], fallback.name());
    assert_eq!(
        data["display_times"]["created_at"],
        waymark_core::time::format_local(waymark_core::testing::instant().get(), fallback)
    );
    // The persisted fields stay UTC: only the display pair is local.
    assert_eq!(
        data["created_at"],
        waymark_core::time::format_utc(waymark_core::testing::instant().get())
    );
    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_explicit_local_zone_reaches_the_web_view() {
    let fixture = populated();
    waymark_core::ops::local_config::set_scalar(
        &fixture.project,
        waymark_core::ops::config::Scalar::Timezone,
        "Pacific/Auckland",
        &waymark_core::ops::OpOptions::new(),
    )
    .expect("the local zone is written");
    let zone = waymark_core::time::parse_timezone("Pacific/Auckland").expect("a known zone");

    let assets = assets();
    let running = start(&fixture, assets.path()).await;
    let detail = get(running.addr(), "/api/item/wm-000000000009").json();
    let times = &detail["data"]["display_times"];
    assert_eq!(times["timezone"], "Pacific/Auckland");
    assert_eq!(
        times["created_at"],
        waymark_core::time::format_local(waymark_core::testing::instant().get(), zone)
    );
    running.shutdown().await.unwrap();
}
