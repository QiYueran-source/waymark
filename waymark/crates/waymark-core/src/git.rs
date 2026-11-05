//! Git-backed change inspection: commit history and diffs for work items.
//!
//! Waymark keeps **no** audit log, no attribution table, and no second history
//! store. A work item is a file in the caller's repository, so the repository is
//! the history: [`history`] runs `git log` over the item's path (or the whole
//! storage root), and [`diff`] runs `git show` or `git diff` over the same paths.
//! Nothing is written, and no Git crate is involved: the `git` binary is the
//! interface, so Waymark sees exactly what the user's Git sees.
//!
//! # Git is optional
//!
//! A project may live outside a repository, and `git` may be missing from `PATH`.
//! Neither is a failure: [`GitProbe`] reports the environment, and both
//! operations return an ordinary report with an empty payload and one
//! informational finding per missing cause in `_meta.diagnostics`:
//! `NOT_A_GIT_REPOSITORY` for the missing repository and `GIT_UNAVAILABLE` for
//! the missing executable. When both are missing, both findings are reported.
//! The command still succeeds. Bad *input* (an unknown item, a malformed path
//! filter) is still an error.
//!
//! # Scope
//!
//! - `history` with an item reports the commits that touched that item's file.
//!   Rename following (`git log --follow`) applies to a single path, so it is
//!   used only in that case; the storage-root scope is a plain `git log`.
//! - `history` without an item, or with `--path`, reports the storage root or
//!   the named path filter. `--path` is project-relative and may not escape the
//!   project.
//! - `diff` with a `--commit` reports what that commit changed for the item (or
//!   the storage root). Without a commit it reports the working tree, staged and
//!   unstaged, against `HEAD`; an untracked item file is diffed against
//!   `/dev/null` so a brand-new item still has a diff. A merge commit's diff is
//!   empty unless it is also a normal patch, which is Git's own rule.
//!
//! Pathspecs are always repository-relative, so a project in a subdirectory of
//! its repository is scoped correctly.

use std::process::{Command, Output};

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::envelope::{Completeness, Finding, Freshness, Meta, Source};
use crate::error::{ValidationIssue, WaymarkError};
use crate::id::WorkItemId;
use crate::store::Project;
use crate::time::Timestamp;

/// How many commits `history` reports when the caller does not say.
pub const DEFAULT_HISTORY_LIMIT: usize = 20;

/// How far the repository walk climbs before giving up.
const MAX_REPOSITORY_DEPTH: usize = 64;

/// The record separator between commits in `git log -z` output.
const RECORD: char = '\0';

/// The field separator inside one commit record.
const FIELD: char = '\u{1f}';

/// The `git log` pretty format, one line per commit.
const LOG_FORMAT: &str = "--format=%H\u{1f}%h\u{1f}%an\u{1f}%ae\u{1f}%ct\u{1f}%s";

/// A repository that contains the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repository {
    root: Utf8PathBuf,
    git_dir: Utf8PathBuf,
    project_prefix: String,
}

impl Repository {
    /// The repository's working-tree root: the directory holding `.git`.
    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    /// The git directory. For a linked worktree this is the worktree's private
    /// directory under the main repository, not `.git`.
    pub fn git_dir(&self) -> &Utf8Path {
        &self.git_dir
    }

    /// The project root relative to [`Repository::root`]; empty when they are
    /// the same directory.
    pub fn project_prefix(&self) -> &str {
        &self.project_prefix
    }

    /// `path` as a repository-relative pathspec, when it is inside the
    /// repository.
    pub fn relative(&self, path: &Utf8Path) -> Option<String> {
        path.strip_prefix(&self.root).ok().map(|relative| {
            // A pathspec is a Git path, not a filesystem path: Git reads and
            // reports `/` on every platform, so a Windows `\` would name
            // nothing.
            relative.as_str().replace('\\', "/")
        })
    }
}

/// The Git environment a project sits in: a repository, an executable, both, or
/// neither.
///
/// Detection never fails and never runs a command it does not need to: absence
/// is data, not an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitProbe {
    repository: Option<Repository>,
    executable: Option<Utf8PathBuf>,
}

impl GitProbe {
    /// Probe `project`: walk up for a repository and search `PATH` for `git`.
    pub fn detect(project: &Project) -> Self {
        Self::detect_with(project, find_executable())
    }

