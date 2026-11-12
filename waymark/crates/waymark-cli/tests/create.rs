//! `waymark create` end to end: the complete initial state on one command line.
//!
//! The domain tests pin the rules; these tests pin the adapter: every catalog
//! input is reachable, `--field` uses the same text grammar as `field update`,
//! `--body-file` is mutually exclusive with `--body`, and `--dry-run` performs
//! the checks without writing.

use std::collections::BTreeMap;
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

fn tempdir() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 path");
    (dir, path)
}

/// A project declaring an integer and a string field.
fn project() -> (tempfile::TempDir, Utf8PathBuf) {
    let (dir, root) = tempdir();
    let init = run(waymark().arg("init").arg("--project").arg(&root));
    assert_eq!(init.status(), 0, "stderr: {}", init.stderr());

    let config_path = root.join(".waymark").join("config.toml");
    let config = std::fs::read_to_string(&config_path).expect("the configuration exists");
    let declarations = "\
[fields.points]
type = \"integer\"
required = false

[fields.note]
type = \"string\"
required = false
";
    std::fs::write(&config_path, format!("{config}\n{declarations}"))
        .expect("the configuration is writable");
    (dir, root)
}

fn create(root: &Utf8Path, title: &str, extra: &[&str]) -> Run {
    let mut command = waymark();
    command
        .arg("--json")
        .arg("create")
        .arg("--project")
        .arg(root)
        .arg("--title")
        .arg(title)
        .arg("--category")
        .arg("task")
        .arg("--owner")
        .arg("frank");
    for argument in extra {
        command.arg(argument);
    }
    run(&mut command)
}

fn create_one(root: &Utf8Path) -> String {
    let result = create(root, "seeded", &[]);
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    result.json()["data"]["id"]
        .as_str()
        .expect("the envelope names the new id")
        .to_owned()
}

fn snapshot(root: &Utf8Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    collect(root, root, &mut files);
    files
}

fn collect(root: &Utf8Path, directory: &Utf8Path, files: &mut BTreeMap<String, Vec<u8>>) {
    for entry in std::fs::read_dir(directory).expect("a readable directory") {
        let entry = entry.expect("a readable directory entry");
        let path = Utf8PathBuf::from_path_buf(entry.path()).expect("a UTF-8 path");
        if path.is_dir() {
            collect(root, &path, files);
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .expect("the file is under the root")
                .to_string();
            files.insert(relative, std::fs::read(&path).expect("a readable file"));
        }
    }
}

#[test]
fn create_accepts_the_complete_initial_state() {
    let (_dir, root) = project();
    let parent = create_one(&root);
    let linked = create_one(&root);
    // Both fixtures belong to frank; with `--cascade` he is the operator that
    // may rewrite the partner.
    let set = run(waymark()
        .arg("--project")
        .arg(&root)
        .arg("config")
        .arg("set")
        .arg("operator")
        .arg("frank"));
    assert_eq!(set.status(), 0, "stderr: {}", set.stderr());

    let result = create(
        &root,
        "composite",
        &[
            "--creator",
            "alice",
            "--body",
            "the body\n",
            "--field",
            "points=7",
            "--field",
            "note=hello",
            "--parent",
            &parent,
            "--dependency",
            &parent,
            "--link",
            &linked,
            "--cascade",
        ],
    );
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let envelope = result.json();
    assert_eq!(envelope["data"]["written"], true);
    assert_eq!(envelope["data"]["item"]["creator"], "alice");
    assert_eq!(envelope["data"]["item"]["owner"], "frank");
    assert_eq!(envelope["data"]["item"]["custom_fields"]["points"], 7);
    assert_eq!(envelope["data"]["item"]["custom_fields"]["note"], "hello");
    assert_eq!(envelope["data"]["item"]["parent"], parent);
    assert_eq!(
        envelope["data"]["item"]["dependencies"],
        serde_json::json!([parent])
    );
    assert_eq!(
        envelope["data"]["item"]["soft_links"],
        serde_json::json!([linked])
    );
    assert_eq!(envelope["data"]["partners"].as_array().unwrap().len(), 1);
    assert_eq!(
        envelope["data"]["fingerprints"].as_array().unwrap().len(),
        2
    );

    let id = envelope["data"]["id"].as_str().expect("the new id");
    let partner_document =
        std::fs::read_to_string(root.join(format!("work/task/{linked}.md"))).unwrap();
    assert!(
        partner_document.contains(&format!("  - {id}")),
        "the partner gained the new ID: {partner_document}"
    );
    let new_document = std::fs::read_to_string(root.join(format!("work/task/{id}.md"))).unwrap();
    assert!(new_document.contains("creator: alice"), "{new_document}");
    assert!(new_document.contains("  points: 7"), "{new_document}");
}

