//! Storage migration, driven through the public seam.
//!
//! Every case starts from a real temporary project and asserts what is on disk
//! afterwards. The seam is
//! `waymark_core::ops::migrate::{plan_storage, migrate_storage, execute}`; the
//! prevalidation is observed through the registered error codes and details.

use std::path::PathBuf;
use std::time::SystemTime;

use camino::{Utf8Path, Utf8PathBuf};
use serde_json::json;
use waymark_core::error::Code;
use waymark_core::model::CategoryKey;
use waymark_core::ops::create::{self, CreateRequest};
use waymark_core::ops::migrate::{self, StorageAction};
use waymark_core::ops::{archive, config, OpOptions};
use waymark_core::testing::fixtures;
use waymark_core::{InitOptions, Project, WorkItemId};

fn options() -> OpOptions {
    OpOptions::new().with_clock(fixtures::shared_clock())
}

fn dry_run() -> OpOptions {
    options().dry_run()
}

fn project() -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 path");
    let project = Project::init(&root, InitOptions::default())
        .expect("the project initialises")
        .with_clock(fixtures::shared_clock())
        .with_id_source(Box::new(waymark_core::testing::SequentialIds::new()));
    (dir, project)
}

fn create_one(project: &Project, title: &str) -> (WorkItemId, Utf8PathBuf) {
    let created = create::create(
        project,
        CreateRequest::new(title, "task", "frank"),
        &options(),
    )
    .expect("creation succeeds");
    (created.item.id, created.path)
}

fn category_dir(project: &Project, key: &str) -> Utf8PathBuf {
    project.root().join(
        project.config().categories[&CategoryKey::parse(key).expect("a key")]
            .path
            .clone(),
    )
}

type SnapshotEntry = (PathBuf, Option<SystemTime>, Option<Vec<u8>>);

/// Every entry under `root`, with its bytes and modification time.
fn snapshot(root: &Utf8Path) -> Vec<SnapshotEntry> {
    let mut entries: Vec<SnapshotEntry> = Vec::new();
    collect(root.as_std_path(), &mut entries);
    entries.sort();
    entries
}

fn collect(directory: &std::path::Path, entries: &mut Vec<SnapshotEntry>) {
    let Ok(read) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        let modified = std::fs::metadata(&path)
            .ok()
            .and_then(|metadata| metadata.modified().ok());
        if path.is_dir() {
            entries.push((path.clone(), modified, None));
            collect(&path, entries);
        } else {
            entries.push((path.clone(), modified, std::fs::read(&path).ok()));
        }
    }
}