    /// Probe `project` with an explicit executable decision.
    ///
    /// `executable: None` means "no usable `git`", which is how tests and
    /// adapters model a missing binary without touching the ambient `PATH`.
    pub fn detect_with(project: &Project, executable: Option<Utf8PathBuf>) -> Self {
        Self {
            repository: find_repository(project),
            executable,
        }
    }

    /// Probe a plain directory that may not be a project yet.
    ///
    /// Configuration resolution runs before or beside a project being opened,
    /// so it needs the same environment a `Project` would report.
    pub fn detect_at(root: &Utf8Path) -> Self {
        Self {
            repository: find_repository_at(root),
            executable: find_executable(),
        }
    }

    /// An environment with neither a repository nor an executable.
    pub fn unavailable() -> Self {
        Self {
            repository: None,
            executable: None,
        }
    }

    /// Build a probe from its parts.
    pub fn from_parts(repository: Option<Repository>, executable: Option<Utf8PathBuf>) -> Self {
        Self {
            repository,
            executable,
        }
    }

    /// The repository containing the project, if any.
    pub fn repository(&self) -> Option<&Repository> {
        self.repository.as_ref()
    }

    /// The `git` executable, if one was found.
    pub fn executable(&self) -> Option<&Utf8Path> {
        self.executable.as_deref()
    }

    /// Whether both a repository and an executable are available.
    pub fn is_available(&self) -> bool {
        self.repository.is_some() && self.executable.is_some()
    }

    /// Why history and diff are unavailable, one finding per missing cause.
    ///
    /// Empty when the probe is available: an available environment has nothing
    /// to report. The repository finding comes first, then the executable
    /// finding, so a report with both causes reads in the order they can be
    /// acted on. Neither finding carries a location: both are environment
    /// facts, not document problems, and the registry declares no location kind
    /// for either code.
    pub fn findings(&self) -> Vec<Finding> {
        let mut findings = Vec::new();
        if self.repository.is_none() {
            findings.push(Finding::from_error(&WaymarkError::not_a_git_repository()));
        }
        if self.executable.is_none() {
            findings.push(Finding::from_error(&WaymarkError::git_unavailable()));
        }
        findings
    }
}

/// Find the repository root containing `project` by walking up to a directory
/// with a `.git` entry.
///
/// A `.git` **file** is accepted: that is how a linked worktree points at its
/// private git directory.
pub fn find_repository(project: &Project) -> Option<Repository> {
    find_repository_at(project.root())
}

/// Find the repository root containing `root`, a plain directory.
///
/// The path form exists because local configuration resolution needs the
/// repository for one identity lookup, not for a project: it runs before or
/// alongside a project being opened.
pub fn find_repository_at(root: &Utf8Path) -> Option<Repository> {
    let start = absolute(root);
    let mut current: Option<&Utf8Path> = Some(start.as_path());
    for _ in 0..MAX_REPOSITORY_DEPTH {
        let directory = current?;
        let dot_git = directory.join(".git");
        if dot_git.is_dir() || dot_git.is_file() {
            let project_prefix = root
                .strip_prefix(directory)
                .map(|relative| relative.as_str().replace('\\', "/"))
                .unwrap_or_default();
            return Some(Repository {
                root: directory.to_path_buf(),
                git_dir: git_dir_for(&dot_git),
                project_prefix,
            });
        }
        current = directory.parent();
    }
    None
}

