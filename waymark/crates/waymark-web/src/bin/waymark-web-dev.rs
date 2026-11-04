//! `waymark-web-dev` — the local development/test launcher for the read-only
//! Web view.
//!
//! It discovers the project (current directory by default, `--project PATH`
//! otherwise), checks that a built frontend exists, then binds loopback only.
//! It never initializes or modifies a project, never runs npm, never downloads
//! anything, and never serves a placeholder when assets are missing.

use std::path::PathBuf;

use camino::Utf8PathBuf;
use clap::Parser;
use waymark_core::store::Project;

use waymark_web::{WebConfig, WebError, DEFAULT_PORT};

/// Serve the read-only Waymark Web view on loopback.
#[derive(Debug, Parser)]
#[command(name = "waymark-web-dev", version, about)]
struct Args {
    /// Project directory to discover from (default: the current directory).
    #[arg(long, value_name = "PATH")]
    project: Option<Utf8PathBuf>,

    /// Loopback port to listen on; 0 chooses a free port.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u16,

    /// Built frontend directory (default: the crate's ui/build).
    #[arg(long, value_name = "DIR")]
    assets: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    if let Err(message) = run(args).await {
        eprintln!("waymark-web-dev: {message}");
        std::process::exit(1);
    }
}

async fn run(args: Args) -> Result<(), String> {
    let start = match args.project {
        Some(path) => path,
        None => current_dir()?,
    };
    // Discovery only reads. A missing or invalid project fails here, before any
    // asset check and before the listener exists; nothing is created.
    let project = Project::discover(&start)
        .map_err(|error| format!("{}: {}", error.code(), error.message()))?;

    let assets = args.assets.unwrap_or_else(waymark_web::default_assets_dir);
    waymark_web::ensure_assets(&assets).map_err(|error| match error {
        WebError::MissingAssets(message) => message,
        other => other.to_string(),
    })?;

    let config = WebConfig {
        port: args.port,
        assets,
    };
    let running = waymark_web::bind(project.clone(), config)
        .await
        .map_err(|error| error.to_string())?;
    println!("waymark-web-dev: project {}", project.root());
    println!("waymark-web-dev: listening on {}", running.url());
    println!("waymark-web-dev: read-only; press Ctrl-C to stop");

    tokio::signal::ctrl_c()
        .await
        .map_err(|error| format!("could not wait for Ctrl-C: {error}"))?;
    running.shutdown().await.map_err(|error| error.to_string())
}

fn current_dir() -> Result<Utf8PathBuf, String> {
    let current = std::env::current_dir()
        .map_err(|error| format!("could not read the current directory: {error}"))?;
    Utf8PathBuf::from_path_buf(current)
        .map_err(|path| format!("the current directory is not UTF-8: {}", path.display()))
}
