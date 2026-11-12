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
    create_owned_by(root, title, "frank")
}

fn create_owned_by(root: &Utf8PathBuf, title: &str, owner: &str) -> String {
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
        .arg(owner));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    result.json()["data"]["id"]
        .as_str()
        .expect("the envelope names the new id")
        .to_owned()
}

/// Make `operator` the acting identity, the way a person does.
fn act_as(root: &Utf8PathBuf, operator: &str) {
    json_ok(root, &["config", "set", "operator", operator]);
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

/// Record a soft link on one side by editing the document, leaving the partner
/// exactly as it is.
fn seed_soft_link(root: &Utf8PathBuf, from: &str, to: &str) {
    let path = root.join("work").join("task").join(format!("{from}.md"));
    let mut text = std::fs::read_to_string(&path).expect("the document is readable");
    if !text.contains("\nsoft_links:") {
        // Insert the key in canonical position: after `updated_at` and any
        // `closed_at`, before the closing fence.
        let fence = text.rfind("\n---").expect("a frontmatter fence");
        text.insert_str(fence, &format!("\nsoft_links:\n  - {to}"));
    }
    std::fs::write(&path, text).expect("the document is writable");
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
fn archive_with_cascade_cleans_inbound_edges_and_reports_each_removal() {
    let (_dir, root) = project();
    act_as(&root, "frank");
    let target = create(&root, "target");
    let dependent = create(&root, "dependent");
    json_ok(&root, &["dependency", "add", &dependent, &target]);

    let envelope = json_ok(&root, &["archive", &target, "--cascade"]);

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
        serde_json::json!([]),
        "without consent nothing is removed, not even a preview"
    );
    let untouched = envelope["data"]["untouched"]
        .as_array()
        .expect("untouched is a list");
    assert_eq!(untouched.len(), 1);
    assert_eq!(untouched[0]["id"], serde_json::json!(dependent));
    let relations: Vec<&str> = untouched[0]["relations"]
        .as_array()
        .expect("relations is a list")
        .iter()
        .map(|value| value.as_str().expect("a relation literal"))
        .collect();
    assert_eq!(relations, vec!["dependencies"]);
    assert_eq!(
        document(&root, &target),
        target_before,
        "nothing was written"
    );
    assert_eq!(document(&root, &dependent), dependent_before);
}

#[test]
fn cascade_is_accepted_on_a_dry_run_too() {
    let (_dir, root) = project();
    act_as(&root, "frank");
    let target = create(&root, "target");
    let dependent = create(&root, "dependent");
    json_ok(&root, &["dependency", "add", &dependent, &target]);
    let before = document(&root, &target);
    let dependent_before = document(&root, &dependent);

    let envelope = json_ok(&root, &["archive", &target, "--cascade", "--dry-run"]);

    assert_eq!(envelope["data"]["written"], serde_json::json!(false));
    assert_eq!(
        envelope["data"]["removals"],
        serde_json::json!([{"from": dependent, "relation": "dependencies"}])
    );
    assert_eq!(envelope["data"]["untouched"], serde_json::json!([]));
    assert_eq!(document(&root, &target), before, "nothing was written");
    assert_eq!(document(&root, &dependent), dependent_before);
}

#[test]
fn archive_without_cascade_leaves_a_foreign_document_alone_and_names_it() {
    let (_dir, root) = project();
    act_as(&root, "frank");
    let target = create(&root, "target");
    let theirs = create_owned_by(&root, "theirs", "alice");
    let mine = create(&root, "mine");
    json_ok(&root, &["dependency", "add", &theirs, &target]);
    // A soft link is a cross-item write, so it is recorded by hand here: the
    // relation under archive is what this test is about, not how the link got
    // there.
    seed_soft_link(&root, &mine, &target);
    let theirs_before = document(&root, &theirs);
    let mine_before = document(&root, &mine);

    let envelope = json_ok(&root, &["archive", &target]);

    assert_eq!(envelope["ok"], serde_json::json!(true));
    assert_eq!(envelope["data"]["written"], serde_json::json!(true));
    assert_eq!(envelope["data"]["removals"], serde_json::json!([]));
    assert_eq!(
        envelope["data"]["affected"],
        serde_json::json!([]),
        "not even my own document is rewritten without consent"
    );
    let untouched = envelope["data"]["untouched"]
        .as_array()
        .expect("untouched is a list");
    // Untouched documents are ordered by stable ID, not by the order the
    // fixtures were created in.
    let mut expected = vec![theirs.as_str(), mine.as_str()];
    expected.sort_unstable();
    let ids: Vec<&str> = untouched
        .iter()
        .map(|item| item["id"].as_str().expect("an id"))
        .collect();
    assert_eq!(ids, expected);
    let relation_of = |id: &str| {
        let entry = untouched
            .iter()
            .find(|item| item["id"] == serde_json::json!(id))
            .expect("the document is named");
        let relations: Vec<&str> = entry["relations"]
            .as_array()
            .expect("relations is a list")
            .iter()
            .map(|value| value.as_str().expect("a relation literal"))
            .collect();
        (relations, entry["path"].clone())
    };
    let (relations, path) = relation_of(&theirs);
    assert_eq!(relations, vec!["dependencies"]);
    assert_eq!(
        path,
        serde_json::json!(root
            .join("work")
            .join("task")
            .join(format!("{theirs}.md"))
            .to_string())
    );
    let (relations, _) = relation_of(&mine);
    assert_eq!(relations, vec!["soft_links"]);
    assert_eq!(document(&root, &theirs), theirs_before);
    assert_eq!(document(&root, &mine), mine_before);
}

#[test]
fn archive_with_cascade_cleans_my_documents_and_names_the_foreign_ones() {
    let (_dir, root) = project();
    act_as(&root, "frank");
    let target = create(&root, "target");
    let theirs = create_owned_by(&root, "theirs", "alice");
    let mine = create(&root, "mine");
    json_ok(&root, &["dependency", "add", &theirs, &target]);
    json_ok(&root, &["dependency", "add", &mine, &target]);
    let theirs_before = document(&root, &theirs);

    let envelope = json_ok(&root, &["archive", &target, "--cascade"]);

    assert_eq!(
        envelope["data"]["removals"],
        serde_json::json!([{"from": mine, "relation": "dependencies"}])
    );
    assert_eq!(
        envelope["data"]["affected"],
        serde_json::json!([{
            "id": mine,
            "path": root.join("work").join("task").join(format!("{mine}.md")).to_string(),
        }])
    );
    let untouched = envelope["data"]["untouched"]
        .as_array()
        .expect("untouched is a list");
    assert_eq!(untouched.len(), 1);
    assert_eq!(untouched[0]["id"], serde_json::json!(theirs));
    assert!(!document(&root, &mine).contains(&target));
    assert_eq!(document(&root, &theirs), theirs_before);
}

#[test]
fn a_dry_run_predicts_exactly_what_the_real_run_does() {
    for cascade in [false, true] {
        let (_dir, root) = project();
        act_as(&root, "frank");
        let target = create(&root, "target");
        let theirs = create_owned_by(&root, "theirs", "alice");
        let mine = create(&root, "mine");
        json_ok(&root, &["dependency", "add", &theirs, &target]);
        json_ok(&root, &["dependency", "add", &mine, &target]);

        let flags: &[&str] = if cascade { &["--cascade"] } else { &[] };
        let mut preview_arguments = vec!["archive", target.as_str(), "--dry-run"];
        preview_arguments.extend_from_slice(flags);
        let preview = json_ok(&root, &preview_arguments);
        assert_eq!(
            preview["data"]["written"],
            serde_json::json!(false),
            "the preview wrote nothing"
        );
        let mine_before = document(&root, &mine);
        let theirs_before = document(&root, &theirs);
        assert_eq!(
            document(&root, &mine),
            mine_before,
            "the preview wrote nothing"
        );
        assert_eq!(document(&root, &theirs), theirs_before);

        let mut real_arguments = vec!["archive", target.as_str()];
        real_arguments.extend_from_slice(flags);
        let real = json_ok(&root, &real_arguments);
        assert_eq!(preview["data"]["removals"], real["data"]["removals"]);
        assert_eq!(preview["data"]["affected"], real["data"]["affected"]);
        assert_eq!(preview["data"]["untouched"], real["data"]["untouched"]);
        assert_eq!(document(&root, &theirs), theirs_before);
        assert_eq!(real["data"]["written"], serde_json::json!(true));
    }
}

#[test]
fn a_child_refuses_the_archive_even_with_cascade() {
    let (_dir, root) = project();
    let parent = create(&root, "parent");
    let child = create(&root, "child");
    json_ok(&root, &["parent", "set", &child, &parent]);

    let (code, envelope) = json_err(&root, &["archive", &parent, "--cascade"]);

    assert_eq!(code, 4);
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("HAS_ACTIVE_CHILDREN")
    );
    assert!(!document(&root, &parent).contains("archived_at"));
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