/// The operator identity Git would attribute a commit to, when it is readable.
///
/// `None` covers every way the lookup can fail — no repository, no executable,
/// an unset `user.name`, an unreadable value — because the operator is advisory:
/// an unavailable identity degrades the fallback and never blocks a write.
pub fn user_name(probe: &GitProbe) -> Option<String> {
    let (executable, repository) = (probe.executable()?, probe.repository()?);
    let output = Command::new(executable)
        .current_dir(repository.root())
        .args(["config", "--get", "user.name"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!name.is_empty()).then_some(name)
}

/// Whether Git's index tracks `path`.
///
/// `false` covers every way the answer can be unavailable — no repository, no
/// executable, a path outside the repository, or a Git failure — because
/// tracking is a repository fact Waymark reports, never a precondition for an
/// ordinary read or write.
pub fn tracks(probe: &GitProbe, path: &Utf8Path) -> bool {
    let (Some(executable), Some(repository)) = (probe.executable(), probe.repository()) else {
        return false;
    };
    let path = absolute(path);
    let Some(relative) = repository.relative(&path) else {
        return false;
    };
    let output = Command::new(executable)
        .current_dir(repository.root())
        .args(["ls-files", "--error-unmatch", "--"])
        .arg(relative)
        .output();
    matches!(output, Ok(output) if output.status.success())
}

/// Find `git` on `PATH`.
pub fn find_executable() -> Option<Utf8PathBuf> {
    let path = std::env::var_os("PATH")?;
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return Utf8PathBuf::from_path_buf(candidate).ok();
        }
    }
    None
}

/// Which slice of the project `history` reports on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryScope {
    /// One work item's document.
    Item,
    /// One explicit project-relative path filter.
    Path,
    /// The configured storage root.
    StorageRoot,
}

impl HistoryScope {
    /// The `snake_case` literal.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Item => "item",
            Self::Path => "path",
            Self::StorageRoot => "storage_root",
        }
    }
}

/// One commit that touched the history scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    /// The full object name.
    pub id: String,
    /// The abbreviated object name Git rendered.
    pub short_id: String,
    /// The author's name.
    pub author: String,
    /// The author's email address.
    pub email: String,
    /// The committer timestamp, in UTC seconds.
    pub committed_at: Timestamp,
    /// The first line of the commit message.
    pub summary: String,
}

/// What `history` was asked for.
#[derive(Debug, Clone)]
pub struct HistoryRequest {
    /// Restrict history to one work item's file.
    pub item: Option<WorkItemId>,
    /// Restrict history to this project-relative path. Mutually exclusive with
    /// `item`.
    pub path: Option<String>,
    /// Maximum commits; `0` means unlimited.
    pub limit: usize,
    /// An explicit environment; `None` probes the project at call time.
    pub probe: Option<GitProbe>,
}

impl HistoryRequest {
    /// The storage root, the default limit, and the ambient environment.
    pub fn new() -> Self {
        Self {
            item: None,
            path: None,
            limit: DEFAULT_HISTORY_LIMIT,
            probe: None,
        }
    }

    /// Restrict to `id`'s document.
    pub fn for_item(mut self, id: WorkItemId) -> Self {
        self.item = Some(id);
        self
    }

    /// Restrict to a project-relative path.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Report at most `limit` commits; `0` is unlimited.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Use `probe` instead of probing the project.
    pub fn with_probe(mut self, probe: GitProbe) -> Self {
        self.probe = Some(probe);
        self
    }
}

impl Default for HistoryRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// The result of [`history`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryReport {
    /// Which slice was reported on.
    pub scope: HistoryScope,
    /// The item, when the scope is one.
    pub item: Option<WorkItemId>,
    /// The explicit path filter, when the scope is one.
    pub path: Option<String>,
    /// The repository-relative pathspecs that were used.
    pub paths: Vec<String>,
    /// The requested limit (`0` is unlimited).
    pub limit: usize,
    /// The repository the report came from, when there is one.
    pub repository_root: Option<Utf8PathBuf>,
    /// The commits, newest first.
    pub commits: Vec<Commit>,
    /// The `_meta` block this report justifies.
    pub meta: Meta,
}

impl HistoryReport {
    /// The `data` block of the machine envelope.
    pub fn data_json(&self) -> serde_json::Value {
        serde_json::json!({
            "scope": self.scope.as_str(),
            "item": self.item.as_ref().map(|id| id.as_str().to_owned()),
            "path": self.path,
            "paths": self.paths,
            "limit": self.limit,
            "repository_root": self.repository_root.as_ref().map(|root| root.to_string()),
            "commits": self.commits.iter().map(|commit| serde_json::json!({
                "id": commit.id,
                "short_id": commit.short_id,
                "author": commit.author,
                "email": commit.email,
                "committed_at": commit.committed_at.to_string(),
                "summary": commit.summary,
            })).collect::<Vec<_>>(),
        })
    }
}

/// Which slice of the project `diff` reports on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffScope {
    /// One work item's document.
    Item,
    /// The configured storage root.
    StorageRoot,
}

