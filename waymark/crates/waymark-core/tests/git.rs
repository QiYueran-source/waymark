//! Git-backed history and diff, through the public library API.
//!
//! The seam is `waymark_core::git::{history, diff}` against a real temporary
//! repository: `git init`, real commits, real working-tree changes. Nothing here
//! mocks the Git interaction, because "the repository is the history" is exactly
//! what these tests have to pin.
//!
//! Every test that needs the `git` binary skips cleanly when it is absent, and
//! the outside-a-repository cases run either way.

use std::process::Command;
use std::sync::Arc;

use camino::Utf8PathBuf;
use chrono::{DateTime, Utc};
use waymark_core::envelope::{Completeness, Freshness, Source};
use waymark_core::error::Code;
use waymark_core::git::{self, DiffBaseline, DiffRequest, HistoryRequest, HistoryScope};
use waymark_core::ops::create::{self, CreateRequest};
use waymark_core::ops::OpOptions;
use waymark_core::testing::{fixtures, SequentialIds};
use waymark_core::{FixedClock, InitOptions, Project, Severity};

/// The instant the fixtures start at.
fn base() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-03T05:43:10Z")
        .expect("a valid instant")
        .with_timezone(&Utc)
}

fn options() -> OpOptions {
    OpOptions::new().with_clock(Arc::new(FixedClock::new(base())))
}

fn utf8(dir: &tempfile::TempDir) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 temporary path")
}

/// `true` when the environment can run the git-backed cases.
fn git_available() -> bool {
    waymark_core::git::find_executable().is_some()
}

