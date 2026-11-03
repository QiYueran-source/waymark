//! The editing commands end to end: `title set`, `body replace`, `field update`.
//!
//! These tests drive the real binary because the adapter owns exactly three
//! things this suite can break: the argument shapes (`--body` vs `--body-file`,
//! repeated `--set`/`--clear`), the schema-aware value parsing, and the routing of
//! the outcome into the envelope and the exit code.

use std::io::Write;
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

/// A project whose configuration declares one field of every shape.
fn project() -> (tempfile::TempDir, Utf8PathBuf) {
    let (dir, root) = tempdir();
    let result = run(waymark().arg("init").arg("--project").arg(&root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());

    let config_path = root.join(".waymark").join("config.toml");
    let config = std::fs::read_to_string(&config_path).expect("the configuration exists");
    let declarations = "\
[fields.effort]
type = \"integer\"
required = true

[fields.note]
type = \"string\"
required = false

[fields.done]
type = \"boolean\"
required = false

[fields.tags]
type = \"list:string\"
required = false

[cache]";
    std::fs::write(&config_path, config.replace("[cache]", declarations))
        .expect("the configuration is writable");
    (dir, root)
}

/// Create one item with its required field set, and return its stable ID.
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
        .arg("original body\n")
        .arg("--field")
        .arg("effort=1"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let id = result.json()["data"]["id"]
        .as_str()
        .expect("the envelope names the new id")
        .to_owned();
    let set = run(waymark()
        .arg("--json")
        .arg("field")
        .arg("update")
        .arg("--project")
        .arg(root)
        .arg(&id)
        .arg("--set")
        .arg("effort=1"));
    assert_eq!(set.status(), 0, "stderr: {}", set.stderr());
    id
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
        .expect("the document exists")
}

#[test]
fn title_set_replaces_the_title_and_reports_the_change() {
    let (_dir, root) = project();
    let id = create(&root);

    let envelope = json_ok(&root, &["title", "set", &id, "  Renamed  thing  "]);

    assert_eq!(envelope["data"]["changed"], serde_json::json!(true));
    assert_eq!(envelope["data"]["written"], serde_json::json!(true));
    assert_eq!(
        envelope["data"]["item"]["title"],
        serde_json::json!("Renamed  thing")
    );
    assert!(document(&root, &id).contains("\ntitle: Renamed  thing\n"));
}

#[test]
fn title_set_rejects_a_multi_line_title_with_exit_three() {
    let (_dir, root) = project();
    let id = create(&root);
    let before = document(&root, &id);

    let (code, envelope) = json_err(&root, &["title", "set", &id, "two\nlines"]);

    assert_eq!(code, 3);
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("VALIDATION_ERROR")
    );
    assert_eq!(
        envelope["error"]["details"]["issues"][0]["path"],
        serde_json::json!("title")
    );
    assert_eq!(document(&root, &id), before);
}

#[test]
fn body_replace_takes_direct_text_a_file_or_stdin_and_shows_unchanged_separately() {
    let (_dir, root) = project();
    let id = create(&root);

    let direct = json_ok(&root, &["body", "replace", &id, "--body", "from text\n\n"]);
    assert_eq!(
        direct["data"]["item"]["body"],
        serde_json::json!("from text\n")
    );

    let file = root.join("body.md");
    std::fs::write(&file, "  from a file  \n\n").unwrap();
    let from_file = json_ok(
        &root,
        &["body", "replace", &id, "--body-file", file.as_str()],
    );
    assert_eq!(
        from_file["data"]["item"]["body"],
        serde_json::json!("  from a file  \n"),
        "leading whitespace is content"
    );

    let mut stdin = waymark()
        .arg("--json")
        .arg("--project")
        .arg(&root)
        .arg("body")
        .arg("replace")
        .arg(&id)
        .arg("--body")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("the binary starts");
    stdin
        .stdin
        .take()
        .expect("the child has stdin")
        .write_all(b"from stdin\n")
        .expect("stdin accepts the body");
    let finished = stdin.wait_with_output().expect("the child finishes");
    let envelope: serde_json::Value =
        serde_json::from_slice(&finished.stdout).expect("stdout is one JSON value");
    assert_eq!(
        envelope["data"]["item"]["body"],
        serde_json::json!("from stdin\n")
    );

    let same = json_ok(&root, &["body", "replace", &id, "--body", "from stdin\n"]);
    assert_eq!(same["data"]["changed"], serde_json::json!(false));
    assert_eq!(same["data"]["written"], serde_json::json!(false));

    let cleared = json_ok(&root, &["body", "replace", &id, "--body", ""]);
    assert_eq!(cleared["data"]["item"]["body"], serde_json::json!(""));
}