impl DiffScope {
    /// The `snake_case` literal.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Item => "item",
            Self::StorageRoot => "storage_root",
        }
    }
}

/// What a diff is taken against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffBaseline {
    /// One commit, as `git show` renders it.
    Commit,
    /// The working tree, staged and unstaged, against `HEAD`.
    WorkingTree,
}

impl DiffBaseline {
    /// The `snake_case` literal.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::WorkingTree => "working_tree",
        }
    }
}

/// What `diff` was asked for.
#[derive(Debug, Clone)]
pub struct DiffRequest {
    /// Restrict the diff to one work item's file.
    pub item: Option<WorkItemId>,
    /// The commit to inspect. When absent the working tree is diffed.
    pub commit: Option<String>,
    /// An explicit environment; `None` probes the project at call time.
    pub probe: Option<GitProbe>,
}

impl DiffRequest {
    /// The storage root and the working tree, in the ambient environment.
    pub fn new() -> Self {
        Self {
            item: None,
            commit: None,
            probe: None,
        }
    }

    /// Restrict the diff to `id`'s document.
    pub fn for_item(mut self, id: WorkItemId) -> Self {
        self.item = Some(id);
        self
    }

    /// Inspect `commit` instead of the working tree.
    pub fn with_commit(mut self, commit: impl Into<String>) -> Self {
        self.commit = Some(commit.into());
        self
    }

    /// Use `probe` instead of probing the project.
    pub fn with_probe(mut self, probe: GitProbe) -> Self {
        self.probe = Some(probe);
        self
    }
}

impl Default for DiffRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// The result of [`diff`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffReport {
    /// Which slice was reported on.
    pub scope: DiffScope,
    /// The item, when the scope is one.
    pub item: Option<WorkItemId>,
    /// What the diff was taken against.
    pub baseline: DiffBaseline,
    /// The commit that was inspected, when one was.
    pub commit: Option<String>,
    /// The repository-relative pathspecs that were used.
    pub paths: Vec<String>,
    /// The unified patch, empty when nothing changed in scope.
    pub patch: String,
    /// Whether the patch is empty.
    pub empty: bool,
    /// The repository the report came from, when there is one.
    pub repository_root: Option<Utf8PathBuf>,
    /// The `_meta` block this report justifies.
    pub meta: Meta,
}

impl DiffReport {
    /// The `data` block of the machine envelope.
    pub fn data_json(&self) -> serde_json::Value {
        serde_json::json!({
            "scope": self.scope.as_str(),
            "item": self.item.as_ref().map(|id| id.as_str().to_owned()),
            "baseline": self.baseline.as_str(),
            "commit": self.commit,
            "paths": self.paths,
            "empty": self.empty,
            "patch": self.patch,
            "repository_root": self.repository_root.as_ref().map(|root| root.to_string()),
        })
    }
}

/// Commit history for one work item, one path filter, or the storage root.
///
/// Outside a repository, or without a `git` binary, the report is empty and
/// carries the matching informational finding(s) — `NOT_A_GIT_REPOSITORY`,
/// `GIT_UNAVAILABLE`, or both; it is not an error.
pub fn history(project: &Project, request: &HistoryRequest) -> Result<HistoryReport, WaymarkError> {
    let scope = resolve_history_scope(project, request)?;
    let probe = request
        .probe
        .clone()
        .unwrap_or_else(|| GitProbe::detect(project));
    let paths = repo_paths(&probe, &scope.project_paths);
    let repository_root = probe.repository().map(|repo| repo.root().to_owned());

    let mut report = HistoryReport {
        scope: scope.kind,
        item: scope.item.clone(),
        path: scope.path.clone(),
        paths,
        limit: request.limit,
        repository_root,
        commits: Vec::new(),
        meta: empty_meta(),
    };

    if !probe.is_available() {
        report.meta = unavailable_meta(&probe);
        return Ok(report);
    }

    let git = Git::new(&probe, "history")?;
    report.meta = available_meta(scope.kind == HistoryScope::Item);
    if !git.has_head()? {
        // A repository without a first commit is not a failure: it simply has
        // no history to report yet.
        return Ok(report);
    }

    let mut args: Vec<String> = vec![
        "log".to_owned(),
        "--no-color".to_owned(),
        "-z".to_owned(),
        LOG_FORMAT.to_owned(),
    ];
    // `--follow` tracks a renamed file, but Git accepts it only with a single
    // path, so it is used exactly in the item scope.
    if scope.kind == HistoryScope::Item {
        args.push("--follow".to_owned());
    }
    if request.limit > 0 {
        args.push(format!("--max-count={}", request.limit));
    }
    args.push("--".to_owned());
    args.extend(report.paths.iter().cloned());

    let output = git.run(&args, &[])?;
    report.commits = parse_commits(&output.stdout);
    Ok(report)
}

