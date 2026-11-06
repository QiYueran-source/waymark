//! Protocol and protection tests: malformed requests, the absence of writes,
//! Host validation, and Origin validation.

mod support;

use support::{assets, fixture, get, item, raw, request, seed, snapshot, start, Fixture};

fn one_item() -> Fixture {
    let fixture = fixture();
    seed(
        &fixture.project,
        &item("wm-000000000001", "bug", "A readable item"),
    );
    fixture
}

#[tokio::test(flavor = "multi_thread")]
async fn every_non_get_or_head_method_is_405() {
    let fixture = one_item();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    for method in ["POST", "PUT", "PATCH", "DELETE", "OPTIONS", "TRACE"] {
        for path in [
            "/api/tree",
            "/api/status",
            "/api/item/wm-000000000001",
            "/api/health",
            "/",
            "/assets/index.html",
            "/api/does-not-exist",
        ] {
            let response = request(running.addr(), method, path, &[]);
            assert_eq!(
                response.status,
                405,
                "{method} {path} must be rejected with 405, body: {}",
                response.text()
            );
            assert!(
                response.header("allow").unwrap_or_default().contains("GET"),
                "{method} {path} must advertise the allowed methods"
            );
            assert_eq!(response.json()["error"]["code"], "METHOD_NOT_ALLOWED");
        }
    }
    // The method refusal precedes authority validation: no exception to "every
    // non-GET/HEAD is 405", even from a hostile Host.
    let hostile = format!("evil.example:{}", running.addr().port());
    let refused = request(
        running.addr(),
        "POST",
        "/api/tree",
        &[("Host", hostile.as_str())],
    );
    assert_eq!(refused.status, 405);
    assert_eq!(refused.json()["error"]["code"], "METHOD_NOT_ALLOWED");

    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn reads_and_rejected_writes_leave_the_project_byte_identical() {
    let fixture = one_item();
    let before = snapshot(fixture.root());
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    for path in [
        "/",
        "/assets/_app/app.js",
        "/api/health",
        "/api/status",
        "/api/tree",
        "/api/tree?view=grouped",
        "/api/item/wm-000000000001",
        "/api/item/wm-000000000001?ignored=1",
    ] {
        assert_eq!(get(running.addr(), path).status, 200, "{path}");
    }
    for method in ["POST", "PUT", "PATCH", "DELETE"] {
        assert_eq!(
            request(running.addr(), method, "/api/tree", &[]).status,
            405
        );
        assert_eq!(
            request(running.addr(), method, "/api/item/wm-000000000001", &[]).status,
            405
        );
    }

    assert_eq!(
        before,
        snapshot(fixture.root()),
        "the read-only surface must not change one byte on disk"
    );
    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn malformed_paths_and_queries_are_json_errors() {
    let fixture = one_item();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let bad_view = get(running.addr(), "/api/tree?view=bogus");
    assert_eq!(bad_view.status, 400);
    assert_eq!(bad_view.json()["error"]["code"], "INVALID_QUERY");

    let repeated_view = get(running.addr(), "/api/tree?view=combined&view=grouped");
    assert_eq!(repeated_view.status, 400);
    assert_eq!(repeated_view.json()["error"]["code"], "INVALID_QUERY");

    let bad_id = get(running.addr(), "/api/item/not-an-id");
    assert_eq!(bad_id.status, 400);
    assert_eq!(bad_id.json()["error"]["code"], "INVALID_ID");

    let missing_id = get(running.addr(), "/api/item/wm-0000000000ff");
    assert_eq!(missing_id.status, 404);
    assert_eq!(missing_id.json()["error"]["code"], "ITEM_NOT_FOUND");

    let unknown = get(running.addr(), "/api/nope");
    assert_eq!(unknown.status, 404);
    assert_eq!(unknown.json()["error"]["code"], "NOT_FOUND");

    let traversal = get(running.addr(), "/assets/%2e%2e/Cargo.toml");
    assert_eq!(traversal.status, 400);
    assert_eq!(traversal.json()["error"]["code"], "INVALID_ASSET_PATH");

    let missing_asset = get(running.addr(), "/assets/_app/nope.js");
    assert_eq!(missing_asset.status, 404);
    assert_eq!(missing_asset.json()["error"]["code"], "ASSET_NOT_FOUND");

    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn host_must_be_loopback_at_the_listening_port() {
    let fixture = one_item();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;
    let addr = running.addr();
    let port = addr.port();
    let localhost = format!("localhost:{port}");
    let ipv4 = format!("127.0.0.1:{port}");
    let ipv6 = format!("[::1]:{port}");
    let foreign = format!("evil.example:{port}");
    let wrong_port = format!("localhost:{}", port.wrapping_add(1));

    for (host, expected) in [
        (localhost.as_str(), 200),
        (ipv4.as_str(), 200),
        (ipv6.as_str(), 200),
    ] {
        let response = request(addr, "GET", "/api/health", &[("Host", host)]);
        assert_eq!(response.status, expected, "Host: {host}");
    }

    for host in [foreign.as_str(), wrong_port.as_str()] {
        for path in ["/api/health", "/", "/assets/index.html"] {
            let response = request(addr, "GET", path, &[("Host", host)]);
            assert_eq!(response.status, 400, "Host: {host} on {path}");
            assert_eq!(response.json()["error"]["code"], "INVALID_HOST");
        }
    }

    // A request without a Host header cannot pass HTTP/1.1 validation.
    let no_host = raw(
        addr,
        "GET /api/health HTTP/1.1\r\nConnection: close\r\n\r\n",
    );
    assert_eq!(no_host.status, 400);

    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn origin_must_match_the_service_origin_when_present() {
    let fixture = one_item();
    let assets = assets();
    let running = start(&fixture, assets.path()).await;
    let addr = running.addr();
    let port = addr.port();
    let same = format!("http://localhost:{port}");
    let wrong_port = format!("http://localhost:{}", port.wrapping_add(1));
    let foreign = format!("http://evil.example:{port}");
    let https = format!("https://localhost:{port}");
    let other_host = format!("http://127.0.0.1:{port}");

    for path in ["/api/health", "/api/tree", "/", "/assets/_app/app.js"] {
        let response = request(addr, "GET", path, &[("Origin", same.as_str())]);
        assert_eq!(response.status, 200, "matching Origin on {path}");

        // No Origin header at all: allowed after Host validation.
        let response = get(addr, path);
        assert_eq!(response.status, 200, "absent Origin on {path}");

        for origin in [
            "null".to_owned(),
            wrong_port.clone(),
            foreign.clone(),
            https.clone(),
            other_host.clone(),
        ] {
            let response = request(addr, "GET", path, &[("Origin", origin.as_str())]);
            assert_eq!(response.status, 400, "Origin: {origin} on {path}");
            assert_eq!(response.json()["error"]["code"], "INVALID_ORIGIN");
        }
    }

    running.shutdown().await.unwrap();
}
