//! The development launcher: clear startup failures, and no project changes.

mod support;

use std::io::BufReader;
use std::process::{Command, Stdio};

use support::{assets, fixture, snapshot, Child};

fn launcher() -> Command {
    Command::new(env!("CARGO_BIN_EXE_waymark-web-dev"))
}

#[test]
fn missing_assets_fail_before_serving_and_do_not_touch_the_project() {
    let fixture = fixture();
    let empty = tempfile::tempdir().expect("an empty assets directory");
    let before = snapshot(fixture.root());

    let output = launcher()
        .arg("--project")
        .arg(fixture.root())
        .arg("--port")
        .arg("0")
        .arg("--assets")
        .arg(empty.path())
        .output()
        .expect("the launcher runs");

    assert!(
        !output.status.success(),
        "missing assets must fail the launch"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("frontend assets are missing"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("lockfile"), "stderr: {stderr}");
    assert!(stderr.contains("build"), "stderr: {stderr}");
    assert!(stderr.contains("never runs npm"), "stderr: {stderr}");
    assert_eq!(
        before,
        snapshot(fixture.root()),
        "a failed launch must not create or modify anything in the project"
    );
}

#[test]
fn missing_project_fails_clearly_without_creating_it() {
    let holder = tempfile::tempdir().expect("a temporary directory");
    let missing = holder.path().join("no-project-here");
    let assets = assets();

    let output = launcher()
        .arg("--project")
        .arg(&missing)
        .arg("--port")
        .arg("0")
        .arg("--assets")
        .arg(assets.path())
        .output()
        .expect("the launcher runs");

    assert!(
        !output.status.success(),
        "a missing project must fail the launch"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("PROJECT_NOT_FOUND"), "stderr: {stderr}");
    assert!(
        !missing.exists(),
        "the launcher must not initialize the project"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn launcher_serves_health_from_the_printed_loopback_url() {
    let fixture = fixture();
    let assets = assets();
    let mut command = launcher();
    command
        .arg("--project")
        .arg(fixture.root())
        .arg("--port")
        .arg("0")
        .arg("--assets")
        .arg(assets.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("the launcher spawns");
    let stdout = child.stdout.take().expect("the launcher stdout is piped");
    let mut child = Child {
        child,
        stdout: BufReader::new(stdout),
    };

    let project_line = child.next_line();
    assert!(project_line.contains("project"), "line: {project_line}");
    let listening = child.next_line();
    assert!(
        listening.contains("listening on http://localhost:"),
        "line: {listening}"
    );
    let port: u16 = listening
        .rsplit(':')
        .next()
        .expect("a port")
        .parse()
        .expect("a numeric port");
    let response = support::get(
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        "/api/health",
    );
    assert_eq!(response.status, 200);
    assert_eq!(response.json()["data"]["read_only"], true);
}
