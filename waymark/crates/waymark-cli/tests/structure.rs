//! `parent`, `link`, `tree`, and `dep tree` end to end.
//!
//! These drive the real binary because the adapter owns three things the domain
//! cannot: the argument shapes, which stream a result goes to, and the exit code.
//! The rules themselves live in `waymark-core` and are covered there.

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

#[test]
fn parent_set_then_clear_round_trips_through_the_envelope() {
    let (_dir, root) = project();
    let parent = create(&root, "epic");
    let child = create(&root, "story");

    let set = json_ok(&root, &["parent", "set", &child, &parent]);
    assert_eq!(set["ok"], serde_json::json!(true));
    assert_eq!(set["data"]["parent"], serde_json::json!(parent));
    assert_eq!(set["data"]["previous_parent"], serde_json::Value::Null);
    assert_eq!(set["data"]["changed"], serde_json::json!(true));
    assert_eq!(set["data"]["written"], serde_json::json!(true));
    assert_eq!(set["data"]["verb"], serde_json::json!("set"));

    let cleared = json_ok(&root, &["parent", "clear", &child]);
    assert_eq!(cleared["data"]["parent"], serde_json::Value::Null);
    assert_eq!(
        cleared["data"]["previous_parent"],
        serde_json::json!(parent)
    );
    assert_eq!(cleared["data"]["verb"], serde_json::json!("clear"));
}

#[test]
fn parent_set_of_the_existing_parent_reports_no_write() {
    let (_dir, root) = project();
    let parent = create(&root, "epic");
    let child = create(&root, "story");
    json_ok(&root, &["parent", "set", &child, &parent]);

    let again = json_ok(&root, &["parent", "set", &child, &parent]);

    assert_eq!(again["data"]["changed"], serde_json::json!(false));
    assert_eq!(again["data"]["written"], serde_json::json!(false));
}

#[test]
fn a_parent_cycle_exits_with_the_registry_class_and_names_the_kind() {
    let (_dir, root) = project();
    let a = create(&root, "a");
    let b = create(&root, "b");
    json_ok(&root, &["parent", "set", &a, &b]);

    let (status, envelope) = json_err(&root, &["parent", "set", &b, &a]);

    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("CYCLE_DETECTED")
    );
    assert_eq!(
        envelope["error"]["details"]["kind"],
        serde_json::json!("parent")
    );
    assert_eq!(status, 3, "invalid input");
}

#[test]
fn parent_dry_run_reports_the_change_without_writing_it() {
    let (_dir, root) = project();
    let parent = create(&root, "epic");
    let child = create(&root, "story");

    let preview = json_ok(&root, &["parent", "set", &child, &parent, "--dry-run"]);

    assert_eq!(preview["data"]["changed"], serde_json::json!(true));
    assert_eq!(preview["data"]["written"], serde_json::json!(false));
    let shown = json_ok(&root, &["show", &child]);
    assert_eq!(shown["data"]["item"]["parent"], serde_json::Value::Null);
}

#[test]
fn link_add_consents_to_both_documents_but_remove_needs_its_own() {
    let (_dir, root) = project();
    let left = create(&root, "left");
    let right = create(&root, "right");

    let added = json_ok(&root, &["link", "add", &left, &right]);
    assert_eq!(added["data"]["other"], serde_json::json!(right));
    assert_eq!(
        added["data"]["item"]["soft_links"],
        serde_json::json!([right])
    );
    assert_eq!(
        added["data"]["other_item"]["soft_links"],
        serde_json::json!([left])
    );
    assert_eq!(
        added["data"]["fingerprints"].as_array().map(|f| f.len()),
        Some(2),
        "both documents were replaced"
    );

    let removed = json_ok(&root, &["link", "remove", &left, &right, "--cascade"]);
    assert_eq!(removed["data"]["soft_links"], serde_json::json!([]));
    assert_eq!(
        removed["data"]["other_item"]["soft_links"],
        serde_json::json!([])
    );
}

