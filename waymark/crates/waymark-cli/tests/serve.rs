//! `waymark serve`'s shared startup rules and the basic build's refusal.
//!
//! Project discovery and the "nothing is created or modified" rule are variant
//! independent; the basic build's `WEB_NOT_INCLUDED` refusal is asserted only in
//! a basic test run, and the Web build's serving behaviour lives in
//! `serve_web.rs`.

use std::process::{Command, Output};

use camino::Utf8PathBuf;

fn waymark() -> Command {
    Command::new(env!("CARGO_BIN_EXE_waymark"))
}

struct Run {
    output: Output,
}

impl Run {
    fn status(&self) -> i32 {
        self.output
            .status
            .code()
            .expect("the process exited normally")
    }

    fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout).into_owned()
    }

    fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.output.stderr).into_owned()
    }

    fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.stdout()).unwrap_or_else(|error| {
            panic!(
                "stdout is not one JSON value ({error}):\nstdout: {}\nstderr: {}",
                self.stdout(),
                self.stderr()
            )
        })
    }
}

fn run(command: &mut Command) -> Run {
    Run {
        output: command.output().expect("the binary runs"),
    }
}

fn tempdir() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 path");
    (dir, path)
}

fn init(root: &Utf8PathBuf) -> Run {
    let result = run(waymark().arg("init").arg("--project").arg(root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    result
}

/// The directory's entries, sorted, so a test can prove nothing appeared.
fn entries(root: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .expect("the directory is readable")
        .map(|entry| {
            entry
                .expect("the entry is readable")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn a_missing_project_fails_before_anything_is_created() {
    let (_dir, root) = tempdir();
    let before = entries(root.as_std_path());

    let result = run(waymark()
        .arg("--json")
        .arg("serve")
        .arg("--project")
        .arg(&root));

    assert_eq!(result.status(), 5, "stderr: {}", result.stderr());
    let envelope = result.json();
    assert_eq!(envelope["v"], 1);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "PROJECT_NOT_FOUND");
    assert_eq!(
        before,
        entries(root.as_std_path()),
        "serve must not create a project or any file when discovery fails"
    );
}

#[test]
fn an_invalid_project_fails_before_binding_and_stays_unchanged() {
    let (_dir, root) = tempdir();
    let metadata = root.join(".waymark");
    std::fs::create_dir_all(&metadata).expect("the metadata directory is writable");
    let config = metadata.join("config.toml");
    std::fs::write(&config, "this is not TOML [[\n").expect("the fixture config is writable");
    let before = entries(root.as_std_path());
    let config_before = std::fs::read(&config).expect("the fixture config is readable");

    let result = run(waymark()
        .arg("--json")
        .arg("serve")
        .arg("--project")
        .arg(&root));

    assert_eq!(result.status(), 3, "stderr: {}", result.stderr());
    assert_eq!(result.json()["error"]["code"], "CONFIG_INVALID");
    assert_eq!(
        before,
        entries(root.as_std_path()),
        "serve must not add a lock file or anything else"
    );
    assert_eq!(
        config_before,
        std::fs::read(&config).expect("the fixture config stays readable"),
        "serve must not rewrite the project it cannot open"
    );
}

#[cfg(not(feature = "web"))]
#[test]
fn a_basic_build_refuses_serve_with_web_not_included() {
    let (_dir, root) = tempdir();
    init(&root);

    let result = run(waymark()
        .arg("--json")
        .arg("serve")
        .arg("--project")
        .arg(&root));

    assert_eq!(result.status(), 3, "stderr: {}", result.stderr());
    let envelope = result.json();
    assert_eq!(envelope["v"], 1);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "WEB_NOT_INCLUDED");
    assert_eq!(envelope["error"]["retryable"], false);
    assert!(
        envelope["error"].get("location").is_none(),
        "the refusal is about the build, not a document"
    );
    assert!(
        envelope["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("waymark-web")),
        "the message must name the Web package: {}",
        envelope["error"]["message"]
    );
    assert!(
        envelope["error"]["details"]
            .as_object()
            .is_some_and(|details| details.is_empty()),
        "the package is named in the message, not in typed details"
    );
}

#[cfg(not(feature = "web"))]
#[test]
fn a_basic_build_explains_the_refusal_on_stderr() {
    let (_dir, root) = tempdir();
    init(&root);

    let result = run(waymark().arg("serve").arg("--project").arg(&root));

    assert_eq!(result.status(), 3, "stderr: {}", result.stderr());
    assert_eq!(result.stdout(), "", "human mode writes no envelope");
    let stderr = result.stderr();
    assert!(
        stderr.contains("waymark-web"),
        "the refusal must name the Web package: {stderr}"
    );
    assert!(
        stderr.contains("serve"),
        "the refusal must mention the command: {stderr}"
    );
}
