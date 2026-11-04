//! `waymark-web-manifest` — generate `assets-manifest.json` for a built frontend.
//!
//! The frontend build (`npm run build` in `crates/waymark-web/ui`) writes
//! `ui/build/` and `ui/build/build-id.txt`; this tool walks that tree, hashes
//! every file, and records the tree beside it as `ui/assets-manifest.json`. It
//! never builds, downloads, or rewrites the frontend: a missing `index.html`, a
//! missing build id, or a release build without `WAYMARK_GIT_SHA` fails, so a
//! manifest never claims provenance it does not have.

use std::env;
use std::path::{Path, PathBuf};

use clap::Parser;

use waymark_web::manifest::{
    self, asset_files, read_build_id, release_git_sha, write, AssetManifest,
};

/// Write the provenance manifest for a built Waymark Web frontend.
#[derive(Debug, Parser)]
#[command(name = "waymark-web-manifest", disable_version_flag = true, about)]
struct Args {
    /// Built frontend directory (default: this crate's ui/build).
    #[arg(long, value_name = "DIR")]
    assets: Option<PathBuf>,

    /// Manifest path (default: assets-manifest.json beside the built directory).
    #[arg(long, value_name = "FILE")]
    out: Option<PathBuf>,

    /// Package version to record (default: this crate's version).
    #[arg(long = "version", value_name = "VERSION")]
    package_version: Option<String>,

    /// Commit to record (default: WAYMARK_GIT_SHA, absent for a local build).
    #[arg(long, value_name = "SHA")]
    git_sha: Option<String>,
}

fn main() {
    let args = Args::parse();
    if let Err(error) = run(args) {
        eprintln!("waymark-web-manifest: {error}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), manifest::ManifestError> {
    let assets = args.assets.unwrap_or_else(default_assets_dir);
    let out = args.out.unwrap_or_else(|| default_manifest_path(&assets));
    let version = args
        .package_version
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned());
    let git_sha = args.git_sha.or_else(|| env::var("WAYMARK_GIT_SHA").ok());
    let release = env::var("WAYMARK_RELEASE").is_ok_and(|value| value == "1");
    let git_sha = release_git_sha(release, git_sha)?;

    // Walk the tree first: a missing index.html is the most direct failure and
    // names the frontend build step, even when the build id is missing too.
    let files = asset_files(&assets)?;
    let build_id = read_build_id(&assets)?;
    let manifest = AssetManifest {
        version,
        git_sha,
        build_id,
        files,
    };
    write(&manifest, &out)?;
    println!(
        "waymark-web-manifest: wrote {} ({} files, build {})",
        out.display(),
        manifest.files.len(),
        manifest.build_id
    );
    Ok(())
}

/// The conventional built tree: this crate's `ui/build`.
fn default_assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/build")
}

/// The manifest lives beside the built tree, not inside it, so embedding never
/// serves the provenance file as an asset.
fn default_manifest_path(assets: &Path) -> PathBuf {
    assets
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("assets-manifest.json")
}
