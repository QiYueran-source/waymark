//! Drive the real `waymark-mcp` binary over a piped stdio pair.
//!
//! The in-process tests exercise the library; this test proves the process
//! boundary too: argument parsing, startup project resolution, the rmcp stdio
//! transport, and the JSON-RPC framing an out-of-process MCP client sees.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

fn write_line(stdin: &mut impl Write, value: &Value) {
    writeln!(stdin, "{value}").expect("the request is written");
    stdin.flush().expect("the request is flushed");
}

fn read_response(lines: &mut impl Iterator<Item = std::io::Result<String>>) -> Value {
    let line = lines
        .next()
        .expect("a response line")
        .expect("readable output");
    serde_json::from_str(&line).expect("the response is JSON")
}

/// Call one tool and return its shared success envelope.
fn call_envelope(
    stdin: &mut impl Write,
    lines: &mut impl Iterator<Item = std::io::Result<String>>,
    id: u64,
    name: &str,
) -> Value {
    write_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": name, "arguments": {}}
        }),
    );
    let called = read_response(lines);
    assert!(
        !called["result"]["isError"].as_bool().unwrap_or(false),
        "{name} is a successful answer: {called}"
    );
    let text = called["result"]["content"][0]["text"]
        .as_str()
        .expect("the envelope is text content");
    let envelope: Value = serde_json::from_str(text).expect("the content is the shared envelope");
    assert_eq!(envelope["ok"], true, "{name}: {envelope}");
    envelope
}

#[test]
fn the_binary_serves_the_catalog_over_a_piped_stdio_pair() {
    let (dir, project) = waymark_core::testing::project();
    let root = project.root().to_path_buf();

    let mut child = Command::new(env!("CARGO_BIN_EXE_waymark-mcp"))
        .arg("--project")
        .arg(root.as_str())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the MCP server binary starts");
    let mut stdin = child.stdin.take().expect("the child has stdin");
    let mut lines = BufReader::new(child.stdout.take().expect("the child has stdout")).lines();

    write_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "waymark-mcp-test", "version": "0"}
            }
        }),
    );
    let initialized = read_response(&mut lines);
    assert_eq!(initialized["id"], 1);
    assert_eq!(initialized["result"]["serverInfo"]["name"], "waymark-mcp");
    assert!(initialized["result"]["capabilities"]["tools"].is_object());

    write_line(
        &mut stdin,
        &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    );
    write_line(
        &mut stdin,
        &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let listed = read_response(&mut lines);
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .expect("the tool list")
        .iter()
        .map(|tool| tool["name"].as_str().expect("a tool name"))
        .collect();
    assert_eq!(names.len(), waymark_mcp::TOOLS.len());
    for expected in waymark_mcp::TOOLS {
        assert!(names.contains(expected), "the catalog names {expected}");
    }

    write_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "waymark_list_work_items", "arguments": {}}
        }),
    );
    let called = read_response(&mut lines);
    let text = called["result"]["content"][0]["text"]
        .as_str()
        .expect("the envelope is text content");
    let envelope: Value = serde_json::from_str(text).expect("the content is the shared envelope");
    assert_eq!(envelope["v"], 1);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["count"], 0);
    assert_eq!(envelope["data"]["limit"], 50);
    assert!(envelope["_meta"]["completeness"].is_string());

    // Closing stdin closes the transport; the server exits cleanly.
    drop(stdin);
    let status = child.wait().expect("the server exits");
    assert!(status.success(), "the server exited with {status}");
    drop(dir);
}

#[test]
fn the_binary_degrades_without_git_and_reports_both_causes() {
    let (dir, project) = waymark_core::testing::project();
    let root = project.root().to_path_buf();

    // No repository and no `git` on the child's PATH: both causes are missing,
    // and both must travel through the same successful envelope.
    let mut child = Command::new(env!("CARGO_BIN_EXE_waymark-mcp"))
        .arg("--project")
        .arg(root.as_str())
        .env_remove("PATH")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the MCP server binary starts");
    let mut stdin = child.stdin.take().expect("the child has stdin");
    let mut lines = BufReader::new(child.stdout.take().expect("the child has stdout")).lines();

    write_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "waymark-mcp-test", "version": "0"}
            }
        }),
    );
    assert_eq!(read_response(&mut lines)["id"], 1);
    write_line(
        &mut stdin,
        &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    );

    for (id, tool) in [(2u64, "waymark_git_history"), (3, "waymark_git_diff")] {
        let envelope = call_envelope(&mut stdin, &mut lines, id, tool);
        let diagnostics = envelope["_meta"]["diagnostics"]
            .as_array()
            .unwrap_or_else(|| panic!("{tool} carries diagnostics: {envelope}"));
        let codes: Vec<&str> = diagnostics
            .iter()
            .map(|finding| finding["code"].as_str().expect("a code"))
            .collect();
        assert_eq!(
            codes,
            vec!["NOT_A_GIT_REPOSITORY", "GIT_UNAVAILABLE"],
            "{tool}"
        );
        for finding in diagnostics {
            assert_eq!(finding["severity"], "info", "{tool}");
            assert_eq!(finding["fixable"], false, "{tool}");
            assert!(finding["location"].is_null(), "{tool}");
        }
    }

    for (id, tool) in [(4u64, "waymark_doctor"), (5, "waymark_validate")] {
        let envelope = call_envelope(&mut stdin, &mut lines, id, tool);
        let findings = envelope["data"]["findings"]
            .as_array()
            .unwrap_or_else(|| panic!("{tool} reports findings: {envelope}"));
        for code in ["NOT_A_GIT_REPOSITORY", "GIT_UNAVAILABLE"] {
            let finding = findings
                .iter()
                .find(|finding| finding["code"] == code)
                .unwrap_or_else(|| panic!("{tool} reports {code}: {findings:?}"));
            assert_eq!(finding["severity"], "info", "{tool} {code}");
            assert_eq!(finding["fixable"], false, "{tool} {code}");
            assert!(finding["location"].is_null(), "{tool} {code}");
        }
    }

    assert!(
        !root.join(".git").exists(),
        "a diagnostic run must never initialize a repository"
    );

    drop(stdin);
    let status = child.wait().expect("the server exits");
    assert!(status.success(), "the server exited with {status}");
    drop(dir);
}

/// The install check a CLI connector runs: one line on stdout, exit 0 when a
/// project is found, the reason on stderr and a nonzero exit when it is not.
#[test]
fn ready_answers_on_one_line_without_serving() {
    let bin = env!("CARGO_BIN_EXE_waymark-mcp");

    let with_project = Command::new(bin)
        .arg("--ready")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("the binary runs");
    assert!(
        with_project.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&with_project.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&with_project.stdout).trim(), "OK");

    let empty = tempfile::tempdir().expect("a temporary directory");
    let without_project = Command::new(bin)
        .arg("--ready")
        .current_dir(empty.path())
        .output()
        .expect("the binary runs");
    assert!(!without_project.status.success());
    assert!(without_project.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&without_project.stderr).contains("no Waymark project"),
        "stderr: {}",
        String::from_utf8_lossy(&without_project.stderr)
    );

    // Logging out is idempotent and succeeds: the server keeps no user state.
    let forget = Command::new(bin)
        .arg("--forget")
        .output()
        .expect("the binary runs");
    assert!(forget.status.success());
    assert_eq!(String::from_utf8_lossy(&forget.stdout).trim(), "OK");

    // The two modes exclude each other rather than silently picking one.
    let both = Command::new(bin)
        .args(["--ready", "--forget"])
        .output()
        .expect("the binary runs");
    assert!(!both.status.success());
}
