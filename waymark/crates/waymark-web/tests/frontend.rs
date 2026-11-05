//! The built frontend, served over real HTTP.
//!
//! `ui/build` is a local build artifact and is deliberately absent from a clean
//! checkout and from the repository, so these tests skip (with a message) when
//! it is missing: ordinary Rust compilation and `cargo test` never run npm and
//! never require frontend assets. When a developer has run `npm run build`,
//! this proves that the shell and every file it references are actually served
//! by the read-only service.

mod support;

use std::path::PathBuf;

use support::{fixture, get, start};

fn build_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("ui")
        .join("build")
}

/// Every root-relative `/_app/...` URL an attribute in the shell points at.
fn referenced_app_urls(html: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for attribute in ["src=\"", "href=\""] {
        for rest in html.split(attribute).skip(1) {
            let Some(end) = rest.find('"') else {
                continue;
            };
            let value = &rest[..end];
            if value.starts_with("/_app/") && !urls.iter().any(|url| url == value) {
                urls.push(value.to_owned());
            }
        }
    }
    urls
}

/// The content type an asset URL must be served with.
fn expected_content_type(url: &str) -> &'static str {
    if url.ends_with(".js") {
        "text/javascript"
    } else if url.ends_with(".css") {
        "text/css"
    } else if url.ends_with(".json") {
        "application/json"
    } else if url.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "application/octet-stream"
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_built_shell_and_every_asset_it_references_are_served() {
    let build = build_dir();
    if !build.join("index.html").is_file() {
        eprintln!(
            "skipping: {} has no index.html; run `npm run build` in crates/waymark-web/ui",
            build.display()
        );
        return;
    }

    let fixture = fixture();
    let running = start(&fixture, &build).await;

    let index = get(running.addr(), "/");
    assert_eq!(index.status, 200, "GET / must serve the built shell");
    let content_type = index.header("content-type").unwrap_or_default().to_owned();
    assert!(
        content_type.starts_with("text/html"),
        "GET / served {content_type:?}, not HTML"
    );
    let html = index.text();
    assert!(html.contains("<!doctype html>"));
    assert!(
        html.contains("Waymark"),
        "the shell must name the product: {html}"
    );
    assert!(
        !html.contains("http://") && !html.contains("https://"),
        "the shell must not reference a remote origin: {html}"
    );

    let urls = referenced_app_urls(&html);
    assert!(
        urls.iter().any(|url| url.ends_with(".js")),
        "the shell must reference at least one hashed script: {urls:?}"
    );
    assert!(
        urls.iter().any(|url| url.ends_with(".css")),
        "the shell must reference its stylesheet: {urls:?}"
    );

    for url in &urls {
        let response = get(running.addr(), url);
        assert_eq!(response.status, 200, "GET {url} must serve the built file");
        let content_type = response
            .header("content-type")
            .unwrap_or_default()
            .to_owned();
        let expected = expected_content_type(url);
        assert!(
            content_type.starts_with(expected),
            "GET {url} served {content_type:?}, expected {expected}"
        );
        assert!(!response.body.is_empty(), "GET {url} served an empty body");
    }

    running.shutdown().await.unwrap();
}