#[test]
fn link_remove_without_cascade_reports_the_partner_left_alone() {
    let (_dir, root) = project();
    let left = create(&root, "left");
    let right = create(&root, "right");
    json_ok(&root, &["link", "add", &left, &right, "--cascade"]);
    // The operator of a removal is the one it acts as, not the item's owner:
    // the CLI's `--owner` is recorded at creation and never inherited.
    json_ok(&root, &["config", "set", "operator", "frank"]);
    let partner_path = root.join(format!("work/task/{right}.md"));
    let partner = std::fs::read_to_string(&partner_path).unwrap();

    let removed = json_ok(&root, &["link", "remove", &left, &right]);

    assert_eq!(removed["data"]["soft_links"], serde_json::json!([]));
    assert_eq!(removed["data"]["other_item"], serde_json::Value::Null);
    assert_eq!(
        removed["data"]["fingerprints"].as_array().map(|f| f.len()),
        Some(1),
        "only the caller's document was replaced"
    );
    assert_eq!(
        removed["data"]["other_untouched"]["id"],
        serde_json::json!(right)
    );
    assert_eq!(removed["data"]["other_untouched"]["owner"], "frank");
    assert_eq!(
        removed["data"]["other_untouched"]["owned"],
        serde_json::json!(true)
    );
    assert_eq!(
        std::fs::read_to_string(&partner_path).unwrap(),
        partner,
        "the partner keeps its bytes and its updated_at"
    );
}

#[test]
fn link_remove_reports_an_absent_link_the_same_with_or_without_consent() {
    let (_dir, root) = project();
    let left = create(&root, "left");
    let right = create(&root, "right");

    let (plain, error) = json_err(&root, &["link", "remove", &left, &right]);
    assert_eq!(error["error"]["code"], "INVALID_RELATIONSHIP");
    let (cascaded, consenting) = json_err(&root, &["link", "remove", &left, &right, "--cascade"]);
    assert_eq!(consenting["error"]["code"], "INVALID_RELATIONSHIP");
    assert_eq!(plain, cascaded, "consent does not change the refusal");
}

#[test]
fn link_accepts_cascade_on_both_subcommands_and_keeps_todays_behaviour() {
    let (_dir, root) = project();
    let left = create(&root, "left");
    let right = create(&root, "right");

    let added = json_ok(&root, &["link", "add", &left, &right, "--cascade"]);
    assert_eq!(
        added["data"]["item"]["soft_links"],
        serde_json::json!([right])
    );
    assert_eq!(
        added["data"]["other_item"]["soft_links"],
        serde_json::json!([left])
    );

    let removed = json_ok(&root, &["link", "remove", &left, &right, "--cascade"]);
    assert_eq!(removed["data"]["soft_links"], serde_json::json!([]));
    assert_eq!(
        removed["data"]["other_item"]["soft_links"],
        serde_json::json!([])
    );
}

#[test]
fn cascade_is_accepted_on_a_link_dry_run() {
    let (_dir, root) = project();
    let left = create(&root, "left");
    let right = create(&root, "right");

    let preview = json_ok(
        &root,
        &["link", "add", &left, &right, "--cascade", "--dry-run"],
    );

    assert_eq!(preview["data"]["written"], serde_json::json!(false));
    let shown = json_ok(&root, &["show", &left]);
    assert_eq!(shown["data"]["item"]["soft_links"], serde_json::json!([]));
}

#[test]
fn a_duplicate_link_is_an_invalid_relationship() {
    let (_dir, root) = project();
    let left = create(&root, "left");
    let right = create(&root, "right");
    json_ok(&root, &["link", "add", &left, &right]);

    let (status, envelope) = json_err(&root, &["link", "add", &left, &right]);

    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("INVALID_RELATIONSHIP")
    );
    assert_eq!(
        envelope["error"]["details"]["relation"],
        serde_json::json!("soft_links")
    );
    assert_eq!(status, 3);
}

