//! Read and query tools over the in-process MCP client.

mod common;

use serde_json::{json, Value};
use waymark_core::query::{DEFAULT_LIMIT, MCP_DEFAULT_LIMIT};

fn ids(envelope: &Value) -> Vec<String> {
    envelope["data"]["items"]
        .as_array()
        .expect("a page has an items array")
        .iter()
        .map(|row| row["id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn get_work_item_returns_the_item_with_relationships_and_archive_metadata() {
    let fixture = common::fixture(&["parent", "child"]);
    let project = fixture.project();
    let parent = fixture.ids[0].clone();
    let child = fixture.ids[1].clone();
    common::set_parent(&project, &child, &parent);
    common::depend(&project, &child, &parent);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call("waymark_get_work_item", json!({"id": child}))
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    let data = &envelope["data"];
    assert_eq!(data["id"], child.as_str());
    assert_eq!(data["item"]["title"], "child");
    assert_eq!(data["item"]["owner"], "frank");
    assert_eq!(data["category"], "task");
    assert!(data["fingerprint"].is_string());
    assert_eq!(data["relationships"]["parent"], parent.as_str());
    assert_eq!(data["relationships"]["dependencies"], json!([parent]));
    assert_eq!(data["relationships"]["historical"], false);
    assert_eq!(data["archived"], false);
    assert_eq!(data["ready"], false, "a dependency on an open item blocks");
    assert_eq!(data["blocked"], true);
    assert_eq!(data["blocked_by"], json!([parent]));
    assert_eq!(envelope["_meta"]["source"], "direct_file");
    assert_eq!(envelope["_meta"]["completeness"], "scoped");

    client.shutdown().await;
}

#[tokio::test]
async fn get_work_item_reports_a_one_sided_soft_link_without_a_repair_command() {
    let fixture = common::fixture(&["records the link", "never records it"]);
    let here = fixture.ids[0].clone();
    let silent = fixture.ids[1].clone();
    let path = common::task_path(&fixture.root, &here);
    let mut item = waymark_core::parse::parse_item_file(&path).expect("the document parses");
    item.soft_links = vec![waymark_core::WorkItemId::parse(&silent).unwrap()];
    std::fs::write(&path, waymark_core::serialize::to_document(&item)).unwrap();
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call("waymark_get_work_item", json!({"id": here}))
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true, "{envelope}");
    let issues = envelope["data"]["relationships"]["soft_link_issues"]
        .as_array()
        .cloned()
        .unwrap_or_else(|| panic!("{envelope}"));
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0]["partner"], silent.as_str());
    assert_eq!(issues[0]["missing"], "link");
    assert!(
        !issues[0].to_string().contains("waymark "),
        "the diagnosis carries no runnable repair: {}",
        issues[0]
    );

    client.shutdown().await;
}

#[tokio::test]
async fn doctor_reports_a_reference_to_an_archived_item_as_the_shared_finding() {
    let fixture = common::fixture(&["retired", "still points here"]);
    let retired = fixture.ids[0].clone();
    let user = fixture.ids[1].clone();
    let project = fixture.project();
    common::depend(&project, &user, &retired);
    waymark_core::ops::archive::archive(&project, &retired, None, &common::options())
        .expect("a non-cascading archive leaves the reference alone");
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client.call("waymark_doctor", json!({})).await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true, "{envelope}");
    let findings = envelope["data"]["findings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let dangling: Vec<&Value> = findings
        .iter()
        .filter(|finding| finding["code"] == "DANGLING_REFERENCE")
        .collect();
    assert_eq!(dangling.len(), 1, "{findings:#?}");
    assert_eq!(dangling[0]["fixable"], false);
    assert!(
        dangling[0]["message"].as_str().unwrap().contains(&retired),
        "{dangling:?}"
    );
    assert!(
        !dangling[0].to_string().contains("waymark "),
        "the diagnosis carries no runnable repair: {}",
        dangling[0]
    );
    assert!(
        !findings
            .iter()
            .any(|finding| finding["code"] == "ONE_SIDED_SOFT_LINK"),
        "an archived partner is not a one-sided link: {findings:#?}"
    );

    client.shutdown().await;
}

