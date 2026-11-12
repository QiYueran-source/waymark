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

/// Create an item owned by `owner`.
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

fn document(root: &Utf8PathBuf, id: &str) -> String {
    std::fs::read_to_string(root.join("work").join("task").join(format!("{id}.md")))
        .expect("the document is readable")
}

#[test]
fn link_add_without_cascade_is_a_consent_refusal_that_writes_nothing() {
    let (_dir, root) = project();
    act_as(&root, "frank");
    let left = create(&root, "left");
    let right = create(&root, "right");
    let before = (document(&root, &left), document(&root, &right));

    let (status, envelope) = json_err(&root, &["link", "add", &left, &right]);

    assert_eq!(status, 4, "a refusal that consent can fix is a conflict");
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("CASCADE_CONSENT_REQUIRED")
    );
    assert_eq!(envelope["error"]["retryable"], serde_json::json!(false));
    assert_eq!(envelope["error"]["details"]["option"], "cascade");
    assert_eq!(envelope["error"]["details"]["id"], serde_json::json!(left));
    assert_eq!(
        envelope["error"]["details"]["other"],
        serde_json::json!(right)
    );
    let message = envelope["error"]["message"]
        .as_str()
        .expect("the error carries a message");
    assert!(
        message.contains("--cascade"),
        "the refusal names the option to re-run with: {message}"
    );
    assert_eq!(
        (document(&root, &left), document(&root, &right)),
        before,
        "a refusal writes neither document"
    );

    // The same request with the option the message named succeeds on both sides.
    let added = json_ok(&root, &["link", "add", &left, &right, "--cascade"]);
    assert_eq!(
        added["data"]["item"]["soft_links"],
        serde_json::json!([right])
    );
    assert_eq!(
        added["data"]["other_item"]["soft_links"],
        serde_json::json!([left])
    );
}

#[test]
fn link_add_against_another_owners_item_is_owned_by_other_with_or_without_consent() {
    let (_dir, root) = project();
    act_as(&root, "frank");
    let mine = create(&root, "mine");
    let theirs = create_owned_by(&root, "theirs", "alice");
    let before = (document(&root, &mine), document(&root, &theirs));

    for flags in [&[][..], &["--cascade"][..]] {
        let mut arguments = vec!["link", "add", mine.as_str(), theirs.as_str()];
        arguments.extend_from_slice(flags);
        let (status, envelope) = json_err(&root, &arguments);
        if flags.is_empty() {
            // Consent is the first answer: it is the only one the caller can
            // fix alone, and it costs nothing to take next.
            assert_eq!(envelope["error"]["code"], "CASCADE_CONSENT_REQUIRED");
            continue;
        }
        assert_eq!(status, 4);
        assert_eq!(
            envelope["error"]["code"],
            serde_json::json!("OWNED_BY_OTHER")
        );
        assert_eq!(envelope["error"]["retryable"], serde_json::json!(false));
        assert_eq!(
            envelope["error"]["details"]["id"],
            serde_json::json!(theirs)
        );
        assert_eq!(envelope["error"]["details"]["owner"], "alice");
        let message = envelope["error"]["message"]
            .as_str()
            .expect("the error carries a message");
        assert!(
            message.contains("re-running will not change the outcome"),
            "the refusal says retrying cannot help: {message}"
        );
    }
    assert_eq!(
        (document(&root, &mine), document(&root, &theirs)),
        before,
        "neither refusal wrote anything"
    );
}

#[test]
fn link_remove_without_cascade_reports_the_partner_left_alone() {
    let (_dir, root) = project();
    act_as(&root, "frank");
    let left = create(&root, "left");
    let right = create(&root, "right");
    json_ok(&root, &["link", "add", &left, &right, "--cascade"]);
    let partner = document(&root, &right);

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
        document(&root, &right),
        partner,
        "the partner keeps its bytes and its updated_at"
    );
}

