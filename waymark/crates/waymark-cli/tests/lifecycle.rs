//! The lifecycle, dependency, and query commands end to end.
//!
//! These tests drive the real binary because that is where the three things this
//! crate owns meet: the argument shapes, the JSON envelope, and the exit code
//! from the registry. Every rule they exercise lives in `waymark-core`; the point
//! here is that the adapter exposes it with the names, streams, and codes the
//! specification promises.

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

/// A project with the default configuration, ready for items.
fn project() -> (tempfile::TempDir, Utf8PathBuf) {
    let (dir, root) = tempdir();
    let result = run(waymark().arg("init").arg("--project").arg(&root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    (dir, root)
}

/// Create one item with the given title and return its stable ID.
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

/// Run a JSON-mode command against the project and require success.
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

/// Run a command and require failure, returning the exit code and the envelope.
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
fn status_set_moves_the_item_and_reports_the_transition() {
    let (_dir, root) = project();
    let id = create(&root, "Fix the thing");

    let envelope = json_ok(&root, &["status", "set", &id, "closed"]);
    assert_eq!(envelope["ok"], serde_json::json!(true));
    assert_eq!(envelope["data"]["id"], serde_json::json!(id));
    assert_eq!(
        envelope["data"]["previous_status"],
        serde_json::json!("open")
    );
    assert_eq!(envelope["data"]["status"], serde_json::json!("closed"));
    assert_eq!(envelope["data"]["changed"], serde_json::json!(true));
    assert_eq!(envelope["data"]["written"], serde_json::json!(true));
    assert!(envelope["data"]["closed_at"].is_string());
    assert_eq!(
        envelope["data"]["item"]["status"],
        serde_json::json!("closed")
    );
    assert_eq!(
        envelope["_meta"]["completeness"],
        serde_json::json!("scoped")
    );
}

#[test]
fn the_conveniences_are_the_same_transition_with_the_status_filled_in() {
    let (_dir, root) = project();
    let id = create(&root, "Fix the thing");

    let started = json_ok(&root, &["start", &id]);
    assert_eq!(started["data"]["status"], serde_json::json!("in_progress"));

    let closed = json_ok(&root, &["close", &id]);
    assert_eq!(closed["data"]["status"], serde_json::json!("closed"));
    assert!(closed["data"]["closed_at"].is_string());

    let reopened = json_ok(&root, &["reopen", &id]);
    assert_eq!(reopened["data"]["status"], serde_json::json!("open"));
    assert!(reopened["data"]["closed_at"].is_null());
}

#[test]
fn repeating_the_current_status_changes_nothing() {
    let (_dir, root) = project();
    let id = create(&root, "Fix the thing");

    let first = json_ok(&root, &["status", "set", &id, "in_progress"]);
    let second = json_ok(&root, &["status", "set", &id, "in_progress"]);

    assert_eq!(second["data"]["changed"], serde_json::json!(false));
    assert_eq!(second["data"]["written"], serde_json::json!(false));
    assert_eq!(
        second["data"]["item"]["updated_at"],
        first["data"]["item"]["updated_at"]
    );
}

#[test]
fn a_derived_status_word_is_rejected_as_a_usage_shaped_validation_error() {
    let (_dir, root) = project();
    let id = create(&root, "Fix the thing");

    let (code, envelope) = json_err(&root, &["status", "set", &id, "blocked"]);
    assert_eq!(code, 3);
    assert_eq!(envelope["ok"], serde_json::json!(false));
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("VALIDATION_ERROR")
    );
    assert_eq!(
        envelope["error"]["details"]["target"],
        serde_json::json!("status")
    );
}

#[test]
fn a_status_change_of_an_absent_item_exits_two() {
    let (_dir, root) = project();

    let (code, envelope) = json_err(&root, &["status", "set", "wm-00000000zzzz", "closed"]);
    assert_eq!(code, 2);
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("ITEM_NOT_FOUND")
    );
}

