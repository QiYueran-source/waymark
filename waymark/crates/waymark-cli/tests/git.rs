//! `waymark git history`, `waymark git diff`, and `waymark agent-doc` end to end.
//!
//! These tests pin the adapter, not the rules: the flag shapes, the streams, the
//! exit codes, and the fact that a project outside a repository still exits 0
//! with an informational finding. The domain behaviour is covered by
//! `crates/waymark-core/tests/git.rs`.
//!
//! Every Git-backed case skips cleanly when the `git` binary is absent.

use std::process::{Command, Output};

use camino::Utf8PathBuf;

fn waymark() -> Command {
    Command::new(env!("CARGO_BIN_EXE_waymark"))
}

/// The binary with no `git` reachable through `PATH`.
///
/// The probe searches `PATH`, so removing it is how a test simulates a host
/// without Git without touching the real repository or the ambient `PATH`.
fn waymark_without_git() -> Command {
    let mut command = waymark();
    command.env_remove("PATH");
    command
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

fn git_available() -> bool {
    waymark_core::git::find_executable().is_some()
}

fn git(root: &Utf8PathBuf, args: &[&str]) {
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
}

/// A project with one created item. The caller decides whether it is a
/// repository.
fn project() -> (tempfile::TempDir, Utf8PathBuf, String) {
    let (dir, root) = tempdir();
    let init = run(waymark().arg("init").arg("--project").arg(&root));
    assert_eq!(init.status(), 0, "stderr: {}", init.stderr());
    let created = run(waymark()
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
    assert_eq!(created.status(), 0, "stderr: {}", created.stderr());
    let id = created.json()["data"]["id"]
        .as_str()
        .expect("the created item has an ID")
        .to_owned();
    (dir, root, id)
}

/// A project inside a repository with the item committed.
fn project_in_repository() -> Option<(tempfile::TempDir, Utf8PathBuf, String)> {
    if !git_available() {
        eprintln!("skipping: no `git` executable on PATH");
        return None;
    }
    let (dir, root, id) = project();
    git(&root, &["init", "--quiet"]);
    git(&root, &["add", "-A"]);
    git(
        &root,
        &["commit", "--quiet", "-m", "add the first work item"],
    );
    Some((dir, root, id))
}

#[test]
fn git_history_json_reports_the_commit_that_added_the_item() {
    let Some((_dir, root, id)) = project_in_repository() else {
        return;
    };
    let result = run(waymark()
        .arg("--json")
        .arg("git")
        .arg("history")
        .arg("--project")
        .arg(&root)
        .arg("--item")
        .arg(&id));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let json = result.json();
    assert_eq!(json["ok"], serde_json::json!(true));
    assert_eq!(json["data"]["scope"], serde_json::json!("item"));
    assert_eq!(json["data"]["item"], serde_json::json!(id));
    assert_eq!(json["data"]["limit"], serde_json::json!(20));
    assert_eq!(
        json["data"]["commits"][0]["summary"],
        serde_json::json!("add the first work item")
    );
    assert_eq!(
        json["_meta"]["diagnostics"],
        serde_json::json!([]),
        "a real repository has nothing to diagnose"
    );
}

#[test]
fn git_history_accepts_a_limit_and_a_path() {
    let Some((_dir, root, _id)) = project_in_repository() else {
        return;
    };
    let result = run(waymark()
        .arg("--json")
        .arg("git")
        .arg("history")
        .arg("--project")
        .arg(&root)
        .arg("--limit")
        .arg("1")
        .arg("--path")
        .arg("work/task"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let json = result.json();
    assert_eq!(json["data"]["scope"], serde_json::json!("path"));
    assert_eq!(json["data"]["path"], serde_json::json!("work/task"));
    assert_eq!(json["data"]["paths"], serde_json::json!(["work/task"]));
    assert_eq!(json["data"]["limit"], serde_json::json!(1));
    assert_eq!(
        json["data"]["commits"].as_array().map(Vec::len),
        Some(1),
        "the limit caps the page"
    );
}

#[test]
fn git_history_renders_commits_on_stdout_in_human_mode() {
    let Some((_dir, root, _id)) = project_in_repository() else {
        return;
    };
    let result = run(waymark()
        .arg("git")
        .arg("history")
        .arg("--project")
        .arg(&root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    assert!(
        result.stdout().contains("add the first work item"),
        "stdout: {}",
        result.stdout()
    );
}

#[test]
fn git_diff_reports_the_working_tree_change_for_an_item() {
    let Some((_dir, root, id)) = project_in_repository() else {
        return;
    };
    let document = root.join(format!("work/task/{id}.md"));
    let original = std::fs::read_to_string(&document).expect("the document is readable");
    std::fs::write(
        &document,
        original.replace("Fix the thing", "Fix the other thing"),
    )
    .expect("the document is writable");

    let result = run(waymark()
        .arg("--json")
        .arg("git")
        .arg("diff")
        .arg("--project")
        .arg(&root)
        .arg("--item")
        .arg(&id));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let json = result.json();
    assert_eq!(json["data"]["scope"], serde_json::json!("item"));
    assert_eq!(json["data"]["baseline"], serde_json::json!("working_tree"));
    assert_eq!(json["data"]["empty"], serde_json::json!(false));
    let patch = json["data"]["patch"].as_str().expect("a patch string");
    assert!(patch.contains("+title: Fix the other thing"), "{patch}");
    assert_eq!(json["_meta"]["diagnostics"], serde_json::json!([]));
}

#[test]
fn git_diff_of_a_commit_works_through_the_cli() {
    let Some((_dir, root, id)) = project_in_repository() else {
        return;
    };
    let history = run(waymark()
        .arg("--json")
        .arg("git")
        .arg("history")
        .arg("--project")
        .arg(&root)
        .arg("--item")
        .arg(&id));
    let commit = history.json()["data"]["commits"][0]["id"]
        .as_str()
        .expect("a commit id")
        .to_owned();

    let result = run(waymark()
        .arg("--json")
        .arg("git")
        .arg("diff")
        .arg("--project")
        .arg(&root)
        .arg("--item")
        .arg(&id)
        .arg("--commit")
        .arg(&commit));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let json = result.json();
    assert_eq!(json["data"]["baseline"], serde_json::json!("commit"));
    assert_eq!(json["data"]["commit"], serde_json::json!(commit));
    assert_eq!(json["data"]["empty"], serde_json::json!(false));
}

#[test]
fn git_diff_prints_a_pipeable_patch_in_human_mode() {
    let Some((_dir, root, id)) = project_in_repository() else {
        return;
    };
    let document = root.join(format!("work/task/{id}.md"));
    let original = std::fs::read_to_string(&document).expect("the document is readable");
    std::fs::write(
        &document,
        original.replace("Fix the thing", "Fix the other thing"),
    )
    .expect("the document is writable");

    let result = run(waymark()
        .arg("git")
        .arg("diff")
        .arg("--project")
        .arg(&root)
        .arg("--item")
        .arg(&id));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    assert!(
        result.stdout().starts_with("diff --git "),
        "the patch is the whole answer:\n{}",
        result.stdout()
    );
    assert!(
        result.stdout().contains("+title: Fix the other thing"),
        "{}",
        result.stdout()
    );
}

#[test]
fn outside_a_repository_history_and_diff_exit_zero_with_an_informational_finding() {
    let (_dir, root, id) = project();

    let history = run(waymark()
        .arg("--json")
        .arg("git")
        .arg("history")
        .arg("--project")
        .arg(&root)
        .arg("--item")
        .arg(&id));
    assert_eq!(
        history.status(),
        0,
        "an absent repository is informational, not a failure. stderr: {}",
        history.stderr()
    );
    let json = history.json();
    assert_eq!(json["ok"], serde_json::json!(true));
    assert_eq!(json["data"]["commits"], serde_json::json!([]));
    assert_eq!(
        json["_meta"]["diagnostics"][0]["code"],
        serde_json::json!("NOT_A_GIT_REPOSITORY")
    );
    assert_eq!(
        json["_meta"]["diagnostics"][0]["severity"],
        serde_json::json!("info")
    );
    assert_eq!(
        json["_meta"]["diagnostics"][0]["fixable"],
        serde_json::json!(false)
    );

    let diff = run(waymark()
        .arg("--json")
        .arg("git")
        .arg("diff")
        .arg("--project")
        .arg(&root)
        .arg("--item")
        .arg(&id));
    assert_eq!(diff.status(), 0, "stderr: {}", diff.stderr());
    let json = diff.json();
    assert_eq!(json["ok"], serde_json::json!(true));
    assert_eq!(json["data"]["empty"], serde_json::json!(true));
    assert_eq!(
        json["_meta"]["diagnostics"][0]["code"],
        serde_json::json!("NOT_A_GIT_REPOSITORY")
    );

    // Human mode keeps the same exit code and says why on stderr.
    let human = run(waymark()
        .arg("git")
        .arg("history")
        .arg("--project")
        .arg(&root));
    assert_eq!(human.status(), 0);
    assert!(
        human.stderr().contains("NOT_A_GIT_REPOSITORY"),
        "stderr: {}",
        human.stderr()
    );
}

#[test]
fn a_missing_git_executable_inside_a_repository_is_git_unavailable_and_exits_zero() {
    let Some((_dir, root, id)) = project_in_repository() else {
        return;
    };

    // The repository is present but the executable is not, so exactly one
    // cause is missing and exactly one finding names it.
    let history = run(waymark_without_git()
        .arg("--json")
        .arg("git")
        .arg("history")
        .arg("--project")
        .arg(&root)
        .arg("--item")
        .arg(&id));
    assert_eq!(history.status(), 0, "stderr: {}", history.stderr());
    let json = history.json();
    assert_eq!(json["ok"], serde_json::json!(true));
    assert_eq!(json["data"]["commits"], serde_json::json!([]));
    assert_eq!(
        json["_meta"]["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .len(),
        1
    );
    let finding = &json["_meta"]["diagnostics"][0];
    assert_eq!(finding["code"], serde_json::json!("GIT_UNAVAILABLE"));
    assert_eq!(finding["severity"], serde_json::json!("info"));
    assert_eq!(finding["fixable"], serde_json::json!(false));
    assert!(finding["location"].is_null(), "{finding}");
    assert!(
        finding["message"]
            .as_str()
            .unwrap_or_default()
            .contains("PATH"),
        "{finding}"
    );

    let diff = run(waymark_without_git()
        .arg("--json")
        .arg("git")
        .arg("diff")
        .arg("--project")
        .arg(&root)
        .arg("--item")
        .arg(&id));
    assert_eq!(diff.status(), 0, "stderr: {}", diff.stderr());
    let json = diff.json();
    assert_eq!(json["ok"], serde_json::json!(true));
    assert_eq!(json["data"]["empty"], serde_json::json!(true));
    assert_eq!(
        json["_meta"]["diagnostics"][0]["code"],
        serde_json::json!("GIT_UNAVAILABLE")
    );
    assert_eq!(
        json["_meta"]["diagnostics"][0]["severity"],
        serde_json::json!("info")
    );
}

#[test]
fn a_missing_repository_and_executable_report_both_findings_and_exit_zero() {
    let (_dir, root, id) = project();

    let history = run(waymark_without_git()
        .arg("--json")
        .arg("git")
        .arg("history")
        .arg("--project")
        .arg(&root)
        .arg("--item")
        .arg(&id));
    assert_eq!(history.status(), 0, "stderr: {}", history.stderr());
    let json = history.json();
    assert_eq!(json["ok"], serde_json::json!(true));
    let codes: Vec<&str> = json["_meta"]["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|finding| finding["code"].as_str().expect("a code"))
        .collect();
    assert_eq!(codes, vec!["NOT_A_GIT_REPOSITORY", "GIT_UNAVAILABLE"]);
    for finding in json["_meta"]["diagnostics"]
        .as_array()
        .expect("diagnostics")
    {
        assert_eq!(finding["severity"], serde_json::json!("info"));
        assert_eq!(finding["fixable"], serde_json::json!(false));
        assert!(finding["location"].is_null(), "{finding}");
    }

    // Human mode keeps the usual exit code and names both causes on stderr.
    let human = run(waymark_without_git()
        .arg("git")
        .arg("history")
        .arg("--project")
        .arg(&root));
    assert_eq!(human.status(), 0, "stderr: {}", human.stderr());
    assert!(
        human.stderr().contains("NOT_A_GIT_REPOSITORY"),
        "{}",
        human.stderr()
    );
    assert!(
        human.stderr().contains("GIT_UNAVAILABLE"),
        "{}",
        human.stderr()
    );
    assert!(
        !root.join(".git").exists(),
        "reading history must never initialize a repository"
    );
}

#[test]
fn git_commands_write_nothing_under_the_metadata_directory() {
    let Some((_dir, root, id)) = project_in_repository() else {
        return;
    };
    let document = root.join(format!("work/task/{id}.md"));
    let original = std::fs::read_to_string(&document).expect("the document is readable");
    std::fs::write(&document, original.replace("Fix the thing", "Changed")).expect("writable");

    let metadata = root.join(".waymark");
    let before = snapshot(&metadata);
    for args in [
        vec!["git", "history", "--project", root.as_str(), "--item", &id],
        vec!["git", "history", "--project", root.as_str()],
        vec!["git", "diff", "--project", root.as_str(), "--item", &id],
        vec!["git", "diff", "--project", root.as_str()],
    ] {
        let result = run(waymark().args(&args));
        assert_eq!(result.status(), 0, "{args:?}: {}", result.stderr());
    }
    let after = snapshot(&metadata);
    assert_eq!(
        before, after,
        "Waymark keeps no audit log, attribution, or history store"
    );
}

fn snapshot(directory: &camino::Utf8Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .expect("the metadata directory is readable")
        .map(|entry| {
            entry
                .expect("a readable entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn agent_doc_lists_every_top_level_command_including_the_new_families() {
    let result = run(waymark().arg("agent-doc"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let text = result.stdout();
    for command in [
        "init",
        "create",
        "show",
        "status",
        "title",
        "body",
        "field",
        "start",
        "close",
        "reopen",
        "dependency",
        "parent",
        "link",
        "tree",
        "dep",
        "list",
        "ready",
        "blocked",
        "archive",
        "query",
        "search",
        "stats",
        "project",
        "git",
        "agent-doc",
        "config",
        "validate",
        "version",
        "serve",
    ] {
        assert!(
            text.contains(&format!("`waymark {command}`")),
            "agent-doc is missing the top-level command `{command}`:\n{text}"
        );
    }
}

#[test]
fn agent_doc_json_describes_the_same_tree_the_parser_accepts() {
    let result = run(waymark().arg("--json").arg("agent-doc"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let json = result.json();
    assert_eq!(json["data"]["path"], serde_json::json!("waymark"));
    let names: Vec<&str> = json["data"]["subcommands"]
        .as_array()
        .expect("subcommands")
        .iter()
        .map(|command| command["name"].as_str().expect("a name"))
        .collect();
    assert!(names.contains(&"git"), "{names:?}");
    assert!(names.contains(&"agent-doc"), "{names:?}");

    // The documented `git history` arguments are the ones the parser accepts.
    let history = json["data"]["subcommands"]
        .as_array()
        .expect("subcommands")
        .iter()
        .find(|command| command["name"] == serde_json::json!("git"))
        .and_then(|git| git["subcommands"].as_array())
        .and_then(|subcommands| {
            subcommands
                .iter()
                .find(|command| command["name"] == serde_json::json!("history"))
        })
        .expect("git history is documented");
    let longs: Vec<&str> = history["arguments"]
        .as_array()
        .expect("arguments")
        .iter()
        .filter_map(|argument| argument["long"].as_str())
        .collect();
    assert_eq!(longs, vec!["--item", "--limit", "--path"], "{longs:?}");
}

#[test]
fn agent_doc_can_describe_one_command_path() {
    let result = run(waymark().arg("agent-doc").arg("--command").arg("git diff"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let text = result.stdout();
    assert!(text.contains("`waymark git diff`"), "{text}");
    assert!(text.contains("--commit"), "{text}");
    assert!(
        !text.contains("`waymark create`"),
        "a selected path reports only that subtree:\n{text}"
    );
}

#[test]
fn agent_doc_adds_no_semantics_of_its_own() {
    // Every `##` heading is a real command path from the parser, and every
    // documented flag really exists on that command: the reference is derived,
    // so it cannot drift.
    let documented = run(waymark().arg("agent-doc"));
    assert_eq!(documented.status(), 0, "stderr: {}", documented.stderr());
    let text = documented.stdout();

    let parsed = run(waymark().arg("--json").arg("agent-doc"));
    let json = parsed.json();
    let mut paths: Vec<String> = Vec::new();
    collect_paths(&json["data"], &mut paths);
    for path in &paths {
        assert!(
            text.contains(&format!("## `{path}`")),
            "the Markdown reference is missing the real command path `{path}`"
        );
    }

    // And nothing documented is prose: no command path appears that the parser
    // does not have.
    let headings: Vec<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix("## `"))
        .filter_map(|line| line.strip_suffix('`'))
        .collect();
    for heading in headings {
        assert!(
            paths.iter().any(|path| path == heading),
            "`{heading}` is documented but is not a command the parser accepts"
        );
    }
}

fn collect_paths(value: &serde_json::Value, out: &mut Vec<String>) {
    if let Some(path) = value["path"].as_str() {
        out.push(path.to_owned());
    }
    if let Some(subcommands) = value["subcommands"].as_array() {
        for command in subcommands {
            collect_paths(command, out);
        }
    }
}
