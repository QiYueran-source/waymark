//! The frontend provenance manifest: a pure tree walk, its file shape, and the
//! agreement checks a build and a release both rely on.
//!
//! The frontend build emits `ui/build/` plus `ui/build-id.txt`; a manifest
//! generator (the `waymark-web-manifest` binary) walks that tree and writes
//! `assets-manifest.json` beside it. The embedding build script then re-reads the
//! manifest and re-walks the tree, so a tree that changed after the manifest was
//! generated fails the build instead of being embedded under a stale claim.
//!
//! Everything here is deliberately free of `crate::` references: `build.rs`
//! includes this file with `#[path]` so the build script and the library agree on
//! what a manifest means, instead of each implementing its own parser.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// One file in the built frontend, with the provenance a release check needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetFile {
    /// The `/`-separated path relative to the asset root.
    pub path: String,
    /// The lowercase hex SHA-256 of the file's bytes.
    pub sha256: String,
    /// The file's size in bytes.
    pub size: u64,
}

/// The immutable record of the tree a Web binary embeds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetManifest {
    /// The workspace package version the frontend was built for.
    pub version: String,
    /// The commit the frontend was built from; absent for a local build.
    pub git_sha: Option<String>,
    /// The id the frontend build emitted for this tree.
    pub build_id: String,
    /// Every embedded file, sorted by [`AssetFile::path`].
    pub files: Vec<AssetFile>,
}