/// The diff of one work item, or of the storage root, for a commit or for the
/// working tree.
///
/// Outside a repository, or without a `git` binary, the patch is empty and the
/// report carries the matching informational finding(s) for each missing cause.
pub fn diff(project: &Project, request: &DiffRequest) -> Result<DiffReport, WaymarkError> {
    let scope = resolve_diff_scope(project, request)?;
    let probe = request
        .probe
        .clone()
        .unwrap_or_else(|| GitProbe::detect(project));
    let paths = repo_paths(&probe, &scope.project_paths);
    let repository_root = probe.repository().map(|repo| repo.root().to_owned());

    let mut report = DiffReport {
        scope: if scope.kind == HistoryScope::Item {
            DiffScope::Item
        } else {
            DiffScope::StorageRoot
        },
        item: scope.item.clone(),
        baseline: if request.commit.is_some() {
            DiffBaseline::Commit
        } else {
            DiffBaseline::WorkingTree
        },
        commit: request.commit.clone(),
        paths,
        patch: String::new(),
        empty: true,
        repository_root,
        meta: empty_meta(),
    };

    if !probe.is_available() {
        report.meta = unavailable_meta(&probe);
        return Ok(report);
    }

    let git = Git::new(&probe, "diff")?;
    report.meta = available_meta(scope.kind == HistoryScope::Item);

    let patch = match &request.commit {
        Some(commit) => {
            let mut args: Vec<String> = vec![
                "show".to_owned(),
                "--no-color".to_owned(),
                "--format=".to_owned(),
                "--patch".to_owned(),
                commit.clone(),
                "--".to_owned(),
            ];
            args.extend(report.paths.iter().cloned());
            let output = git.run(&args, &[])?;
            String::from_utf8_lossy(&output.stdout).into_owned()
        }
        None => git.working_tree_diff(&report.paths, &scope)?,
    };

    report.patch = patch;
    report.empty = report.patch.trim().is_empty();
    Ok(report)
}

/// The resolved scope shared by history and diff.
struct Scope {
    kind: HistoryScope,
    item: Option<WorkItemId>,
    path: Option<String>,
    project_paths: Vec<String>,
}

fn resolve_history_scope(
    project: &Project,
    request: &HistoryRequest,
) -> Result<Scope, WaymarkError> {
    if request.item.is_some() && request.path.is_some() {
        return Err(WaymarkError::validation(
            "git history",
            vec![ValidationIssue::new(
                "path",
                "`item` and `path` are mutually exclusive: name an item or a path, not both",
            )],
        ));
    }
    match (&request.item, &request.path) {
        (Some(id), None) => {
            let relative = item_path(project, id)?;
            Ok(Scope {
                kind: HistoryScope::Item,
                item: Some(id.clone()),
                path: None,
                project_paths: vec![relative],
            })
        }
        (None, Some(raw)) => {
            let relative = normalize_path_filter(raw)?;
            Ok(Scope {
                kind: HistoryScope::Path,
                item: None,
                path: Some(relative.clone()),
                project_paths: vec![relative],
            })
        }
        (None, None) => Ok(Scope {
            kind: HistoryScope::StorageRoot,
            item: None,
            path: None,
            project_paths: vec![project.config().storage_root.trim_matches('/').to_owned()],
        }),
        (Some(_), Some(_)) => unreachable!("guarded above"),
    }
}

