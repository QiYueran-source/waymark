//! `waymark doctor` and the widened `waymark validate`, end to end.
//!
//! The domain suite proves what the findings mean; these tests prove the adapter
//! routes them: syntax, streams, the envelope, and the exit class strict mode
//! chooses from the registry.

use std::process::{Command, Output};
use std::time::{Duration, Instant};

use camino::Utf8PathBuf;
use waymark_core::lock::{with_project_lock, LockWait};
use waymark_core::Project;

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
    let result = run(waymark().arg("init").arg("--project").arg(&root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    (dir, root)
}

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

/// Give one item a dependency on a stable ID that does not exist.
fn add_missing_dependency(root: &Utf8PathBuf, id: &str) {
    let path = root.join(format!("work/task/{id}.md"));
    let mut item = waymark_core::parse::parse_item_file(&path).expect("the document parses");
    item.dependencies
        .push(waymark_core::WorkItemId::parse("wm-0000000000zz").unwrap());
    std::fs::write(&path, waymark_core::serialize::to_document(&item)).unwrap();
}

#[test]
fn doctor_reports_a_whole_project_and_exits_zero_by_default() {
    let (_dir, root) = project();
    let id = create_one(&root);
    add_missing_dependency(&root, &id);

    let result = run(waymark()
        .arg("--json")
        .arg("doctor")
        .arg("--project")
        .arg(&root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let envelope = result.json();
    assert_eq!(envelope["ok"], true);
    let data = &envelope["data"];
    assert!(
        data["scope"]
            .as_array()
            .unwrap()
            .iter()
            .any(|part| part == "work_items"),
        "{}",
        result.stdout()
    );
    assert!(
        data["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| finding["code"] == "DANGLING_REFERENCE"),
        "{}",
        result.stdout()
    );
    assert_eq!(data["strict"], false);
    assert_eq!(data["counts"]["errors"], 1);
    assert_eq!(envelope["_meta"]["source"], "memory_projection");
    assert!(
        data["repairs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|repair| repair["kind"] == "projection_rebuild"),
        "{}",
        result.stdout()
    );
}

#[test]
fn doctor_strict_returns_the_invalid_data_class_but_keeps_the_report_envelope() {
    let (_dir, root) = project();
    let id = create_one(&root);
    add_missing_dependency(&root, &id);

    let result = run(waymark()
        .arg("--json")
        .arg("doctor")
        .arg("--project")
        .arg(&root)
        .arg("--strict"));
    assert_eq!(result.status(), 3, "stderr: {}", result.stderr());
    let envelope = result.json();
    assert_eq!(envelope["ok"], true, "the report is still the answer");
    assert_eq!(envelope["data"]["strict"], true);
    assert!(envelope["data"]["counts"]["errors"].as_u64().unwrap() > 0);
}

#[test]
fn a_warning_only_doctor_report_succeeds_even_in_strict_mode() {
    let (_dir, root) = project();
    let id = create_one(&root);
    let path = root.join(format!("work/task/{id}.md"));
    let document = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|line| {
            if line.starts_with("updated_at: ") {
                "updated_at: 2020-01-01T00:00:00Z"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    std::fs::write(&path, document).unwrap();

    let result = run(waymark()
        .arg("--json")
        .arg("doctor")
        .arg("--project")
        .arg(&root)
        .arg("--strict"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    assert_eq!(result.json()["data"]["counts"]["errors"], 0);
    assert!(
        result.json()["data"]["counts"]["warnings"]
            .as_u64()
            .unwrap()
            > 0
    );
}

#[test]
fn validate_reviews_the_whole_project_not_only_the_configuration() {
    let (_dir, root) = project();
    let id = create_one(&root);
    add_missing_dependency(&root, &id);

    let result = run(waymark()
        .arg("--json")
        .arg("validate")
        .arg("--project")
        .arg(&root)
        .arg("--strict"));
    assert_eq!(result.status(), 3, "stderr: {}", result.stderr());
    let data = &result.json()["data"];
    assert!(data["scope"]
        .as_array()
        .unwrap()
        .iter()
        .any(|part| part == "work_items"));
    assert!(data["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| finding["code"] == "DANGLING_REFERENCE"));

    let human = run(waymark()
        .arg("validate")
        .arg("--project")
        .arg(&root)
        .arg("--strict"));
    assert_eq!(human.status(), 3);
    assert!(
        human.stderr().contains("DANGLING_REFERENCE"),
        "{}",
        human.stderr()
    );
    assert!(
        human.stderr().contains("reviewed"),
        "the human text names the reviewed scope: {}",
        human.stderr()
    );
}

#[test]
fn doctor_reports_a_held_lock_without_waiting() {
    let (_dir, root) = project();
    let project = Project::discover(&root).expect("the project opens");

    let started = Instant::now();
    let result = with_project_lock(&project, LockWait::default_policy(), || {
        let result = run(waymark()
            .arg("--json")
            .arg("doctor")
            .arg("--project")
            .arg(&root));
        Ok(result)
    })
    .unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "doctor waited on the held lock"
    );
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let findings = result.json()["data"]["findings"].clone();
    assert!(
        findings
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| finding["code"] == "LOCK_HELD" && finding["severity"] == "info"),
        "{}",
        result.stdout()
    );
}