/// Why a manifest could not be produced or trusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// The asset root itself is not a directory.
    AssetsMissing {
        /// The directory that was expected.
        path: PathBuf,
    },
    /// `index.html` is not in the tree.
    IndexMissing {
        /// The file that was expected.
        path: PathBuf,
    },
    /// The frontend build did not emit its build id.
    BuildIdMissing {
        /// The file that was expected.
        path: PathBuf,
    },
    /// The manifest file could not be read or parsed.
    ManifestUnreadable {
        /// The manifest path.
        path: PathBuf,
        /// What went wrong.
        message: String,
    },
    /// The manifest is well-formed JSON but not a valid provenance record.
    ManifestInvalid {
        /// Why the record cannot be trusted.
        message: String,
    },
    /// A tree entry is neither a file nor a directory (for example a symlink).
    UnsupportedEntry {
        /// The offending entry.
        path: PathBuf,
    },
    /// A path in the tree is not UTF-8, so it cannot be served as a URL.
    NonUtf8Path {
        /// The offending entry.
        path: PathBuf,
    },
    /// The manifest claims a different workspace version than this build.
    VersionMismatch {
        /// The running package version.
        expected: String,
        /// The version in the manifest.
        found: String,
    },
    /// The manifest's commit is not the commit this build was told to embed.
    GitShaMismatch {
        /// `WAYMARK_GIT_SHA`.
        expected: String,
        /// The manifest's value.
        found: Option<String>,
    },
    /// The manifest's build id is not the tree's build id.
    BuildIdMismatch {
        /// The id in the manifest.
        manifest: String,
        /// The id in the tree.
        tree: String,
    },
    /// A release build has no commit to prove provenance with.
    ReleaseWithoutGitSha,
    /// The tree no longer matches the manifest that describes it.
    StaleTree {
        /// The first disagreement, in a human-readable form.
        detail: String,
    },
    /// An operating-system failure.
    Io {
        /// The path being read or written.
        path: PathBuf,
        /// What went wrong.
        message: String,
    },
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AssetsMissing { path } => write!(
                formatter,
                "the built frontend was not found at {}.\n\
                 Build it first:\n\
                 1. install frontend dependencies from the lockfile (npm ci),\n\
                 2. build the frontend (npm run build),\n\
                 3. generate the provenance manifest (waymark-web-manifest).",
                path.display()
            ),
            Self::IndexMissing { path } => write!(
                formatter,
                "the built frontend has no index.html at {}.\n\
                 Build the frontend first (npm ci && npm run build), then generate \
                 the provenance manifest (waymark-web-manifest).",
                path.display()
            ),
            Self::BuildIdMissing { path } => write!(
                formatter,
                "the frontend build emitted no build id at {}.\n\
                 Rebuild the frontend (npm run build); the build id is written by \
                 the frontend build, not by the manifest generator.",
                path.display()
            ),
            Self::ManifestUnreadable { path, message } => write!(
                formatter,
                "could not read the provenance manifest at {}: {message}.\n\
                 Generate it after the frontend build: waymark-web-manifest.",
                path.display()
            ),
            Self::ManifestInvalid { message } => write!(formatter, "{message}"),
            Self::UnsupportedEntry { path } => write!(
                formatter,
                "the frontend tree contains an unsupported entry at {}; only regular \
                 files and directories are embedded",
                path.display()
            ),
            Self::NonUtf8Path { path } => write!(
                formatter,
                "the frontend tree contains a non-UTF-8 path at {}; every asset path \
                 must be servable as a URL",
                path.display()
            ),
            Self::VersionMismatch { expected, found } => write!(
                formatter,
                "the provenance manifest is for version {found}, but this build is \
                 version {expected}.\n\
                 Regenerate the manifest after the frontend build: waymark-web-manifest.",
            ),
            Self::GitShaMismatch { expected, found } => write!(
                formatter,
                "the provenance manifest records commit {}, but this build was told \
                 WAYMARK_GIT_SHA={expected}.\n\
                 Regenerate the manifest in the same environment that builds the binary.",
                found.as_deref().unwrap_or("(none)"),
            ),
            Self::BuildIdMismatch { manifest, tree } => write!(
                formatter,
                "the provenance manifest records build id {manifest:?}, but the tree \
                 records {tree:?}.\n\
                 Regenerate the manifest after the frontend build: waymark-web-manifest.",
            ),
            Self::ReleaseWithoutGitSha => write!(
                formatter,
                "WAYMARK_RELEASE=1 requires WAYMARK_GIT_SHA so the embedded manifest \
                 names the commit it was built from"
            ),
            Self::StaleTree { detail } => write!(
                formatter,
                "the frontend tree does not match its provenance manifest: {detail}.\n\
                 Regenerate the manifest after the frontend build: waymark-web-manifest.",
            ),
            Self::Io { path, message } => {
                write!(formatter, "could not read {}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for ManifestError {}

/// Walk `root` and return every file, sorted by relative path.
///
/// This is the pure file-walk/hash function: it reads bytes but writes nothing.
/// It refuses a tree without `index.html`, because an asset set without the SPA
/// shell is not a frontend.
pub fn asset_files(root: &Path) -> Result<Vec<AssetFile>, ManifestError> {
    if !root.is_dir() {
        return Err(ManifestError::AssetsMissing {
            path: root.to_path_buf(),
        });
    }
    let index = root.join("index.html");
    if !index.is_file() {
        return Err(ManifestError::IndexMissing { path: index });
    }
    let mut files = Vec::new();
    walk(root, root, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

/// Build the manifest for an already-built tree.
pub fn build_manifest(
    root: &Path,
    version: &str,
    git_sha: Option<&str>,
    build_id: &str,
) -> Result<AssetManifest, ManifestError> {
    Ok(AssetManifest {
        version: version.to_owned(),
        git_sha: git_sha.map(str::to_owned),
        build_id: build_id.to_owned(),
        files: asset_files(root)?,
    })
}

/// Read and validate one `assets-manifest.json`.
pub fn read(path: &Path) -> Result<AssetManifest, ManifestError> {
    let text = fs::read_to_string(path).map_err(|error| ManifestError::ManifestUnreadable {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let manifest: AssetManifest =
        serde_json::from_str(&text).map_err(|error| ManifestError::ManifestUnreadable {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    validate_shape(&manifest)?;
    Ok(manifest)
}

/// Write one manifest as pretty JSON, so a human can diff it.
pub fn write(manifest: &AssetManifest, path: &Path) -> Result<(), ManifestError> {
    let mut text = serde_json::to_string_pretty(manifest).map_err(|error| ManifestError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    text.push('\n');
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| ManifestError::Io {
            path: parent.to_path_buf(),
            message: error.to_string(),
        })?;
    }
    fs::write(path, text).map_err(|error| ManifestError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

/// The build id the frontend emitted into `root`.
pub fn read_build_id(root: &Path) -> Result<String, ManifestError> {
    let path = root.join("build-id.txt");
    let text = fs::read_to_string(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ManifestError::BuildIdMissing { path: path.clone() }
        } else {
            ManifestError::Io {
                path: path.clone(),
                message: error.to_string(),
            }
        }
    })?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(ManifestError::BuildIdMissing { path });
    }
    Ok(trimmed.to_owned())
}

/// Require `manifest` to describe exactly the tree at `root`, at `expected_version`.
pub fn validate_tree(
    root: &Path,
    manifest: &AssetManifest,
    expected_version: &str,
) -> Result<(), ManifestError> {
    if manifest.version != expected_version {
        return Err(ManifestError::VersionMismatch {
            expected: expected_version.to_owned(),
            found: manifest.version.clone(),
        });
    }
    let actual = asset_files(root)?;
    if actual == manifest.files {
        return Ok(());
    }
    Err(ManifestError::StaleTree {
        detail: first_difference(&manifest.files, &actual),
    })
}

/// Require the tree's emitted build id to be the one the manifest records.
pub fn verify_build_id(root: &Path, manifest: &AssetManifest) -> Result<(), ManifestError> {
    let tree = read_build_id(root)?;
    if tree != manifest.build_id {
        return Err(ManifestError::BuildIdMismatch {
            manifest: manifest.build_id.clone(),
            tree,
        });
    }
    Ok(())
}

/// Apply the release rule: `WAYMARK_RELEASE=1` needs a commit.
pub fn release_git_sha(
    release: bool,
    git_sha: Option<String>,
) -> Result<Option<String>, ManifestError> {
    match git_sha
        .map(|sha| sha.trim().to_owned())
        .filter(|sha| !sha.is_empty())
    {
        Some(sha) => Ok(Some(sha)),
        None if release => Err(ManifestError::ReleaseWithoutGitSha),
        None => Ok(None),
    }
}

fn walk(root: &Path, directory: &Path, files: &mut Vec<AssetFile>) -> Result<(), ManifestError> {
    let entries = fs::read_dir(directory).map_err(|error| ManifestError::Io {
        path: directory.to_path_buf(),
        message: error.to_string(),
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| ManifestError::Io {
            path: directory.to_path_buf(),
            message: error.to_string(),
        })?;
        let path = entry.path();
        let entry_type = entry.file_type().map_err(|error| ManifestError::Io {
            path: path.clone(),
            message: error.to_string(),
        })?;
        if entry_type.is_dir() {
            walk(root, &path, files)?;
        } else if entry_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .expect("the walker only descends into the root")
                .to_str()
                .ok_or_else(|| ManifestError::NonUtf8Path { path: path.clone() })?
                .replace('\\', "/");
            let bytes = fs::read(&path).map_err(|error| ManifestError::Io {
                path: path.clone(),
                message: error.to_string(),
            })?;
            files.push(AssetFile {
                path: relative,
                sha256: sha256_hex(&bytes),
                size: bytes.len() as u64,
            });
        } else {
            return Err(ManifestError::UnsupportedEntry { path });
        }
    }
    Ok(())
}

fn validate_shape(manifest: &AssetManifest) -> Result<(), ManifestError> {
    if manifest.files.is_empty() {
        return Err(ManifestError::ManifestInvalid {
            message: "the provenance manifest lists no files; it cannot describe a frontend"
                .to_owned(),
        });
    }
    if !manifest.files.iter().any(|file| file.path == "index.html") {
        return Err(ManifestError::ManifestInvalid {
            message: "the provenance manifest has no index.html entry".to_owned(),
        });
    }
    let mut previous: Option<&str> = None;
    for file in &manifest.files {
        if !safe_relative(&file.path) {
            return Err(ManifestError::ManifestInvalid {
                message: format!(
                    "the provenance manifest lists an unsafe path {:?}",
                    file.path
                ),
            });
        }
        if let Some(previous) = previous {
            if previous >= file.path.as_str() {
                return Err(ManifestError::ManifestInvalid {
                    message: format!(
                        "the provenance manifest must list unique paths in sorted order; \
                         {:?} follows {previous:?}",
                        file.path
                    ),
                });
            }
        }
        previous = Some(&file.path);
        if file.sha256.len() != 64
            || !file
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ManifestError::ManifestInvalid {
                message: format!(
                    "the provenance manifest's sha256 for {:?} is not 64 lowercase hex digits",
                    file.path
                ),
            });
        }
    }
    Ok(())
}

/// A relative, `/`-separated path with no traversal and no empty segment.
fn safe_relative(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains('\0') {
        return false;
    }
    path.split('/')
        .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

fn first_difference(manifest: &[AssetFile], tree: &[AssetFile]) -> String {
    if manifest.len() != tree.len() {
        return format!(
            "the manifest lists {} files but the tree has {}",
            manifest.len(),
            tree.len()
        );
    }
    for (manifest, tree) in manifest.iter().zip(tree) {
        if manifest.path != tree.path {
            return format!(
                "the manifest lists {:?} where the tree has {:?}",
                manifest.path, tree.path
            );
        }
        if manifest.sha256 != tree.sha256 || manifest.size != tree.size {
            return format!(
                "{} is {} bytes with sha256 {} in the manifest but {} bytes with sha256 {} in the tree",
                manifest.path, manifest.size, manifest.sha256, tree.size, tree.sha256
            );
        }
    }
    "the manifest and the tree disagree".to_owned()
}

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a temporary tree");
        fs::write(dir.path().join("index.html"), "<!doctype html>\n")
            .expect("the fixture index is writable");
        fs::create_dir_all(dir.path().join("_app")).expect("the fixture directory exists");
        fs::write(dir.path().join("_app/app.js"), "console.log('waymark');\n")
            .expect("the fixture asset is writable");
        fs::write(dir.path().join("b.txt"), "b").expect("the fixture file is writable");
        dir
    }

    #[test]
    fn asset_files_lists_sorted_paths_with_sha256_and_size() {
        let dir = tree();
        let files = asset_files(dir.path()).expect("the fixture tree is walkable");
        let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(paths, ["_app/app.js", "b.txt", "index.html"]);
        assert_eq!(
            files[0].sha256,
            "eea34958777f931d7877a6ac6780475d10d70803f03b7825281fe6119c64c52a"
        );
        assert_eq!(files[0].size, 24);
        assert_eq!(
            files[1].sha256,
            "3e23e8160039594a33894f6564e1b1348bbd7a0088d42c4acb73eeaed59c009d"
        );
        assert_eq!(files[1].size, 1);
        assert_eq!(
            files[2].sha256,
            "335fca8574f060eea24ebcdae6b78f32414f5de03da1084fd0e73d710768e3a9"
        );
        assert_eq!(files[2].size, 16);
    }

    #[test]
    fn asset_files_fails_when_index_html_is_missing() {
        let dir = tempfile::tempdir().expect("a temporary tree");
        fs::write(dir.path().join("app.js"), "x").expect("a file is writable");
        let error = asset_files(dir.path()).expect_err("a tree without index.html is not usable");
        let message = error.to_string();
        assert!(matches!(error, ManifestError::IndexMissing { .. }));
        assert!(message.contains("index.html"), "message: {message}");
        assert!(message.contains("npm run build"), "message: {message}");
        assert!(
            message.contains("waymark-web-manifest"),
            "message: {message}"
        );
    }

    #[test]
    fn asset_files_fails_when_the_root_is_missing() {
        let dir = tempfile::tempdir().expect("a temporary tree");
        let missing = dir.path().join("build");
        let error = asset_files(&missing).expect_err("a missing tree is not usable");
        assert!(matches!(error, ManifestError::AssetsMissing { .. }));
        assert!(error.to_string().contains("npm run build"));
    }

    #[test]
    fn build_manifest_records_the_tree_and_its_provenance() {
        let dir = tree();
        let manifest = build_manifest(dir.path(), "0.1.0", Some("abc123"), "build-42")
            .expect("the fixture tree has a manifest");
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(manifest.git_sha.as_deref(), Some("abc123"));
        assert_eq!(manifest.build_id, "build-42");
        assert_eq!(manifest.files.len(), 3);
        assert_eq!(manifest.files[0].path, "_app/app.js");
    }

    #[test]
    fn build_manifest_accepts_an_absent_commit() {
        let dir = tree();
        let manifest = build_manifest(dir.path(), "0.1.0", None, "build-42")
            .expect("a local build has no sha");
        assert_eq!(manifest.git_sha, None);
    }

    #[test]
    fn validate_tree_accepts_the_tree_the_manifest_describes() {
        let dir = tree();
        let manifest =
            build_manifest(dir.path(), "0.1.0", None, "build-42").expect("a manifest is built");
        validate_tree(dir.path(), &manifest, "0.1.0").expect("the tree matches its manifest");
    }

    #[test]
    fn validate_tree_rejects_a_stale_tree() {
        let dir = tree();
        let manifest =
            build_manifest(dir.path(), "0.1.0", None, "build-42").expect("a manifest is built");
        fs::write(dir.path().join("b.txt"), "changed").expect("the tree changes");
        let error = validate_tree(dir.path(), &manifest, "0.1.0")
            .expect_err("a changed tree must not match the old manifest");
        assert!(matches!(error, ManifestError::StaleTree { .. }));
        assert!(error.to_string().contains("waymark-web-manifest"));
    }

    #[test]
    fn validate_tree_rejects_a_version_mismatch() {
        let dir = tree();
        let manifest =
            build_manifest(dir.path(), "0.1.0", None, "build-42").expect("a manifest is built");
        let error = validate_tree(dir.path(), &manifest, "0.2.0")
            .expect_err("a manifest for another version must be rejected");
        assert!(matches!(
            error,
            ManifestError::VersionMismatch {
                expected,
                found
            } if expected == "0.2.0" && found == "0.1.0"
        ));
    }

    #[test]
    fn read_rejects_unsorted_or_duplicate_files() {
        let dir = tempfile::tempdir().expect("a temporary tree");
        let path = dir.path().join("assets-manifest.json");
        let manifest = AssetManifest {
            version: "0.1.0".to_owned(),
            git_sha: None,
            build_id: "build-42".to_owned(),
            files: vec![
                AssetFile {
                    path: "b.txt".to_owned(),
                    sha256: "00".to_owned(),
                    size: 1,
                },
                AssetFile {
                    path: "a.txt".to_owned(),
                    sha256: "00".to_owned(),
                    size: 1,
                },
            ],
        };
        fs::write(&path, serde_json::to_string(&manifest).unwrap())
            .expect("the manifest is written");
        let error = read(&path).expect_err("an unsorted manifest is not trusted");
        assert!(matches!(error, ManifestError::ManifestInvalid { .. }));
    }

    #[test]
    fn a_manifest_without_an_index_entry_is_not_trusted() {
        let dir = tempfile::tempdir().expect("a temporary tree");
        let path = dir.path().join("assets-manifest.json");
        let manifest = AssetManifest {
            version: "0.1.0".to_owned(),
            git_sha: None,
            build_id: "build-42".to_owned(),
            files: vec![AssetFile {
                path: "a.txt".to_owned(),
                sha256: "00".to_owned(),
                size: 1,
            }],
        };
        fs::write(&path, serde_json::to_string(&manifest).unwrap())
            .expect("the manifest is written");
        let error = read(&path).expect_err("an asset set without index.html is not a frontend");
        assert!(matches!(error, ManifestError::ManifestInvalid { .. }));
        assert!(error.to_string().contains("index.html"));
    }

    #[test]
    fn write_then_read_round_trips() {
        let dir = tree();
        let manifest =
            build_manifest(dir.path(), "0.1.0", Some("abc123"), "build-42").expect("a manifest");
        let path = dir.path().join("assets-manifest.json");
        write(&manifest, &path).expect("the manifest is writable");
        assert_eq!(read(&path).expect("the manifest is readable"), manifest);
        let text = fs::read_to_string(&path).expect("the manifest is readable as text");
        assert!(text.ends_with('\n'), "the manifest has a trailing newline");
    }

    #[test]
    fn read_build_id_strips_the_emitted_newline() {
        let dir = tempfile::tempdir().expect("a temporary tree");
        fs::write(dir.path().join("build-id.txt"), "build-42\n").expect("a build id is written");
        assert_eq!(
            read_build_id(dir.path()).expect("the build id is readable"),
            "build-42"
        );
    }

    #[test]
    fn read_build_id_fails_when_the_frontend_did_not_emit_one() {
        let dir = tempfile::tempdir().expect("a temporary tree");
        let error = read_build_id(dir.path()).expect_err("no build id is a failure");
        assert!(matches!(error, ManifestError::BuildIdMissing { .. }));
        assert!(error.to_string().contains("npm run build"));
    }

    #[test]
    fn release_git_sha_requires_a_commit_in_a_release_build() {
        assert_eq!(release_git_sha(false, None).unwrap(), None);
        assert_eq!(
            release_git_sha(true, Some("abc123".to_owned())).unwrap(),
            Some("abc123".to_owned())
        );
        let error = release_git_sha(true, None).expect_err("a release needs a commit");
        assert!(matches!(error, ManifestError::ReleaseWithoutGitSha));
        assert!(error.to_string().contains("WAYMARK_GIT_SHA"));
        assert!(matches!(
            release_git_sha(true, Some("  ".to_owned())),
            Err(ManifestError::ReleaseWithoutGitSha)
        ));
    }
}
