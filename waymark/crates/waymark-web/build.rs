//! Embed the built Waymark Web frontend when the `embed` feature is enabled.
//!
//! Embedding is off by default: an ordinary build or test compiles with no
//! `ui/build` and no npm. With `embed`, the frontend must already be built
//! (`npm ci && npm run build` in `ui/`) and its provenance manifest generated
//! (`cargo run -p waymark-web --bin waymark-web-manifest`). This script never runs
//! npm. It re-walks the tree, verifies it against the manifest, gzips every file
//! once at build time, and writes the table the library serves.
//!
//! `WAYMARK_SKIP_UI_BUILD=1` means "reuse a tree built earlier in this
//! workflow"; it never relaxes the requirement for `index.html` or the manifest.
//! `WAYMARK_RELEASE=1` additionally requires `WAYMARK_GIT_SHA`, so a release
//! binary always carries the commit it was built from.

#[path = "src/manifest.rs"]
#[allow(dead_code)]
mod manifest;

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use flate2::write::GzEncoder;
use flate2::Compression;

fn main() {
    println!("cargo:rerun-if-changed=ui/build");
    println!("cargo:rerun-if-changed=ui/assets-manifest.json");
    for name in [
        "WAYMARK_SKIP_UI_BUILD",
        "WAYMARK_RELEASE",
        "WAYMARK_GIT_SHA",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    if env::var_os("CARGO_FEATURE_EMBED").is_some() {
        embed(&out_dir);
    } else {
        write_generated(&out_dir, &[], None);
    }
}

fn embed(out_dir: &Path) {
    let crate_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let assets = crate_dir.join("ui/build");
    let manifest_path = crate_dir.join("ui/assets-manifest.json");

    // Deliberately not consulted for whether assets are required: a tree built
    // earlier still has to carry index.html and a manifest that describes it.
    let _ = env::var_os("WAYMARK_SKIP_UI_BUILD");

    let release = env::var("WAYMARK_RELEASE").is_ok_and(|value| value == "1");
    let git_sha = env::var("WAYMARK_GIT_SHA").ok();
    let git_sha = fail(manifest::release_git_sha(release, git_sha));

    let record = fail(manifest::read(&manifest_path));
    fail(manifest::validate_tree(
        &assets,
        &record,
        env!("CARGO_PKG_VERSION"),
    ));
    fail(manifest::verify_build_id(&assets, &record));
    if let Some(expected) = git_sha {
        if record.git_sha.as_deref() != Some(expected.as_str()) {
            fail::<()>(Err(manifest::ManifestError::GitShaMismatch {
                expected,
                found: record.git_sha.clone(),
            }));
        }
    }

    let mut generated = Vec::with_capacity(record.files.len());
    for file in &record.files {
        let source = assets.join(&file.path);
        let bytes =
            fail::<Vec<u8>>(
                fs::read(&source).map_err(|error| manifest::ManifestError::Io {
                    path: source.clone(),
                    message: error.to_string(),
                }),
            );
        let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
        fail::<()>(
            io::copy(&mut bytes.as_slice(), &mut encoder)
                .map(|_| ())
                .map_err(|error| manifest::ManifestError::Io {
                    path: source.clone(),
                    message: error.to_string(),
                }),
        );
        let gzipped =
            fail::<Vec<u8>>(
                encoder
                    .finish()
                    .map_err(|error| manifest::ManifestError::Io {
                        path: source.clone(),
                        message: error.to_string(),
                    }),
            );
        let target = out_dir.join("gz").join(format!("{}.gz", file.path));
        if let Some(parent) = target.parent() {
            fail::<()>(
                fs::create_dir_all(parent).map_err(|error| manifest::ManifestError::Io {
                    path: parent.to_path_buf(),
                    message: error.to_string(),
                }),
            );
        }
        fail::<()>(
            fs::write(&target, &gzipped).map_err(|error| manifest::ManifestError::Io {
                path: target.clone(),
                message: error.to_string(),
            }),
        );
        generated.push((file.path.clone(), target));
    }

    let manifest_copy = out_dir.join("assets-manifest.json");
    fail::<()>(
        fs::copy(&manifest_path, &manifest_copy)
            .map(|_| ())
            .map_err(|error| manifest::ManifestError::Io {
                path: manifest_copy.clone(),
                message: error.to_string(),
            }),
    );
    write_generated(out_dir, &generated, Some(&manifest_copy));
}

/// Write `embedded_assets.rs`: the table of gzipped files and the manifest text.
fn write_generated(out_dir: &Path, files: &[(String, PathBuf)], manifest: Option<&Path>) {
    let mut code = String::new();
    code.push_str("// @generated by waymark-web/build.rs; do not edit.\n\n");
    code.push_str(
        "#[allow(dead_code)]\n\
         pub(crate) struct EmbeddedFile {\n\
         \x20   pub(crate) path: &'static str,\n\
         \x20   pub(crate) gzip: &'static [u8],\n\
         }\n\n",
    );
    code.push_str("pub(crate) static FILES: &[EmbeddedFile] = &[\n");
    for (path, target) in files {
        let target = target.to_str().expect("OUT_DIR is a UTF-8 path").to_owned();
        code.push_str(&format!(
            "    EmbeddedFile {{ path: {path:?}, gzip: include_bytes!({target:?}) }},\n"
        ));
    }
    code.push_str("];\n\n");
    match manifest {
        Some(path) => {
            let path = path.to_str().expect("OUT_DIR is a UTF-8 path");
            code.push_str(&format!(
                "pub(crate) static MANIFEST_JSON: Option<&'static str> = Some(include_str!({path:?}));\n"
            ));
        }
        None => code.push_str("pub(crate) static MANIFEST_JSON: Option<&'static str> = None;\n"),
    }
    fs::write(out_dir.join("embedded_assets.rs"), code)
        .expect("the build script can write into OUT_DIR");
}

/// Turn a preparation error into a build failure with its instructions.
fn fail<T>(result: Result<T, manifest::ManifestError>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("waymark-web: {error}"),
    }
}