/// Run `git` in `root`, panicking with its stderr on failure.
fn git(root: &camino::Utf8Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "user.name=Waymark Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .expect("the git binary runs");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A project in a fresh Git repository, with one committed work item.
///
/// Returns the temporary directory, the project, the item ID, and the path of
/// the item's document relative to the project root.
fn project_in_repository() -> Option<(tempfile::TempDir, Project, String, Utf8PathBuf)> {
    if !git_available() {
        eprintln!("skipping: no `git` executable on PATH");
        return None;
    }
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = utf8(&dir);
    git(&root, &["init", "--quiet"]);

    let project = Project::init(&root, InitOptions::default()).expect("init succeeds");
    let project = project.with_id_source(Box::new(SequentialIds::starting_at(1)));

    let created = create::create(
        &project,
        CreateRequest::new("Fix the thing", "task", "frank").with_body("first body\n"),
        &options(),
    )
    .expect("the item is created");
    let id = created.item.id.as_str().to_owned();
    let relative = created
        .path
        .strip_prefix(&root)
        .expect("the document is under the project root")
        .to_path_buf();

    git(&root, &["add", "-A"]);
    git(
        &root,
        &["commit", "--quiet", "-m", "add the first work item"],
    );
    Some((dir, project, id, relative))
}

#[test]
fn history_reports_the_commits_that_touched_one_item() {
    let Some((_dir, project, id, relative)) = project_in_repository() else {
        return;
    };

    let report = git::history(
        &project,
        &HistoryRequest::new()
            .for_item(waymark_core::WorkItemId::parse(&id).unwrap())
            .with_limit(10),
    )
    .expect("history is a success, not an error");

    assert_eq!(report.scope, HistoryScope::Item);
    assert_eq!(
        report.item.as_ref().map(|id| id.as_str()),
        Some(id.as_str())
    );
    assert_eq!(
        report.paths,
        vec![relative.as_str().to_owned()],
        "the pathspec is project-relative and the repository root is the project"
    );
    assert_eq!(report.commits.len(), 1);
    assert_eq!(report.commits[0].summary, "add the first work item");
    assert_eq!(report.commits[0].author, "Waymark Test");
    assert_eq!(report.commits[0].email, "test@example.com");
    assert!(!report.commits[0].id.is_empty());
    assert!(report.commits[0].short_id.len() < report.commits[0].id.len());
    assert!(
        report.meta.diagnostics.is_empty(),
        "a real repository has nothing to diagnose: {:?}",
        report.meta.diagnostics
    );
    assert_eq!(
        report.repository_root.as_ref().map(|path| path.as_str()),
        Some(project.root().as_str())
    );
    assert_eq!(report.meta.source, Source::DirectFile);
    assert_eq!(
        report.meta.completeness,
        Completeness::Scoped,
        "an item scope covers one document"
    );
    assert_eq!(report.meta.freshness, Freshness::Fresh);
}

#[test]
fn history_without_an_item_covers_the_whole_storage_root() {
    let Some((_dir, project, _id, _relative)) = project_in_repository() else {
        return;
    };

    let report = git::history(&project, &HistoryRequest::new()).expect("history succeeds");
    assert_eq!(report.scope, HistoryScope::StorageRoot);
    assert_eq!(report.paths, vec!["work".to_owned()]);
    assert_eq!(report.meta.completeness, Completeness::Complete);
    assert!(
        !report.commits.is_empty(),
        "the storage root has at least the commit that added the item"
    );
}

#[test]
fn the_history_limit_is_an_upper_bound() {
    let Some((_dir, project, id, relative)) = project_in_repository() else {
        return;
    };
    git(
        project.root(),
        &[
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "an unrelated commit",
        ],
    );
    // Touch the item again so a second commit names it.
    let document = project.root().join(&relative);
    std::fs::write(&document, "---\nschema: 1\n---\nrewritten\n").unwrap();
    git(project.root(), &["add", "-A"]);
    git(
        project.root(),
        &["commit", "--quiet", "-m", "touch the item"],
    );

    let item = waymark_core::WorkItemId::parse(&id).unwrap();
    let all = git::history(&project, &HistoryRequest::new().for_item(item.clone()))
        .expect("history succeeds");
    assert_eq!(all.commits.len(), 2, "two commits touched the file");

    let one = git::history(
        &project,
        &HistoryRequest::new().for_item(item).with_limit(1),
    )
    .expect("history succeeds");
    assert_eq!(one.commits.len(), 1);
    assert_eq!(one.commits[0].summary, "touch the item");
}

#[test]
fn a_path_filter_narrows_history_and_cannot_escape_the_project() {
    let Some((_dir, project, _id, _relative)) = project_in_repository() else {
        return;
    };

    let report = git::history(
        &project,
        &HistoryRequest::new().with_path("work/task".to_owned()),
    )
    .expect("history succeeds");
    assert_eq!(report.scope, HistoryScope::Path);
    assert_eq!(report.paths, vec!["work/task".to_owned()]);
    assert!(!report.commits.is_empty());

    let escaped = git::history(
        &project,
        &HistoryRequest::new().with_path("../outside".to_owned()),
    )
    .expect_err("an escaping path is invalid input");
    assert_eq!(escaped.code(), Code::ValidationError);
}

#[test]
fn diff_of_the_working_tree_shows_an_edited_item() {
    let Some((_dir, project, id, relative)) = project_in_repository() else {
        return;
    };
    let document = project.root().join(&relative);
    let original = std::fs::read_to_string(&document).unwrap();
    std::fs::write(&document, original.replace("first body", "changed body")).unwrap();

    let report = git::diff(
        &project,
        &DiffRequest::new().for_item(waymark_core::WorkItemId::parse(&id).unwrap()),
    )
    .expect("diff is a success, not an error");

    assert_eq!(report.baseline, DiffBaseline::WorkingTree);
    assert!(!report.empty);
    assert!(
        report.patch.contains("+changed body") && report.patch.contains("-first body"),
        "the patch names both sides:\n{}",
        report.patch
    );
    assert!(report.meta.diagnostics.is_empty());
    assert_eq!(report.meta.freshness, Freshness::Fresh);
    assert_eq!(report.meta.completeness, Completeness::Scoped);
    assert_eq!(
        report.meta.warnings,
        Vec::new(),
        "a diff never promotes a note to a warning"
    );
}

#[test]
fn diff_of_a_commit_shows_what_that_commit_changed_for_the_item() {
    let Some((_dir, project, id, relative)) = project_in_repository() else {
        return;
    };
    let item = waymark_core::WorkItemId::parse(&id).unwrap();
    let history = git::history(&project, &HistoryRequest::new().for_item(item.clone()))
        .expect("history succeeds");
    let commit = history.commits[0].id.clone();

    let report = git::diff(
        &project,
        &DiffRequest::new()
            .for_item(item)
            .with_commit(commit.clone()),
    )
    .expect("diff succeeds");

    assert_eq!(report.baseline, DiffBaseline::Commit);
    assert_eq!(report.commit.as_deref(), Some(commit.as_str()));
    assert!(!report.empty, "the commit created the item");
    assert!(
        report.patch.contains("work/task") && report.patch.contains("Fix the thing"),
        "the patch is scoped to the item file:\n{}",
        report.patch
    );
    assert_eq!(report.paths, vec![relative.as_str().to_owned()]);
}

#[test]
fn a_brand_new_untracked_item_still_has_a_working_tree_diff() {
    let Some((_dir, project, _id, _relative)) = project_in_repository() else {
        return;
    };
    let created = create::create(
        &project,
        CreateRequest::new("A second thing", "task", "frank"),
        &options(),
    )
    .expect("the item is created");

    let report =
        git::diff(&project, &DiffRequest::new().for_item(created.item.id)).expect("diff succeeds");
    assert!(!report.empty, "an untracked document is a whole new file");
    assert!(
        report.patch.contains("A second thing"),
        "the patch shows the new document:\n{}",
        report.patch
    );
}

#[test]
fn a_project_outside_a_repository_reports_only_the_repository_finding() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = utf8(&dir);
    let project = Project::init(&root, InitOptions::default()).expect("init succeeds");
    // Isolate the cause: an executable is present, the repository is not.
    let probe = git::GitProbe::from_parts(None, Some(Utf8PathBuf::from("/nonexistent/git")));

    let report = git::history(&project, &HistoryRequest::new().with_probe(probe.clone()))
        .expect("history succeeds");
    assert!(report.commits.is_empty());
    assert!(
        report.repository_root.is_none(),
        "there is no repository to name"
    );
    assert_eq!(report.meta.diagnostics.len(), 1);
    let finding = &report.meta.diagnostics[0];
    assert_eq!(finding.code, Code::NotAGitRepository);
    assert_eq!(finding.severity, Severity::Info);
    assert!(!finding.fixable);
    assert!(
        finding.location.is_none(),
        "the registry declares no location kind for NOT_A_GIT_REPOSITORY"
    );
    assert!(
        finding.message.contains("repository"),
        "the message names the missing repository: {}",
        finding.message
    );

    let report = git::diff(&project, &DiffRequest::new().with_probe(probe)).expect("diff succeeds");
    assert!(report.patch.is_empty());
    assert!(report.empty);
    assert_eq!(report.meta.diagnostics.len(), 1);
    assert_eq!(report.meta.diagnostics[0].code, Code::NotAGitRepository);
    assert_eq!(report.meta.diagnostics[0].severity, Severity::Info);
    assert!(report.meta.diagnostics[0].location.is_none());
    assert_eq!(
        report.meta.freshness,
        Freshness::Unknown,
        "without Git there is no history to be fresh about"
    );
    assert!(report.meta.warnings.is_empty());
}

