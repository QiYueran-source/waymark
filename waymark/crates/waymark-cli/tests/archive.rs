//! `waymark archive` end to end, plus the archived-only list range.
//!
//! The rules live in `waymark-core`; these tests pin the adapter: the argument
//! shapes, the envelope, the exit code, and the fact that a dry run writes
//! nothing.

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

fn create(root: &Utf8PathBuf, title: &str) -> String {
    let result = run(waymark()
        .arg("--json")
        .arg("create")
        .arg("--project")
        .arg(root)
        .arg("--title")
        .arg(title)
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

fn document(root: &Utf8PathBuf, id: &str) -> String {
    std::fs::read_to_string(root.join("work").join("task").join(format!("{id}.md")))
        .expect("the document is readable")
}

#[test]
fn archive_reports_the_marker_the_reason_and_the_unchanged_status() {
    let (_dir, root) = project();
    let id = create(&root, "Retire me");
    json_ok(&root, &["status", "set", &id, "closed"]);
    let before = document(&root, &id);

    let envelope = json_ok(&root, &["archive", &id, "--reason", "superseded"]);

    assert_eq!(envelope["ok"], serde_json::json!(true));
    assert_eq!(envelope["data"]["id"], serde_json::json!(id));
    assert!(envelope["data"]["archived_at"].is_string());
    assert_eq!(
        envelope["data"]["archive_reason"],
        serde_json::json!("superseded")
    );
    assert_eq!(envelope["data"]["status"], serde_json::json!("closed"));
    assert_eq!(envelope["data"]["written"], serde_json::json!(true));
    assert_eq!(
        envelope["data"]["removals"],
        serde_json::json!([]),
        "nothing points at it"
    );

    let after = document(&root, &id);
    assert!(after.contains("archived_at:"));
    assert!(after.contains("archive_reason: superseded"));
    let updated_before = before
        .lines()
        .find(|line| line.starts_with("updated_at:"))
        .expect("the document has an updated_at");
    let updated_after = after
        .lines()
        .find(|line| line.starts_with("updated_at:"))
        .expect("the document still has an updated_at");
    assert_eq!(
        updated_before, updated_after,
        "archive does not move updated_at"
    );
    assert!(after.contains("status: closed"));
    assert!(after.contains("closed_at:"));
}

#[test]
fn archive_cleans_inbound_edges_and_reports_each_removal() {
    let (_dir, root) = project();
    let target = create(&root, "target");
    let dependent = create(&root, "dependent");
    json_ok(&root, &["dependency", "add", &dependent, &target]);

    let envelope = json_ok(&root, &["archive", &target]);

    assert_eq!(
        envelope["data"]["removals"],
        serde_json::json!([{"from": dependent, "relation": "dependencies"}])
    );
    assert_eq!(
        envelope["data"]["affected"],
        serde_json::json!([{
            "id": dependent,
            "path": root.join("work").join("task").join(format!("{dependent}.md")).to_string(),
        }])
    );
    assert!(!document(&root, &dependent).contains(&target));
}

#[test]
fn a_dry_run_reports_the_plan_and_writes_nothing() {
    let (_dir, root) = project();
    let target = create(&root, "target");
    let dependent = create(&root, "dependent");
    json_ok(&root, &["dependency", "add", &dependent, &target]);
    let target_before = document(&root, &target);
    let dependent_before = document(&root, &dependent);

    let envelope = json_ok(
        &root,
        &["archive", &target, "--dry-run", "--reason", "later"],
    );

    assert_eq!(envelope["data"]["written"], serde_json::json!(false));
    assert!(envelope["data"]["archived_at"].is_string());
    assert_eq!(
        envelope["data"]["removals"],
        serde_json::json!([{"from": dependent, "relation": "dependencies"}])
    );
    assert_eq!(
        document(&root, &target),
        target_before,
        "nothing was written"
    );
    assert_eq!(document(&root, &dependent), dependent_before);
}

#[test]
fn cascade_is_accepted_and_changes_nothing_about_the_result() {
    let (_dir, root) = project();
    let target = create(&root, "target");
    let dependent = create(&root, "dependent");
    json_ok(&root, &["dependency", "add", &dependent, &target]);

    let envelope = json_ok(&root, &["archive", &target, "--cascade"]);

    assert_eq!(
        envelope["data"]["removals"],
        serde_json::json!([{"from": dependent, "relation": "dependencies"}])
    );
    assert!(!document(&root, &dependent).contains(&target));
}

#[test]
fn cascade_is_accepted_on_a_dry_run_too() {
    let (_dir, root) = project();
    let target = create(&root, "target");
    let before = document(&root, &target);

    let envelope = json_ok(&root, &["archive", &target, "--cascade", "--dry-run"]);

    assert_eq!(envelope["data"]["written"], serde_json::json!(false));
    assert_eq!(document(&root, &target), before, "nothing was written");
}

#[test]
fn a_child_refuses_the_archive_with_the_child_list_and_conflict_exit() {
    let (_dir, root) = project();
    let parent = create(&root, "parent");
    let child = create(&root, "child");
    json_ok(&root, &["parent", "set", &child, &parent]);

    let (code, envelope) = json_err(&root, &["archive", &parent]);

    assert_eq!(code, 4, "HAS_ACTIVE_CHILDREN is a conflict");
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("HAS_ACTIVE_CHILDREN")
    );
    assert_eq!(
        envelope["error"]["details"]["children"],
        serde_json::json!([child])
    );
    assert!(!document(&root, &parent).contains("archived_at"));
}

#[test]
fn re_archiving_is_a_conflict_that_names_the_code() {
    let (_dir, root) = project();
    let id = create(&root, "work");
    let first = json_ok(&root, &["archive", &id, "--reason", "first"]);
    let before = document(&root, &id);

    let (code, envelope) = json_err(&root, &["archive", &id, "--reason", "second"]);

    assert_eq!(code, 4);
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("ALREADY_ARCHIVED")
    );
    assert_eq!(document(&root, &id), before);
    assert_eq!(first["data"]["archive_reason"], serde_json::json!("first"));
}

