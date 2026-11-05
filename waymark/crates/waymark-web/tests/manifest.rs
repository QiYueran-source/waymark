//! The manifest generator, invoked exactly as the release pipeline invokes it:
//! as a process over a built tree, with its provenance coming from the
//! environment.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn tree() -> TempDir {
    let dir = tempfile::tempdir().expect("a temporary tree");
    fs::write(dir.path().join("index.html"), "<!doctype html>\n")
        .expect("the fixture index is writable");
    fs::create_dir_all(dir.path().join("_app")).expect("the fixture directory exists");
    fs::write(
        dir.path().join("_app").join("app.js"),
        "console.log('waymark');\n",
    )
    .expect("the fixture asset is writable");
    fs::write(dir.path().join("build-id.txt"), "build-42\n").expect("the build id is writable");
    dir
}

fn generator() -> Command {
    Command::new(env!("CARGO_BIN_EXE_waymark-web-manifest"))
}

fn run(assets: &Path, out: &Path) -> Output {
    generator()
        .arg("--assets")
        .arg(assets)
        .arg("--out")
        .arg(out)
        .arg("--version")
        .arg("0.1.0")
        .env_remove("WAYMARK_GIT_SHA")
        .env_remove("WAYMARK_RELEASE")
        .output()
        .expect("the manifest generator runs")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn generate(assets: &Path, out: &Path) -> serde_json::Value {
    let output = run(assets, out);
    assert!(
        output.status.success(),
        "the generator must succeed: {}",
        stderr(&output)
    );
    serde_json::from_str(&fs::read_to_string(out).expect("the manifest is written"))
        .expect("the manifest is JSON")
}

#[test]
fn writes_a_sorted_manifest_with_per_file_sha256_and_size() {
    let assets = tree();
    let out = assets.path().join("assets-manifest.json");
    let manifest = generate(assets.path(), &out);

    assert_eq!(manifest["version"], "0.1.0");
    assert_eq!(manifest["git_sha"], serde_json::Value::Null);
    assert_eq!(manifest["build_id"], "build-42");

    let files = manifest["files"]
        .as_array()
        .expect("the manifest lists files");
    let paths: Vec<&str> = files
        .iter()
        .map(|file| file["path"].as_str().expect("a path"))
        .collect();
    assert_eq!(paths, ["_app/app.js", "build-id.txt", "index.html"]);

    assert_eq!(
        files[0]["sha256"],
        "eea34958777f931d7877a6ac6780475d10d70803f03b7825281fe6119c64c52a"
    );
    assert_eq!(files[0]["size"], 24);
    assert_eq!(
        files[2]["sha256"],
        "335fca8574f060eea24ebcdae6b78f32414f5de03da1084fd0e73d710768e3a9"
    );
    assert_eq!(files[2]["size"], 16);
}

#[test]
fn fails_when_index_html_is_missing() {
    let assets = tempfile::tempdir().expect("a temporary tree");
    fs::write(assets.path().join("app.js"), "x").expect("a file is writable");
    let out = assets.path().join("assets-manifest.json");

    let output = run(assets.path(), &out);

    assert!(
        !output.status.success(),
        "no index.html must fail generation"
    );
    let message = stderr(&output);
    assert!(message.contains("index.html"), "stderr: {message}");
    assert!(message.contains("npm run build"), "stderr: {message}");
    assert!(
        message.contains("waymark-web-manifest"),
        "stderr: {message}"
    );
    assert!(
        !out.exists(),
        "a failed generation must not write a manifest"
    );
}

#[test]
fn fails_when_the_frontend_did_not_emit_a_build_id() {
    let assets = tree();
    fs::remove_file(assets.path().join("build-id.txt")).expect("the build id is removable");
    let out = assets.path().join("assets-manifest.json");

    let output = run(assets.path(), &out);

    assert!(!output.status.success(), "no build id must fail generation");
    let message = stderr(&output);
    assert!(message.contains("build id"), "stderr: {message}");
    assert!(message.contains("npm run build"), "stderr: {message}");
    assert!(!out.exists());
}

#[test]
fn a_release_build_without_a_commit_fails() {
    let assets = tree();
    let out = assets.path().join("assets-manifest.json");

    let output = generator()
        .arg("--assets")
        .arg(assets.path())
        .arg("--out")
        .arg(&out)
        .arg("--version")
        .arg("0.1.0")
        .env_remove("WAYMARK_GIT_SHA")
        .env("WAYMARK_RELEASE", "1")
        .output()
        .expect("the manifest generator runs");

    assert!(!output.status.success(), "a release needs WAYMARK_GIT_SHA");
    assert!(
        stderr(&output).contains("WAYMARK_GIT_SHA"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(!out.exists());
}

#[test]
fn records_the_commit_when_waymark_git_sha_is_set() {
    let assets = tree();
    let out = assets.path().join("assets-manifest.json");

    let output = generator()
        .arg("--assets")
        .arg(assets.path())
        .arg("--out")
        .arg(&out)
        .arg("--version")
        .arg("0.1.0")
        .env("WAYMARK_GIT_SHA", "abc123")
        .env_remove("WAYMARK_RELEASE")
        .output()
        .expect("the manifest generator runs");

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&out).expect("the manifest is written"))
            .expect("the manifest is JSON");
    assert_eq!(manifest["git_sha"], "abc123");
}

#[test]
fn the_manifest_is_written_beside_the_tree_by_default() {
    let assets = tempfile::tempdir().expect("a temporary tree");
    let build = assets.path().join("build");
    fs::create_dir_all(build.join("_app")).expect("the build directory exists");
    fs::write(build.join("index.html"), "<!doctype html>\n").expect("the index is writable");
    fs::write(
        build.join("_app").join("app.js"),
        "console.log('waymark');\n",
    )
    .expect("the asset is writable");
    fs::write(build.join("build-id.txt"), "build-42\n").expect("the build id is writable");

    let output = generator()
        .arg("--assets")
        .arg(&build)
        .arg("--version")
        .arg("0.1.0")
        .env_remove("WAYMARK_GIT_SHA")
        .env_remove("WAYMARK_RELEASE")
        .output()
        .expect("the manifest generator runs");
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        assets.path().join("assets-manifest.json").is_file(),
        "the manifest belongs beside the build directory"
    );
}
