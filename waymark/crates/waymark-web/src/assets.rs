//! The frontend asset seam.
//!
//! `GET /` serves `<assets>/index.html`; `GET /assets/<path>` serves
//! `<assets>/<path>`, and `GET /_app/<path>` serves `<assets>/_app/<path>`
//! because that is where SvelteKit writes its hashed build output. The directory
//! is chosen by the launcher (`--assets`), so a
//! production build can reuse the same server. Nothing is embedded and no
//! placeholder is generated: when `index.html` is missing the page route explains
//! how to build the frontend.
//!
//! Paths are rejected when they contain `.`/`..` segments, backslashes, NUL, or
//! an empty segment, and the canonical target must stay under the canonical asset
//! root, so a crafted URL or a symlink cannot read outside the build directory.

use std::path::Path;

use axum::body::Body;
use axum::extract::{Path as UrlPath, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use std::fs;

use crate::respond;
use crate::server::AppState;

/// The SPA shell.
pub(crate) async fn index(State(state): State<AppState>) -> Response {
    let path = state.assets.join("index.html");
    match fs::read(&path) {
        Ok(bytes) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!(
                "waymark-web: the Web assets are missing ({} not found).\n\
                 Install frontend dependencies from the lockfile, build the frontend, then \
                 restart the service with --assets pointing at the build output.\n",
                path.display()
            ),
        )
            .into_response(),
    }
}

/// One static file under the asset directory.
pub(crate) async fn asset(
    State(state): State<AppState>,
    UrlPath(relative): UrlPath<String>,
) -> Response {
    serve_relative(state, relative).await
}

/// One hashed SvelteKit build file: `/_app/<path>` maps to `<assets>/_app/<path>`.
///
/// The generated `index.html` references `/_app/...`; the prefix is part of the
/// on-disk layout, so it is restored here rather than stripped.
pub(crate) async fn app_asset(
    State(state): State<AppState>,
    UrlPath(relative): UrlPath<String>,
) -> Response {
    serve_relative(state, format!("_app/{relative}")).await
}

async fn serve_relative(state: AppState, relative: String) -> Response {
    let Some(relative) = safe_relative(&relative) else {
        return respond::bad_request(
            "INVALID_ASSET_PATH",
            "the asset path contains a forbidden segment",
        );
    };
    let target = match resolve(&state.assets, relative).await {
        Some(target) => target,
        None => {
            return respond::adapter_error(
                StatusCode::NOT_FOUND,
                "ASSET_NOT_FOUND",
                "the requested asset does not exist",
                serde_json::json!({ "path": relative }),
            )
        }
    };
    match fs::read(&target) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type(&target))
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(bytes))
            .expect("a response with a known content type builds"),
        Err(_) => respond::adapter_error(
            StatusCode::NOT_FOUND,
            "ASSET_NOT_FOUND",
            "the requested asset does not exist",
            serde_json::json!({ "path": relative }),
        ),
    }
}

/// A relative path with no traversal and no empty segments.
fn safe_relative(path: &str) -> Option<&str> {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains('\0') {
        return None;
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return None;
        }
    }
    Some(path)
}

/// Canonicalize the target and require it to stay inside the asset root.
async fn resolve(root: &Path, relative: &str) -> Option<std::path::PathBuf> {
    let canonical_root = fs::canonicalize(root).ok()?;
    let canonical = fs::canonicalize(canonical_root.join(relative)).ok()?;
    if !canonical.starts_with(&canonical_root) || !canonical.is_file() {
        return None;
    }
    Some(canonical)
}

/// A conservative content type per extension; the default is opaque bytes.
fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "wasm" => "application/wasm",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
