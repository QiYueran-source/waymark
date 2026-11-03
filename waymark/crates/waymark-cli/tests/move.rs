//! `waymark move` end to end.
//!
//! The adapter owns the category-key parsing, the dry-run flag, and the envelope;
//! the relocation rules are the core suite's. These tests drive the real binary in
//! both output modes.

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

/// A project that declares `task` (from init) and `bug`.
fn project() -> (tempfile::TempDir, Utf8PathBuf) {
    let (dir, root) = tempdir();
    let result = run(waymark().arg("init").arg("--project").arg(&root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let added = run(waymark()
        .arg("--project")
        .arg(&root)
        .arg("config")
        .arg("add")
        .arg("category")
        .arg("bug")
        .arg("work/bugs")
        .arg("--name")
        .arg("Bugs"));
    assert_eq!(added.status(), 0, "stderr: {}", added.stderr());
    (dir, root)
}

fn create(root: &Utf8PathBuf) -> String {
    let result = run(waymark()
        .arg("--json")
        .arg("create")
        .arg("--project")
        .arg(root)
        .arg("--title")
        .arg("Fix the thing")
        .arg("--category")
        .arg("task")
        .arg("--owner")
        .arg("frank")
        .arg("--body")
        .arg("original body\n"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    result.json()["data"]["id"]
        .as_str()
        .expect("the envelope names the new id")
        .to_owned()
}

fn task_path(root: &Utf8PathBuf, id: &str) -> Utf8PathBuf {
    root.join("work").join("task").join(format!("{id}.md"))
}

fn bug_path(root: &Utf8PathBuf, id: &str) -> Utf8PathBuf {
    root.join("work").join("bugs").join(format!("{id}.md"))
}

fn document(path: &Utf8PathBuf) -> String {
    std::fs::read_to_string(path).expect("the document exists")
}

fn json_ok(root: &Utf8PathBuf, arguments: &[&str]) -> serde_json::Value {
    let mut command = waymark();
    command.arg("--json").arg("--project").arg(root);
    for argument in arguments {
        command.arg(argument);
    }
    let result = run(&mut command);
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    result.json()
}

fn json_err(root: &Utf8PathBuf, arguments: &[&str]) -> (i32, serde_json::Value) {
    let mut command = waymark();
    command.arg("--json").arg("--project").arg(root);
    for argument in arguments {
        command.arg(argument);
    }
    let result = run(&mut command);
    assert_ne!(result.status(), 0, "stdout: {}", result.stdout());
    (result.status(), result.json())
}

#[test]
fn move_relocates_the_document_and_reports_the_shared_envelope() {
    let (_dir, root) = project();
    let id = create(&root);
    let source = task_path(&root, &id);
    let target = bug_path(&root, &id);
    assert!(source.exists());

    let envelope = json_ok(&root, &["move", &id, "bug"]);

    assert_eq!(envelope["v"], serde_json::json!(1));
    assert_eq!(envelope["ok"], serde_json::json!(true));
    assert_eq!(envelope["data"]["id"], serde_json::json!(id));
    assert_eq!(
        envelope["data"]["previous_category"],
        serde_json::json!("task")
    );
    assert_eq!(envelope["data"]["category"], serde_json::json!("bug"));
    assert_eq!(envelope["data"]["changed"], serde_json::json!(true));
    assert_eq!(envelope["data"]["written"], serde_json::json!(true));
    assert_eq!(
        envelope["data"]["previous_path"],
        serde_json::json!(source.as_str())
    );
    assert_eq!(envelope["data"]["path"], serde_json::json!(target.as_str()));
    assert_eq!(
        envelope["data"]["item"]["category"],
        serde_json::json!("bug")
    );
    assert!(!source.exists(), "the old document is removed");
    assert!(target.exists(), "the document lives at the new path");
    assert!(document(&target).contains("\ncategory: bug\n"));
}

#[test]
fn move_prints_human_narrative_on_stderr() {
    let (_dir, root) = project();
    let id = create(&root);

    let human = run(waymark()
        .arg("--project")
        .arg(&root)
        .arg("move")
        .arg(&id)
        .arg("bug"));

    assert_eq!(human.status(), 0, "stderr: {}", human.stderr());
    assert!(
        human.stdout().is_empty(),
        "human mode prints nothing to stdout: {}",
        human.stdout()
    );
    assert!(
        human.stderr().contains("moved") && human.stderr().contains("task -> bug"),
        "stderr: {}",
        human.stderr()
    );
    assert!(!task_path(&root, &id).exists());
    assert!(bug_path(&root, &id).exists());
}

#[test]
fn moving_to_the_current_category_reports_unchanged_and_writes_nothing() {
    let (_dir, root) = project();
    let id = create(&root);
    let source = task_path(&root, &id);
    let before = document(&source);

    let envelope = json_ok(&root, &["move", &id, "task"]);

    assert_eq!(envelope["data"]["changed"], serde_json::json!(false));
    assert_eq!(envelope["data"]["written"], serde_json::json!(false));
    assert_eq!(document(&source), before);
}

#[test]
fn a_dry_run_reports_the_destination_and_writes_nothing() {
    let (_dir, root) = project();
    let id = create(&root);
    let source = task_path(&root, &id);
    let before = document(&source);

    let envelope = json_ok(&root, &["move", &id, "bug", "--dry-run"]);

    assert_eq!(envelope["data"]["changed"], serde_json::json!(true));
    assert_eq!(envelope["data"]["written"], serde_json::json!(false));
    assert_eq!(
        envelope["data"]["path"],
        serde_json::json!(bug_path(&root, &id).as_str())
    );
    assert!(source.exists());
    assert_eq!(document(&source), before);
    assert!(!bug_path(&root, &id).exists());
}

#[test]
fn an_undeclared_category_is_a_validation_error() {
    let (_dir, root) = tempdir();
    let result = run(waymark().arg("init").arg("--project").arg(&root));
    assert_eq!(result.status(), 0);
    let id = create(&root);
    let before = document(&task_path(&root, &id));

    let (code, envelope) = json_err(&root, &["move", &id, "bug"]);

    assert_eq!(code, 3);
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("VALIDATION_ERROR")
    );
    assert!(envelope["error"]["message"]
        .as_str()
        .expect("a message")
        .contains("not a category"));
    assert_eq!(document(&task_path(&root, &id)), before);
}

#[test]
fn a_malformed_category_key_is_a_validation_error() {
    let (_dir, root) = project();
    let id = create(&root);

    let (code, envelope) = json_err(&root, &["move", &id, "Bug"]);

    assert_eq!(code, 3);
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("VALIDATION_ERROR")
    );
    assert!(!bug_path(&root, &id).exists());
}

#[test]
fn an_occupied_destination_is_a_write_conflict() {
    let (_dir, root) = project();
    let id = create(&root);
    let source = task_path(&root, &id);
    let before = document(&source);
    let target = bug_path(&root, &id);
    std::fs::create_dir(&target).expect("the destination directory is creatable");

    let (code, envelope) = json_err(&root, &["move", &id, "bug"]);

    assert_eq!(code, 4);
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("WRITE_CONFLICT")
    );
    assert_eq!(document(&source), before);
    assert!(source.exists());
}
