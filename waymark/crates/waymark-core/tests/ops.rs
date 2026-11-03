//! `create` and `read`: the two operations the walking skeleton needs.

use camino::Utf8PathBuf;
use waymark_core::envelope::Completeness;
use waymark_core::error::Code;
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::{create, read, OpOptions};
use waymark_core::testing::fixtures;
use waymark_core::Project;

fn utf8(dir: &tempfile::TempDir) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 temporary path")
}

fn options() -> OpOptions {
    OpOptions::new().with_clock(fixtures::shared_clock())
}

#[test]
fn create_writes_one_document_named_from_the_new_stable_id() {
    let (_dir, project) = fixtures::deterministic_project();
    let created = create::create(
        &project,
        CreateRequest::new("Fix the thing", "task", "frank"),
        &options(),
    )
    .expect("creation succeeds");

    assert!(created.written);
    assert!(created.path.exists(), "{} was not written", created.path);
    assert!(created.path.ends_with("wm-000000000000.md"));
    assert_eq!(created.item.id.as_str(), "wm-000000000000");

    let on_disk = std::fs::read_to_string(&created.path).unwrap();
    assert_eq!(on_disk, waymark_core::serialize::to_document(&created.item));
}

#[test]
fn create_stamps_the_clock_and_never_asks_the_caller_for_it() {
    let (_dir, project) = fixtures::deterministic_project();
    let created = create::create(
        &project,
        CreateRequest::new("t", "task", "frank"),
        &options(),
    )
    .unwrap();
    assert_eq!(created.item.created_at.to_string(), "2026-11-03T05:43:10Z");
    assert_eq!(created.item.updated_at, created.item.created_at);
    assert_eq!(created.item.status, waymark_core::Status::Open);
    assert_eq!(created.item.creator, None);
    assert_eq!(created.item.body, "");
    assert!(created.item.dependencies.is_empty());
    assert!(created.item.custom_fields.is_empty());
}

#[test]
fn create_records_the_operator_as_creator() {
    let (_dir, project) = fixtures::deterministic_project();
    let created = create::create(
        &project,
        CreateRequest::new("t", "task", "frank"),
        &options().with_operator("alice"),
    )
    .unwrap();
    assert_eq!(created.item.creator.as_deref(), Some("alice"));
}

#[test]
fn create_keeps_the_body_and_its_canonical_newline() {
    let (_dir, project) = fixtures::deterministic_project();
    let created = create::create(
        &project,
        CreateRequest::new("t", "task", "frank").with_body("hello\n\n\n"),
        &options(),
    )
    .unwrap();
    assert_eq!(created.item.body, "hello\n");
    let reread = read::read(&project, created.item.id.as_str()).unwrap();
    assert_eq!(reread.item.body, "hello\n");
}

#[test]
fn create_trims_the_title_and_the_owner() {
    let (_dir, project) = fixtures::deterministic_project();
    let created = create::create(
        &project,
        CreateRequest::new("  Fix the thing  ", " task ", "  frank  "),
        &options(),
    )
    .unwrap();
    assert_eq!(created.item.title, "Fix the thing");
    assert_eq!(created.item.category.as_str(), "task");
    assert_eq!(created.item.owner, "frank");
}

#[test]
fn a_missing_title_fails_the_whole_create_and_writes_nothing() {
    let (_dir, project) = fixtures::deterministic_project();
    let error = create::create(
        &project,
        CreateRequest::new("   ", "task", "frank"),
        &options(),
    )
    .unwrap_err();
    assert_eq!(error.code(), Code::ValidationError);
    assert!(error.message().contains("title"), "{error}");
    assert!(project.inventory().unwrap().entries.is_empty());
}

#[test]
fn a_missing_category_fails_the_whole_create_and_writes_nothing() {
    let (_dir, project) = fixtures::deterministic_project();
    let error =
        create::create(&project, CreateRequest::new("t", "", "frank"), &options()).unwrap_err();
    assert_eq!(error.code(), Code::ValidationError);
    assert!(error.message().contains("category"), "{error}");
    assert!(project.inventory().unwrap().entries.is_empty());
}

#[test]
fn a_missing_owner_fails_the_whole_create_and_writes_nothing() {
    let (_dir, project) = fixtures::deterministic_project();
    let error =
        create::create(&project, CreateRequest::new("t", "task", ""), &options()).unwrap_err();
    assert_eq!(error.code(), Code::ValidationError);
    assert!(error.message().contains("owner"), "{error}");
    assert!(project.inventory().unwrap().entries.is_empty());
}

#[test]
fn an_undeclared_category_is_refused_by_create() {
    let (_dir, project) = fixtures::deterministic_project();
    let error = create::create(
        &project,
        CreateRequest::new("t", "bug", "frank"),
        &options(),
    )
    .unwrap_err();
    assert_eq!(error.code(), Code::ValidationError);
    assert!(error.message().contains("not a category"), "{error}");
    assert!(project.inventory().unwrap().entries.is_empty());
}

