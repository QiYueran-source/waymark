//! The in-process MCP client harness shared by the adapter tests.
//!
//! The server and the client are connected through a duplex pipe with the real
//! rmcp JSON-RPC framing, so a test exercises the same transport boundary an
//! out-of-process stdio client would. Nothing reaches into the server: every
//! observation goes through the protocol.

#![allow(dead_code)]

use camino::Utf8PathBuf;
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
