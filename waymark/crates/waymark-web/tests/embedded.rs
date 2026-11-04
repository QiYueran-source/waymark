//! The embedded frontend, served over real HTTP.
//!
//! These tests only exist for a build with the `embed` feature: an ordinary
//! build has no frontend to serve and must not acquire one implicitly. They run
//! against the tree the build script embedded, so they prove the same bytes the
//! release ships, not a fixture.

#![cfg(feature = "embed")]

mod support;

use std::io::Read;

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use support::{fixture, get, request};

fn gunzip(body: &[u8]) -> Vec<u8> {
    let mut decoder = GzDecoder::new(body);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .expect("an embedded asset is served gzip-encoded");
    out
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// The first root-absolute `/_app/...` URL the shell points at.
fn first_app_url(html: &str) -> String {
    for attribute in ["src=\"", "href=\""] {
        for rest in html.split(attribute).skip(1) {
            let Some(end) = rest.find('"') else {
                continue;
            };
            let value = &rest[..end];
            if value.starts_with("/_app/") {
                return value.to_owned();
            }
        }
    }
    panic!("the shell must reference a hashed /_app asset: {html}");
}

fn entry<'a>(manifest: &'a waymark_web::AssetManifest, path: &str) -> &'a waymark_web::AssetFile {
    manifest
        .files
        .iter()
        .find(|file| file.path == path)
        .unwrap_or_else(|| panic!("the manifest must list {path}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn the_embedded_shell_and_assets_are_served_and_match_the_manifest() {
    let manifest =
        waymark_web::embedded_manifest().expect("an embedded build exposes its manifest");
    assert_eq!(
        manifest.version,
        env!("CARGO_PKG_VERSION"),
        "the manifest belongs to this package version"
    );
    assert!(
        manifest
            .files
            .windows(2)
            .all(|pair| pair[0].path < pair[1].path),
        "the manifest lists paths in sorted order"
    );

    let fixture = fixture();
    let running = waymark_web::bind(
        fixture.project.clone(),
        waymark_web::WebConfig::embedded().with_port(0),
    )
    .await
    .expect("the loopback listener binds");

    let index = get(running.addr(), "/");
    assert_eq!(index.status, 200, "GET / must serve the embedded shell");
    assert_eq!(
        index.header("content-encoding"),
        Some("gzip"),
        "embedded assets are served with Content-Encoding: gzip"
    );
    let content_type = index.header("content-type").unwrap_or_default();
    assert!(
        content_type.starts_with("text/html"),
        "GET / served {content_type:?}, not HTML"
    );
    let html_bytes = gunzip(&index.body);
    let index_entry = entry(manifest, "index.html");
    assert_eq!(
        sha256_hex(&html_bytes),
        index_entry.sha256,
        "the served index.html bytes hash to the manifest's entry"
    );
    assert_eq!(html_bytes.len() as u64, index_entry.size);
    let html = String::from_utf8(html_bytes).expect("the shell is UTF-8");
    assert!(
        html.contains("<!doctype html>") || html.contains("<!DOCTYPE html>"),
        "the shell is an HTML document"
    );

    let url = first_app_url(&html);
    let asset = get(running.addr(), &url);
    assert_eq!(
        asset.status, 200,
        "GET {url} must resolve from the embedded tree"
    );
    assert_eq!(asset.header("content-encoding"), Some("gzip"));
    let asset_bytes = gunzip(&asset.body);
    let asset_path = url.trim_start_matches('/');
    let asset_entry = entry(manifest, asset_path);
    assert_eq!(sha256_hex(&asset_bytes), asset_entry.sha256);
    assert_eq!(asset_bytes.len() as u64, asset_entry.size);
    let asset_type = asset.header("content-type").unwrap_or_default();
    let expected = if url.ends_with(".js") {
        "text/javascript"
    } else if url.ends_with(".css") {
        "text/css"
    } else {
        "application/octet-stream"
    };
    assert!(
        asset_type.starts_with(expected),
        "GET {url} served {asset_type:?}, expected {expected}"
    );

    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_non_get_method_is_405_and_a_foreign_host_is_rejected() {
    let fixture = fixture();
    let running = waymark_web::bind(
        fixture.project.clone(),
        waymark_web::WebConfig::embedded().with_port(0),
    )
    .await
    .expect("the loopback listener binds");

    let refused = request(running.addr(), "POST", "/", &[]);
    assert_eq!(refused.status, 405);
    assert_eq!(refused.json()["error"]["code"], "METHOD_NOT_ALLOWED");

    let hostile = format!("evil.example:{}", running.addr().port());
    let rejected = request(running.addr(), "GET", "/", &[("Host", hostile.as_str())]);
    assert_eq!(rejected.status, 400);
    assert_eq!(rejected.json()["error"]["code"], "INVALID_HOST");

    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_embedded_path_is_404() {
    let fixture = fixture();
    let running = waymark_web::bind(
        fixture.project.clone(),
        waymark_web::WebConfig::embedded().with_port(0),
    )
    .await
    .expect("the loopback listener binds");

    let missing = get(running.addr(), "/_app/does-not-exist.js");
    assert_eq!(missing.status, 404);
    assert_eq!(missing.json()["error"]["code"], "ASSET_NOT_FOUND");
    assert_eq!(missing.header("content-encoding"), None);

    running.shutdown().await.unwrap();
}