fn diagnostics(error: &waymark_core::error::WaymarkError) -> Vec<serde_json::Value> {
    error
        .details()
        .get("diagnostics")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn item_at(path: &Utf8Path) -> waymark_core::model::WorkItem {
    waymark_core::parse::parse_item_file(path).expect("the document parses")
}

// -- dry run ----------------------------------------------------------------

#[test]
fn a_storage_dry_run_performs_every_check_and_writes_nothing() {
    let (_dir, project) = project();
    create_one(&project, "First");
    create_one(&project, "Second");

    let before = snapshot(project.root());
    let report = migrate::migrate_storage(
        &project,
        &StorageAction::Root {
            path: "items".to_owned(),
        },
        &dry_run(),
    )
    .expect("the dry run succeeds");
    assert_eq!(snapshot(project.root()), before, "nothing was written");

    assert!(report.dry_run);
    assert!(!report.written);
    assert_eq!(report.previous_revision, 0);
    assert_eq!(report.revision, 1);
    assert!(report.config_changed);
    assert_eq!(report.moves.len(), 1);
    assert_eq!(report.moves[0].outcome, "skipped");
    assert!(report.moves[0].to.ends_with("items/task"));
    assert!(report
        .targets
        .iter()
        .all(|target| target.outcome == "skipped"));

    // The configuration still names the old root and the old directory exists.
    let config = std::fs::read_to_string(project.config_path()).expect("the configuration");
    assert!(config.contains("storage_root = \"work\""), "{config}");
    assert!(category_dir(&project, "task").is_dir());
}

#[test]
fn a_category_key_dry_run_reports_the_rewritten_values_without_writing() {
    let (_dir, project) = project();
    create_one(&project, "First");

    let before = snapshot(project.root());
    let report = migrate::migrate_storage(
        &project,
        &StorageAction::CategoryKey {
            from: CategoryKey::parse("task").expect("a key"),
            to: CategoryKey::parse("chore").expect("a key"),
        },
        &dry_run(),
    )
    .expect("the dry run succeeds");
    assert_eq!(snapshot(project.root()), before, "nothing was written");
    assert!(!report.written);
    assert_eq!(report.count, 1);
    assert_eq!(report.rewritten.len(), 1);
    assert_eq!(report.rewritten[0].from.as_str(), "task");
    assert_eq!(report.rewritten[0].to.as_str(), "chore");
    assert!(report.rewritten[0].path.ends_with(".md"));
}

// -- pure relocation --------------------------------------------------------

#[test]
fn a_pure_relocation_preserves_content_bytes_fingerprints_and_updated_at() {
    let (_dir, project) = project();
    let (_, first) = create_one(&project, "First");
    let (_, second) = create_one(&project, "Second");
    let bytes_before = std::fs::read(&first).expect("the document");
    let fingerprint_before = project.fingerprint(&first).expect("a fingerprint");
    let updated_before = item_at(&first).updated_at;

    let report = migrate::migrate_storage(
        &project,
        &StorageAction::Root {
            path: "items".to_owned(),
        },
        &options(),
    )
    .expect("the storage migration runs");
    assert!(report.written);
    assert_eq!(report.previous_revision, 0);
    assert_eq!(report.revision, 1);
    assert_eq!(report.moves.len(), 1);
    assert_eq!(report.moves[0].outcome, "applied");

    let old_dir = project.root().join("work/task");
    assert!(!old_dir.exists(), "the old directory is gone");
    let new_first = project
        .root()
        .join("items/task")
        .join(first.file_name().expect("a file name"));
    let new_second = project
        .root()
        .join("items/task")
        .join(second.file_name().expect("a file name"));
    assert_eq!(
        std::fs::read(&new_first).expect("the moved document"),
        bytes_before,
        "the bytes are byte-identical at the new location"
    );
    assert_eq!(
        project.fingerprint(&new_first).expect("a fingerprint"),
        fingerprint_before,
        "the fingerprint is untouched"
    );
    assert_eq!(item_at(&new_first).updated_at, updated_before);
    assert!(new_second.exists());

    let config = std::fs::read_to_string(project.config_path()).expect("the configuration");
    assert!(config.contains("storage_root = \"items\""), "{config}");
    assert!(config.contains("path = \"items/task\""), "{config}");
    assert!(config.contains("version = \"1.1\""), "{config}");
}

// -- category key -----------------------------------------------------------

#[test]
fn a_category_key_change_rewrites_every_persisted_value_including_archived_items() {
    let (_dir, project) = project();
    let (active, active_path) = create_one(&project, "Active");
    let (archived, archived_path) = create_one(&project, "Archived");
    archive::archive(&project, archived.as_str(), Some("done"), &options())
        .expect("the item is archived");
    assert!(item_at(&archived_path).is_archived());

    let report = migrate::migrate_storage(
        &project,
        &StorageAction::CategoryKey {
            from: CategoryKey::parse("task").expect("a key"),
            to: CategoryKey::parse("chore").expect("a key"),
        },
        &options(),
    )
    .expect("the key migration runs");
    assert!(report.written);
    assert_eq!(report.count, 2);
    assert_eq!(report.rewritten.len(), 2);
    assert!(report
        .rewritten
        .iter()
        .all(|rewritten| rewritten.from.as_str() == "task" && rewritten.to.as_str() == "chore"));

    let old_dir = project.root().join("work/task");
    assert!(!old_dir.exists());
    let new_dir = project.root().join("work/chore");
    assert!(new_dir.is_dir(), "the directory follows the new key");
    let new_active = new_dir.join(active_path.file_name().expect("a file name"));
    let new_archived = new_dir.join(archived_path.file_name().expect("a file name"));
    assert_eq!(item_at(&new_active).category.as_str(), "chore");
    assert_eq!(item_at(&new_archived).category.as_str(), "chore");
    assert_eq!(item_at(&new_archived).id, archived);
    assert!(
        item_at(&new_archived).is_archived(),
        "the archive marker is preserved"
    );
    assert_eq!(item_at(&new_active).id, active);

    let config = std::fs::read_to_string(project.config_path()).expect("the configuration");
    assert!(config.contains("[categories.chore]"), "{config}");
    assert!(!config.contains("[categories.task]"), "{config}");
    assert!(config.contains("path = \"work/chore\""), "{config}");
}

// -- prevalidation ----------------------------------------------------------

#[test]
fn absolute_and_escaping_targets_are_precondition_failures() {
    let (_dir, project) = project();

    let absolute = migrate::plan_storage(
        &project,
        &StorageAction::Root {
            path: "/tmp/anywhere".to_owned(),
        },
    )
    .expect_err("an absolute path is refused");
    assert_eq!(absolute.code(), Code::MigrationPreconditionFailed);
    assert!(
        diagnostics(&absolute)
            .iter()
            .any(|diagnostic| diagnostic["kind"] == json!("absolute_path")),
        "{:?}",
        absolute.details()
    );

    let escaping = migrate::plan_storage(
        &project,
        &StorageAction::Root {
            path: "../outside".to_owned(),
        },
    )
    .expect_err("an escaping path is refused");
    assert_eq!(escaping.code(), Code::MigrationPreconditionFailed);
    assert!(
        diagnostics(&escaping)
            .iter()
            .any(|diagnostic| diagnostic["kind"] == json!("escaping_path")),
        "{:?}",
        escaping.details()
    );
}

#[test]
fn overlapping_existing_and_case_equivalent_targets_are_path_conflicts() {
    let (_dir, project) = project();
    config::add_category(
        &project,
        &CategoryKey::parse("bug").expect("a key"),
        "work/bug",
        None,
        &options(),
    )
    .expect("the second category is added");

    let overlap = migrate::plan_storage(
        &project,
        &StorageAction::CategoryPath {
            key: CategoryKey::parse("task").expect("a key"),
            path: "work/bug".to_owned(),
        },
    )
    .expect_err("an existing category path is refused");
    assert_eq!(overlap.code(), Code::MigrationPathConflict);
    assert!(
        overlap.details()["conflict"]
            .as_str()
            .expect("a conflict")
            .contains("existing category path"),
        "{:?}",
        overlap.details()
    );

    let descendant = migrate::plan_storage(
        &project,
        &StorageAction::CategoryPath {
            key: CategoryKey::parse("task").expect("a key"),
            path: "work/task/nested".to_owned(),
        },
    )
    .expect_err("a target inside its own source is refused");
    assert_eq!(descendant.code(), Code::MigrationPathConflict);

    let case = migrate::plan_storage(
        &project,
        &StorageAction::CategoryPath {
            key: CategoryKey::parse("task").expect("a key"),
            path: "Work/task".to_owned(),
        },
    )
    .expect_err("a case-equivalent target is refused");
    assert_eq!(case.code(), Code::MigrationPathConflict);
    assert!(
        case.details()["conflict"]
            .as_str()
            .expect("a conflict")
            .to_ascii_lowercase()
            .contains("case-equivalent"),
        "{:?}",
        case.details()
    );

    // A non-empty target is never overwritten.
    std::fs::create_dir_all(project.root().join("occupied/task")).expect("the target directory");
    std::fs::write(project.root().join("occupied/task/keep.md"), b"data").expect("a file");
    let occupied = migrate::plan_storage(
        &project,
        &StorageAction::Root {
            path: "occupied".to_owned(),
        },
    )
    .expect_err("an existing non-empty target is refused");
    assert_eq!(occupied.code(), Code::MigrationPathConflict);
    assert!(
        occupied.details()["conflict"]
            .as_str()
            .expect("a conflict")
            .contains("non-empty"),
        "{:?}",
        occupied.details()
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_target_component_is_a_precondition_failure() {
    let (_dir, project) = project();
    std::fs::create_dir_all(project.root().join("real")).expect("the real directory");
    std::os::unix::fs::symlink(project.root().join("real"), project.root().join("link"))
        .expect("the symbolic link");

    let error = migrate::plan_storage(
        &project,
        &StorageAction::Root {
            path: "link/items".to_owned(),
        },
    )
    .expect_err("a symlinked target is refused");
    assert_eq!(error.code(), Code::MigrationPreconditionFailed);
    assert!(
        diagnostics(&error)
            .iter()
            .any(|diagnostic| diagnostic["kind"] == json!("symlink_in_path")),
        "{:?}",
        error.details()
    );
}

#[test]
fn a_missing_source_reports_every_diagnostic_before_writing() {
    let (_dir, project) = project();
    config::add_category(
        &project,
        &CategoryKey::parse("bug").expect("a key"),
        "work/bug",
        None,
        &options(),
    )
    .expect("the second category is added");
    std::fs::remove_dir_all(project.root().join("work/task")).expect("the source is removed");
    std::fs::remove_dir_all(project.root().join("work/bug")).expect("the source is removed");
    let before = snapshot(project.root());

    let error = migrate::plan_storage(
        &project,
        &StorageAction::Root {
            path: "items".to_owned(),
        },
    )
    .expect_err("missing source directories are refused");
    assert_eq!(error.code(), Code::MigrationPreconditionFailed);
    let diagnostics = diagnostics(&error);
    let missing: Vec<&serde_json::Value> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic["kind"] == json!("source_missing"))
        .collect();
    assert_eq!(missing.len(), 2, "every missing source is reported");
    assert_eq!(snapshot(project.root()), before, "nothing was written");
}

#[test]
fn an_incomplete_projection_blocks_a_storage_migration_before_any_write() {
    let (_dir, project) = project();
    create_one(&project, "First");
    let broken = project.root().join("work/task/wm-ffffffffffff.md");
    std::fs::write(&broken, "not a work item").expect("the broken file");
    let before = snapshot(project.root());

    let error = migrate::migrate_storage(
        &project,
        &StorageAction::Root {
            path: "items".to_owned(),
        },
        &options(),
    )
    .expect_err("an incomplete projection is refused");
    assert_eq!(error.code(), Code::MigrationPreconditionFailed);
    assert!(
        diagnostics(&error)
            .iter()
            .any(|diagnostic| diagnostic["kind"] == json!("incomplete_projection")),
        "{:?}",
        error.details()
    );
    assert_eq!(snapshot(project.root()), before, "nothing was written");
}

#[cfg(unix)]
#[test]
fn an_unreadable_source_file_is_a_precondition_failure() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, project) = project();
    create_one(&project, "First");
    let unreadable = project.root().join("work/task/notes.txt");
    std::fs::write(&unreadable, b"notes").expect("the extra file");
    let original = std::fs::metadata(&unreadable)
        .expect("the file exists")
        .permissions();
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000))
        .expect("the file is made unreadable");
    let result = migrate::plan_storage(
        &project,
        &StorageAction::Root {
            path: "items".to_owned(),
        },
    );
    std::fs::set_permissions(&unreadable, original).expect("the permissions are restored");

    let error = result.expect_err("an unreadable source file is refused");
    assert_eq!(error.code(), Code::MigrationPreconditionFailed);
    assert!(
        diagnostics(&error)
            .iter()
            .any(|diagnostic| diagnostic["kind"] == json!("source_unreadable")),
        "{:?}",
        error.details()
    );
}