#[test]
fn a_missing_git_binary_is_git_unavailable_even_inside_a_repository() {
    let Some((_dir, project, _id, _relative)) = project_in_repository() else {
        return;
    };
    let probe = git::GitProbe::from_parts(
        git::find_repository(&project),
        None, // no usable executable
    );
    assert!(!probe.is_available());

    let report = git::history(&project, &HistoryRequest::new().with_probe(probe.clone()))
        .expect("history succeeds");
    assert!(report.commits.is_empty());
    assert_eq!(report.meta.diagnostics.len(), 1);
    let finding = &report.meta.diagnostics[0];
    assert_eq!(finding.code, Code::GitUnavailable);
    assert_eq!(finding.severity, Severity::Info);
    assert!(!finding.fixable);
    assert!(
        finding.location.is_none(),
        "a missing binary is not a document location"
    );
    assert!(
        finding.message.contains("PATH"),
        "the message names the missing executable: {}",
        finding.message
    );

    let report = git::diff(&project, &DiffRequest::new().with_probe(probe)).expect("diff succeeds");
    assert!(report.empty);
    assert_eq!(report.meta.diagnostics.len(), 1);
    assert_eq!(report.meta.diagnostics[0].code, Code::GitUnavailable);
    assert_eq!(report.meta.diagnostics[0].severity, Severity::Info);
}

