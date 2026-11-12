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
fn doctor_and_validate_report_git_availability_as_info_and_exit_zero() {
    let (_dir, root) = project();

    // No `git` on the child's PATH and no repository in the project: both
    // causes are missing, so both findings must be reported.
    for command in ["doctor", "validate"] {
        let result = run(waymark()
            .env_remove("PATH")
            .arg("--json")
            .arg(command)
            .arg("--project")
            .arg(&root));
        assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
        let envelope = result.json();
        assert_eq!(envelope["ok"], true, "{command}");
        let findings = envelope["data"]["findings"]
            .as_array()
            .unwrap_or_else(|| panic!("{command} reports findings"));
        for code in ["NOT_A_GIT_REPOSITORY", "GIT_UNAVAILABLE"] {
            let finding = findings
                .iter()
                .find(|finding| finding["code"] == code)
                .unwrap_or_else(|| panic!("{command} reports {code}: {findings:?}"));
            assert_eq!(finding["severity"], "info", "{command} {code}");
            assert_eq!(finding["fixable"], false, "{command} {code}");
            assert!(finding["location"].is_null(), "{command} {code}");
            assert!(
                !finding["message"].as_str().unwrap_or_default().is_empty(),
                "{command} {code}"
            );
        }
        assert!(
            !root.join(".git").exists(),
            "{command} must never initialize a repository"
        );
    }
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

/// Diagnostics travel the one dispatch and the one exit mapping like every
/// other command: an unopenable project is a `PROJECT_NOT_FOUND` failure with
/// the registry's exit class, not a special diagnostic exit.
#[test]
fn doctor_observes_a_closed_parent_at_info_severity_and_stays_healthy() {
    let (_dir, root) = project();
    let parent = create_one(&root);
    let child = create_one(&root);
    for arguments in [
        vec!["parent", "set", child.as_str(), parent.as_str()],
        vec!["close", parent.as_str()],
    ] {
        let mut command = waymark();
        command.arg("--project").arg(&root);
        for argument in arguments {
            command.arg(argument);
        }
        let result = run(&mut command);
        assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    }

    let result = run(waymark()
        .arg("--json")
        .arg("doctor")
        .arg("--strict")
        .arg("--project")
        .arg(&root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let data = result.json()["data"].clone();
    let observation = data["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|finding| finding["code"] == "CLOSED_PARENT")
        .unwrap_or_else(|| panic!("{}", result.stdout()))
        .clone();
    assert_eq!(observation["severity"], serde_json::json!("info"));
    assert_eq!(observation["fixable"], serde_json::json!(false));
    assert!(
        observation["message"].as_str().unwrap().contains(&child),
        "{observation}"
    );
    // An observation never makes a project unhealthy.
    assert_eq!(data["valid"], serde_json::json!(true));
    assert_eq!(data["counts"]["errors"], serde_json::json!(0));

    let human = run(waymark().arg("doctor").arg("--project").arg(&root));
    assert!(
        human.stderr().contains("CLOSED_PARENT"),
        "{}",
        human.stderr()
    );
}

/// Write a soft link onto one document only, the way a non-cascading add or a
/// hand edit leaves it.
fn record_one_sided_link(root: &Utf8PathBuf, id: &str, partner: &str) {
    let path = root.join(format!("work/task/{id}.md"));
    let mut item = waymark_core::parse::parse_item_file(&path).expect("the document parses");
    item.soft_links = vec![waymark_core::WorkItemId::parse(partner).unwrap()];
    std::fs::write(&path, waymark_core::serialize::to_document(&item)).unwrap();
}

fn archive_one(root: &Utf8PathBuf, id: &str) {
    let result = run(waymark().arg("archive").arg(id).arg("--project").arg(root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
}

#[test]
fn doctor_keeps_the_one_sided_and_archived_soft_link_findings_apart_and_offers_no_command() {
    let (_dir, root) = project();
    let stale = create_one(&root);
    let silent = create_one(&root);
    let partial = create_one(&root);
    let linked = create_one(&root);
    // `linked` records a link to a live partner that never recorded it back.
    record_one_sided_link(&root, &linked, &silent);
    // `partial` also still points at an item that is about to be archived; a
    // non-cascading archive leaves that reference where it is.
    record_one_sided_link(&root, &partial, &stale);
    archive_one(&root, &stale);

    let before = std::fs::read(root.join(format!("work/task/{partial}.md"))).unwrap();
    let result = run(waymark()
        .arg("--json")
        .arg("doctor")
        .arg("--project")
        .arg(&root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let findings = result.json()["data"]["findings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let of = |code: &str| -> Vec<serde_json::Value> {
        findings
            .iter()
            .filter(|finding| finding["code"] == code)
            .cloned()
            .collect()
    };

    let dangling = of("DANGLING_REFERENCE");
    assert_eq!(dangling.len(), 1, "{findings:#?}");
    assert_eq!(dangling[0]["severity"], "error");
    assert!(
        dangling[0]["message"].as_str().unwrap().contains(&stale),
        "{dangling:?}"
    );
    assert_eq!(
        dangling[0]["location"]["kind"], "work_item",
        "located on the referencing item"
    );

    let one_sided = of("ONE_SIDED_SOFT_LINK");
    assert_eq!(one_sided.len(), 1, "{findings:#?}");
    assert!(
        one_sided[0]["message"].as_str().unwrap().contains(&silent),
        "{one_sided:?}"
    );
    assert!(
        !one_sided[0]["message"].as_str().unwrap().contains(&stale),
        "an archived partner is not re-reported as one-sided: {one_sided:?}"
    );

    for finding in dangling.iter().chain(one_sided.iter()) {
        assert_eq!(finding["fixable"], false, "{finding}");
        let rendered = serde_json::to_string(finding).unwrap();
        assert!(
            !rendered.contains("waymark "),
            "no repair command: {rendered}"
        );
    }

    // Read-only: the documents come out byte-identical.
    assert_eq!(
        before,
        std::fs::read(root.join(format!("work/task/{partial}.md"))).unwrap()
    );
}

#[test]
fn show_reports_a_one_sided_soft_link_without_suggesting_a_repair() {
    let (_dir, root) = project();
    let here = create_one(&root);
    let silent = create_one(&root);
    record_one_sided_link(&root, &here, &silent);

    let result = run(waymark()
        .arg("--json")
        .arg("show")
        .arg(&here)
        .arg("--project")
        .arg(&root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let envelope = result.json();
    let issues = envelope["data"]["relationships"]["soft_link_issues"]
        .as_array()
        .cloned()
        .unwrap_or_else(|| panic!("{}", result.stdout()));
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0]["partner"], silent.as_str());
    assert_eq!(issues[0]["missing"], "link");
    assert_eq!(issues[0]["partner_owner"], "frank");
    assert!(
        issues[0]["reason"].as_str().unwrap().contains(&silent),
        "{issues:?}"
    );
    assert!(
        !issues[0].to_string().contains("waymark "),
        "no repair command: {}",
        issues[0]
    );

    // The human view says the same thing.
    let human = run(waymark().arg("show").arg(&here).arg("--project").arg(&root));
    assert_eq!(human.status(), 0, "stderr: {}", human.stderr());
    assert!(
        human.stderr().contains("soft link")
            && human.stderr().contains(&silent)
            && human.stderr().contains("never recorded"),
        "{}",
        human.stderr()
    );
}

#[test]
fn a_diagnostic_failure_uses_the_shared_exit_mapping() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let empty = Utf8PathBuf::from_path_buf(dir.path().to_path_buf())
        .expect("a UTF-8 path")
        .join("no-project");
    std::fs::create_dir_all(&empty).unwrap();

    for command in ["doctor", "validate"] {
        let result = run(waymark()
            .arg("--json")
            .arg("--project")
            .arg(&empty)
            .arg(command));
        assert_eq!(result.status(), 5, "{command}: {}", result.stderr());
        assert_eq!(
            result.json()["error"]["code"],
            serde_json::json!("PROJECT_NOT_FOUND"),
            "{command}"
        );
    }
}

/// Run `doctor` on a healthy project and assert the asset provenance check
/// added nothing. The basic build has no manifest; the Web build's manifest
/// matches, so both variants must stay silent.
fn assert_no_asset_mismatch() {
    let (_dir, root) = project();
    let result = run(waymark()
        .arg("--json")
        .arg("doctor")
        .arg("--project")
        .arg(&root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let envelope = result.json();
    let findings = envelope["data"]["findings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        findings
            .iter()
            .all(|finding| finding["code"] != "ASSETS_VERSION_MISMATCH"),
        "the embedded manifest matches this build: {findings:?}"
    );
}

/// A Web build embeds a manifest generated for the same version and shell, so
/// the provenance check adds no finding to a healthy project.
#[cfg(feature = "web")]
#[test]
fn a_web_build_reports_no_asset_mismatch_for_its_matching_manifest() {
    assert_no_asset_mismatch();
}

/// A basic build embeds no manifest, so there is nothing to disagree with.
#[cfg(not(feature = "web"))]
#[test]
fn a_basic_build_reports_no_asset_mismatch() {
    assert_no_asset_mismatch();
}

#[test]
fn validate_and_doctor_report_a_broken_local_document_at_its_file() {
    let (_dir, root) = project();
    std::fs::write(
        root.join(".waymark").join("user.toml"),
        "timezone = \"Mars/Olympus\"\n",
    )
    .unwrap();

    for command in ["validate", "doctor"] {
        let result = run(waymark()
            .arg("--json")
            .arg(command)
            .arg("--strict")
            .arg("--project")
            .arg(&root));
        assert_eq!(result.status(), 3, "{command}: {}", result.stderr());
        let envelope = result.json();
        assert_eq!(envelope["ok"], true, "{command}: {}", result.stdout());
        let findings = envelope["data"]["findings"]
            .as_array()
            .expect("the report lists findings");
        let local: Vec<&serde_json::Value> = findings
            .iter()
            .filter(|finding| {
                finding["location"]["kind"] == "file"
                    && finding["location"]["path"]
                        .as_str()
                        .is_some_and(|path| path.ends_with("user.toml"))
            })
            .collect();
        assert_eq!(local.len(), 1, "{command}: {}", result.stdout());
        assert_eq!(local[0]["code"], "CONFIG_INVALID");
        assert_eq!(local[0]["severity"], "error");
        assert_eq!(
            local[0]["fixable"], false,
            "a preference problem is repairable by hand, not by a command"
        );
        assert!(
            local[0]["message"]
                .as_str()
                .unwrap_or_default()
                .contains("timezone"),
            "{command}: {}",
            result.stdout()
        );
    }
}