// -- stale preview ----------------------------------------------------------

#[test]
fn a_stale_storage_plan_is_rechecked_and_never_applied() {
    let (_dir, project) = project();
    let (_, path) = create_one(&project, "First");
    let plan = migrate::plan_storage(
        &project,
        &StorageAction::Root {
            path: "items".to_owned(),
        },
    )
    .expect("the plan is built");

    let text = std::fs::read_to_string(&path).expect("the document");
    std::fs::write(&path, text.replacen("First", "Edited by hand", 1)).expect("the hand edit");

    let error =
        migrate::execute(&project, &plan, &options()).expect_err("the stale plan is refused");
    assert_eq!(error.code(), Code::MigrationPreconditionFailed);
    assert!(
        diagnostics(&error)
            .iter()
            .any(|diagnostic| diagnostic["kind"] == json!("fingerprint_disagreement")),
        "{:?}",
        error.details()
    );
    assert!(!project.root().join("items").exists(), "nothing moved");
    assert_eq!(item_at(&path).title, "Edited by hand");

    // The command surface rebuilds the plan, so it re-derives and succeeds with
    // the edited bytes rather than applying the stale plan.
    let report = migrate::migrate_storage(
        &project,
        &StorageAction::Root {
            path: "items".to_owned(),
        },
        &options(),
    )
    .expect("the real run re-derives");
    assert!(report.written);
    let moved = project
        .root()
        .join("items/task")
        .join(path.file_name().expect("a file name"));
    assert_eq!(item_at(&moved).title, "Edited by hand");
}

