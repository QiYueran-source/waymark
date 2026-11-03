//! `waymark git history` and `waymark git diff`: change inspection over the
//! repository.
//!
//! Both arms are thin: `waymark_core::git` owns detection, scoping, and the Git
//! invocation, and decides that a missing repository or a missing binary is an
//! informational finding rather than a failure. The adapter only translates
//! flags and renders the report.
//!
//! # Where the output goes
//!
//! In JSON mode the envelope is the only thing on stdout. In human mode the
//! report *is* the answer — commit lines or a unified patch — so it goes to
//! stdout, exactly like `show --raw` prints the document; findings, prose, and
//! progress notes go to stderr. The patch is therefore pipeable:
//! `waymark git diff --item ID | git apply`.

use clap::{Args, Subcommand};
use waymark_core::envelope::success_envelope;
use waymark_core::error::WaymarkError;
use waymark_core::git;
use waymark_core::ops::read::normalize;
use waymark_core::Project;

use super::{emit, err_line, open_project};

/// Arguments of `waymark git`.
#[derive(Debug, Args)]
pub struct GitArgs {
    #[command(subcommand)]
    pub command: GitCommand,
}

/// The `git` subcommands.
#[derive(Debug, Subcommand)]
pub enum GitCommand {
    /// Commit history for one work item, one path, or the storage root.
    History(HistoryArgs),
    /// The diff of one work item, or of the storage root.
    Diff(DiffArgs),
}

/// Arguments of `waymark git history`.
#[derive(Debug, Args)]
pub struct HistoryArgs {
    /// Restrict history to one work item's document.
    #[arg(long, value_name = "ID", conflicts_with = "path")]
    pub item: Option<String>,

    /// Report at most this many commits; `0` means unlimited.
    #[arg(long, value_name = "N", default_value_t = git::DEFAULT_HISTORY_LIMIT)]
    pub limit: usize,

    /// Restrict history to a project-relative path instead of the storage root.
    #[arg(long, value_name = "P")]
    pub path: Option<String>,
}

/// Arguments of `waymark git diff`.
#[derive(Debug, Args)]
pub struct DiffArgs {
    /// Restrict the diff to one work item's document.
    #[arg(long, value_name = "ID")]
    pub item: Option<String>,

    /// Inspect this commit instead of the working tree.
    #[arg(long, value_name = "C")]
    pub commit: Option<String>,
}

/// Run `waymark git …`.
pub fn run(
    args: GitArgs,
    project: Option<&camino::Utf8Path>,
    json: bool,
) -> Result<(), WaymarkError> {
    let project = open_project(project)?;
    match args.command {
        GitCommand::History(args) => run_history(&project, &args, json),
        GitCommand::Diff(args) => run_diff(&project, &args, json),
    }
}

fn run_history(project: &Project, args: &HistoryArgs, json: bool) -> Result<(), WaymarkError> {
    let mut request = git::HistoryRequest::new().with_limit(args.limit);
    if let Some(item) = &args.item {
        request = request.for_item(normalize(item)?);
    }
    if let Some(path) = &args.path {
        request = request.with_path(path.clone());
    }

    let report = git::history(project, &request)?;
    if json {
        emit(&success_envelope(report.data_json(), report.meta.clone()))?;
        return Ok(());
    }

    if report.commits.is_empty() {
        for finding in &report.meta.diagnostics {
            err_line(&format!("{}: {}", finding.code, finding.message));
        }
        err_line("no commits");
        return Ok(());
    }
    let scope = match (&report.item, &report.path) {
        (Some(item), _) => item.to_string(),
        (None, Some(path)) => path.clone(),
        (None, None) => report
            .paths
            .first()
            .cloned()
            .unwrap_or_else(|| "the storage root".to_owned()),
    };
    println!("# {scope}");
    for commit in &report.commits {
        println!(
            "{}  {}  {:<16} {}",
            commit.short_id, commit.committed_at, commit.author, commit.summary
        );
    }
    for finding in &report.meta.diagnostics {
        err_line(&format!("{}: {}", finding.code, finding.message));
    }
    Ok(())
}

fn run_diff(project: &Project, args: &DiffArgs, json: bool) -> Result<(), WaymarkError> {
    let mut request = git::DiffRequest::new();
    if let Some(item) = &args.item {
        request = request.for_item(normalize(item)?);
    }
    if let Some(commit) = &args.commit {
        request = request.with_commit(commit.clone());
    }

    let report = git::diff(project, &request)?;
    if json {
        emit(&success_envelope(report.data_json(), report.meta.clone()))?;
        return Ok(());
    }

    if report.empty {
        for finding in &report.meta.diagnostics {
            err_line(&format!("{}: {}", finding.code, finding.message));
        }
        err_line("no changes");
        return Ok(());
    }
    print!("{}", report.patch);
    for finding in &report.meta.diagnostics {
        err_line(&format!("{}: {}", finding.code, finding.message));
    }
    Ok(())
}
