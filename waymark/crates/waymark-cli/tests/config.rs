//! The configuration commands at the adapter seam: argument parsing, output
//! routing, and exit codes. The domain rules are tested in `waymark-core`.
//!
//! These tests deliberately assert what the *adapter* decides: which stream a
//! thing goes to, what `--json` contains, and which exit code the registry gives
//! a refusal. They never re-implement a validation rule.

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

fn project() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 path");
    let init = run(waymark().arg("init").arg("--project").arg(&root));
    assert_eq!(init.status(), 0, "stderr: {}", init.stderr());
    (dir, root)
}

fn config(root: &Utf8PathBuf, args: &[&str]) -> Run {
    let mut command = waymark();
    command.arg("config");
    for argument in args {
        command.arg(argument);
    }
    run(command.arg("--project").arg(root))
}

fn create_one(root: &Utf8PathBuf, category: &str) -> String {
    let result = run(waymark()
        .arg("create")
        .arg("--project")
        .arg(root)
        .arg("--title")
        .arg("Fix the thing")
        .arg("--category")
        .arg(category)
        .arg("--owner")
        .arg("frank"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    result.stdout()
}

/// The canonical configuration bytes of a project.
fn config_bytes(root: &Utf8PathBuf) -> String {
    std::fs::read_to_string(root.join(".waymark").join("config.toml")).unwrap()
}

/// Every work-item file as (path, bytes).
fn items(root: &Utf8PathBuf) -> Vec<(Utf8PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let work = root.join("work");
    for entry in std::fs::read_dir(&work).expect("the storage root exists") {
        let entry = entry.unwrap();
        let path = Utf8PathBuf::from_path_buf(entry.path()).unwrap();
        if entry.file_type().unwrap().is_dir() {
            for file in std::fs::read_dir(&path).unwrap() {
                let file = file.unwrap();
                let path = Utf8PathBuf::from_path_buf(file.path()).expect("a UTF-8 path");
                found.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
    }
    found.sort();
    found
}

#[test]
fn config_show_prints_the_document_in_human_mode_and_json_in_machine_mode() {
    let (_dir, root) = project();
    let human = config(&root, &["show"]);
    assert_eq!(human.status(), 0, "stderr: {}", human.stderr());
    assert_eq!(human.stdout(), config_bytes(&root));

    let machine = run(waymark()
        .arg("--json")
        .arg("config")
        .arg("show")
        .arg("--project")
        .arg(&root));
    assert_eq!(machine.status(), 0);
    let data = &machine.json()["data"]["config"];
    assert_eq!(data["storage_root"], "work");
    assert_eq!(data["timezone"], "UTC");
    assert_eq!(data["revision"], 0);
    assert_eq!(data["version"], "1.0");
    assert_eq!(data["categories"]["task"]["path"], "work/task");
    assert_eq!(data["cache"]["capacity"], 512);
}

#[test]
fn config_set_scalar_writes_once_and_reports_the_revision_in_json() {
    let (_dir, root) = project();
    let run_result = run(waymark()
        .arg("--json")
        .arg("config")
        .arg("set")
        .arg("operator")
        .arg("frank")
        .arg("--project")
        .arg(&root));
    assert_eq!(run_result.status(), 0, "stderr: {}", run_result.stderr());
    let data = &run_result.json()["data"];
    assert_eq!(data["changed"], true);
    assert_eq!(data["previous_revision"], 0);
    assert_eq!(data["revision"], 1);
    assert_eq!(data["config"]["operator"], "frank");
    assert_eq!(data["config"]["version"], "1.1");
    assert!(config_bytes(&root).contains("operator = \"frank\""));
}

#[test]
fn config_set_scalar_is_idempotent() {
    let (_dir, root) = project();
    config(&root, &["set", "timezone", "UTC"]);
    let before = config_bytes(&root);
    let second = run(waymark()
        .arg("--json")
        .arg("config")
        .arg("set")
        .arg("timezone")
        .arg("UTC")
        .arg("--project")
        .arg(&root));
    assert_eq!(second.status(), 0);
    assert_eq!(second.json()["data"]["changed"], false);
    assert_eq!(second.json()["data"]["revision"], 0);
    assert_eq!(config_bytes(&root), before);
}

#[test]
fn config_add_and_remove_category_round_trips() {
    let (_dir, root) = project();
    let added = config(&root, &["add", "category", "bug", "work/bugs"]);
    assert_eq!(added.status(), 0, "stderr: {}", added.stderr());
    assert!(config_bytes(&root).contains("[categories.bug]"));
    assert!(root.join("work").join("bugs").is_dir());

    let renamed = config(&root, &["set", "category", "bug", "--name", "Defects"]);
    assert_eq!(renamed.status(), 0, "stderr: {}", renamed.stderr());
    assert!(config_bytes(&root).contains("name = \"Defects\""));

    let removed = config(&root, &["remove", "category", "bug"]);
    assert_eq!(removed.status(), 0, "stderr: {}", removed.stderr());
    assert!(!config_bytes(&root).contains("[categories.bug]"));
}

#[test]
fn removing_a_category_in_use_exits_three_and_changes_no_item() {
    let (_dir, root) = project();
    config(&root, &["add", "category", "bug", "work/bugs"]);
    create_one(&root, "bug");
    let before = items(&root);
    let config_before = config_bytes(&root);

    let refused = run(waymark()
        .arg("--json")
        .arg("config")
        .arg("remove")
        .arg("category")
        .arg("bug")
        .arg("--project")
        .arg(&root));
    assert_eq!(refused.status(), 3, "stderr: {}", refused.stderr());
    let envelope = refused.json();
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "VALIDATION_ERROR");
    assert!(envelope["error"]["details"]["issues"]
        .as_array()
        .expect("a per-issue list")
        .iter()
        .any(|issue| issue["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("work/bugs"))));
    assert_eq!(items(&root), before);
    assert_eq!(config_bytes(&root), config_before);
}

#[test]
fn config_add_field_declares_an_optional_field_and_rewrites_no_item() {
    let (_dir, root) = project();
    create_one(&root, "task");
    let before = items(&root);

    let added = config(
        &root,
        &["add", "field", "effort", "integer", "--default", "3"],
    );
    assert_eq!(added.status(), 0, "stderr: {}", added.stderr());
    let document = config_bytes(&root);
    assert!(document.contains("[fields.effort]"), "{document}");
    assert!(document.contains("type = \"integer\""));
    assert!(document.contains("required = false"));
    assert!(document.contains("default = 3"));
    assert_eq!(items(&root), before, "no item may be rewritten");

    let changed = config(&root, &["set", "field", "effort", "default", "5"]);
    assert_eq!(changed.status(), 0, "stderr: {}", changed.stderr());
    assert!(config_bytes(&root).contains("default = 5"));
}

#[test]
fn a_type_change_exits_six_and_names_the_migration() {
    let (_dir, root) = project();
    config(&root, &["add", "field", "effort", "integer"]);
    let before = config_bytes(&root);
    let refused = run(waymark()
        .arg("--json")
        .arg("config")
        .arg("set")
        .arg("field")
        .arg("effort")
        .arg("type")
        .arg("string")
        .arg("--project")
        .arg(&root));
    assert_eq!(refused.status(), 6, "stderr: {}", refused.stderr());
    assert_eq!(refused.json()["error"]["code"], "ACTION_REQUIRES_MIGRATION");
    assert_eq!(config_bytes(&root), before);
}

#[test]
fn a_required_field_addition_exits_six() {
    let (_dir, root) = project();
    let refused = config(&root, &["add", "field", "effort", "integer", "--required"]);
    assert_eq!(refused.status(), 6, "stderr: {}", refused.stderr());
    assert!(!config_bytes(&root).contains("fields.effort"));
}

#[test]
fn an_invalid_scalar_exits_three_and_writes_nothing() {
    let (_dir, root) = project();
    let before = config_bytes(&root);
    let refused = config(&root, &["set", "timezone", "Mars/Olympus"]);
    assert_eq!(refused.status(), 3, "stderr: {}", refused.stderr());
    assert_eq!(config_bytes(&root), before);
}

#[test]
fn config_validate_reports_config_invalid_findings_and_exits_three() {
    let (_dir, root) = project();
    let mut document = config_bytes(&root);
    document = document.replace("name = \"Task\"", "name = \"\"");
    std::fs::write(root.join(".waymark").join("config.toml"), document).unwrap();

    let result = run(waymark()
        .arg("--json")
        .arg("config")
        .arg("validate")
        .arg("--project")
        .arg(&root));
    assert_eq!(result.status(), 3, "stderr: {}", result.stderr());
    let envelope = result.json();
    // The finding lives on the failure envelope's code; a validation run that
    // finds problems is a failure, so stdout carries the error envelope.
    assert_eq!(envelope["error"]["code"], "CONFIG_INVALID");

    let human = config(&root, &["validate"]);
    assert_eq!(human.status(), 3);
    assert!(
        human.stderr().contains("CONFIG_INVALID"),
        "{}",
        human.stderr()
    );
    assert!(
        human.stderr().contains("categories.task.name"),
        "{}",
        human.stderr()
    );
}

#[test]
fn config_validate_is_clean_on_a_fresh_project() {
    let (_dir, root) = project();
    let result = run(waymark()
        .arg("--json")
        .arg("config")
        .arg("validate")
        .arg("--project")
        .arg(&root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let data = &result.json()["data"];
    assert_eq!(data["valid"], true);
    assert_eq!(data["findings"].as_array().unwrap().len(), 0);
}

#[test]
fn validate_reports_the_configuration_scope_and_exits_three_on_errors() {
    let (_dir, root) = project();
    let clean = run(waymark()
        .arg("--json")
        .arg("validate")
        .arg("--project")
        .arg(&root));
    assert_eq!(clean.status(), 0, "stderr: {}", clean.stderr());
    assert_eq!(clean.json()["data"]["scope"][0], "configuration");

    let document = config_bytes(&root).replace("timezone = \"UTC\"", "timezone = \"Nope/Nope\"");
    std::fs::write(root.join(".waymark").join("config.toml"), document).unwrap();
    let dirty = run(waymark()
        .arg("--json")
        .arg("validate")
        .arg("--project")
        .arg(&root));
    assert_eq!(dirty.status(), 3, "stderr: {}", dirty.stderr());
}

#[test]
fn project_info_describes_the_project() {
    let (_dir, root) = project();
    let info = run(waymark()
        .arg("--json")
        .arg("project")
        .arg("info")
        .arg("--project")
        .arg(&root));
    assert_eq!(info.status(), 0, "stderr: {}", info.stderr());
    let data = &info.json()["data"];
    assert_eq!(data["storage_root"], "work");
    assert_eq!(data["version"], "1.0");
    assert_eq!(data["revision"], 0);
    assert_eq!(data["categories"]["task"]["name"], "Task");
    assert_eq!(data["item_count"], 0);

    let human = run(waymark()
        .arg("project")
        .arg("info")
        .arg("--project")
        .arg(&root));
    assert_eq!(human.status(), 0);
    assert!(human.stderr().contains("task"), "{}", human.stderr());
}

#[test]
fn config_dry_run_reports_without_writing() {
    let (_dir, root) = project();
    let before = config_bytes(&root);
    let result = run(waymark()
        .arg("--json")
        .arg("config")
        .arg("set")
        .arg("operator")
        .arg("frank")
        .arg("--dry-run")
        .arg("--project")
        .arg(&root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    assert_eq!(result.json()["data"]["changed"], true);
    assert_eq!(config_bytes(&root), before);
}