fn resolve_diff_scope(project: &Project, request: &DiffRequest) -> Result<Scope, WaymarkError> {
    if let Some(commit) = &request.commit {
        let trimmed = commit.trim();
        if trimmed.is_empty() || trimmed.starts_with('-') || trimmed.contains(char::is_whitespace) {
            return Err(WaymarkError::validation(
                "git diff",
                vec![ValidationIssue::new(
                    "commit",
                    "a commit must be a revision that does not start with `-` and contains no whitespace",
                )],
            ));
        }
    }
    match &request.item {
        Some(id) => Ok(Scope {
            kind: HistoryScope::Item,
            item: Some(id.clone()),
            path: None,
            project_paths: vec![item_path(project, id)?],
        }),
        None => Ok(Scope {
            kind: HistoryScope::StorageRoot,
            item: None,
            path: None,
            project_paths: vec![project.config().storage_root.trim_matches('/').to_owned()],
        }),
    }
}

/// The project-relative path of `id`'s document.
fn item_path(project: &Project, id: &WorkItemId) -> Result<String, WaymarkError> {
    let entry = project.locate(id)?;
    Ok(entry
        .path
        .strip_prefix(project.root())
        .map(|relative| relative.as_str().to_owned())
        .unwrap_or_else(|_| entry.path.as_str().to_owned()))
}

/// Validate a project-relative path filter: no absolute paths, no `..`.
fn normalize_path_filter(raw: &str) -> Result<String, WaymarkError> {
    let mut parts: Vec<&str> = Vec::new();
    for component in Utf8Path::new(raw.trim()).components() {
        match component {
            Utf8Component::CurDir => {}
            Utf8Component::Normal(part) => parts.push(part),
            _ => {
                return Err(WaymarkError::validation(
                    "git history",
                    vec![ValidationIssue::new(
                        "path",
                        format!("{raw:?} must be a relative path inside the project"),
                    )],
                )
                .at_field("path"));
            }
        }
    }
    if parts.is_empty() {
        return Err(WaymarkError::validation(
            "git history",
            vec![ValidationIssue::new("path", "the path filter is empty")],
        )
        .at_field("path"));
    }
    Ok(parts.join("/"))
}

/// Rewrite project-relative paths as repository-relative pathspecs.
fn repo_paths(probe: &GitProbe, project_paths: &[String]) -> Vec<String> {
    let prefix = probe
        .repository()
        .map(|repo| repo.project_prefix().to_owned())
        .unwrap_or_default();
    project_paths
        .iter()
        .map(|path| {
            let path = path.replace('\\', "/");
            if prefix.is_empty() {
                path
            } else if path.is_empty() {
                prefix.to_owned()
            } else {
                format!("{prefix}/{path}")
            }
        })
        .collect()
}

fn empty_meta() -> Meta {
    Meta {
        source: Source::DirectFile,
        completeness: Completeness::Scoped,
        freshness: Freshness::Unknown,
        diagnostics: Vec::new(),
        warnings: Vec::new(),
    }
}

fn available_meta(scoped: bool) -> Meta {
    Meta {
        source: Source::DirectFile,
        completeness: if scoped {
            Completeness::Scoped
        } else {
            Completeness::Complete
        },
        freshness: Freshness::Fresh,
        diagnostics: Vec::new(),
        warnings: Vec::new(),
    }
}

fn unavailable_meta(probe: &GitProbe) -> Meta {
    Meta {
        source: Source::DirectFile,
        completeness: Completeness::Scoped,
        freshness: Freshness::Unknown,
        diagnostics: probe.findings(),
        warnings: Vec::new(),
    }
}

/// Parse `git log -z` output into commits.
fn parse_commits(bytes: &[u8]) -> Vec<Commit> {
    let text = String::from_utf8_lossy(bytes);
    let mut commits = Vec::new();
    for record in text.split(RECORD) {
        let record = record.trim_start_matches('\n');
        if record.is_empty() {
            continue;
        }
        let mut fields = record.split(FIELD);
        let (Some(id), Some(short_id), Some(author), Some(email), Some(seconds), Some(summary)) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            continue;
        };
        let Some(committed_at) = seconds
            .parse::<i64>()
            .ok()
            .and_then(|seconds| DateTime::<Utc>::from_timestamp(seconds, 0))
            .map(Timestamp::new)
        else {
            continue;
        };
        commits.push(Commit {
            id: id.to_owned(),
            short_id: short_id.to_owned(),
            author: author.to_owned(),
            email: email.to_owned(),
            committed_at,
            summary: summary.to_owned(),
        });
    }
    commits
}