#[tokio::test]
async fn list_returns_a_page_with_filters_projection_and_meta() {
    let fixture = common::fixture(&["alpha", "bravo", "charlie"]);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_list_work_items",
            json!({"filter": {"owner": "frank"}, "fields": ["id", "title"]}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["count"], 3);
    assert_eq!(envelope["data"]["total"], 3);
    assert_eq!(envelope["data"]["offset"], 0);
    assert_eq!(envelope["data"]["limit"], 50, "the MCP default limit");
    assert_eq!(envelope["data"]["archive"], "active");
    assert_eq!(envelope["data"]["fields"], json!(["id", "title"]));
    assert_eq!(envelope["data"]["compact"], false);
    let rows = envelope["data"]["items"].as_array().unwrap();
    assert!(
        rows[0]["item"].get("owner").is_none(),
        "a field projection omits unselected frontmatter"
    );
    assert_eq!(envelope["_meta"]["source"], "memory_projection");
    assert_eq!(envelope["_meta"]["completeness"], "complete");

    let result = client
        .call(
            "waymark_list_work_items",
            json!({"limit": 1, "offset": 1, "sort": "title"}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["count"], 1);
    assert_eq!(envelope["data"]["total"], 3);
    assert_eq!(ids(&envelope), vec![fixture.ids[1].clone()]);

    let result = client
        .call("waymark_list_work_items", json!({"archive": "archived"}))
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["count"], 0, "no archived items exist");
    assert_eq!(envelope["data"]["archive"], "archived");

    client.shutdown().await;
}

#[tokio::test]
async fn a_closed_parent_reaches_the_mcp_list_and_ready_rows() {
    let fixture = common::fixture(&["parent", "child"]);
    let project = fixture.project();
    let parent = fixture.ids[0].clone();
    let child = fixture.ids[1].clone();
    common::set_parent(&project, &child, &parent);
    waymark_core::ops::status::set_status(
        &project,
        &parent,
        waymark_core::Status::Closed,
        &common::options(),
    )
    .expect("the parent closes");
    let client = common::Client::connect(fixture.root.clone()).await;

    let row_of = |envelope: &Value, id: &str| {
        envelope["data"]["items"]
            .as_array()
            .expect("a page has an items array")
            .iter()
            .find(|row| row["id"] == json!(id))
            .unwrap_or_else(|| panic!("{id} is listed"))
            .clone()
    };

    for (tool, arguments) in [
        ("waymark_list_work_items", json!({"limit": 100})),
        ("waymark_get_ready_items", json!({"limit": 100})),
    ] {
        let result = client.call(tool, arguments).await;
        let envelope = common::envelope(&result);
        let row = row_of(&envelope, &child);
        assert_eq!(row["closed_parent"], json!(parent), "{tool}");
        assert_eq!(row["ready"], json!(true), "{tool}");
    }

    client.shutdown().await;
}

#[tokio::test]
async fn the_mcp_list_default_is_50_while_the_cli_default_is_100() {
    assert_eq!(MCP_DEFAULT_LIMIT, 50);
    assert_eq!(DEFAULT_LIMIT, 100);

    let titles: Vec<String> = (0..60).map(|index| format!("item {index:02}")).collect();
    let titles: Vec<&str> = titles.iter().map(String::as_str).collect();
    let fixture = common::fixture(&titles);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call("waymark_list_work_items", common::no_args())
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["count"], 50);
    assert_eq!(envelope["data"]["limit"], 50);
    assert_eq!(envelope["data"]["total"], 60);

    let result = client
        .call("waymark_list_work_items", json!({"limit": 60}))
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["count"], 60);

    // The same default applies to the derived collection tools.
    for tool in ["waymark_get_ready_items", "waymark_get_blocked_items"] {
        let result = client.call(tool, common::no_args()).await;
        let envelope = common::envelope(&result);
        assert_eq!(envelope["data"]["limit"], 50, "{tool}");
    }

    client.shutdown().await;
}

