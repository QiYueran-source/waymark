//! The frontend asset seam.
//!
//! `GET /` serves `index.html`; `GET /assets/<path>` serves `<path>`, and
//! `GET /_app/<path>` serves `_app/<path>` because that is where SvelteKit writes
//! its hashed build output. A development run reads those files from the
//! directory the launcher chose; a Web release serves the same layout from the
//! tree embedded in the binary.
//!
//! Nothing is generated at runtime and no placeholder is served: when the
//! development directory has no `index.html` the page route explains how to build
//! the frontend, and a Web build cannot start without an embedded tree at all.
//!
//! Directory paths are rejected when they contain `.`/`..` segments,
//! backslashes, NUL, or an empty segment, and the canonical target must stay
//! under the canonical asset root, so a crafted URL or a symlink cannot read
//! outside the build directory. Embedded paths simply have to name a file in the
//! embedded table; there is no filesystem to escape.
//!
//! Embedded files were gzipped by the build script, so every embedded response
//! carries `Content-Encoding: gzip`. The content-type mapping is shared with the
//! directory path.

use std::path::Path;

use axum::body::Body;
use axum::extract::{Path as UrlPath, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use std::fs;

use crate::respond;
use crate::server::{AppState, AssetSource};

/// The SPA shell.
pub(crate) async fn index(State(state): State<AppState>) -> Response {
    match state.assets.as_ref() {
        AssetSource::Directory(directory) => directory_index(directory).await,
        #[cfg(feature = "embed")]
        AssetSource::Embedded => embedded("index.html"),
    }
}

async fn directory_index(directory: &Path) -> Response {
    let path = directory.join("index.html");
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

/// One hashed SvelteKit build file: `/_app/<path>` maps to `_app/<path>`.
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
    if !crate::manifest::safe_relative(&relative) {
        return respond::bad_request(
            "INVALID_ASSET_PATH",
            "the asset path contains a forbidden segment",
        );
    }
    match state.assets.as_ref() {
        AssetSource::Directory(directory) => serve_directory(directory, &relative).await,
        #[cfg(feature = "embed")]
        AssetSource::Embedded => embedded(&relative),
    }
}

async fn serve_directory(directory: &Path, relative: &str) -> Response {
    let target = match resolve(directory, relative).await {
        Some(target) => target,
        None => return asset_not_found(relative),
    };
    match fs::read(&target) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type(&target))
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(bytes))
            .expect("a response with a known content type builds"),
        Err(_) => asset_not_found(relative),
    }
}

/// Serve one file from the tree embedded at build time.
///
/// The bytes are already gzip-compressed, so they go out verbatim under
/// `Content-Encoding: gzip`; the content type still comes from the path.
#[cfg(feature = "embed")]
fn embedded(relative: &str) -> Response {
    let Some(gzip) = crate::embedded::asset(relative) else {
        return asset_not_found(relative);
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type(Path::new(relative)))
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONTENT_ENCODING, "gzip")
        .body(Body::from(gzip))
        .expect("a response with a known content type builds")
}

fn asset_not_found(relative: &str) -> Response {
    respond::adapter_error(
        StatusCode::NOT_FOUND,
        "ASSET_NOT_FOUND",
        "the requested asset does not exist",
        serde_json::json!({ "path": relative }),
    )
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