#[test]
fn create_reads_the_body_from_a_file() {
    let (_dir, root) = project();
    let (body_dir, body_root) = tempdir();
    let body_path = body_root.join("body.md");
    std::fs::write(&body_path, "from the file\n").expect("the body file is writable");

    let result = create(&root, "from file", &["--body-file", body_path.as_str()]);
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let envelope = result.json();
    assert_eq!(envelope["data"]["item"]["body"], "from the file\n");
    drop(body_dir);

    // Both body inputs at once is a usage error, not a domain guess.
    let both = create(
        &root,
        "both",
        &["--body", "inline", "--body-file", body_path.as_str()],
    );
    assert_ne!(both.status(), 0);
}

#[test]
fn create_dry_run_performs_every_check_and_writes_nothing() {
    let (_dir, root) = project();
    let parent = create_one(&root);
    let linked = create_one(&root);
    let before = snapshot(&root);

    let planned = create(
        &root,
        "planned",
        &[
            "--field",
            "points=7",
            "--parent",
            &parent,
            "--dependency",
            &parent,
            "--link",
            &linked,
            "--dry-run",
        ],
    );
    assert_eq!(planned.status(), 0, "stderr: {}", planned.stderr());
    let envelope = planned.json();
    assert_eq!(envelope["data"]["written"], false);
    assert!(envelope["data"]["fingerprints"]
        .as_array()
        .unwrap()
        .is_empty());
    let planned_id = envelope["data"]["id"].as_str().expect("the preview id");
    assert!(!root.join(format!("work/task/{planned_id}.md")).exists());
    assert_eq!(snapshot(&root), before, "a dry run writes nothing");

    // The dry run still validates the field type.
    let invalid = create(&root, "bad", &["--field", "points=nope", "--dry-run"]);
    assert_eq!(invalid.status(), 3, "stderr: {}", invalid.stderr());
    let envelope = invalid.json();
    assert_eq!(envelope["error"]["code"], "VALIDATION_ERROR");
    assert_eq!(
        envelope["error"]["details"]["issues"][0]["path"],
        "custom_fields.points"
    );
    assert_eq!(snapshot(&root), before);
}

#[test]
fn create_link_without_cascade_leaves_the_partner_byte_identical_and_names_it() {
    let (_dir, root) = project();
    let linked = create_one(&root);
    let partner_path = root.join(format!("work/task/{linked}.md"));
    let before = std::fs::read(&partner_path).expect("the partner document reads");

    let result = create(&root, "linked", &["--link", &linked]);
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let envelope = result.json();
    let id = envelope["data"]["id"].as_str().expect("the new id");

    assert_eq!(envelope["data"]["written"], true);
    assert_eq!(envelope["data"]["partners"], serde_json::json!([]));
    assert_eq!(
        envelope["data"]["untouched"],
        serde_json::json!([{ "id": linked, "path": partner_path.to_string() }])
    );
    assert_eq!(
        std::fs::read(&partner_path).expect("the partner document reads"),
        before,
        "the partner is never silently edited"
    );
    let new_document = std::fs::read_to_string(root.join(format!("work/task/{id}.md")))
        .expect("the new document reads");
    assert!(
        new_document.contains(&format!("  - {linked}")),
        "{new_document}"
    );
}

#[test]
fn create_link_with_cascade_rewrites_a_partner_the_operator_owns_and_reports_it() {
    let (_dir, root) = project();
    let linked = create_one(&root);
    let partner_path = root.join(format!("work/task/{linked}.md"));

    // The partner is frank's; the operator is recorded the way a person records
    // one, so Git's `user.name` cannot decide the answer.
    let set = run(waymark()
        .arg("--project")
        .arg(&root)
        .arg("config")
        .arg("set")
        .arg("operator")
        .arg("frank"));
    assert_eq!(set.status(), 0, "stderr: {}", set.stderr());

    let result = create(&root, "linked", &["--link", &linked, "--cascade"]);
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let envelope = result.json();
    let id = envelope["data"]["id"].as_str().expect("the new id");

    assert_eq!(
        envelope["data"]["partners"][0]["id"],
        serde_json::json!(linked)
    );
    assert_eq!(
        envelope["data"]["partners"][0]["path"],
        partner_path.to_string()
    );
    assert_eq!(envelope["data"]["untouched"], serde_json::json!([]));
    let partner_document = std::fs::read_to_string(&partner_path).expect("the partner reads");
    assert!(
        partner_document.contains(&format!("  - {id}")),
        "the partner records the link when it was rewritten: {partner_document}"
    );
}

#[test]
fn create_link_with_cascade_leaves_a_partner_owned_by_someone_else_alone() {
    let (_dir, root) = project();
    let linked = create_one(&root);
    let partner_path = root.join(format!("work/task/{linked}.md"));
    let before = std::fs::read(&partner_path).expect("the partner document reads");
    // Recorded after the partner exists, so the fixture stays the operator's.
    let set = run(waymark()
        .arg("--project")
        .arg(&root)
        .arg("config")
        .arg("set")
        .arg("operator")
        .arg("alice"));
    assert_eq!(set.status(), 0, "stderr: {}", set.stderr());

    let result = create(&root, "linked", &["--link", &linked, "--cascade"]);
    assert_eq!(
        result.status(),
        0,
        "a foreign partner is named, not refused: {}",
        result.stderr()
    );
    let envelope = result.json();
    assert_eq!(envelope["data"]["partners"], serde_json::json!([]));
    assert_eq!(
        envelope["data"]["untouched"][0]["id"],
        serde_json::json!(linked)
    );
    assert_eq!(
        std::fs::read(&partner_path).expect("the partner document reads"),
        before,
        "consent never reaches another owner's document"
    );
}