/// A `git` invocation bound to one repository and one executable.
struct Git {
    executable: Utf8PathBuf,
    root: Utf8PathBuf,
    operation: &'static str,
}

impl Git {
    fn new(probe: &GitProbe, operation: &'static str) -> Result<Self, WaymarkError> {
        match (probe.executable(), probe.repository()) {
            (Some(executable), Some(repository)) => Ok(Self {
                executable: executable.to_path_buf(),
                root: repository.root().to_path_buf(),
                operation,
            }),
            _ => Err(WaymarkError::git_error(
                operation,
                "no usable Git environment was found",
            )),
        }
    }

    fn has_head(&self) -> Result<bool, WaymarkError> {
        let output = self.run(
            &[
                "rev-parse".to_owned(),
                "--verify".to_owned(),
                "--quiet".to_owned(),
                "HEAD".to_owned(),
            ],
            &[1, 128],
        )?;
        Ok(output.status.success())
    }

    /// Diff the working tree, staged and unstaged, against `HEAD`.
    ///
    /// An untracked item file has no `HEAD` side, so it is diffed against
    /// `/dev/null`; the storage root deliberately reports tracked changes only,
    /// exactly as `git diff HEAD -- <storage_root>` does.
    fn working_tree_diff(&self, paths: &[String], scope: &Scope) -> Result<String, WaymarkError> {
        if scope.kind == HistoryScope::Item {
            let path = &paths[0];
            let tracked = self.run(
                &[
                    "ls-files".to_owned(),
                    "--error-unmatch".to_owned(),
                    "--".to_owned(),
                    path.clone(),
                ],
                &[1],
            )?;
            if !tracked.status.success() {
                if cfg!(windows) {
                    return Ok(String::new());
                }
                let output = self.run(
                    &[
                        "diff".to_owned(),
                        "--no-index".to_owned(),
                        "--no-color".to_owned(),
                        "--".to_owned(),
                        "/dev/null".to_owned(),
                        path.clone(),
                    ],
                    &[1],
                )?;
                return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
            }
        }
        let mut args: Vec<String> = vec![
            "diff".to_owned(),
            "--no-color".to_owned(),
            "HEAD".to_owned(),
            "--".to_owned(),
        ];
        args.extend(paths.iter().cloned());
        let output = self.run(&args, &[])?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Run `git` in the repository root and reject unexpected exit codes.
    fn run(&self, args: &[String], allowed: &[i32]) -> Result<Output, WaymarkError> {
        let output = Command::new(&self.executable)
            .current_dir(&self.root)
            .arg("-c")
            .arg("core.quotepath=false")
            .args(args)
            .output()
            .map_err(|source| {
                WaymarkError::git_error(
                    self.operation,
                    format!("cannot run {}: {source}", self.executable),
                )
            })?;
        let code = output.status.code().unwrap_or(-1);
        if !output.status.success() && !allowed.contains(&code) {
            return Err(WaymarkError::git_error(
                self.operation,
                first_line(&output.stderr),
            ));
        }
        Ok(output)
    }
}

fn first_line(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("the command exited unsuccessfully")
        .to_owned()
}

/// The git directory for a `.git` entry, resolving a linked worktree's
/// `gitdir:` file.
fn git_dir_for(dot_git: &Utf8Path) -> Utf8PathBuf {
    if dot_git.is_dir() {
        return dot_git.to_path_buf();
    }
    let Ok(text) = std::fs::read_to_string(dot_git) else {
        return dot_git.to_path_buf();
    };
    let Some(target) = text.lines().find_map(|line| line.strip_prefix("gitdir:")) else {
        return dot_git.to_path_buf();
    };
    let target = Utf8PathBuf::from(target.trim());
    if target.is_absolute() {
        target
    } else {
        dot_git
            .parent()
            .unwrap_or_else(|| Utf8Path::new("."))
            .join(target)
    }
}

/// Make `path` absolute against the current directory, when it is relative.
fn absolute(path: &Utf8Path) -> Utf8PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    match std::env::current_dir() {
        Ok(cwd) => Utf8PathBuf::from_path_buf(cwd)
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf()),
        Err(_) => path.to_path_buf(),
    }
}