#[test]
fn every_missing_required_field_is_reported_together() {
    let (_dir, project) = fixtures::deterministic_project();
    let error = create::create(&project, CreateRequest::new("", "", ""), &options()).unwrap_err();
    assert_eq!(error.code(), Code::ValidationError);
    let issues = error.details()["issues"].as_array().unwrap();
    let paths: Vec<&str> = issues
        .iter()
        .filter_map(|issue| issue["path"].as_str())
        .collect();
    assert!(paths.contains(&"title"), "{paths:?}");
    assert!(paths.contains(&"category"), "{paths:?}");
    assert!(paths.contains(&"owner"), "{paths:?}");
}

#[test]
fn a_multi_line_title_is_refused() {
    let (_dir, project) = fixtures::deterministic_project();
    let error = create::create(
        &project,
        CreateRequest::new("line one\nline two", "task", "frank"),
        &options(),
    )
    .unwrap_err();
    assert_eq!(error.code(), Code::ValidationError);
    assert!(error.message().contains("single line"), "{error}");
}

#[test]
fn a_dry_run_computes_the_item_without_writing_it() {
    let (_dir, project) = fixtures::deterministic_project();
    let created = create::create(
        &project,
        CreateRequest::new("t", "task", "frank"),
        &options().dry_run(),
    )
    .unwrap();
    assert!(!created.written);
    assert!(created.fingerprint.is_none());
    assert!(!created.path.exists());
    assert_eq!(
        waymark_core::serialize::to_document(&created.item),
        "---\nschema: 1\nid: wm-000000000000\ntitle: t\ncategory: task\nstatus: open\nowner: frank\ncreated_at: 2026-11-03T05:43:10Z\nupdated_at: 2026-11-03T05:43:10Z\n---\n"
    );
}

#[test]
fn two_creates_get_two_ids() {
    let (_dir, project) = fixtures::deterministic_project();
    let first = create::create(&project, CreateRequest::new("a", "task", "f"), &options()).unwrap();
    let second =
        create::create(&project, CreateRequest::new("b", "task", "f"), &options()).unwrap();
    assert_ne!(first.item.id, second.item.id);
    assert!(first.path.exists() && second.path.exists());
}

#[test]
fn create_repairs_a_missing_write_lock_ignore_entry() {
    let (dir, project) = fixtures::deterministic_project();
    let gitignore = utf8(&dir).join(".waymark/.gitignore");
    std::fs::write(&gitignore, "").unwrap();
    create::create(&project, CreateRequest::new("t", "task", "f"), &options()).unwrap();
    let ignore = std::fs::read_to_string(&gitignore).unwrap();
    assert!(ignore.lines().any(|line| line == "write.lock"), "{ignore}");
}

#[test]
fn read_returns_the_document_the_fields_and_the_meta_block() {
    let (_dir, project) = fixtures::deterministic_project();
    let created =
        create::create(&project, CreateRequest::new("t", "task", "f"), &options()).unwrap();

    let read_back = read::read(&project, created.item.id.as_str()).unwrap();
    assert_eq!(read_back.item, created.item);
    assert_eq!(read_back.path, created.path);
    assert_eq!(read_back.category.as_str(), "task");
    assert_eq!(read_back.fingerprint, created.fingerprint.unwrap());
    assert_eq!(read_back.meta.completeness, Completeness::Scoped);
}

#[test]
fn read_accepts_an_uppercase_id() {
    let (_dir, project) = fixtures::deterministic_project();
    let created =
        create::create(&project, CreateRequest::new("t", "task", "f"), &options()).unwrap();
    let read_back = read::read(&project, "WM-000000000000").unwrap();
    assert_eq!(read_back.item.id, created.item.id);
}

#[test]
fn read_of_an_absent_id_is_item_not_found() {
    let (_dir, project) = fixtures::deterministic_project();
    let error = read::read(&project, "wm-000000000000").unwrap_err();
    assert_eq!(error.code(), Code::ItemNotFound);
}

#[test]
fn read_of_a_malformed_id_is_invalid_id() {
    let (_dir, project) = fixtures::deterministic_project();
    for input in ["nonsense", "wm-short", "wm-0000000000000", ""] {
        let error = read::read(&project, input).unwrap_err();
        assert_eq!(error.code(), Code::InvalidId, "input {input:?}");
    }
}

#[test]
fn read_of_a_document_with_a_mismatched_id_is_an_error() {
    let (_dir, project) = fixtures::deterministic_project();
    let created =
        create::create(&project, CreateRequest::new("t", "task", "f"), &options()).unwrap();
    // Move the file so its stem no longer matches the persisted ID.
    let moved = created.path.with_file_name("wm-000000000001.md");
    std::fs::rename(&created.path, &moved).unwrap();
    let error = read::read(&project, "wm-000000000000").unwrap_err();
    assert_eq!(error.code(), Code::ItemNotFound);
}