#[test]
fn the_owner_mismatch_warning_reaches_meta_and_does_not_fail_the_command() {
    let (dir, root) = walls_project();
    let id = create(&root, "Fix the thing");

    let result = run(waymark()
        .arg("--json")
        .arg("--project")
        .arg(&root)
        .arg("status")
        .arg("set")
        .arg(&id)
        .arg("closed"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let envelope = result.json();
    assert_eq!(
        envelope["_meta"]["warnings"][0]["code"],
        serde_json::json!("OWNER_MISMATCH")
    );
    assert!(envelope["_meta"]["warnings"][0]["message"]
        .as_str()
        .unwrap()
        .contains("frank"));
    drop(dir);
}

/// A project whose configured operator is somebody other than `frank`, so an
/// owner mismatch is guaranteed.
fn walls_project() -> (tempfile::TempDir, Utf8PathBuf) {
    let (dir, root) = tempdir();
    let result = run(waymark()
        .arg("init")
        .arg("--project")
        .arg(&root)
        .arg("--operator")
        .arg("someone-else"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    (dir, root)
}

#[test]
fn dependency_add_and_remove_update_the_item_and_its_readiness() {
    let (_dir, root) = project();
    let blocker = create(&root, "blocker");
    let dependent = create(&root, "dependent");

    let added = json_ok(&root, &["dependency", "add", &dependent, &blocker]);
    assert_eq!(added["data"]["dependency"], serde_json::json!(blocker));
    assert_eq!(added["data"]["dependencies"], serde_json::json!([blocker]));

    let blocked = json_ok(&root, &["blocked"]);
    assert_eq!(blocked["data"]["count"], serde_json::json!(1));
    assert_eq!(
        blocked["data"]["items"][0]["id"],
        serde_json::json!(dependent)
    );
    assert_eq!(
        blocked["data"]["items"][0]["blocked_by"],
        serde_json::json!([blocker])
    );
    assert_eq!(
        blocked["data"]["items"][0]["blocked"],
        serde_json::json!(true)
    );

    let removed = json_ok(&root, &["dependency", "remove", &dependent, &blocker]);
    assert_eq!(removed["data"]["dependencies"], serde_json::json!([]));

    let blocked = json_ok(&root, &["blocked"]);
    assert_eq!(blocked["data"]["count"], serde_json::json!(0));
}

#[test]
fn closing_the_blocker_makes_the_dependent_ready() {
    let (_dir, root) = project();
    let blocker = create(&root, "blocker");
    let dependent = create(&root, "dependent");
    json_ok(&root, &["dependency", "add", &dependent, &blocker]);

    let before = json_ok(&root, &["ready"]);
    assert_eq!(before["data"]["items"][0]["id"], serde_json::json!(blocker));

    json_ok(&root, &["close", &blocker]);

    let after = json_ok(&root, &["ready"]);
    let ids: Vec<&str> = after["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec![dependent.as_str()]);
}

#[test]
fn a_self_dependency_and_a_cycle_exit_three_with_the_contract_details() {
    let (_dir, root) = project();
    let a = create(&root, "a");
    let b = create(&root, "b");
    let c = create(&root, "c");
    json_ok(&root, &["dependency", "add", &b, &a]);
    json_ok(&root, &["dependency", "add", &c, &b]);

    let (code, envelope) = json_err(&root, &["dependency", "add", &a, &a]);
    assert_eq!(code, 3);
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("INVALID_RELATIONSHIP")
    );
    assert_eq!(
        envelope["error"]["details"]["relation"],
        serde_json::json!("dependencies")
    );

    let (code, envelope) = json_err(&root, &["dependency", "add", &a, &c]);
    assert_eq!(code, 3);
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("CYCLE_DETECTED")
    );
    let cycle = envelope["error"]["details"]["cycle"].as_array().unwrap();
    assert!(cycle.len() >= 4, "{cycle:?}");
    assert_eq!(cycle.first(), cycle.last());
}

#[test]
fn a_duplicate_dependency_is_rejected_rather_than_ignored() {
    let (_dir, root) = project();
    let a = create(&root, "a");
    let b = create(&root, "b");
    json_ok(&root, &["dependency", "add", &a, &b]);

    let (code, envelope) = json_err(&root, &["dependency", "add", &a, &b]);
    assert_eq!(code, 3);
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("INVALID_RELATIONSHIP")
    );
}

#[test]
fn a_dangling_dependency_blocks_and_is_reported_in_meta() {
    let (_dir, root) = project();
    let id = create(&root, "item");

    // A hand edit is the only way to create a dangling pointer; the operation
    // refuses it, so write the document directly.
    let document = root.join("work").join("task").join(format!("{id}.md"));
    let text = std::fs::read_to_string(&document).unwrap();
    assert!(text.starts_with("---\n"), "{text}");
    let edited = text.replacen("---\n", "---\ndependencies:\n  - wm-00000000zzzz\n", 1);
    std::fs::write(&document, edited).unwrap();

    let blocked = json_ok(&root, &["blocked"]);
    assert_eq!(blocked["data"]["count"], serde_json::json!(1));
    assert_eq!(
        blocked["data"]["items"][0]["blocked_by"],
        serde_json::json!(["wm-00000000zzzz"])
    );
    assert_eq!(
        blocked["_meta"]["diagnostics"][0]["code"],
        serde_json::json!("DANGLING_REFERENCE")
    );
    assert_eq!(
        blocked["_meta"]["completeness"],
        serde_json::json!("complete")
    );
}

#[test]
fn list_ready_and_blocked_all_report_a_complete_projection() {
    let (_dir, root) = project();
    let open = create(&root, "open");
    let closed = create(&root, "closed");
    json_ok(&root, &["close", &closed]);

    let list = json_ok(&root, &["list"]);
    assert_eq!(list["data"]["count"], serde_json::json!(2));
    assert_eq!(list["_meta"]["completeness"], serde_json::json!("complete"));
    assert_eq!(
        list["_meta"]["source"],
        serde_json::json!("memory_projection")
    );

    // Rows are ordered by stable ID, which is generated, not by creation time.
    let mut all_ids: Vec<&str> = list["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect();
    all_ids.sort();
    let mut expected = vec![open.as_str(), closed.as_str()];
    expected.sort();
    assert_eq!(all_ids, expected);

    let ready = json_ok(&root, &["ready"]);
    assert_eq!(ready["data"]["items"][0]["id"], serde_json::json!(open));
    assert_eq!(ready["data"]["items"][0]["ready"], serde_json::json!(true));

    // A page of items is frontmatter plus the derived columns: no body and no
    // path, because neither belongs to a query answer.
    let row = &list["data"]["items"][0];
    assert!(row["item"].get("body").is_none(), "{row}");
    assert!(row["item"].get("path").is_none(), "{row}");
    assert!(row.get("path").is_none(), "{row}");

    let blocked = json_ok(&root, &["blocked"]);
    assert_eq!(blocked["data"]["count"], serde_json::json!(0));
}

#[test]
fn list_filters_by_status_and_category() {
    let (_dir, root) = project();
    let open = create(&root, "open");
    let closed = create(&root, "closed");
    json_ok(&root, &["close", &closed]);

    let only_open = json_ok(&root, &["list", "--status", "open"]);
    assert_eq!(only_open["data"]["items"][0]["id"], serde_json::json!(open));

    let only_closed = json_ok(&root, &["list", "--status", "closed"]);
    assert_eq!(
        only_closed["data"]["items"][0]["id"],
        serde_json::json!(closed)
    );

    let only_bugs = json_ok(&root, &["list", "--category", "bug"]);
    assert_eq!(only_bugs["data"]["count"], serde_json::json!(0));

    let tasks = json_ok(&root, &["list", "--category", "task"]);
    assert_eq!(tasks["data"]["count"], serde_json::json!(2));
}

#[test]
fn list_in_human_mode_writes_a_table_to_stderr_and_nothing_to_stdout() {
    let (_dir, root) = project();
    let id = create(&root, "Fix the thing");

    let result = run(waymark().arg("list").arg("--project").arg(&root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    assert!(result.stdout().is_empty(), "{}", result.stdout());
    assert!(result.stderr().contains(&id), "{}", result.stderr());
    assert!(result.stderr().contains("Fix the thing"));
}

#[test]
fn the_human_narrative_names_the_edge_in_reading_order() {
    let (_dir, root) = project();
    let blocker = create(&root, "blocker");
    let dependent = create(&root, "dependent");

    let result = run(waymark()
        .arg("--project")
        .arg(&root)
        .arg("dependency")
        .arg("add")
        .arg(&dependent)
        .arg(&blocker));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    assert!(
        result
            .stderr()
            .contains(&format!("{dependent} depends on {blocker}")),
        "{}",
        result.stderr()
    );
}

#[test]
fn show_reports_readiness_children_and_dependents() {
    let (_dir, root) = project();
    let blocker = create(&root, "blocker");
    let dependent = create(&root, "dependent");
    json_ok(&root, &["dependency", "add", &dependent, &blocker]);

    let dependent_view = json_ok(&root, &["show", &dependent]);
    assert_eq!(dependent_view["data"]["ready"], serde_json::json!(false));
    assert_eq!(dependent_view["data"]["blocked"], serde_json::json!(true));
    assert_eq!(
        dependent_view["data"]["blocked_by"],
        serde_json::json!([blocker])
    );

    let blocker_view = json_ok(&root, &["show", &blocker]);
    assert_eq!(blocker_view["data"]["ready"], serde_json::json!(true));
    assert_eq!(
        blocker_view["data"]["relationships"]["dependents"],
        serde_json::json!([dependent])
    );
    assert_eq!(
        blocker_view["data"]["relationships"]["children"],
        serde_json::json!([])
    );
}

#[test]
fn show_keeps_its_scoped_meta_even_though_readiness_needs_the_whole_project() {
    let (_dir, root) = project();
    let id = create(&root, "Fix the thing");

    let envelope = json_ok(&root, &["show", &id]);
    assert_eq!(
        envelope["_meta"]["completeness"],
        serde_json::json!("scoped")
    );
    assert_eq!(
        envelope["_meta"]["source"],
        serde_json::json!("direct_file")
    );
}

#[test]
fn a_closed_item_never_reports_ready_or_blocked() {
    let (_dir, root) = project();
    let blocker = create(&root, "blocker");
    let dependent = create(&root, "dependent");
    json_ok(&root, &["dependency", "add", &dependent, &blocker]);
    json_ok(&root, &["close", &dependent]);

    // The closed dependent still waits for an open blocker, but it is not
    // blocked: closed work is not waiting on anything.
    let envelope = json_ok(&root, &["show", &dependent]);
    assert_eq!(envelope["data"]["ready"], serde_json::json!(false));
    assert_eq!(envelope["data"]["blocked"], serde_json::json!(false));
    assert_eq!(envelope["data"]["blocked_by"], serde_json::json!([]));
}

#[test]
fn show_still_reads_one_document_when_a_sibling_cannot_be_parsed() {
    let (_dir, root) = project();
    let id = create(&root, "readable");
    let broken = create(&root, "broken");

    // An external edit that no operation would make: the frontmatter no longer
    // closes. The document is unreadable, but it is not the one being shown.
    std::fs::write(
        root.join("work").join("task").join(format!("{broken}.md")),
        "---\nid: not even close\n",
    )
    .unwrap();

    let result = run(waymark()
        .arg("--json")
        .arg("--project")
        .arg(&root)
        .arg("show")
        .arg(&id));
    assert_eq!(
        result.status(),
        0,
        "a broken sibling must not block a direct-file read: {}",
        result.stderr()
    );
    let envelope = result.json();
    assert_eq!(envelope["data"]["id"], serde_json::json!(id));
    assert!(envelope["data"]["ready"].is_null());
    assert!(envelope["data"]["blocked"].is_null());
    assert_eq!(
        envelope["_meta"]["diagnostics"][0]["code"],
        serde_json::json!("INCOMPLETE_PROJECTION")
    );
    assert_eq!(
        envelope["_meta"]["completeness"],
        serde_json::json!("scoped")
    );
}

#[test]
fn a_global_graph_read_fails_explicitly_rather_than_returning_a_partial_page() {
    let (_dir, root) = project();
    create(&root, "readable");
    let broken = create(&root, "broken");
    std::fs::write(
        root.join("work").join("task").join(format!("{broken}.md")),
        "---\nid: not even close\n",
    )
    .unwrap();

    for command in ["list", "ready", "blocked"] {
        let mut invocation = waymark();
        invocation
            .arg("--json")
            .arg("--project")
            .arg(&root)
            .arg(command);
        let result = run(&mut invocation);
        assert_ne!(result.status(), 0, "{command} returned a partial page");
        let envelope = result.json();
        assert_eq!(envelope["ok"], serde_json::json!(false), "{command}");
        assert_eq!(
            envelope["error"]["code"],
            serde_json::json!("INCOMPLETE_PROJECTION"),
            "{command}"
        );
    }
}