#[test]
fn a_missing_repository_and_executable_report_both_findings() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = utf8(&dir);
    let project = Project::init(&root, InitOptions::default()).expect("init succeeds");
    let probe = git::GitProbe::unavailable();
    assert!(probe.repository().is_none());
    assert!(probe.executable().is_none());

    let report = git::history(&project, &HistoryRequest::new().with_probe(probe.clone()))
        .expect("history succeeds with an empty payload");
    assert!(report.commits.is_empty());
    assert_eq!(
        report
            .meta
            .diagnostics
            .iter()
            .map(|finding| finding.code)
            .collect::<Vec<_>>(),
        vec![Code::NotAGitRepository, Code::GitUnavailable],
        "one finding per missing cause, each naming its own"
    );
    assert!(report
        .meta
        .diagnostics
        .iter()
        .any(|finding| finding.message.contains("repository")));
    assert!(report
        .meta
        .diagnostics
        .iter()
        .any(|finding| finding.message.contains("PATH")));
    for finding in &report.meta.diagnostics {
        assert_eq!(finding.severity, Severity::Info);
        assert!(!finding.fixable);
        assert!(finding.location.is_none());
    }

    let report = git::diff(&project, &DiffRequest::new().with_probe(probe))
        .expect("diff succeeds with an empty payload");
    assert!(report.empty);
    assert_eq!(
        report
            .meta
            .diagnostics
            .iter()
            .map(|finding| finding.code)
            .collect::<Vec<_>>(),
        vec![Code::NotAGitRepository, Code::GitUnavailable]
    );
}

#[test]
fn probing_finds_a_repository_above_the_project_and_resolves_a_worktree_gitdir_file() {
    let Some((dir, project, _id, _relative)) = project_in_repository() else {
        return;
    };
    let found = git::find_repository(&project).expect("the project is in a repository");
    assert_eq!(found.root(), project.root());
    assert_eq!(found.project_prefix(), "");
    assert_eq!(found.git_dir(), project.root().join(".git"));

    // A linked worktree has a `.git` *file* containing `gitdir: <path>`, not a
    // directory. Reproduce that shape over a directory project with an absolute
    // path, so the fixture means the same thing on every platform.
    let other = tempfile::tempdir().expect("a second temporary directory");
    let other_root = utf8(&other);
    let gitdir = other_root.join("waymark-linked-worktree");
    std::fs::write(other_root.join(".git"), format!("gitdir: {gitdir}\n")).unwrap();
    let linked = Project::init(&other_root, InitOptions::default()).expect("init succeeds");
    let found = git::find_repository(&linked).expect("a gitdir file still marks a repository");
    assert_eq!(found.git_dir(), gitdir);
    drop(dir);
}