#[test]
fn link_remove_cascade_against_another_owners_item_is_owned_by_other() {
    let (_dir, root) = project();
    act_as(&root, "frank");
    let mine = create(&root, "mine");
    let theirs = create_owned_by(&root, "theirs", "alice");
    // Record the link on both sides by hand: `link add` against a foreign
    // partner is refused now, so the fixture is the state under test.
    for (from, to) in [(&mine, &theirs), (&theirs, &mine)] {
        let path = root.join("work").join("task").join(format!("{from}.md"));
        let text = std::fs::read_to_string(&path).expect("the document is readable");
        let fence = text.rfind("\n---").expect("a frontmatter fence");
        let mut seeded = text[..fence].to_owned();
        seeded.push_str(&format!("\nsoft_links:\n  - {to}"));
        seeded.push_str(&text[fence..]);
        std::fs::write(&path, seeded).expect("the document is writable");
    }
    let before = (document(&root, &mine), document(&root, &theirs));

    let (status, envelope) = json_err(&root, &["link", "remove", &mine, &theirs, "--cascade"]);

    assert_eq!(status, 4);
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("OWNED_BY_OTHER")
    );
    assert_eq!(
        (document(&root, &mine), document(&root, &theirs)),
        before,
        "a refused removal writes neither document"
    );

    // Without consent the removal is mine to make and always succeeds.
    let removed = json_ok(&root, &["link", "remove", &mine, &theirs]);
    assert_eq!(removed["data"]["soft_links"], serde_json::json!([]));
    assert_eq!(document(&root, &theirs), before.1);
}

#[test]
fn a_repeated_link_add_without_cascade_repeats_the_consent_refusal() {
    let (_dir, root) = project();
    act_as(&root, "frank");
    let left = create(&root, "left");
    let right = create(&root, "right");
    let before = (document(&root, &left), document(&root, &right));

    for _ in 0..2 {
        let (status, envelope) = json_err(&root, &["link", "add", &left, &right]);
        assert_eq!(status, 4);
        assert_eq!(envelope["error"]["code"], "CASCADE_CONSENT_REQUIRED");
    }
    assert_eq!((document(&root, &left), document(&root, &right)), before);
}

#[test]
fn a_cross_item_write_with_no_operator_names_the_identity_source() {
    let (_dir, root) = project();
    let left = create(&root, "left");
    let right = create(&root, "right");

    let (status, envelope) = json_err(&root, &["link", "add", &left, &right, "--cascade"]);

    assert_eq!(status, 3, "a missing identity is a configuration failure");
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("CONFIG_INVALID")
    );
    let message = envelope["error"]["message"]
        .as_str()
        .expect("the error carries a message");
    assert!(
        message.contains("no operator identity is configured"),
        "the refusal names what is missing: {message}"
    );
    assert!(
        message.contains("user.toml"),
        "the refusal names the document that supplies it: {message}"
    );
    assert_eq!(envelope["error"]["location"]["key"], "user.toml");
}

#[test]
fn the_consent_refusal_is_what_the_human_output_reports() {
    let (_dir, root) = project();
    act_as(&root, "frank");
    let left = create(&root, "left");
    let right = create(&root, "right");

    let result = run(waymark()
        .arg("--project")
        .arg(&root)
        .arg("link")
        .arg("add")
        .arg(&left)
        .arg(&right));

    assert_eq!(result.status(), 4, "stderr: {}", result.stderr());
    let stderr = result.stderr();
    assert!(
        stderr.starts_with("error: "),
        "human output leads with the failure: {stderr}"
    );
    assert!(
        stderr.contains(left.as_str()),
        "the caller's item is named: {stderr}"
    );
    assert!(
        stderr.contains(right.as_str()),
        "the document that would also be written is named: {stderr}"
    );
    assert!(
        stderr.contains("--cascade"),
        "human output names the option to re-run with: {stderr}"
    );
    assert!(
        !stderr.contains("was not changed"),
        "no document was written, so nothing is reported as left alone: {stderr}"
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
fn link_accepts_cascade_on_both_subcommands_and_writes_both_sides() {
    let (_dir, root) = project();
    act_as(&root, "frank");
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
    act_as(&root, "frank");
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
    act_as(&root, "frank");
    let left = create(&root, "left");
    let right = create(&root, "right");
    json_ok(&root, &["link", "add", &left, &right, "--cascade"]);

    let (status, envelope) = json_err(&root, &["link", "add", &left, &right, "--cascade"]);

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
