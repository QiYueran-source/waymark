//! The command-line adapter end to end: the envelope, the streams, and the exit
//! codes.
//!
//! Each test drives the real binary, so it covers argument parsing, output
//! routing, and the registry-to-exit-code mapping together — the three things
//! that live in this crate and nowhere else.

use std::process::{Command, Output};

use camino::Utf8PathBuf;

/// The binary under test.
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

/// Initialize a project and return its root.
fn init(root: &Utf8PathBuf) -> Run {
    run(waymark().arg("init").arg("--project").arg(root))
}

/// Create one item and return its stable ID, parsed out of the JSON envelope.
fn create_one(root: &Utf8PathBuf) -> String {
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

#[test]
fn init_creates_the_configuration_and_the_metadata_directory() {
    let (dir, root) = tempdir();
    let result = init(&root);
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());

    let metadata = root.join(".waymark");
    assert!(metadata.join("config.toml").is_file());
    assert!(metadata.join(".gitignore").is_file());
    assert!(root.join("work").join("task").is_dir());
    assert!(
        result.stdout().is_empty(),
        "human mode writes to stderr only"
    );
    assert!(
        result.stderr().contains("initialised"),
        "{}",
        result.stderr()
    );
    drop(dir);
}

#[test]
fn init_in_json_mode_puts_the_envelope_on_stdout() {
    let (_dir, root) = tempdir();
    let result = run(waymark()
        .arg("--json")
        .arg("init")
        .arg("--project")
        .arg(&root));
    assert_eq!(result.status(), 0);

    let envelope = result.json();
    assert_eq!(envelope["v"], serde_json::json!(1));
    assert_eq!(envelope["ok"], serde_json::json!(true));
    assert_eq!(
        envelope["data"]["config_path"],
        serde_json::json!(root.join(".waymark").join("config.toml").to_string())
    );
    assert_eq!(
        envelope["data"]["categories"][0]["key"],
        serde_json::json!("task")
    );
    assert_eq!(
        envelope["_meta"]["source"],
        serde_json::json!("direct_file")
    );
    assert_eq!(
        envelope["_meta"]["completeness"],
        serde_json::json!("scoped")
    );
    assert_eq!(envelope["_meta"]["freshness"], serde_json::json!("fresh"));
}

