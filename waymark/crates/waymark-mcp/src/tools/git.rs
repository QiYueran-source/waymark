//! Git inspection tools: commit history and diffs.
//!
//! `waymark_core::git` owns repository detection, path scoping, and the Git
//! invocation. Its contract is explicit: outside a repository, or without a
//! `git` binary, the report is empty and carries one informational finding per
//! missing cause — `NOT_A_GIT_REPOSITORY` for the repository and
//! `GIT_UNAVAILABLE` for the executable, both when both are missing. It is a
//! successful answer, not a business failure. These tools therefore return the
//! shared success envelope and let `_meta.diagnostics` describe the environment.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use waymark_core::git;
use waymark_core::ops::read::normalize;

use crate::envelope::{self, ToolResult};
use crate::server::WaymarkServer;

/// Inputs of `waymark_git_history`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GitHistoryArgs {
    /// Restrict history to one work item's document. Mutually exclusive with
    /// `path`; naming both is `VALIDATION_ERROR`.
    #[serde(default)]
    pub item: Option<String>,

    /// Restrict history to a project-relative path instead of the storage root.
    #[serde(default)]
    pub path: Option<String>,

    /// Maximum commits; `0` means unlimited. Defaults to 20.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Inputs of `waymark_git_diff`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GitDiffArgs {
    /// Restrict the diff to one work item's document. Omitted means the whole
    /// configured storage root.
    #[serde(default)]
    pub item: Option<String>,

    /// Inspect this commit instead of the working tree.
    #[serde(default)]
    pub commit: Option<String>,
}

#[tool_router(router = git_router, vis = "pub(crate)")]
impl WaymarkServer {
    /// Read commit history for one work item, one project-relative path, or the
    /// configured storage root. Newest first, bounded by `limit`. Outside a Git
    /// repository, or without a `git` executable, the result is empty and
    /// `_meta.diagnostics` carries one informational finding per missing cause
    /// (`NOT_A_GIT_REPOSITORY`, `GIT_UNAVAILABLE`); the call still succeeds.
    #[tool(
        name = "waymark_git_history",
        annotations(title = "Read the project's Git history")
    )]
    async fn waymark_git_history(
        &self,
        Parameters(args): Parameters<GitHistoryArgs>,
    ) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let mut request = git::HistoryRequest::new()
                .with_limit(args.limit.unwrap_or(git::DEFAULT_HISTORY_LIMIT));
            if let Some(item) = &args.item {
                request = request.for_item(normalize(item)?);
            }
            if let Some(path) = &args.path {
                request = request.with_path(path.clone());
            }
            let report = git::history(&project, &request)?;
            Ok(envelope::success(report.data_json(), report.meta.clone()))
        })
    }

    /// Read the unified diff of one work item's document, or of the storage
    /// root, against the working tree or one commit. Outside a Git repository,
    /// or without a `git` executable, the patch is empty and
    /// `_meta.diagnostics` carries one informational finding per missing cause
    /// (`NOT_A_GIT_REPOSITORY`, `GIT_UNAVAILABLE`); the call still succeeds.
    #[tool(
        name = "waymark_git_diff",
        annotations(title = "Read a work-item Git diff")
    )]
    async fn waymark_git_diff(&self, Parameters(args): Parameters<GitDiffArgs>) -> ToolResult {
        envelope::respond(|| {
            let project = self.project()?;
            let mut request = git::DiffRequest::new();
            if let Some(item) = &args.item {
                request = request.for_item(normalize(item)?);
            }
            if let Some(commit) = &args.commit {
                request = request.with_commit(commit.clone());
            }
            let report = git::diff(&project, &request)?;
            Ok(envelope::success(report.data_json(), report.meta.clone()))
        })
    }
}