#[tokio::test]
async fn search_returns_title_hits_first_without_snippets() {
    let fixture = common::fixture_with(vec![
        ("needle in the title", None),
        ("body only", Some("the needle is in the body")),
        ("unrelated", None),
    ]);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call("waymark_search_work_items", json!({"text": "needle"}))
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["count"], 2);
    assert_eq!(envelope["data"]["total"], 2);
    assert_eq!(envelope["data"]["text"], "needle");
    assert_eq!(envelope["data"]["snippets"], false);
    assert_eq!(envelope["data"]["limit"], 50);
    let rows = envelope["data"]["items"].as_array().unwrap();
    assert_eq!(rows[0]["title_hit"], true);
    assert_eq!(rows[1]["title_hit"], false);
    assert_eq!(
        ids(&envelope),
        vec![fixture.ids[0].clone(), fixture.ids[1].clone()]
    );
    assert!(
        rows.iter().all(|row| row.get("body").is_none()),
        "search never returns body text"
    );
    assert_eq!(envelope["_meta"]["completeness"], "complete");

    client.shutdown().await;
}

#[tokio::test]
async fn tree_and_dependency_tree_return_the_bounded_shared_structures() {
    let fixture = common::fixture(&["root", "child", "dependent"]);
    let project = fixture.project();
    common::set_parent(&project, &fixture.ids[1], &fixture.ids[0]);
    common::set_parent(&project, &fixture.ids[2], &fixture.ids[1]);
    common::depend(&project, &fixture.ids[2], &fixture.ids[1]);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client.call("waymark_get_tree", common::no_args()).await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    let roots = envelope["data"]["roots"].as_array().unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0]["id"], fixture.ids[0].as_str());
    assert_eq!(roots[0]["children"][0]["id"], fixture.ids[1].as_str());
    assert_eq!(
        roots[0]["children"][0]["children"][0]["id"],
        fixture.ids[2].as_str()
    );
    assert_eq!(envelope["_meta"]["completeness"], "complete");

    let result = client
        .call(
            "waymark_get_tree",
            json!({"root": fixture.ids[0], "depth": 0}),
        )
        .await;
    let envelope = common::envelope(&result);
    let roots = envelope["data"]["roots"].as_array().unwrap();
    assert_eq!(roots[0]["id"], fixture.ids[0].as_str());
    assert!(roots[0]["children"].as_array().unwrap().is_empty());

    let result = client
        .call("waymark_get_dependency_tree", json!({"id": fixture.ids[2]}))
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["data"]["id"], fixture.ids[2].as_str());
    assert_eq!(envelope["data"]["flat"], false);
    assert_eq!(envelope["data"]["tree"]["id"], fixture.ids[2].as_str());
    assert_eq!(
        envelope["data"]["tree"]["children"][0]["id"],
        fixture.ids[1].as_str()
    );
    assert_eq!(envelope["data"]["rows"], json!([]));

    let result = client
        .call(
            "waymark_get_dependency_tree",
            json!({"id": fixture.ids[2], "flat": true, "depth": 1}),
        )
        .await;
    let envelope = common::envelope(&result);
    let rows = envelope["data"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["depth"], 0);
    assert_eq!(rows[0]["from"], Value::Null);
    assert_eq!(rows[1]["depth"], 1);
    assert_eq!(rows[1]["from"], fixture.ids[2].as_str());

    client.shutdown().await;
}