#[test]
fn reading_history_and_diff_writes_nothing_under_the_metadata_directory() {
    let Some((_dir, project, id, relative)) = project_in_repository() else {
        return;
    };
    // Make a working-tree change so the diff has something to say.
    let document = project.root().join(&relative);
    let original = std::fs::read_to_string(&document).unwrap();
    std::fs::write(&document, original.replace("first body", "changed body")).unwrap();

    let metadata = project.metadata_dir();
    let before = snapshot(&metadata);

    let item = waymark_core::WorkItemId::parse(&id).unwrap();
    git::history(&project, &HistoryRequest::new().for_item(item.clone())).expect("history");
    git::history(&project, &HistoryRequest::new()).expect("history of the storage root");
    git::diff(&project, &DiffRequest::new().for_item(item.clone())).expect("diff");
    git::diff(&project, &DiffRequest::new()).expect("diff of the storage root");

    let after = snapshot(&metadata);
    assert_eq!(
        before, after,
        "Waymark keeps no audit log, attribution, or history store: the repository is the history"
    );
    // The only things allowed to exist there are the configuration, the
    // metadata ignore file, and the write lock that `create` left behind. No new
    // file of any kind was introduced by the read-only Git commands.
    assert!(
        after
            .iter()
            .all(|name| name == "config.toml" || name == ".gitignore" || name == "write.lock"),
        "unexpected files under .waymark/: {after:?}"
    );
}

/// The file names directly under `directory`, sorted.
fn snapshot(directory: &camino::Utf8Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .expect("the metadata directory is readable")
        .map(|entry| {
            entry
                .expect("a readable entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn an_unknown_item_is_a_failure_not_an_informational_result() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = utf8(&dir);
    let project = Project::init(&root, InitOptions::default()).expect("init succeeds");
    let missing = waymark_core::WorkItemId::parse("wm-0000000000zz").unwrap();

    let error = git::history(&project, &HistoryRequest::new().for_item(missing.clone()))
        .expect_err("an unknown item is a failure");
    assert_eq!(error.code(), Code::ItemNotFound);

    let error = git::diff(&project, &DiffRequest::new().for_item(missing))
        .expect_err("an unknown item is a failure");
    assert_eq!(error.code(), Code::ItemNotFound);
}

#[test]
fn the_repository_is_discovered_even_when_the_project_is_not_its_root() {
    if !git_available() {
        eprintln!("skipping: no `git` executable on PATH");
        return;
    }
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = utf8(&dir);
    git(&root, &["init", "--quiet"]);
    let nested = root.join("nested").join("project");
    std::fs::create_dir_all(&nested).unwrap();
    let project = Project::init(&nested, InitOptions::default()).expect("init succeeds");
    create::create(
        &project,
        CreateRequest::new("Nested item", "task", "frank"),
        &options(),
    )
    .expect("the item is created");
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "--quiet", "-m", "nested project"]);

    let report = git::history(&project, &HistoryRequest::new()).expect("history succeeds");
    assert_eq!(
        report.paths,
        vec!["nested/project/work".to_owned()],
        "the pathspec is rewritten against the repository root"
    );
    assert_eq!(report.commits.len(), 1);
    assert_eq!(report.commits[0].summary, "nested project");
}

#[test]
fn an_empty_repository_is_a_success_with_no_commits() {
    if !git_available() {
        eprintln!("skipping: no `git` executable on PATH");
        return;
    }
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = utf8(&dir);
    git(&root, &["init", "--quiet"]);
    let project = Project::init(&root, InitOptions::default()).expect("init succeeds");

    let report = git::history(&project, &HistoryRequest::new()).expect("history succeeds");
    assert!(report.commits.is_empty());
    assert!(report.meta.diagnostics.is_empty());
}

#[test]
fn fixtures_still_build_a_project_for_the_non_git_cases() {
    // Keeps the shared fixture seam honest without depending on Git.
    let (_dir, project) = fixtures::project();
    assert!(project.root().exists());
}
