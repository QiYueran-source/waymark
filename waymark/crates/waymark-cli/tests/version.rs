//! `waymark version` in both build variants.
//!
//! The basic build names `cli` and `mcp`; only a Web build names `web` and
//! carries the embedded frontend manifest. Each variant's tests are compiled
//! only into that variant, so `cargo test -p waymark-cli` and
//! `cargo test -p waymark-cli --features web` assert opposite shapes.

use std::process::{Command, Output};

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

/// The workspace version the binary was built from.
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(not(feature = "web"))]
#[test]
fn a_basic_build_reports_cli_and_mcp_only() {
    let result = run(waymark().arg("version"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    assert_eq!(result.stdout(), format!("waymark {VERSION} (cli, mcp)\n"));
    assert_eq!(result.stderr(), "");
}

#[cfg(not(feature = "web"))]
#[test]
fn a_basic_build_omits_assets_from_the_json_data() {
    let result = run(waymark().arg("--json").arg("version"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let envelope = result.json();
    assert_eq!(envelope["v"], 1);
    assert_eq!(envelope["ok"], true);
    assert!(envelope.get("_meta").is_some(), "the envelope keeps _meta");
    assert_eq!(
        envelope["data"],
        serde_json::json!({
            "version": VERSION,
            "capabilities": ["cli", "mcp"],
        })
    );
    assert!(
        envelope["data"].get("assets").is_none(),
        "a basic build has no manifest, so the key is absent rather than null"
    );
}

#[cfg(feature = "web")]
#[test]
fn a_web_build_reports_web_last() {
    let result = run(waymark().arg("version"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    assert_eq!(
        result.stdout(),
        format!("waymark {VERSION} (cli, mcp, web)\n")
    );
    assert_eq!(result.stderr(), "");
}

#[cfg(feature = "web")]
#[test]
fn a_web_build_json_carries_the_embedded_manifest() {
    let result = run(waymark().arg("--json").arg("version"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let envelope = result.json();
    assert_eq!(envelope["v"], 1);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["version"], VERSION);
    assert_eq!(
        envelope["data"]["capabilities"],
        serde_json::json!(["cli", "mcp", "web"])
    );

    let assets = envelope["data"]
        .get("assets")
        .expect("a Web build embeds the manifest");
    assert_eq!(assets["version"], VERSION);
    assert!(
        assets["build_id"].as_str().is_some_and(|id| !id.is_empty()),
        "the manifest names the frontend build: {assets}"
    );
    let files = assets["files"]
        .as_array()
        .expect("the manifest lists its files");
    assert!(
        files.iter().any(|file| file["path"] == "index.html"),
        "the manifest describes the shell"
    );
    assert!(
        files
            .iter()
            .all(|file| file["sha256"].as_str().is_some_and(|sha| sha.len() == 64)),
        "every entry carries a SHA-256"
    );
}