#[tokio::test]
async fn ready_and_blocked_items_share_the_collection_arguments() {
    let fixture = common::fixture(&["blocker", "waiting"]);
    let project = fixture.project();
    common::depend(&project, &fixture.ids[1], &fixture.ids[0]);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_get_ready_items",
            json!({"filter": {"owner": "frank"}}),
        )
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    assert_eq!(ids(&envelope), vec![fixture.ids[0].clone()]);

    let result = client
        .call("waymark_get_blocked_items", common::no_args())
        .await;
    let envelope = common::envelope(&result);
    assert_eq!(ids(&envelope), vec![fixture.ids[1].clone()]);
    let row = &envelope["data"]["items"][0];
    assert_eq!(row["blocked_by"], json!([fixture.ids[0]]));
    assert_eq!(envelope["data"]["archive"], "active");
    assert_eq!(envelope["_meta"]["completeness"], "complete");

    client.shutdown().await;
}

#[tokio::test]
async fn stats_reports_counts_only() {
    let fixture = common::fixture(&["one", "two", "three"]);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client.call("waymark_get_stats", common::no_args()).await;
    let envelope = common::envelope(&result);
    assert_eq!(envelope["ok"], true);
    let data = &envelope["data"];
    assert_eq!(data["total"], 3);
    assert_eq!(data["by_status"]["open"], 3);
    assert_eq!(data["by_status"]["closed"], 0);
    assert_eq!(data["by_category"]["task"], 3);
    assert_eq!(data["ready"], 3);
    assert_eq!(data["blocked"], 0);
    assert_eq!(data["counts_only"], true);
    assert_eq!(data["archive"], "active");
    assert!(data.get("items").is_none(), "stats never returns item rows");
    assert_eq!(envelope["_meta"]["completeness"], "complete");

    client.shutdown().await;
}

/// Search is not a reduced surface: the archive range and the filter document
/// select the population exactly as they do for the list tools.
#[tokio::test]
async fn search_follows_the_archive_range_and_the_filter_document() {
    let fixture = common::fixture_with(vec![
        ("needle in a closed item", None),
        ("needle in an archived item", None),
    ]);
    let client = common::Client::connect(fixture.root.clone()).await;

    for (tool, arguments) in [
        (
            "waymark_set_status",
            json!({"id": fixture.ids[0], "status": "closed"}),
        ),
        ("waymark_archive_work_item", json!({"id": fixture.ids[1]})),
    ] {
        let envelope = common::envelope(&client.call(tool, arguments).await);
        assert_eq!(envelope["ok"], true, "{tool}: {envelope}");
    }

    // Default range is active: the closed item is still active work, the
    // archived one is not.
    let active = common::envelope(
        &client
            .call("waymark_search_work_items", json!({"text": "needle"}))
            .await,
    );
    assert_eq!(active["data"]["count"], 1, "{active}");
    assert_eq!(ids(&active), vec![fixture.ids[0].clone()]);

    // A document status filter narrows the same population.
    let closed = common::envelope(
        &client
            .call(
                "waymark_search_work_items",
                json!({"text": "needle", "filter": {"status": "closed"}}),
            )
            .await,
    );
    assert_eq!(closed["data"]["count"], 1, "{closed}");
    assert_eq!(ids(&closed), vec![fixture.ids[0].clone()]);

    let open = common::envelope(
        &client
            .call(
                "waymark_search_work_items",
                json!({"text": "needle", "filter": {"status": "open"}}),
            )
            .await,
    );
    assert_eq!(open["data"]["count"], 0, "{open}");

    // An archived-only range is reachable and returns only the archived item.
    let archived = common::envelope(
        &client
            .call(
                "waymark_search_work_items",
                json!({"text": "needle", "archive": "archived"}),
            )
            .await,
    );
    assert_eq!(archived["data"]["count"], 1, "{archived}");
    assert_eq!(ids(&archived), vec![fixture.ids[1].clone()]);

    let all = common::envelope(
        &client
            .call(
                "waymark_search_work_items",
                json!({"text": "needle", "archive": "all"}),
            )
            .await,
    );
    assert_eq!(all["data"]["count"], 2, "{all}");

    client.shutdown().await;
}