#[test]
fn create_writes_one_document_and_names_it_in_human_mode() {
    let (_dir, root) = tempdir();
    init(&root);
    let result = run(waymark()
        .arg("create")
        .arg("--project")
        .arg(&root)
        .arg("--title")
        .arg("Fix the thing")
        .arg("--category")
        .arg("task")
        .arg("--owner")
        .arg("frank")
        .arg("--body")
        .arg("body text here"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    assert!(
        result.stdout().is_empty(),
        "human mode writes to stderr only"
    );
    assert!(
        result.stderr().contains("created wm-"),
        "{}",
        result.stderr()
    );

    let entries: Vec<_> = std::fs::read_dir(root.join("work").join("task"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(entries.len(), 1, "{entries:?}");
    let document =
        std::fs::read_to_string(root.join("work").join("task").join(&entries[0])).unwrap();
    assert!(document.starts_with("---\nschema: 1\nid: "), "{document}");
    assert!(document.contains("\ntitle: Fix the thing\n"), "{document}");
    assert!(document.contains("\ncategory: task\n"), "{document}");
    assert!(document.contains("\nstatus: open\n"), "{document}");
    assert!(document.contains("\nowner: frank\n"), "{document}");
    assert!(document.ends_with("---\nbody text here\n"), "{document}");
    // No optional field is written as null or as an empty list.
    for absent in ["creator:", "closed_at:", "parent:", "dependencies:", "null"] {
        assert!(!document.contains(absent), "{absent} in {document}");
    }
}

#[test]
fn create_in_json_mode_returns_the_item_and_its_path() {
    let (_dir, root) = tempdir();
    init(&root);
    let result = run(waymark()
        .arg("--json")
        .arg("create")
        .arg("--project")
        .arg(&root)
        .arg("--title")
        .arg("Fix the thing")
        .arg("--category")
        .arg("task")
        .arg("--owner")
        .arg("frank"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let envelope = result.json();
    let id = envelope["data"]["id"].as_str().expect("an id");
    assert!(id.starts_with("wm-"), "{id}");
    assert_eq!(id.len(), 15, "{id}");
    assert_eq!(
        envelope["data"]["item"]["title"],
        serde_json::json!("Fix the thing")
    );
    assert_eq!(
        envelope["data"]["item"]["status"],
        serde_json::json!("open")
    );
    assert_eq!(envelope["data"]["item"]["schema"], serde_json::json!(1));
    assert!(envelope["data"]["item"]["creator"].is_null());
    assert_eq!(envelope["data"]["written"], serde_json::json!(true));
    assert!(envelope["data"]["path"]
        .as_str()
        .unwrap()
        .ends_with(&format!("{id}.md")));
}

#[test]
fn show_returns_the_fields_the_body_and_the_relationships_in_json() {
    let (_dir, root) = tempdir();
    init(&root);
    let id = create_one(&root);

    let result = run(waymark()
        .arg("--json")
        .arg("show")
        .arg(&id)
        .arg("--project")
        .arg(&root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let envelope = result.json();
    assert_eq!(envelope["ok"], serde_json::json!(true));
    assert_eq!(envelope["data"]["id"], serde_json::json!(id));
    assert_eq!(
        envelope["data"]["item"]["title"],
        serde_json::json!("Fix the thing")
    );
    assert_eq!(
        envelope["data"]["item"]["category"],
        serde_json::json!("task")
    );
    assert_eq!(
        envelope["data"]["item"]["owner"],
        serde_json::json!("frank")
    );
    assert_eq!(envelope["data"]["item"]["body"], serde_json::json!(""));
    assert_eq!(
        envelope["data"]["relationships"]["dependencies"],
        serde_json::json!([])
    );
    assert!(envelope["data"]["relationships"]["parent"].is_null());
    assert_eq!(envelope["data"]["fingerprint"].as_str().unwrap().len(), 64);
}

#[test]
fn show_in_human_mode_describes_the_item_on_stderr() {
    let (_dir, root) = tempdir();
    init(&root);
    let id = create_one(&root);

    let result = run(waymark().arg("show").arg(&id).arg("--project").arg(&root));
    assert_eq!(result.status(), 0);
    assert!(
        result.stdout().is_empty(),
        "human mode writes to stderr only"
    );
    let narrative = result.stderr();
    for expected in ["Fix the thing", "task", "open", "frank"] {
        assert!(
            narrative.contains(expected),
            "{expected} missing from {narrative}"
        );
    }
    assert!(narrative.contains(&id), "{narrative}");
}

#[test]
fn show_raw_prints_the_document_on_stdout() {
    let (_dir, root) = tempdir();
    init(&root);
    let id = create_one(&root);

    let result = run(waymark()
        .arg("show")
        .arg(&id)
        .arg("--raw")
        .arg("--project")
        .arg(&root));
    assert_eq!(result.status(), 0);
    let document = result.stdout();
    assert!(document.starts_with("---\nschema: 1\n"), "{document}");
    assert!(document.ends_with("---\n"), "{document}");
}

#[test]
fn a_missing_required_field_exits_three_and_writes_nothing() {
    let (_dir, root) = tempdir();
    init(&root);
    let result = run(waymark()
        .arg("create")
        .arg("--project")
        .arg(&root)
        .arg("--title")
        .arg("  ")
        .arg("--category")
        .arg("task")
        .arg("--owner")
        .arg("frank"));
    assert_eq!(result.status(), 3, "stderr: {}", result.stderr());
    assert!(result.stderr().contains("title"), "{}", result.stderr());
    assert!(std::fs::read_dir(root.join("work").join("task"))
        .unwrap()
        .next()
        .is_none());
}

#[test]
fn a_failure_in_json_mode_still_puts_one_json_value_on_stdout() {
    let (_dir, root) = tempdir();
    init(&root);
    let result = run(waymark()
        .arg("--json")
        .arg("show")
        .arg("wm-nope")
        .arg("--project")
        .arg(&root));
    assert_eq!(result.status(), 3, "an invalid id is invalid input");
    let envelope = result.json();
    assert_eq!(envelope["v"], serde_json::json!(1));
    assert_eq!(envelope["ok"], serde_json::json!(false));
    assert_eq!(envelope["error"]["code"], serde_json::json!("INVALID_ID"));
    assert_eq!(envelope["error"]["v"], serde_json::json!(1));
    assert_eq!(envelope["error"]["retryable"], serde_json::json!(false));
}

#[test]
fn showing_an_absent_item_exits_two() {
    let (_dir, root) = tempdir();
    init(&root);
    let result = run(waymark()
        .arg("--json")
        .arg("show")
        .arg("wm-000000000000")
        .arg("--project")
        .arg(&root));
    assert_eq!(result.status(), 2, "stderr: {}", result.stderr());
    assert_eq!(
        result.json()["error"]["code"],
        serde_json::json!("ITEM_NOT_FOUND")
    );
}

#[test]
fn running_outside_a_project_exits_five() {
    let (dir, root) = tempdir();
    let result = run(waymark()
        .arg("show")
        .arg("wm-000000000000")
        .arg("--project")
        .arg(&root));
    assert_eq!(result.status(), 5, "stderr: {}", result.stderr());
    assert!(
        result.stderr().contains("no Waymark project"),
        "{}",
        result.stderr()
    );
    drop(dir);
}

#[test]
fn a_bad_flag_exits_one_and_names_usage_error_on_stderr() {
    let result = run(waymark().arg("--nonsense"));
    assert_eq!(result.status(), 1, "stderr: {}", result.stderr());
    assert!(
        result.stderr().contains("USAGE_ERROR"),
        "a usage failure names its code: {}",
        result.stderr()
    );
    assert!(
        result.stdout().is_empty(),
        "human mode writes nothing to stdout: {}",
        result.stdout()
    );
}

#[test]
fn an_unknown_subcommand_exits_one_and_names_usage_error_on_stderr() {
    let result = run(waymark().arg("frobnicate"));
    assert_eq!(result.status(), 1, "stderr: {}", result.stderr());
    assert!(
        result.stderr().contains("USAGE_ERROR"),
        "a usage failure names its code: {}",
        result.stderr()
    );
}

#[test]
fn a_usage_failure_keeps_stdout_machine_only_in_json_mode() {
    let result = run(waymark().arg("--json").arg("frobnicate"));
    assert_eq!(result.status(), 1, "stderr: {}", result.stderr());
    // One JSON value on stdout, prose on stderr: the same split every other
    // failure uses, so a parser never has to look at stderr for a usage error.
    assert_eq!(result.stdout().lines().count(), 1, "{}", result.stdout());
    assert!(
        result.stderr().contains("frobnicate"),
        "{}",
        result.stderr()
    );
}

#[test]
fn a_bad_flag_in_json_mode_writes_one_failure_envelope_on_stdout() {
    let result = run(waymark().arg("--json").arg("--nonsense"));
    assert_eq!(result.status(), 1, "stderr: {}", result.stderr());
    let envelope = result.json();
    assert_eq!(envelope["v"], serde_json::json!(1));
    assert_eq!(envelope["ok"], serde_json::json!(false));
    assert_eq!(envelope["error"]["code"], serde_json::json!("USAGE_ERROR"));
    assert_eq!(envelope["error"]["retryable"], serde_json::json!(false));
    assert_eq!(envelope["error"]["details"], serde_json::json!({}));
}

#[test]
fn an_unknown_subcommand_in_json_mode_keeps_the_json_contract() {
    let result = run(waymark().arg("--json").arg("frobnicate"));
    assert_eq!(result.status(), 1, "stderr: {}", result.stderr());
    assert_eq!(
        result.json()["error"]["code"],
        serde_json::json!("USAGE_ERROR")
    );
}

#[test]
fn usage_error_is_absent_from_the_domain_registry() {
    // A domain failure in JSON mode never reports the adapter's usage code, so a
    // consumer can tell "you typed the command wrong" from "the project is
    // broken" without consulting the exit code.
    let (_dir, root) = tempdir();
    init(&root);
    let result = run(waymark()
        .arg("--json")
        .arg("show")
        .arg("wm-000000000000")
        .arg("--project")
        .arg(&root));
    assert_eq!(result.status(), 2, "stderr: {}", result.stderr());
    assert_ne!(
        result.json()["error"]["code"],
        serde_json::json!("USAGE_ERROR")
    );
}

#[test]
fn help_is_a_successful_invocation() {
    let result = run(waymark().arg("--help"));
    assert_eq!(result.status(), 0);
    assert!(result.stdout().contains("waymark"), "{}", result.stdout());
}

#[test]
fn a_command_works_from_inside_the_project_without_the_project_flag() {
    let (_dir, root) = tempdir();
    init(&root);
    let nested = root.join("work").join("task");
    let result = run(waymark()
        .arg("--json")
        .arg("create")
        .current_dir(nested.as_std_path())
        .arg("--title")
        .arg("nested")
        .arg("--category")
        .arg("task")
        .arg("--owner")
        .arg("frank"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    assert!(result.json()["data"]["id"]
        .as_str()
        .unwrap()
        .starts_with("wm-"));
}

#[test]
fn two_creates_produce_two_distinct_ids_and_two_documents() {
    let (_dir, root) = tempdir();
    init(&root);
    let first = create_one(&root);
    let second = create_one(&root);
    assert_ne!(first, second);
    let count = std::fs::read_dir(root.join("work").join("task"))
        .unwrap()
        .count();
    assert_eq!(count, 2);
}

#[test]
fn a_dry_run_reports_without_writing() {
    let (_dir, root) = tempdir();
    init(&root);
    let result = run(waymark()
        .arg("--json")
        .arg("create")
        .arg("--project")
        .arg(&root)
        .arg("--title")
        .arg("t")
        .arg("--category")
        .arg("task")
        .arg("--owner")
        .arg("f")
        .arg("--dry-run"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    assert_eq!(result.json()["data"]["written"], serde_json::json!(false));
    assert!(std::fs::read_dir(root.join("work").join("task"))
        .unwrap()
        .next()
        .is_none());
}

#[test]
fn init_accepts_a_custom_category_storage_root_and_operator() {
    let (_dir, root) = tempdir();
    let result = run(waymark()
        .arg("init")
        .arg("--project")
        .arg(&root)
        .arg("--category")
        .arg("issue")
        .arg("--storage-root")
        .arg("items")
        .arg("--operator")
        .arg("frank")
        .arg("--timezone")
        .arg("Asia/Shanghai"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    assert!(root.join("items").join("issue").is_dir());

    let config = std::fs::read_to_string(root.join(".waymark").join("config.toml")).unwrap();
    assert!(config.contains("storage_root = \"items\""), "{config}");
    assert!(config.contains("[categories.issue]"), "{config}");
    // The identity seeds are local, so they never appear in the shared document.
    assert!(!config.contains("operator ="), "{config}");
    assert!(!config.contains("timezone ="), "{config}");
    assert!(!config.contains("[cache]"), "{config}");

    let local = std::fs::read_to_string(root.join(".waymark").join("user.toml")).unwrap();
    assert!(local.contains("operator = \"frank\""), "{local}");
    assert!(local.contains("timezone = \"Asia/Shanghai\""), "{local}");

    let created = run(waymark()
        .arg("--json")
        .arg("create")
        .arg("--project")
        .arg(&root)
        .arg("--title")
        .arg("t")
        .arg("--category")
        .arg("issue")
        .arg("--owner")
        .arg("frank"));
    assert_eq!(created.status(), 0, "stderr: {}", created.stderr());
}

// ---------------------------------------------------------------------------
// The local user document
// ---------------------------------------------------------------------------

/// Run `git` in `root` with a fixed identity, panicking on failure.
fn git(root: &Utf8PathBuf, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "user.name=Waymark Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .expect("the git binary runs");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn init_creates_the_local_document_with_the_machine_defaults() {
    let (_dir, root) = tempdir();
    let result = init(&root);
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());

    let local = std::fs::read_to_string(root.join(".waymark").join("user.toml"))
        .expect("first init creates the local document");
    assert!(local.contains("[cache]"), "{local}");
    assert!(local.contains("enabled = true"), "{local}");
    assert!(
        local.contains("capacity = 512"),
        "the cache snapshot is explicit: {local}"
    );
    assert!(
        result.stderr().contains("local settings"),
        "the human output names it: {}",
        result.stderr()
    );

    let result = run(waymark()
        .arg("--json")
        .arg("init")
        .arg("--project")
        .arg(&root));
    assert_eq!(result.status(), 0);
    let envelope = result.json();
    assert_eq!(
        envelope["data"]["local_config_path"],
        serde_json::json!(root.join(".waymark").join("user.toml").to_string())
    );
    assert_eq!(
        envelope["data"]["local_config_tracked"],
        serde_json::json!(false),
        "an untracked document is not a mistake"
    );
}

#[test]
fn init_leaves_an_existing_local_document_alone() {
    let (_dir, root) = tempdir();
    assert_eq!(init(&root).status(), 0);
    let path = root.join(".waymark").join("user.toml");

    // A hand edit survives re-initialization, seeds or not.
    std::fs::write(&path, "operator = \"manual\"\n").expect("a hand edit");
    let result = run(waymark()
        .arg("init")
        .arg("--project")
        .arg(&root)
        .arg("--operator")
        .arg("someone-else"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    assert_eq!(
        std::fs::read_to_string(&path).expect("the document"),
        "operator = \"manual\"\n"
    );

    // A missing one is not recreated.
    std::fs::remove_file(&path).expect("the document is removable");
    assert_eq!(init(&root).status(), 0);
    assert!(!path.exists(), "a repeat creates nothing");
}

#[test]
fn init_reports_a_tracked_local_document_without_touching_the_index() {
    if waymark_core::git::find_executable().is_none() {
        eprintln!("skipping: no `git` executable on PATH");
        return;
    }
    let (_dir, root) = tempdir();
    git(&root, &["init", "--quiet"]);
    assert_eq!(init(&root).status(), 0);
    let path = root.join(".waymark").join("user.toml");
    assert!(path.is_file());
    git(&root, &["add", "--force", ".waymark/user.toml"]);
    let staged = git(&root, &["ls-files", "-s", ".waymark/user.toml"]);
    let before = std::fs::read_to_string(&path).expect("the document");

    let result = run(waymark()
        .arg("--json")
        .arg("init")
        .arg("--project")
        .arg(&root)
        .arg("--operator")
        .arg("someone-else"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    assert_eq!(
        result.json()["data"]["local_config_tracked"],
        serde_json::json!(true)
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("the document"),
        before,
        "a tracked document is not rewritten"
    );
    assert_eq!(
        git(&root, &["ls-files", "-s", ".waymark/user.toml"]),
        staged,
        "the Git index is unmodified"
    );

    // The human stream says the same thing in words.
    let result = init(&root);
    assert!(
        result.stderr().contains("tracked by Git"),
        "the report names the problem: {}",
        result.stderr()
    );
}
