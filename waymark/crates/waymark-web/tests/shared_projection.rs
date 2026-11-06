//! The shared projection: one snapshot for the whole service lifetime, an
//! inventory check per request, a single-flight refresh, and a stale marker
//! that keeps business reads honest after a failed refresh.
//!
//! These tests drive real HTTP against a live loopback listener, so they
//! observe the contract a browser sees: source changes appear on the next
//! request, and a failed refresh never serves stale data.

mod support;

use support::{assets, fixture, get, item, seed, start, Fixture};

/// One root bug already on disk when the service starts.
fn seeded() -> Fixture {
    let fixture = fixture();
    seed(
        &fixture.project,
        &item("wm-000000000001", "bug", "First bug"),
    );
    fixture
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_after_startup_is_visible_to_the_next_request() {
    let fixture = seeded();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let first = get(running.addr(), "/api/tree");
    assert_eq!(first.status, 200);
    assert!(
        !first.text().contains("Second bug"),
        "the tree starts without the second item"
    );

    // A new document appears after the service started.
    seed(
        &fixture.project,
        &item("wm-000000000002", "task", "Second bug"),
    );

    let second = get(running.addr(), "/api/tree");
    assert_eq!(second.status, 200);
    assert!(
        second.text().contains("Second bug"),
        "the next request sees the new document"
    );

    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edited_document_is_visible_to_the_next_request() {
    let fixture = seeded();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let first = get(running.addr(), "/api/item/wm-000000000001").json();
    assert_eq!(first["data"]["title"], "First bug");

    let mut edited = item("wm-000000000001", "bug", "First bug");
    edited.body = "## Revised\n\nlater\n".to_owned();
    seed(&fixture.project, &edited);

    let second = get(running.addr(), "/api/item/wm-000000000001").json();
    assert_eq!(second["data"]["title"], "First bug");
    assert!(second["data"]["body_html"]
        .as_str()
        .unwrap()
        .contains("Revised"));
    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_refresh_keeps_the_last_snapshot_but_reads_refuse_it() {
    let fixture = seeded();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let tree = get(running.addr(), "/api/tree");
    assert_eq!(tree.status, 200);

    // Break the bug category so the next inventory walk and refresh fail.
    let category = fixture.root().join("work").join("bugs");
    std::fs::remove_dir_all(&category).expect("the category directory is removable");
    let file = std::fs::File::create(&category).expect("a file can replace the directory");
    file.sync_all().expect("the replacement file is durable");

    let failure = get(running.addr(), "/api/tree");
    assert_eq!(failure.status, 500);
    let body = failure.json();
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "INCOMPLETE_PROJECTION");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("complete"),
        "message: {}",
        body["error"]["message"]
    );
    assert!(body.get("_meta").is_some());

    let item_failure = get(running.addr(), "/api/item/wm-000000000001").json();
    assert_eq!(item_failure["ok"], false);
    assert_eq!(item_failure["error"]["code"], "INCOMPLETE_PROJECTION");

    // Remove the obstruction; the next request refreshes successfully again.
    std::fs::remove_file(&category).expect("the replacement file is removable");
    let recovered = get(running.addr(), "/api/tree");
    assert_eq!(recovered.status, 200);

    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_broken_document_does_not_prevent_startup_or_item_reads() {
    let fixture = seeded();
    // A document with no id/title/status parses as a failure: the projection
    // starts scoped, never preventing the service from serving.
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

    let health = get(running.addr(), "/api/health");
    assert_eq!(health.status, 200);

    let tree = get(running.addr(), "/api/tree");
    assert_eq!(tree.status, 500);
    assert_eq!(tree.json()["error"]["code"], "INCOMPLETE_PROJECTION");

    // The scoped read still answers for a parsed document.
    let detail = get(running.addr(), "/api/item/wm-000000000001").json();
    assert_eq!(detail["ok"], true);
    assert_eq!(detail["data"]["title"], "First bug");
    assert_eq!(detail["_meta"]["completeness"], "scoped");

    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_reads_share_one_refresh_and_see_one_published_tree() {
    let fixture = seeded();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    // Warm the snapshot, then change the sources under it.
    let warm = get(running.addr(), "/api/tree");
    assert_eq!(warm.status, 200);
    seed(
        &fixture.project,
        &item("wm-000000000002", "task", "Second bug"),
    );

    // N parallel requests all observe the change and all agree. The client is
    // synchronous, so the burst runs on plain threads — leaving the tokio
    // workers free to drive the server's async accept loop.
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..16 {
            let addr = running.addr();
            handles.push(scope.spawn(move || get(addr, "/api/tree")));
        }
        for handle in handles {
            let response = handle.join().expect("the parallel request completes");
            assert_eq!(response.status, 200);
            assert!(
                response.text().contains("Second bug"),
                "every concurrent read sees the refreshed tree"
            );
        }
    });

    running.shutdown().await.unwrap();
}
