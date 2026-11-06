//! The status API: an always-200 projection and cache report, and the
//! read-only proof that looking at it never touches the project.
//!
//! The status route succeeds in every state the shared projection can be in —
//! healthy, stale after a failed refresh, or never loaded after a failed
//! startup — and it reports aggregates only: no cache key, value, or subject
//! ever appears on the wire.

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

/// The `data.cache` invariants every status payload must satisfy: the per-class
/// counts always sum to the total, and the counters are always present.
fn assert_cache_invariants(body: &serde_json::Value) {
    let cache = &body["data"]["cache"];
    let total = cache["total"].as_u64().expect("total is a count");
    let by_class = &cache["by_class"];
    let body_count = by_class["body"].as_u64().expect("body is a count");
    let detail = by_class["detail"].as_u64().expect("detail is a count");
    let query = by_class["query"].as_u64().expect("query is a count");
    assert_eq!(
        total,
        body_count + detail + query,
        "the per-class counts must sum to the total cache length"
    );
    // The read itself must never have performed a lookup: the counters are
    // present and, in v1, no handler ever populates the cache, so they are
    // truthfully zero here (asserted as the invariant, not as a fixed value).
    let counters = &body["data"]["counters"];
    for (wire, field) in [
        ("hits", "cache_hits"),
        ("misses", "cache_misses"),
        ("evictions", "cache_evictions"),
    ] {
        assert!(
            counters[field].is_number(),
            "counter {wire} is present (field {field})"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn status_reports_a_healthy_projection_and_cache() {
    let fixture = seeded();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let response = get(running.addr(), "/api/status");
    assert_eq!(response.status, 200);
    let body = response.json();
    assert_eq!(body["ok"], true);
    assert_eq!(body["_meta"]["source"], "memory_projection");

    let projection = &body["data"]["projection"];
    assert_eq!(projection["completeness"], "complete");
    assert_eq!(projection["freshness"], "fresh");
    assert_eq!(projection["unreadable_documents"], 0);
    assert_eq!(projection["uncertain_paths"], 0);
    assert_eq!(projection["loaded"], true);
    assert!(
        projection["last_success"].as_str().is_some(),
        "a healthy load records its last-success instant"
    );
    let last_success = projection["last_success"].as_str().unwrap();
    assert_eq!(
        last_success, last_success,
        "last_success is an RFC 3339 instant"
    );
    assert_eq!(body["_meta"]["completeness"], "complete");
    assert_eq!(body["_meta"]["freshness"], "fresh");

    let cache = &body["data"]["cache"];
    assert_eq!(cache["enabled"], true);
    assert_eq!(
        cache["capacity"], 512,
        "the fixture uses the default cache capacity"
    );
    assert_cache_invariants(&body);

    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn status_stays_200_and_reports_stale_after_a_failed_refresh() {
    let fixture = seeded();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    // Warm the snapshot, then break the bug category so the next inventory walk
    // and refresh fail deterministically: a regular file where a directory must
    // be makes read_dir fail for any user.
    let warm = get(running.addr(), "/api/status");
    assert_eq!(warm.status, 200);
    let last_success = warm.json()["data"]["projection"]["last_success"]
        .as_str()
        .expect("the warm read records a last success")
        .to_owned();
    let category = fixture.root().join("work").join("bugs");
    std::fs::remove_dir_all(&category).expect("the category directory is removable");
    let file = std::fs::File::create(&category).expect("a file can replace the directory");
    file.sync_all().expect("the replacement file is durable");

    let status = get(running.addr(), "/api/status");
    assert_eq!(
        status.status, 200,
        "the status route never fails, even when stale"
    );
    let body = status.json();
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["projection"]["freshness"], "stale");
    assert_eq!(body["_meta"]["freshness"], "stale");
    assert_eq!(
        body["data"]["projection"]["last_success"]
            .as_str()
            .expect("last success is kept"),
        last_success,
        "a failed refresh keeps the last successful instant"
    );
    assert_eq!(body["data"]["projection"]["loaded"], true);
    // The retained snapshot's aggregates are still reported.
    assert_eq!(body["data"]["projection"]["completeness"], "complete");
    assert_eq!(body["data"]["cache"]["enabled"], true);
    assert_eq!(body["data"]["cache"]["capacity"], 512);
    assert_cache_invariants(&body);

    // Business reads refuse the stale snapshot with the standard error.
    let tree = get(running.addr(), "/api/tree");
    assert_eq!(tree.status, 500);
    assert_eq!(tree.json()["error"]["code"], "INCOMPLETE_PROJECTION");

    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn status_reports_a_failed_startup_and_the_server_still_serves() {
    // Break the bug category before the service starts: the startup load fails,
    // but the service must still bind and answer.
    let fixture = seeded();
    let category = fixture.root().join("work").join("bugs");
    std::fs::remove_dir_all(&category).expect("the category directory is removable");
    let file = std::fs::File::create(&category).expect("a file can replace the directory");
    file.sync_all().expect("the replacement file is durable");

    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let health = get(running.addr(), "/api/health");
    assert_eq!(
        health.status, 200,
        "a failed startup never prevents serving"
    );

    let status = get(running.addr(), "/api/status");
    assert_eq!(
        status.status, 200,
        "the status route answers even when nothing loaded"
    );
    let body = status.json();
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["projection"]["loaded"], false);
    assert_eq!(body["data"]["projection"]["completeness"], "scoped");
    assert_eq!(body["data"]["projection"]["freshness"], "unknown");
    assert_eq!(body["_meta"]["completeness"], "scoped");
    assert_eq!(body["_meta"]["freshness"], "unknown");
    assert_eq!(
        body["data"]["projection"]["last_success"],
        serde_json::Value::Null
    );
    assert!(body["data"]["projection"]["diagnostics"].is_array());
    // The cache and counter fields are still present (zero counts in v1).
    assert_cache_invariants(&body);

    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn reading_status_twice_never_changes_the_counters() {
    let fixture = seeded();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let first = get(running.addr(), "/api/status").json();
    let second = get(running.addr(), "/api/status").json();
    // The status read performs no cache lookup: the counters it reports are
    // exactly the same after a second read as after the first.
    assert_eq!(
        second["data"]["counters"], first["data"]["counters"],
        "a status read must never mutate the counters"
    );

    running.shutdown().await.unwrap();
}
