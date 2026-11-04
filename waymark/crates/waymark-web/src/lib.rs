//! Read-only Web service for Waymark.
//!
//! This crate is a thin adapter over [`waymark_core`]: every domain rule — which
//! items exist, how a tree is shaped, what readiness means, what an error code
//! is — comes from the core. The Web layer adds HTTP, Host/Origin validation,
//! Markdown rendering, and static asset serving, and it deliberately links no
//! write operation.
//!
//! # Read-only by construction
//!
//! The only core modules this crate imports are reads: [`waymark_core::readiness`]
//! (for the graph), [`waymark_core::ops::list`] (for the complete `_meta`),
//! [`waymark_core::ops::tree`] (for the hierarchy), and
//! [`waymark_core::ops::read`] (for one item). No module under `ops::create`,
//! `ops::edit`, `ops::status`, `ops::relationship`, `ops::parent`, `ops::link`,
//! `ops::archive`, `ops::config`, or `ops::mediated` is referenced, so there is
//! no code path from a request to a write. Reads never take the write lock.
//!
//! # Loopback only
//!
//! [`serve`] and [`bind`] always bind `127.0.0.1`; [`WebConfig`] has no address
//! or interface option, so there is no way to ask for a LAN or public listener.
//! Every request must also pass [`guard`](mod@guard): `Host` must name
//! `localhost` or a loopback IP at the actual listening port, and an `Origin`
//! header, when present, must be the matching `http://host:port` origin.
//!
//! # The asset seam
//!
//! The frontend is a SvelteKit build. In a development run the launcher names a
//! directory and `GET /` serves `<assets>/index.html`; `GET /assets/<path>`
//! serves `<assets>/<path>`. With the off-by-default `embed` feature,
//! `build.rs` compiles the built tree into the binary instead, gzipped at build
//! time, and [`WebConfig::embedded`] serves it with `Content-Encoding: gzip`.
//! A build without `embed` has no embedded tree and still starts without assets
//! (so `cargo test` needs no npm); `GET /` explains missing development assets
//! instead of serving a placeholder. [`ensure_assets`] is the launcher's startup
//! check.
//!
//! [`embedded_manifest`] exposes the provenance manifest a Web build embeds, so
//! an adapter (`waymark version --json`, `waymark doctor`) can report exactly
//! which tree the binary carries.

mod api;
mod assets;
mod embedded;
mod guard;
pub mod manifest;
mod render;
mod respond;
mod server;

pub use api::{CategoryView, SHAPES};
pub use manifest::{AssetFile, AssetManifest, AssetMismatch};

/// The provenance manifest embedded by an `embed` build, or `None`.
///
/// The manifest and the asset table come from the same build script run, so a
/// `Some` value describes exactly the bytes this binary serves.
pub fn embedded_manifest() -> Option<&'static AssetManifest> {
    embedded::manifest()
}

/// Why this binary's embedded frontend disagrees with the build, if it does.
///
/// A basic build embeds no frontend and returns `None`; a Web build whose
/// manifest matches the version and the embedded `index.html` also returns
/// `None`. The disagreement is reported, never enforced: `serve` still starts.
pub fn embedded_manifest_mismatch() -> Option<AssetMismatch> {
    let manifest = embedded::manifest()?;
    let index_html = embedded::index_html().expect("an embed build embeds index.html");
    manifest::verify_agreement(manifest, env!("CARGO_PKG_VERSION"), index_html).err()
}
pub use server::{
    bind, default_assets_dir, ensure_assets, serve, Running, WebConfig, WebError, LOOPBACK,
};

/// The default loopback port, re-exported from `waymark_core` so the Web
/// service, the launcher, and the CLI share one value.
pub use waymark_core::DEFAULT_PORT;
