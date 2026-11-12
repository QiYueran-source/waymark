//! The in-process MCP client harness shared by the adapter tests.
//!
//! The server and the client are connected through a duplex pipe with the real
//! rmcp JSON-RPC framing, so a test exercises the same transport boundary an
//! out-of-process stdio client would. Nothing reaches into the server: every
//! observation goes through the protocol.

#![allow(dead_code)]

use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};
use rmcp::model::{CallToolRequestParam, CallToolResult, Tool};
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceExt};
use serde_json::{json, Value};
use tempfile::TempDir;
use waymark_core::error::WaymarkError;
use waymark_core::ops::create::{self, CreateRequest};
use waymark_core::ops::OpOptions;
use waymark_core::testing;
use waymark_mcp::WaymarkServer;

/// A running in-process server and its connected client.
pub struct Client {
    pub client: RunningService<RoleClient, ()>,
    server_task: tokio::task::JoinHandle<()>,
}

impl Client {
    /// Connect a client to a server that resolves projects from `start`.
    pub async fn connect(start: Utf8PathBuf) -> Self {
        let (server_transport, client_transport) = tokio::io::duplex(1 << 20);
        let server = WaymarkServer::new(start);
        let server_task = tokio::spawn(async move {
            if let Ok(service) = server.serve(server_transport).await {
                let _ = service.waiting().await;
            }
        });
        let client = ().serve(client_transport).await.expect("the client initialises");
        Self {
            client,
            server_task,
        }
    }

    /// Call a tool, asserting the protocol layer succeeded.
    pub async fn call(&self, name: &str, arguments: Value) -> CallToolResult {
        self.try_call(name, arguments)
            .await
            .expect("the tool call is a protocol success")
    }

    /// Call a tool and keep a protocol failure as `Err`, for the tests that
    /// assert JSON-RPC errors stay separate from business envelopes.
    pub async fn try_call(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<CallToolResult, rmcp::service::ServiceError> {
        self.client
            .call_tool(CallToolRequestParam {
                name: name.to_owned().into(),
                arguments: match arguments {
                    Value::Object(map) => Some(map),
                    Value::Null => None,
                    other => panic!("tool arguments must be an object, got {other}"),
                },
            })
            .await
    }

    /// The tools the server advertises.
    pub async fn tools(&self) -> Vec<Tool> {
        self.client
            .list_tools(Default::default())
            .await
            .expect("the tool list is a protocol success")
            .tools
    }

    /// Close the client and wait for the server to observe the closed transport.
    pub async fn shutdown(self) {
        let _ = self.client.cancel().await;
        let _ = self.server_task.await;
    }
}

/// Parse the shared envelope out of a tool result.
pub fn envelope(result: &CallToolResult) -> Value {
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .expect("a tool result carries a text content");
    let value: Value = serde_json::from_str(&text.text).expect("the tool result content is JSON");
    assert_eq!(
        result.structured_content.as_ref(),
        Some(&value),
        "the structured content and the text content must be the same envelope"
    );
    value
}

/// A temporary project on disk, with the stable IDs of the seeded items.
pub struct Fixture {
    pub dir: TempDir,
    pub root: Utf8PathBuf,
    pub ids: Vec<String>,
}

impl Fixture {
    pub fn project(&self) -> waymark_core::Project {
        waymark_core::Project::discover(&self.root).expect("the fixture project opens")
    }
}

/// A project with one item per title, all in the default `task` category.
pub fn fixture(titles: &[&str]) -> Fixture {
    fixture_with(titles.iter().map(|title| (*title, None::<&str>)).collect())
}

/// A project with one item per `(title, body)` pair.
pub fn fixture_with(items: Vec<(&str, Option<&str>)>) -> Fixture {
    let (dir, project) = testing::deterministic_project();
    let root = project.root().to_path_buf();
    let mut ids = Vec::new();
    for (title, body) in items {
        let mut request = CreateRequest::new(title, "task", "frank");
        if let Some(body) = body {
            request = request.with_body(body);
        }
        let created = create::create(
            &project,
            request,
            &OpOptions::new().with_clock(testing::shared_clock()),
        )
        .expect("the fixture item is created");
        ids.push(created.item.id.to_string());
    }
    Fixture { dir, root, ids }
}

/// Open project operations for a fixture: a clock and no operator override.
pub fn options() -> OpOptions {
    OpOptions::new().with_clock(testing::shared_clock())
}

/// Make `id` wait for `blocking` through the shared operation.
pub fn depend(project: &waymark_core::Project, id: &str, blocking: &str) {
    waymark_core::ops::relationship::add_dependency(project, id, blocking, &options())
        .expect("the dependency is added");
}

/// Make `parent` the parent of `id` through the shared operation.
pub fn set_parent(project: &waymark_core::Project, id: &str, parent: &str) {
    waymark_core::ops::parent::set_parent(project, id, parent, &options())
        .expect("the parent is set");
}

/// A business failure from a tool result, with its typed JSON.
pub fn business_error(result: &CallToolResult) -> Value {
    let envelope = envelope(result);
    assert_eq!(envelope["ok"], false, "expected a failure envelope");
    assert_eq!(
        result.is_error,
        Some(true),
        "a business failure is marked isError"
    );
    envelope["error"].clone()
}

/// Whether a code literal is a member of the shared registry, by deserializing
/// it into [`waymark_core::Code`]. This is the "no second taxonomy" check.
pub fn is_registered_code(code: &Value) -> bool {
    serde_json::from_value::<waymark_core::Code>(code.clone()).is_ok()
}

/// Build a [`WaymarkError`] failure for comparison with an MCP failure body.
pub fn shared_failure(error: &WaymarkError) -> Value {
    waymark_core::envelope::failure_envelope(error)["error"].clone()
}

/// A temp directory that is not a Waymark project.
pub fn empty_dir() -> (TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf())
        .expect("the temporary directory path is UTF-8");
    (dir, root)
}