#[test]
fn an_archived_item_rejects_a_later_write_with_item_archived() {
    let (_dir, root) = project();
    let id = create(&root, "work");
    json_ok(&root, &["archive", &id]);

    let (code, envelope) = json_err(&root, &["title", "set", &id, "renamed"]);

    assert_eq!(code, 4);
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("ITEM_ARCHIVED")
    );
    assert_eq!(
        envelope["error"]["details"]["operation"],
        serde_json::json!("title")
    );
}

#[test]
fn show_keeps_the_archived_items_own_edges_and_marks_them_historical() {
    let (_dir, root) = project();
    let retired = create(&root, "retired");
    let prerequisite = create(&root, "prerequisite");
    json_ok(&root, &["dependency", "add", &retired, &prerequisite]);
    json_ok(&root, &["archive", &retired]);

    let shown = json_ok(&root, &["show", &retired]);

    assert_eq!(shown["data"]["archived"], serde_json::json!(true));
    assert_eq!(
        shown["data"]["relationships"]["historical"],
        serde_json::json!(true)
    );
    assert_eq!(
        shown["data"]["relationships"]["dependencies"],
        serde_json::json!([prerequisite]),
        "the archived document keeps its own edges as history"
    );
    assert_eq!(shown["data"]["ready"], serde_json::json!(false));
    assert_eq!(shown["data"]["blocked"], serde_json::json!(false));
}

#[test]
fn list_hides_archived_items_until_the_archived_range_is_asked_for() {
    let (_dir, root) = project();
    let retired = create(&root, "retired");
    let kept = create(&root, "kept");
    json_ok(&root, &["archive", &retired]);

    let active = json_ok(&root, &["list"]);
    let rows = active["data"]["items"]
        .as_array()
        .expect("rows are an array");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], serde_json::json!(kept));

    let archived = json_ok(&root, &["list", "--archived"]);
    let rows = archived["data"]["items"]
        .as_array()
        .expect("rows are an array");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], serde_json::json!(retired));
    assert_eq!(rows[0]["ready"], serde_json::json!(false));
    assert_eq!(rows[0]["blocked"], serde_json::json!(false));

    for command in ["ready", "blocked"] {
        let envelope = json_ok(&root, &[command, "--archived"]);
        assert_eq!(
            envelope["data"]["items"],
            serde_json::json!([]),
            "{command} --archived selects no active work"
        );
    }

    // `show` still reads it, by stable ID, with its archive metadata.
    let shown = json_ok(&root, &["show", &retired]);
    assert_eq!(shown["data"]["id"], serde_json::json!(retired));
    assert!(shown["data"]["item"]["archived_at"].is_string());
}