#[test]
fn create_link_dry_run_reports_the_same_partners_as_the_write() {
    let (_dir, root) = project();
    let linked = create_one(&root);
    let before = snapshot(&root);

    let preview = create(&root, "linked", &["--link", &linked, "--dry-run"]);
    assert_eq!(preview.status(), 0, "stderr: {}", preview.stderr());
    let envelope = preview.json();
    assert_eq!(envelope["data"]["written"], false);
    assert_eq!(envelope["data"]["partners"], serde_json::json!([]));
    assert_eq!(
        envelope["data"]["untouched"][0]["id"],
        serde_json::json!(linked)
    );
    assert_eq!(snapshot(&root), before, "a dry run writes nothing");
}

#[test]
fn the_human_output_names_the_partner_it_left_alone() {
    let (_dir, root) = project();
    let linked = create_one(&root);
    let partner_path = root.join(format!("work/task/{linked}.md"));
    let before = std::fs::read(&partner_path).expect("the partner document reads");

    let result = run(waymark()
        .arg("--project")
        .arg(&root)
        .arg("create")
        .arg("--title")
        .arg("linked")
        .arg("--category")
        .arg("task")
        .arg("--owner")
        .arg("frank")
        .arg("--link")
        .arg(&linked));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());

    let stderr = result.stderr();
    let line = format!("{linked} left alone (it did not record the link)");
    assert!(
        stderr.contains("created wm-"),
        "the new item leads the report: {stderr}"
    );
    assert!(
        stderr.contains(&line),
        "the partner is named as untouched: {stderr}"
    );
    assert!(
        !stderr.contains("updated "),
        "nothing but the new document was written: {stderr}"
    );
    assert_eq!(
        std::fs::read(&partner_path).expect("the partner document reads"),
        before,
        "the sentence describes the bytes that did not move"
    );
}

#[test]
fn a_create_with_a_link_and_no_operator_is_refused_and_writes_nothing() {
    let (_dir, root) = project();
    let linked = create_one(&root);
    let before = snapshot(&root);

    // No local document and no Git identity in an isolated checkout: nobody can
    // be shown entitled to rewrite the partner, so the create fails the way
    // `link add --cascade` fails rather than silently leaving the partner.
    let result = run(waymark()
        .arg("--json")
        .arg("create")
        .arg("--project")
        .arg(&root)
        .arg("--title")
        .arg("linked")
        .arg("--category")
        .arg("task")
        .arg("--owner")
        .arg("frank")
        .arg("--link")
        .arg(&linked)
        .arg("--cascade"));
    assert_eq!(
        result.status(),
        3,
        "a missing identity is a configuration failure: {}",
        result.stderr()
    );
    let envelope = result.json();
    assert_eq!(envelope["error"]["code"], "CONFIG_INVALID");
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
    assert_eq!(
        envelope["error"]["location"]["key"], "user.toml",
        "the refusal is located where the identity is recorded"
    );
    assert_eq!(snapshot(&root), before, "a refusal writes nothing");
}

#[test]
fn create_link_with_cascade_rewrites_the_partner_of_a_user_identified_only_by_git() {
    if waymark_core::git::find_executable().is_none() {
        eprintln!("skipping: no `git` executable on PATH");
        return;
    }
    let (_dir, root) = project();
    let linked = create_one(&root);
    let partner_path = root.join(format!("work/task/{linked}.md"));
    // A fresh clone on a machine that never ran `waymark config set`: Git's
    // `user.name` is the only identity, and it still makes frank the operator.
    let repository = Command::new("git")
        .current_dir(&root)
        .args(["init", "--quiet"])
        .output()
        .expect("the git binary runs");
    assert!(repository.status.success(), "git init failed");
    let identity = Command::new("git")
        .current_dir(&root)
        .args(["config", "user.name", "frank"])
        .output()
        .expect("the git binary runs");
    assert!(identity.status.success(), "git config failed");

    let result = create(&root, "linked", &["--link", &linked, "--cascade"]);
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let envelope = result.json();
    let id = envelope["data"]["id"].as_str().expect("the new id");

    assert_eq!(
        envelope["data"]["partners"][0]["id"],
        serde_json::json!(linked),
        "Git's user.name owns the partner, so consent rewrites it"
    );
    assert!(
        std::fs::read_to_string(&partner_path)
            .expect("the partner reads")
            .contains(&format!("  - {id}")),
        "the partner records the link"
    );
}