// -- execution failure and rollback ----------------------------------------

#[cfg(unix)]
#[test]
fn an_execution_failure_reports_per_target_outcomes_and_restores_the_move() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, project) = project();
    let (_, path) = create_one(&project, "First");
    let plan = migrate::plan_storage(
        &project,
        &StorageAction::Root {
            path: "items".to_owned(),
        },
    )
    .expect("the plan is built");

    // Staging the configuration needs a temporary file beside it. If this probe
    // can still write, the test cannot simulate a failure (for example as root).
    let metadata_dir = project.metadata_dir();
    let probe = metadata_dir.join("probe");
    if std::fs::write(&probe, b"probe").is_err() {
        return;
    }
    std::fs::remove_file(&probe).expect("the probe is removed");

    let original = std::fs::metadata(&metadata_dir)
        .expect("the directory exists")
        .permissions();
    std::fs::set_permissions(&metadata_dir, std::fs::Permissions::from_mode(0o555))
        .expect("the directory is made read-only");
    let result = migrate::execute(&project, &plan, &options());
    std::fs::set_permissions(&metadata_dir, original).expect("the permissions are restored");

    let error = result.expect_err("the configuration cannot be staged");
    assert_eq!(error.code(), Code::MigrationFailed);
    let per_target = error.details()["per_target"]
        .as_array()
        .expect("per-target outcomes");
    assert!(!per_target.is_empty());
    assert!(
        per_target
            .iter()
            .any(|outcome| outcome["outcome"] == json!("failed")),
        "{per_target:?}"
    );
    assert!(
        per_target.iter().any(|outcome| outcome["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("config.toml"))),
        "{per_target:?}"
    );

    // The applied rename was restored: the old state is intact.
    assert!(path.exists(), "the document is back at its old path");
    assert!(
        !project.root().join("items").exists(),
        "the move was undone"
    );
    let config = std::fs::read_to_string(project.config_path()).expect("the configuration");
    assert!(config.contains("storage_root = \"work\""), "{config}");
    assert!(config.contains("version = \"1.0\""), "{config}");
}

// -- outside Git ------------------------------------------------------------

#[test]
fn a_storage_migration_outside_git_warns_and_proceeds() {
    let (_dir, project) = project();
    if waymark_core::git::find_repository(&project).is_some() {
        return;
    }
    create_one(&project, "First");

    let preview = migrate::migrate_storage(
        &project,
        &StorageAction::Root {
            path: "items".to_owned(),
        },
        &dry_run(),
    )
    .expect("the preview succeeds");
    assert!(preview.meta.warnings.iter().any(|warning| {
        warning.code == Code::NotAGitRepository && warning.message.contains("Git")
    }));

    let real = migrate::migrate_storage(
        &project,
        &StorageAction::Root {
            path: "items".to_owned(),
        },
        &options(),
    )
    .expect("the migration proceeds outside Git");
    assert!(real.written);
    assert!(real
        .meta
        .warnings
        .iter()
        .any(|warning| warning.code == Code::NotAGitRepository));
}