/// The empty argument object.
pub fn no_args() -> Value {
    json!({})
}

/// Every regular file under `root`, keyed by repository-relative path, with the
/// exact bytes on disk. Used to prove a dry run wrote nothing.
pub fn snapshot(root: &Utf8Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    collect_files(root, root, &mut files);
    files
}

fn collect_files(root: &Utf8Path, directory: &Utf8Path, files: &mut BTreeMap<String, Vec<u8>>) {
    for entry in std::fs::read_dir(directory).expect("a readable directory") {
        let entry = entry.expect("a readable directory entry");
        let path = Utf8PathBuf::from_path_buf(entry.path()).expect("a UTF-8 path");
        if path.is_dir() {
            collect_files(root, &path, files);
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .expect("the file is under the root")
                .to_string();
            files.insert(relative, std::fs::read(&path).expect("a readable file"));
        }
    }
}

/// Read one document as text.
pub fn read(path: &Utf8Path) -> String {
    std::fs::read_to_string(path).expect("the document is readable")
}

/// Warm the write lock so a later snapshot does not see the lock file appear.
///
/// Every mediated write takes the one project lock; the lock file is the
/// mutual-exclusion artifact, not business data. Taking it once before a
/// write-nothing assertion keeps the comparison strictly byte-for-byte.
pub fn warm_lock(root: &Utf8Path) -> waymark_core::Project {
    let project = waymark_core::Project::discover(root).expect("the project opens");
    waymark_core::lock::with_project_lock(
        &project,
        waymark_core::lock::LockWait::immediate(),
        || Ok::<(), waymark_core::error::WaymarkError>(()),
    )
    .expect("the lock is acquirable");
    project
}