#[test]
fn body_flags_are_mutually_exclusive_and_at_least_one_is_optional() {
    let (_dir, root) = project();
    let id = create(&root);

    let (code, envelope) = json_err(
        &root,
        &["body", "replace", &id, "--body", "a", "--body-file", "b"],
    );
    assert_eq!(code, 1, "a flag conflict is a usage error");
    assert_eq!(envelope["error"]["code"], serde_json::json!("USAGE_ERROR"));

    // No body flag at all leaves the body alone rather than clearing it.
    let envelope = json_ok(&root, &["body", "replace", &id]);
    assert_eq!(envelope["data"]["changed"], serde_json::json!(false));
    assert_eq!(
        envelope["data"]["item"]["body"],
        serde_json::json!("original body\n")
    );
}

#[test]
fn field_update_parses_values_against_the_declared_schema() {
    let (_dir, root) = project();
    let id = create(&root);

    let envelope = json_ok(
        &root,
        &[
            "field",
            "update",
            &id,
            "--set",
            "effort=7",
            "--set",
            "note=hello world",
            "--set",
            "done=false",
            "--set",
            "tags=[\"a\",\"b\"]",
        ],
    );

    assert_eq!(
        envelope["data"]["item"]["custom_fields"]["effort"],
        serde_json::json!(7)
    );
    assert_eq!(
        envelope["data"]["item"]["custom_fields"]["note"],
        serde_json::json!("hello world")
    );
    assert_eq!(
        envelope["data"]["item"]["custom_fields"]["done"],
        serde_json::json!(false)
    );
    assert_eq!(
        envelope["data"]["item"]["custom_fields"]["tags"],
        serde_json::json!(["a", "b"])
    );
    assert_eq!(
        envelope["data"]["changed_fields"],
        serde_json::json!(["done", "effort", "note", "tags"])
    );
}

#[test]
fn field_update_accepts_a_json_string_literal_for_a_string_field() {
    let (_dir, root) = project();
    let id = create(&root);

    let envelope = json_ok(&root, &["field", "update", &id, "--set", "note=\"123\""]);
    assert_eq!(
        envelope["data"]["item"]["custom_fields"]["note"],
        serde_json::json!("123")
    );

    // A bare token is taken verbatim, including an `=` and the empty string.
    let envelope = json_ok(&root, &["field", "update", &id, "--set", "note=a=b"]);
    assert_eq!(
        envelope["data"]["item"]["custom_fields"]["note"],
        serde_json::json!("a=b")
    );
    let envelope = json_ok(&root, &["field", "update", &id, "--set", "note="]);
    assert_eq!(
        envelope["data"]["item"]["custom_fields"]["note"],
        serde_json::json!(""),
        "an empty string is a valid string value"
    );
}

#[test]
fn field_update_rejects_values_that_do_not_match_the_declared_type() {
    let (_dir, root) = project();
    let id = create(&root);
    let before = document(&root, &id);

    for (key, value) in [
        ("effort", "3.0"),
        ("effort", "+3"),
        ("effort", "0x10"),
        ("effort", "three"),
        ("done", "TRUE"),
        ("done", "1"),
        ("tags", "a,b"),
        ("tags", "[1,2]"),
        ("note", "[1,2]"),
    ] {
        let (code, envelope) = json_err(
            &root,
            &["field", "update", &id, "--set", &format!("{key}={value}")],
        );
        assert_eq!(code, 3, "{key}={value}");
        assert_eq!(
            envelope["error"]["code"],
            serde_json::json!("VALIDATION_ERROR"),
            "{key}={value}"
        );
        let issues = envelope["error"]["details"]["issues"]
            .as_array()
            .expect("per-field issues");
        assert_eq!(
            issues[0]["path"],
            serde_json::json!(format!("custom_fields.{key}"))
        );
        assert_eq!(document(&root, &id), before, "{key}={value} wrote nothing");
    }
}