#[test]
fn a_dry_run_commits_nothing_and_leaves_the_project_empty() {
    let (_dir, project) = fixtures::deterministic_project();
    let planned = create::create(
        &project,
        CreateRequest::new("t", "task", "f"),
        &options().dry_run(),
    )
    .unwrap();
    assert!(!planned.written);
    assert!(project.inventory().unwrap().entries.is_empty());
    let error = read::read(&project, planned.item.id.as_str()).unwrap_err();
    assert_eq!(error.code(), Code::ItemNotFound);
}

#[test]
fn create_reads_the_project_lock_for_the_whole_write() {
    let (_dir, project) = fixtures::deterministic_project();
    // A second handle at the same lock path is enough to see contention: the
    // lock is per file, not per handle.
    let lock_path = utf8(&_dir).join(".waymark/write.lock");
    waymark_core::lock::with_project_lock(
        &project,
        waymark_core::lock::LockWait::default_policy(),
        || {
            assert!(waymark_core::lock::is_store_locked(&utf8(&_dir)).unwrap());
            Ok(())
        },
    )
    .unwrap();

    // And an operation that cannot get the lock must not write.
    let held = waymark_core::lock::with_project_lock(
        &project,
        waymark_core::lock::LockWait::default_policy(),
        || {
            let error = create::create(
                &project,
                CreateRequest::new("t", "task", "f"),
                &options().with_lock_wait(waymark_core::lock::LockWait::immediate()),
            )
            .unwrap_err();
            assert_eq!(error.code(), Code::LockTimeout);
            assert!(error.retryable());
            Ok(())
        },
    );
    held.unwrap();
    assert!(lock_path.exists());
    assert!(project.inventory().unwrap().entries.is_empty());
}

#[test]
fn a_created_document_is_in_canonical_form_on_disk() {
    let (_dir, project) = fixtures::deterministic_project();
    let created =
        create::create(&project, CreateRequest::new("t", "task", "f"), &options()).unwrap();
    let bytes = std::fs::read(&created.path).unwrap();
    let parsed = waymark_core::parse::parse_canonical(&bytes, &created.path)
        .expect("the bytes are canonical");
    assert_eq!(parsed, created.item);
}

#[test]
fn project_configuration_is_visible_through_the_project() {
    let (_dir, project): (tempfile::TempDir, Project) = fixtures::project();
    assert!(project
        .category(&waymark_core::CategoryKey::parse("task").unwrap())
        .is_some());
    assert_eq!(project.config().storage_root, "work");
}

/// A clock that records whether the store lock was held when it was asked for
/// the time.
///
/// Timestamps are taken inside the write path, so this observes the lock from
/// inside the operation rather than from a racing thread.
struct LockProbeClock {
    inner: waymark_core::FixedClock,
    root: Utf8PathBuf,
    held: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl waymark_core::Clock for LockProbeClock {
    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        let held = waymark_core::lock::is_store_locked(&self.root).unwrap_or(false);
        self.held.store(held, std::sync::atomic::Ordering::SeqCst);
        waymark_core::Clock::now(&self.inner)
    }
}

#[test]
fn the_write_happens_while_the_store_lock_is_held() {
    let (dir, project) = fixtures::deterministic_project();
    let root = utf8(&dir);
    let held = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let options = OpOptions::new().with_clock(std::sync::Arc::new(LockProbeClock {
        inner: fixtures::clock(),
        root,
        held: held.clone(),
    }));
    let created =
        create::create(&project, CreateRequest::new("t", "task", "frank"), &options).unwrap();
    assert!(
        held.load(std::sync::atomic::Ordering::SeqCst),
        "the timestamps were taken without the store lock held"
    );
    assert!(created.path.exists());
}

#[test]
fn a_fingerprint_change_between_read_and_write_is_a_conflict() {
    let (_dir, project) = fixtures::deterministic_project();
    let created = create::create(
        &project,
        CreateRequest::new("t", "task", "frank"),
        &options(),
    )
    .unwrap();

    // Someone edits the document outside Waymark.
    let edited = format!(
        "{}{}",
        waymark_core::serialize::to_document(&created.item),
        "external\n"
    );
    std::fs::write(&created.path, &edited).unwrap();

    let error = waymark_core::write::replace_checked(
        &created.path,
        b"mine\n",
        &created.fingerprint.unwrap(),
        &created.item.id,
    )
    .unwrap_err();
    assert_eq!(error.code(), Code::WriteConflict);
    assert!(error.details()["expected_fingerprint"].is_string());
    assert!(error.details()["actual_fingerprint"].is_string());
    assert_ne!(
        error.details()["expected_fingerprint"],
        error.details()["actual_fingerprint"]
    );
    assert_eq!(std::fs::read_to_string(&created.path).unwrap(), edited);
}
