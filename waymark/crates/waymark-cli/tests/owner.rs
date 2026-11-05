//! `waymark owner transfer` end to end.
//!
//! The adapter owns the argument shape, the dry-run flag, and the routing into the
//! shared envelope; the rules are the core suite's. These tests drive the real
//! binary in both output modes.

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

fn project() -> (tempfile::TempDir, Utf8PathBuf) {
    let (dir, root) = tempdir();
    let result = run(waymark().arg("init").arg("--project").arg(&root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
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
        .arg("frank"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    result.json()["data"]["id"]
        .as_str()
        .expect("the envelope names the new id")
        .to_owned()
}

fn document(root: &Utf8PathBuf, id: &str) -> String {
    std::fs::read_to_string(root.join("work").join("task").join(format!("{id}.md")))
        .expect("the document exists")
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

#[test]
fn owner_transfer_replaces_the_owner_and_reports_the_shared_envelope() {
    let (_dir, root) = project();
    let id = create(&root);

    let envelope = json_ok(&root, &["owner", "transfer", &id, "  bob  "]);

    assert_eq!(envelope["v"], serde_json::json!(1));
    assert_eq!(envelope["ok"], serde_json::json!(true));
    assert_eq!(envelope["data"]["id"], serde_json::json!(id));
    assert_eq!(
        envelope["data"]["previous_owner"],
        serde_json::json!("frank")
    );
    assert_eq!(envelope["data"]["owner"], serde_json::json!("bob"));
    assert_eq!(envelope["data"]["changed"], serde_json::json!(true));
    assert_eq!(envelope["data"]["written"], serde_json::json!(true));
    assert_eq!(envelope["data"]["item"]["owner"], serde_json::json!("bob"));
    assert_eq!(envelope["data"]["item"]["id"], serde_json::json!(id));
    assert_eq!(
        envelope["data"]["path"],
        serde_json::json!(root.join("work").join("task").join(format!("{id}.md")))
    );
    assert!(document(&root, &id).contains("\nowner: bob\n"));
}

#[test]
fn owner_transfer_prints_human_narrative_on_stderr() {
    let (_dir, root) = project();
    let id = create(&root);

    let human = run(waymark()
        .arg("--project")
        .arg(&root)
        .arg("owner")
        .arg("transfer")
        .arg(&id)
        .arg("bob"));

    assert_eq!(human.status(), 0);
    assert!(
        human.stdout().is_empty(),
        "human mode prints nothing to stdout: {}",
        human.stdout()
    );
    assert!(
        human.stderr().contains("transferred") && human.stderr().contains("frank -> bob"),
        "stderr: {}",
        human.stderr()
    );
    assert!(document(&root, &id).contains("\nowner: bob\n"));
}

#[test]
fn transferring_to_the_current_owner_reports_unchanged_and_writes_nothing() {
    let (_dir, root) = project();
    let id = create(&root);
    let before = document(&root, &id);

    let envelope = json_ok(&root, &["owner", "transfer", &id, "frank"]);

    assert_eq!(envelope["data"]["changed"], serde_json::json!(false));
    assert_eq!(envelope["data"]["written"], serde_json::json!(false));
    assert_eq!(document(&root, &id), before);
}

#[test]
fn a_dry_run_reports_the_transfer_and_writes_nothing() {
    let (_dir, root) = project();
    let id = create(&root);
    let before = document(&root, &id);

    let envelope = json_ok(&root, &["owner", "transfer", &id, "bob", "--dry-run"]);

    assert_eq!(envelope["data"]["changed"], serde_json::json!(true));
    assert_eq!(envelope["data"]["written"], serde_json::json!(false));
    assert_eq!(envelope["data"]["owner"], serde_json::json!("bob"));
    assert_eq!(document(&root, &id), before);
}

#[test]
fn a_mismatched_operator_warns_in_the_envelope_and_on_stderr() {
    let (_dir, root) = project();
    let id = create(&root);
    std::fs::write(
        root.join(".waymark").join("user.toml"),
        "operator = \"alice\"\n",
    )
    .expect("the local document is writable");

    let envelope = json_ok(&root, &["owner", "transfer", &id, "bob"]);
    assert_eq!(
        envelope["_meta"]["warnings"][0]["code"],
        serde_json::json!("OWNER_MISMATCH")
    );

    let human = run(waymark()
        .arg("--project")
        .arg(&root)
        .arg("owner")
        .arg("transfer")
        .arg(&id)
        .arg("carol"));
    assert_eq!(human.status(), 0, "a mismatch never fails");
    assert!(
        human.stderr().contains("OWNER_MISMATCH"),
        "stderr: {}",
        human.stderr()
    );
    assert!(document(&root, &id).contains("\nowner: carol\n"));
}