/// The workspace's `waymark` CLI binary, built next to this test executable.
///
/// `cargo test --workspace` builds every workspace binary, so the equivalence
/// tests can drive the real CLI process. Running only `cargo test -p
/// waymark-mcp` builds just this package's targets; the assertion says so
/// instead of silently skipping the comparison.
pub fn waymark_binary() -> Utf8PathBuf {
    let executable = std::env::current_exe().expect("the test executable path");
    let target_dir = executable
        .parent()
        .and_then(std::path::Path::parent)
        .expect("the test executable sits under the target directory");
    let name = if cfg!(windows) {
        "waymark.exe"
    } else {
        "waymark"
    };
    let candidate = Utf8PathBuf::from_path_buf(target_dir.join(name)).expect("a UTF-8 binary path");
    assert!(
        candidate.is_file(),
        "the `waymark` binary is missing at {candidate}; run `cargo build -p waymark-cli` \
         (or the workspace gate `cargo test --workspace`) before the CLI-parity tests"
    );
    candidate
}

/// Run one CLI command in JSON mode against `root` and return its envelope.
pub fn cli_json(root: &Utf8Path, arguments: &[&str]) -> Value {
    let output = std::process::Command::new(waymark_binary())
        .arg("--json")
        .arg("--project")
        .arg(root)
        .args(arguments)
        .output()
        .expect("the CLI binary runs");
    assert!(
        output.status.success(),
        "the CLI command {arguments:?} failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "the CLI did not emit one JSON envelope ({error}): {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

/// Copy a project directory to a fresh temporary directory.
pub fn copy_project(source: &Utf8Path) -> (TempDir, Utf8PathBuf) {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8PathBuf::from_path_buf(directory.path().to_path_buf())
        .expect("the temporary directory path is UTF-8");
    copy_tree(source, &root);
    (directory, root)
}

fn copy_tree(source: &Utf8Path, destination: &Utf8Path) {
    for entry in std::fs::read_dir(source).expect("a readable project directory") {
        let entry = entry.expect("a readable directory entry");
        let path = Utf8PathBuf::from_path_buf(entry.path()).expect("a UTF-8 path");
        let name = path.file_name().expect("a file name");
        let target = destination.join(name);
        if path.is_dir() {
            std::fs::create_dir_all(&target).expect("a creatable directory");
            copy_tree(&path, &target);
        } else if path.is_file() {
            std::fs::copy(&path, &target).expect("a copyable file");
        }
    }
}

/// A project snapshot with operation-clock values normalized.
///
/// Two adapters run the same write against equivalent projects at different
/// instants. The document timestamps are the operation clock (system time by
/// default in both adapters), so comparing on-disk equality normalizes exactly
/// those values and leaves every other byte, including field order, untouched.
pub fn normalized_snapshot(root: &Utf8Path) -> BTreeMap<String, Vec<u8>> {
    snapshot(root)
        .into_iter()
        .map(|(path, bytes)| {
            if path.ends_with(".md") {
                (
                    path,
                    normalize_clocks(&String::from_utf8_lossy(&bytes)).into_bytes(),
                )
            } else {
                (path, bytes)
            }
        })
        .collect()
}

/// Replace frontmatter clock values with a placeholder, leaving everything else.
pub fn normalize_clocks(text: &str) -> String {
    normalize(text, false)
}

/// Replace frontmatter clock values and the stable ID with placeholders.
pub fn normalize_identity(text: &str) -> String {
    normalize(text, true)
}

fn normalize(text: &str, replace_id: bool) -> String {
    let mut in_frontmatter = false;
    let mut fences = 0;
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        if line == "---" {
            fences += 1;
            in_frontmatter = fences == 1;
            lines.push(line.to_owned());
            continue;
        }
        let key = line.split_once(':').map(|(key, _)| key);
        if in_frontmatter
            && matches!(
                key,
                Some("created_at" | "updated_at" | "closed_at" | "archived_at")
            )
        {
            lines.push(format!("{}: <clock>", key.unwrap()));
            continue;
        }
        if in_frontmatter && replace_id && key == Some("id") {
            lines.push("id: <id>".to_owned());
            continue;
        }
        lines.push(line.to_owned());
    }
    let mut normalized = lines.join("\n");
    if text.ends_with('\n') {
        normalized.push('\n');
    }
    normalized
}

/// The `work/task` document path of one fixture item.
pub fn task_path(root: &Utf8Path, id: &str) -> Utf8PathBuf {
    root.join("work").join("task").join(format!("{id}.md"))
}