#[test]
fn tree_renders_parent_edges_and_reports_unresolved_items() {
    let (_dir, root) = project();
    let parent = create(&root, "epic");
    let child = create(&root, "story");
    json_ok(&root, &["parent", "set", &child, &parent]);

    // An orphan whose parent does not exist, written behind the tool's back.
    let orphan = create(&root, "orphan");
    let path = root.join("work").join("task").join(format!("{orphan}.md"));
    let text = std::fs::read_to_string(&path).unwrap();
    let patched = text.replace(
        &format!("id: {orphan}"),
        &format!("id: {orphan}\nparent: wm-0000000000zz"),
    );
    std::fs::write(&path, patched).unwrap();

    let human = run(waymark().arg("--project").arg(&root).arg("tree"));
    assert_eq!(human.status(), 0, "stderr: {}", human.stderr());
    assert!(human.stdout().is_empty(), "human output never uses stdout");
    let lines = human.stderr();
    assert!(
        lines.contains(&format!("{parent} epic")),
        "the root is rendered:\n{lines}"
    );
    assert!(
        lines.contains(&format!("  - {child} story")),
        "a child is indented under a parent edge:\n{lines}"
    );
    assert!(
        lines.contains("unresolved relations:") && lines.contains("parent_missing"),
        "an orphan is reported, not promoted:\n{lines}"
    );

    let envelope = json_ok(&root, &["tree"]);
    assert_eq!(
        envelope["_meta"]["completeness"],
        serde_json::json!("complete")
    );
    let roots = envelope["data"]["roots"].as_array().unwrap();
    let epic = roots
        .iter()
        .find(|node| node["id"] == serde_json::json!(parent))
        .expect("the parent is a root");
    assert_eq!(epic["children"][0]["id"], serde_json::json!(child));
    assert_eq!(
        envelope["data"]["unresolved"][0]["reason"],
        serde_json::json!("parent_missing")
    );
}

#[test]
fn tree_depth_bounds_the_read() {
    let (_dir, root) = project();
    let top = create(&root, "top");
    let middle = create(&root, "middle");
    let bottom = create(&root, "bottom");
    json_ok(&root, &["parent", "set", &middle, &top]);
    json_ok(&root, &["parent", "set", &bottom, &middle]);

    let shallow = json_ok(&root, &["tree", "--root", &top, "--depth", "1"]);
    let root_node = &shallow["data"]["roots"][0];
    assert_eq!(root_node["id"], serde_json::json!(top));
    assert_eq!(root_node["children"][0]["id"], serde_json::json!(middle));
    assert!(
        root_node["children"][0]["children"]
            .as_array()
            .unwrap()
            .is_empty(),
        "depth 1 stops below the first level"
    );

    let alone = json_ok(&root, &["tree", "--root", &top, "--depth", "0"]);
    assert!(alone["data"]["roots"][0]["children"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn dep_tree_renders_dependency_edges_in_both_forms() {
    let (_dir, root) = project();
    let top = create(&root, "top");
    let blocker = create(&root, "blocker");
    json_ok(&root, &["dependency", "add", &top, &blocker]);

    let nested = run(waymark()
        .arg("--project")
        .arg(&root)
        .arg("dep")
        .arg("tree")
        .arg(&top));
    assert_eq!(nested.status(), 0, "stderr: {}", nested.stderr());
    assert!(
        nested.stderr().contains(&format!("  -> {blocker} blocker")),
        "a dependency edge uses its own marker:\n{}",
        nested.stderr()
    );

    let flat = json_ok(&root, &["dep", "tree", &top, "--flat"]);
    assert_eq!(flat["data"]["flat"], serde_json::json!(true));
    let rows = flat["data"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "pre-order: the start, then its blocker");
    assert_eq!(rows[0]["id"], serde_json::json!(top));
    assert_eq!(rows[0]["depth"], serde_json::json!(0));
    assert_eq!(rows[1]["id"], serde_json::json!(blocker));
    assert_eq!(rows[1]["from"], serde_json::json!(top));
}

#[test]
fn a_missing_tree_root_exits_with_item_not_found() {
    let (_dir, root) = project();

    let (status, envelope) = json_err(&root, &["tree", "--root", "wm-0000000000zz"]);
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("ITEM_NOT_FOUND")
    );
    assert_eq!(status, 2, "not found is its own exit class");

    let (status, envelope) = json_err(&root, &["dep", "tree", "wm-0000000000zz"]);
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("ITEM_NOT_FOUND")
    );
    assert_eq!(status, 2);
}
