//! The MCP service: one Waymark server over one working directory.
//!
//! The server is a thin adapter. It never keeps a second copy of the project or
//! a second interpretation of a rule: each tool discovers the project from the
//! configured start directory, calls `waymark_core`, and renders the result.
//! That keeps a long-lived server honest when the files or the configuration
//! change underneath it.
//!
//! # Project resolution
//!
//! `start` is the explicit `--project` directory or the process working
//! directory. A tool that needs a project calls [`Project::discover`], which
//! walks up to the nearest `.waymark/config.toml`. A missing project is the
//! registry's `PROJECT_NOT_FOUND` business failure, not a panic and not a
//! protocol error; `waymark_project_init` still works where no project exists.

use camino::{Utf8Path, Utf8PathBuf};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::tool_handler;
use waymark_core::error::WaymarkError;
use waymark_core::Project;

/// The Waymark MCP server.
#[derive(Clone)]
pub struct WaymarkServer {
    start: Utf8PathBuf,
    tool_router: ToolRouter<Self>,
}

impl WaymarkServer {
    /// A server that resolves its project from `start`.
    pub fn new(start: Utf8PathBuf) -> Self {
        let mut tool_router = Self::project_router();
        tool_router.merge(Self::config_router());
        tool_router.merge(Self::migrate_router());
        tool_router.merge(Self::read_router());
        Self { start, tool_router }
    }

    /// The directory the project is discovered from.
    pub fn start(&self) -> &Utf8Path {
        &self.start
    }

    /// Discover the project for this call.
    ///
    /// Deliberately per call: the MCP server is long-lived, and a configuration
    /// or file change must be visible to the next tool call exactly as it would
    /// be to the next CLI invocation.
    pub fn project(&self) -> Result<Project, WaymarkError> {
        Project::discover(&self.start)
    }
}

#[tool_handler]
impl rmcp::ServerHandler for WaymarkServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: rmcp::model::Implementation {
                name: "waymark-mcp".to_owned(),
                title: Some("Waymark".to_owned()),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                ..Default::default()
            },
            instructions: Some(
                "Waymark work-item tracker. Every successful tool returns the shared versioned \
                 envelope {v, ok: true, data, _meta}; every business failure returns \
                 {v, ok: false, error} with a stable code, typed details, an optional location, \
                 and retryable. Read the tool descriptions for arguments and defaults."
                    .to_owned(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