#[test]
fn field_update_rejects_duplicates_conflicts_and_unknown_or_required_clears() {
    let (_dir, root) = project();
    let id = create(&root);
    let before = document(&root, &id);

    let cases: Vec<Vec<&str>> = vec![
        vec!["--set", "note=a", "--set", "note=b"],
        vec!["--set", "note=a", "--clear", "note"],
        vec!["--clear", "note", "--clear", "note"],
        vec!["--set", "nope=1"],
        vec!["--clear", "effort"],
    ];
    for case in cases {
        let mut arguments = vec!["field", "update", id.as_str()];
        arguments.extend(case.iter().copied());
        let (code, envelope) = json_err(&root, &arguments);
        assert_eq!(code, 3, "{case:?}");
        assert_eq!(
            envelope["error"]["code"],
            serde_json::json!("VALIDATION_ERROR"),
            "{case:?}"
        );
        assert_eq!(document(&root, &id), before, "{case:?} wrote nothing");
    }
}

#[test]
fn a_field_update_that_changes_nothing_is_not_a_write() {
    let (_dir, root) = project();
    let id = create(&root);
    let before = document(&root, &id);

    let envelope = json_ok(
        &root,
        &[
            "field", "update", &id, "--set", "effort=1", "--clear", "note",
        ],
    );
    assert_eq!(envelope["data"]["changed"], serde_json::json!(false));
    assert_eq!(envelope["data"]["written"], serde_json::json!(false));
    assert_eq!(envelope["data"]["changed_fields"], serde_json::json!([]));
    assert_eq!(document(&root, &id), before);
}

#[test]
fn edits_report_an_owner_mismatch_as_a_warning_on_stderr_and_in_meta() {
    let (_dir, root) = project();
    let id = create(&root);

    // `--operator` is not a global flag, so the mismatch comes from the project
    // configuration's operator differing from the owner.
    let config_path = root.join(".waymark").join("config.toml");
    let config = std::fs::read_to_string(&config_path).unwrap();
    std::fs::write(
        &config_path,
        config.replace("timezone = ", "operator = \"alice\"\ntimezone = "),
    )
    .unwrap();

    let envelope = json_ok(&root, &["title", "set", &id, "Renamed"]);
    assert_eq!(
        envelope["_meta"]["warnings"][0]["code"],
        serde_json::json!("OWNER_MISMATCH")
    );

    let human = run(waymark()
        .arg("--project")
        .arg(&root)
        .arg("title")
        .arg("set")
        .arg(&id)
        .arg("Renamed again"));
    assert_eq!(human.status(), 0);
    assert!(
        human.stdout().is_empty(),
        "human mode prints nothing to stdout"
    );
    assert!(
        human.stderr().contains("OWNER_MISMATCH"),
        "stderr: {}",
        human.stderr()
    );
    assert!(document(&root, &id).contains("Renamed again"));
}

#[test]
fn a_dry_run_reports_the_edit_and_writes_nothing() {
    let (_dir, root) = project();
    let id = create(&root);
    let before = document(&root, &id);

    let envelope = json_ok(&root, &["title", "set", &id, "Renamed", "--dry-run"]);

    assert_eq!(envelope["data"]["changed"], serde_json::json!(true));
    assert_eq!(envelope["data"]["written"], serde_json::json!(false));
    assert_eq!(
        envelope["data"]["item"]["title"],
        serde_json::json!("Renamed")
    );
    assert_eq!(document(&root, &id), before);
}

#[test]
fn an_archived_item_rejects_every_edit_with_item_archived() {
    let (_dir, root) = project();
    let id = create(&root);
    let path = root.join("work").join("task").join(format!("{id}.md"));
    let archived = std::fs::read_to_string(&path).unwrap().replace(
        "updated_at: ",
        "archived_at: 2026-11-03T05:43:10Z\nupdated_at: ",
    );
    std::fs::write(&path, archived).unwrap();

    for arguments in [
        vec!["title", "set", id.as_str(), "Renamed"],
        vec!["body", "replace", id.as_str(), "--body", "x"],
        vec!["field", "update", id.as_str(), "--set", "effort=2"],
    ] {
        let (code, envelope) = json_err(&root, &arguments);
        assert_eq!(code, 4, "{arguments:?}");
        assert_eq!(
            envelope["error"]["code"],
            serde_json::json!("ITEM_ARCHIVED"),
            "{arguments:?}"
        );
    }
}
