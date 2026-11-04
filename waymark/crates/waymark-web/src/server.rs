//! The server: loopback binding, router assembly, and startup/shutdown.
//!
//! The listener address is not configurable: it is always `127.0.0.1`, which is
//! what "no LAN or public binding option" means at the type level. The port is
//! the only networking knob, and `0` asks the operating system for one (used by
//! tests and the launcher's `--port 0`).

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::routing::get;
use axum::{middleware, Router};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use waymark_core::Project;

use crate::api;
use crate::assets;
use crate::guard;
use crate::{respond, DEFAULT_PORT};

/// The one address the service ever binds.
pub const LOOPBACK: Ipv4Addr = Ipv4Addr::LOCALHOST;

/// Where the static frontend comes from.
///
/// The development launcher reads a directory the developer built; a Web
/// release embeds the same tree at compile time. The server itself does not care
/// which, and a plain build has no embedded tree to reach for.
#[derive(Debug, Clone)]
pub(crate) enum AssetSource {
    Directory(PathBuf),
    #[cfg(feature = "embed")]
    Embedded,
}

/// Everything the service needs that is not the project.
#[derive(Debug, Clone)]
pub struct WebConfig {
    /// The loopback port to bind; `0` asks the operating system to choose one.
    ///
    /// There is deliberately no host/interface field: the service binds
    /// [`LOOPBACK`] and nothing else.
    pub port: u16,
    pub(crate) assets: AssetSource,
}

impl WebConfig {
    /// A configuration for `assets` on the default port.
    pub fn new(assets: impl Into<PathBuf>) -> Self {
        Self {
            port: DEFAULT_PORT,
            assets: AssetSource::Directory(assets.into()),
        }
    }

    /// A configuration for the frontend embedded in this binary.
    ///
    /// Only available with the `embed` feature: a build without a frontend must
    /// not be able to claim it serves one.
    #[cfg(feature = "embed")]
    pub fn embedded() -> Self {
        Self {
            port: DEFAULT_PORT,
            assets: AssetSource::Embedded,
        }
    }

    /// Use `port` (tests pass `0`).
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }
}

impl Default for WebConfig {
    fn default() -> Self {
        Self::new(default_assets_dir())
    }
}

/// The launcher's default asset directory: this crate's `ui/build`.
///
/// Phase 2 builds SvelteKit into that directory. The path is resolved at compile
/// time because this binary is a development launcher run from the checkout.
pub fn default_assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/build")
}

/// A startup or runtime failure of the Web service.
#[derive(Debug, thiserror::Error)]
pub enum WebError {
    /// The loopback listener could not be bound.
    #[error("could not bind 127.0.0.1:{port}: {source}")]
    Bind {
        /// The requested port.
        port: u16,
        /// The operating-system failure.
        source: io::Error,
    },
    /// The operating system did not report the listening address.
    #[error("could not read the listening address: {0}")]
    Address(io::Error),
    /// The server stopped with an I/O failure.
    #[error("the read-only Web service stopped with an error: {0}")]
    Serve(io::Error),
    /// Ctrl-C could not be awaited.
    #[error("could not wait for Ctrl-C: {0}")]
    Signal(io::Error),
    /// Startup refused to run: the frontend assets are not there.
    #[error("{0}")]
    MissingAssets(String),
}

/// Check that `dir` contains a built frontend shell.
///
/// This is the launcher's check, not `bind`'s: ordinary `cargo build` and
/// `cargo test` never need assets, and API tests point at a temporary fixture.
/// It never creates, downloads, or builds anything.
pub fn ensure_assets(dir: &Path) -> Result<(), WebError> {
    let index = dir.join("index.html");
    if index.is_file() {
        return Ok(());
    }
    Err(WebError::MissingAssets(format!(
        "frontend assets are missing: {} was not found.\n\
         Prepare them before launching:\n\
         1. install frontend dependencies from the lockfile,\n\
         2. build the frontend,\n\
         3. relaunch waymark-web-dev with --assets pointing at the build output.\n\
         The launcher never runs npm, downloads dependencies, or serves a placeholder.",
        index.display()
    )))
}

/// The immutable per-request state.
#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) project: Project,
    /// The actual bound port, which is the only port Host/Origin may name.
    pub(crate) port: u16,
    pub(crate) assets: Arc<AssetSource>,
}

/// A running loopback listener.
///
/// Drop without calling [`Running::shutdown`] aborts the server task.
pub struct Running {
    addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    handle: JoinHandle<io::Result<()>>,
}

impl Running {
    /// The address actually bound, including the port when `0` was requested.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The origin a browser should open: `http://localhost:<port>`.
    pub fn url(&self) -> String {
        format!("http://localhost:{}", self.addr.port())
    }

    /// Stop accepting connections and wait for the server task to finish.
    pub async fn shutdown(mut self) -> Result<(), WebError> {
        if let Some(sender) = self.shutdown.take() {
            let _ = sender.send(());
        }
        match self.handle.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(source)) => Err(WebError::Serve(source)),
            Err(error) => Err(WebError::Serve(io::Error::other(error))),
        }
    }
}

/// Bind the loopback listener and start serving in a background task.
///
/// This is the test and launcher seam: it returns once the listener is bound, so
/// a caller can read the actual port before the first request. [`serve`] wraps it
/// with Ctrl-C handling.
pub async fn bind(project: Project, config: WebConfig) -> Result<Running, WebError> {
    let listener = TcpListener::bind((LOOPBACK, config.port))
        .await
        .map_err(|source| WebError::Bind {
            port: config.port,
            source,
        })?;
    let addr = listener.local_addr().map_err(WebError::Address)?;
    let state = AppState {
        project,
        port: addr.port(),
        assets: Arc::new(config.assets),
    };
    let app = router(state);
    let (shutdown, stopped) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = stopped.await;
            })
            .await
    });
    Ok(Running {
        addr,
        shutdown: Some(shutdown),
        handle,
    })
}

/// Serve until Ctrl-C.
///
/// Prints the loopback origin to standard error because this is the
/// developer-facing entry point. The service never binds anything except
/// [`LOOPBACK`].
pub async fn serve(project: Project, config: WebConfig) -> Result<(), WebError> {
    let running = bind(project, config).await?;
    eprintln!(
        "waymark-web: listening on {} (loopback only)",
        running.url()
    );
    tokio::signal::ctrl_c().await.map_err(WebError::Signal)?;
    running.shutdown().await
}

/// The unknown-path handler; `fallback` needs a real async handler.
async fn not_found() -> axum::response::Response {
    respond::not_found()
}

/// The complete route table.
///
/// All routes are `GET`/`HEAD`-only; the guard also turns every other method
/// into `405` before routing, including for unknown paths.
pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(assets::index))
        .route("/assets/{*path}", get(assets::asset))
        // SvelteKit emits its hashed build output under `/_app/...`; serve that
        // prefix from `<assets>/_app/...` so the frontend can keep SvelteKit's
        // default paths.
        .route("/_app/{*path}", get(assets::app_asset))
        .route("/api/health", get(api::health))
        .route("/api/tree", get(api::tree))
        .route("/api/item/{id}", get(api::item))
        .fallback(not_found)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            guard::validate,
        ))
        .with_state(state)
}
