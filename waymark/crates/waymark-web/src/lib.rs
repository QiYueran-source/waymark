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
//! # The asset seam (phase 2)
//!
//! The frontend is a SvelteKit build in a directory chosen by the launcher.
//! `GET /` serves `<assets>/index.html`; `GET /assets/<path>` serves
//! `<assets>/<path>`. Nothing is embedded and nothing is generated: a build
//! without assets starts (so `cargo test` needs no npm) but `GET /` explains the
//! missing assets instead of serving a placeholder. [`ensure_assets`] is the
//! launcher's startup check.

mod api;
mod assets;
mod guard;
mod render;
mod respond;
mod server;

pub use api::{CategoryView, SHAPES};
pub use server::{
    bind, default_assets_dir, ensure_assets, serve, Running, WebConfig, WebError, LOOPBACK,
};

/// The default development port, mirrored from the launcher's `--port` default.
pub const DEFAULT_PORT: u16 = 7331;
