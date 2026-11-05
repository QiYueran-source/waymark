//! `waymark migrate version|schema` at the adapter seam: argument shapes,
//! stream routing, envelope contents, and exit codes. The domain rules are
//! tested in `waymark-core`.

use std::process::{Command, Output};

use camino::{Utf8Path, Utf8PathBuf};

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

fn project() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 path");
    let init = run(waymark().arg("init").arg("--project").arg(&root));
    assert_eq!(init.status(), 0, "stderr: {}", init.stderr());
    (dir, root)
}

fn create_one(root: &Utf8Path) -> String {
    let result = run(waymark()
        .arg("create")
        .arg("--project")
        .arg(root)
        .arg("--title")
        .arg("Fix the thing")
        .arg("--category")
        .arg("task")
        .arg("--owner")
        .arg("frank")
        .arg("--json"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let envelope = result.json();
    envelope["data"]["id"]
        .as_str()
        .expect("the created ID is reported")
        .to_owned()
}

fn item_path(root: &Utf8Path, id: &str) -> Utf8PathBuf {
    root.join("work").join("task").join(format!("{id}.md"))
}

/// Rewrite the document's generation to the previous readable one.
fn downgrade(path: &Utf8Path) {
    let text = std::fs::read_to_string(path).expect("the document is readable");
    std::fs::write(path, text.replacen("schema: 1\n", "schema: 0\n", 1))
        .expect("the document is writable");
}

fn migrate(root: &Utf8Path, args: &[&str]) -> Run {
    let mut command = waymark();
    command.arg("migrate");
    for argument in args {
        command.arg(argument);
    }
    run(command.arg("--project").arg(root))
}

#[test]
fn a_version_migration_previews_then_writes_one_envelope_each_time() {
    let (_dir, root) = project();
    let id = create_one(&root);
    let path = item_path(&root, &id);
    downgrade(&path);

    let preview = migrate(&root, &["version", "--to", "1", "--dry-run", "--json"]);
    assert_eq!(preview.status(), 0, "stderr: {}", preview.stderr());
    let envelope = preview.json();
    assert_eq!(envelope["ok"], serde_json::json!(true));
    assert_eq!(envelope["data"]["migration"], serde_json::json!("version"));
    assert_eq!(envelope["data"]["dry_run"], serde_json::json!(true));
    assert_eq!(envelope["data"]["count"], serde_json::json!(1));
    assert_eq!(envelope["data"]["written"], serde_json::json!(false));
    assert_eq!(
        envelope["data"]["targets"][0]["outcome"],
        serde_json::json!("skipped")
    );
    assert!(std::fs::read_to_string(&path)
        .expect("the document is readable")
        .contains("schema: 0"));

    let real = migrate(&root, &["version", "--to", "1", "--json"]);
    assert_eq!(real.status(), 0, "stderr: {}", real.stderr());
    let envelope = real.json();
    assert_eq!(envelope["data"]["written"], serde_json::json!(true));
    assert_eq!(envelope["data"]["count"], serde_json::json!(1));
    assert_eq!(
        envelope["data"]["targets"][0]["outcome"],
        serde_json::json!("applied")
    );
    assert!(std::fs::read_to_string(&path)
        .expect("the document is readable")
        .contains("schema: 1"));
}

#[test]
fn an_unsupported_target_generation_exits_with_the_version_class() {
    let (_dir, root) = project();
    let result = migrate(&root, &["version", "--to", "2", "--json"]);
    assert_eq!(result.status(), 6);
    let envelope = result.json();
    assert_eq!(envelope["ok"], serde_json::json!(false));
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("MIGRATION_UNSUPPORTED_GENERATION")
    );
    assert_eq!(
        envelope["error"]["details"]["requested"],
        serde_json::json!(2)
    );
}

#[test]
fn a_stale_item_is_refused_by_a_mediated_write_but_show_marks_it() {
    let (_dir, root) = project();
    let id = create_one(&root);
    let path = item_path(&root, &id);
    downgrade(&path);

    let refused = run(waymark()
        .arg("title")
        .arg("set")
        .arg(&id)
        .arg("Changed")
        .arg("--project")
        .arg(&root)
        .arg("--json"));
    assert_eq!(refused.status(), 6);
    assert_eq!(
        refused.json()["error"]["code"],
        serde_json::json!("ITEM_NEEDS_MIGRATION")
    );

    let shown = run(waymark()
        .arg("show")
        .arg(&id)
        .arg("--project")
        .arg(&root)
        .arg("--json"));
    assert_eq!(shown.status(), 0, "stderr: {}", shown.stderr());
    let envelope = shown.json();
    assert_eq!(envelope["data"]["needs_migration"], serde_json::json!(true));
    assert_eq!(envelope["data"]["item"]["schema"], serde_json::json!(0));
    assert_eq!(
        envelope["data"]["item"]["title"],
        serde_json::json!("Fix the thing")
    );
}

#[test]
fn a_schema_migration_adds_a_required_field_and_applies_its_default() {
    let (_dir, root) = project();
    let id = create_one(&root);

    let result = migrate(
        &root,
        &[
            "schema",
            "--add-required-field",
            "effort",
            "--type",
            "integer",
            "--default",
            "3",
            "--json",
        ],
    );
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let envelope = result.json();
    assert_eq!(envelope["data"]["migration"], serde_json::json!("schema"));
    assert_eq!(envelope["data"]["written"], serde_json::json!(true));
    assert_eq!(envelope["data"]["config_changed"], serde_json::json!(true));
    assert_eq!(envelope["data"]["count"], serde_json::json!(1));

    let shown = run(waymark()
        .arg("show")
        .arg(&id)
        .arg("--project")
        .arg(&root)
        .arg("--json"));
    let envelope = shown.json();
    assert_eq!(
        envelope["data"]["item"]["custom_fields"]["effort"],
        serde_json::json!(3)
    );

    let config = run(waymark()
        .arg("config")
        .arg("show")
        .arg("--project")
        .arg(&root));
    let text = config.stdout();
    assert!(text.contains("[fields.effort]"), "{text}");
    assert!(text.contains("required = true"), "{text}");
}

#[test]
fn migrating_schema_without_an_action_is_a_usage_error() {
    let (_dir, root) = project();
    let result = migrate(&root, &["schema", "--json"]);
    assert_eq!(result.status(), 1);
    assert_eq!(
        result.json()["error"]["code"],
        serde_json::json!("USAGE_ERROR")
    );
}

#[test]
fn human_output_goes_to_stderr_and_stdout_stays_empty() {
    let (_dir, root) = project();
    let id = create_one(&root);
    downgrade(&item_path(&root, &id));

    let result = migrate(&root, &["version", "--to", "1", "--dry-run"]);
    assert_eq!(result.status(), 0);
    assert_eq!(result.stdout(), "", "a human run writes no envelope");
    assert!(
        result.stderr().contains("dry run; nothing written"),
        "stderr: {}",
        result.stderr()
    );
    assert!(result.stderr().contains(&id), "stderr: {}", result.stderr());
}

#[test]
fn a_migration_outside_git_carries_the_warning_in_meta() {
    let (_dir, root) = project();
    let id = create_one(&root);
    downgrade(&item_path(&root, &id));

    let preview = migrate(&root, &["version", "--to", "1", "--dry-run", "--json"]);
    assert_eq!(preview.status(), 0, "stderr: {}", preview.stderr());
    let envelopes = preview.json();
    let warnings = envelopes["_meta"]["warnings"]
        .as_array()
        .expect("the warnings array is present");
    if let Some(warning) = warnings
        .iter()
        .find(|warning| warning["code"] == serde_json::json!("NOT_A_GIT_REPOSITORY"))
    {
        assert!(warning["message"]
            .as_str()
            .expect("a message")
            .contains("Git"));
    }
}

// -- storage migration ------------------------------------------------------

#[test]
fn a_storage_migration_previews_then_moves_the_root() {
    let (_dir, root) = project();
    let id = create_one(&root);
    let old = item_path(&root, &id);

    let preview = migrate(
        &root,
        &["storage", "--root", "items", "--dry-run", "--json"],
    );
    assert_eq!(preview.status(), 0, "stderr: {}", preview.stderr());
    let envelope = preview.json();
    assert_eq!(envelope["ok"], serde_json::json!(true));
    assert_eq!(envelope["data"]["migration"], serde_json::json!("storage"));
    assert_eq!(envelope["data"]["dry_run"], serde_json::json!(true));
    assert_eq!(envelope["data"]["written"], serde_json::json!(false));
    assert_eq!(envelope["data"]["revision"], serde_json::json!(1));
    assert_eq!(
        envelope["data"]["moves"][0]["outcome"],
        serde_json::json!("skipped")
    );
    assert!(envelope["data"]["moves"][0]["to"]
        .as_str()
        .expect("a target")
        .ends_with("items/task"));
    assert_eq!(envelope["data"]["rewritten"], serde_json::json!([]));
    assert!(old.exists(), "the dry run moved nothing");
    assert!(
        std::fs::read_to_string(root.join(".waymark").join("config.toml"))
            .expect("the configuration")
            .contains("storage_root = \"work\"")
    );

    let real = migrate(&root, &["storage", "--root", "items", "--json"]);
    assert_eq!(real.status(), 0, "stderr: {}", real.stderr());
    let envelope = real.json();
    assert_eq!(envelope["data"]["written"], serde_json::json!(true));
    assert_eq!(
        envelope["data"]["moves"][0]["outcome"],
        serde_json::json!("applied")
    );
    assert!(!old.exists());
    assert!(root
        .join("items")
        .join("task")
        .join(format!("{id}.md"))
        .exists());
    let config = std::fs::read_to_string(root.join(".waymark").join("config.toml"))
        .expect("the configuration");
    assert!(config.contains("storage_root = \"items\""), "{config}");
    assert!(config.contains("version = \"1.1\""), "{config}");
}

#[test]
fn a_storage_key_migration_rewrites_the_category_value() {
    let (_dir, root) = project();
    let id = create_one(&root);

    let result = migrate(
        &root,
        &["storage", "--category-key", "task", "chore", "--json"],
    );
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let envelope = result.json();
    assert_eq!(envelope["data"]["migration"], serde_json::json!("storage"));
    assert_eq!(envelope["data"]["count"], serde_json::json!(1));
    assert_eq!(
        envelope["data"]["rewritten"][0]["from"],
        serde_json::json!("task")
    );
    assert_eq!(
        envelope["data"]["rewritten"][0]["to"],
        serde_json::json!("chore")
    );
    assert!(!root.join("work").join("task").exists());
    let moved = root.join("work").join("chore").join(format!("{id}.md"));
    let document = std::fs::read_to_string(&moved).expect("the moved document");
    assert!(document.contains("category: chore"), "{document}");
}

#[test]
fn storage_actions_are_mutually_exclusive() {
    let (_dir, root) = project();
    let result = migrate(
        &root,
        &[
            "storage",
            "--root",
            "items",
            "--category-path",
            "task",
            "work/other",
            "--json",
        ],
    );
    assert_eq!(result.status(), 1);
    assert_eq!(
        result.json()["error"]["code"],
        serde_json::json!("USAGE_ERROR")
    );
}

#[test]
fn a_storage_migration_outside_git_carries_the_warning_in_meta() {
    let (_dir, root) = project();
    create_one(&root);

    let preview = migrate(
        &root,
        &["storage", "--root", "items", "--dry-run", "--json"],
    );
    assert_eq!(preview.status(), 0, "stderr: {}", preview.stderr());
    let envelope = preview.json();
    let warnings = envelope["_meta"]["warnings"]
        .as_array()
        .expect("the warnings array is present");
    if let Some(warning) = warnings
        .iter()
        .find(|warning| warning["code"] == serde_json::json!("NOT_A_GIT_REPOSITORY"))
    {
        assert!(warning["message"]
            .as_str()
            .expect("a message")
            .contains("Git"));
    }
}
