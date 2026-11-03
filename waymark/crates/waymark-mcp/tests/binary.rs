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
    assert_eq!(names.len(), waymark_mcp::PHASE_A_TOOLS.len());
    for expected in waymark_mcp::PHASE_A_TOOLS {
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
