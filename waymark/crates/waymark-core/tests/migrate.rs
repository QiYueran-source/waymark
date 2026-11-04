//! Version and schema migration, driven through the public seam.
//!
//! Every case starts from a real temporary project, writes real documents, and
//! then asserts what is on disk afterwards. The migration seam is
//! `waymark_core::ops::migrate::{plan_version, plan_schema, execute,
//! migrate_version, migrate_schema}`; the read-only gate is observed through
//! `ops::{edit, archive, list, read}`.

use camino::{Utf8Path, Utf8PathBuf};
use serde_json::json;
use waymark_core::config::{Config, FieldDecl, FieldType};
use waymark_core::envelope::Warning;
use waymark_core::error::Code;
use waymark_core::lock::{with_project_lock, LockWait};
use waymark_core::model::{FieldKey, FieldValue, WorkItem, FORMAT_GENERATION};
use waymark_core::ops::config::{self, FieldAttribute, Scalar};
use waymark_core::ops::create::{self, CreateRequest};
use waymark_core::ops::edit::{self, FieldChange, FieldChangeEntry};
use waymark_core::ops::list::{self, Query};
use waymark_core::ops::migrate::{self, SchemaAction, StorageAction};
use waymark_core::ops::{archive, read, OpOptions};
use waymark_core::testing::fixtures;
use waymark_core::{InitOptions, Project};

fn options() -> OpOptions {
    OpOptions::new().with_clock(fixtures::shared_clock())
}

fn dry_run() -> OpOptions {
    options().dry_run()
}

fn field_key(text: &str) -> FieldKey {
    FieldKey::parse(text).expect("a valid field key")
}

fn project_with_fields(fields: &[(&str, FieldType, bool)]) -> (tempfile::TempDir, Project) {
    let mut config = Config::fresh();
    for (key, ty, required) in fields {
        config.fields.insert(
            field_key(key),
            FieldDecl {
                ty: ty.clone(),
                required: *required,
                default: None,
            },
        );
    }
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 path");
    let project = Project::init(
        &root,
        InitOptions {
            config: Some(config),
            ..InitOptions::default()
        },
    )
    .expect("the project initialises")
    .with_clock(fixtures::shared_clock())
    .with_id_source(Box::new(waymark_core::testing::SequentialIds::new()));
    (dir, project)
}

fn create_one(project: &Project, title: &str) -> (waymark_core::WorkItemId, Utf8PathBuf) {
    let created = create::create(
        project,
        CreateRequest::new(title, "task", "frank"),
        &options(),
    )
    .expect("creation succeeds");
    (created.item.id, created.path)
}

/// Rewrite a document's persisted generation to the previous readable one,
/// exactly as an older release (or a hand edit) would leave it.
fn downgrade(path: &Utf8Path) {
    let text = std::fs::read_to_string(path).expect("the document is readable");
    let downgraded = text.replacen("schema: 1\n", "schema: 0\n", 1);
    assert_ne!(text, downgraded, "the document declares generation 1");
    std::fs::write(path, downgraded).expect("the document is writable");
}

fn item_at(path: &Utf8Path) -> WorkItem {
    waymark_core::parse::parse_item_file(path).expect("the document parses")
}

fn config_text(project: &Project) -> String {
    std::fs::read_to_string(project.config_path()).expect("the configuration is readable")
}

/// Rewrite the configuration's version literal by hand.
fn set_config_version(project: &Project, version: &str) {
    let text = config_text(project);
    let current = config::show(project)
        .expect("the configuration loads")
        .version;
    let replaced = text.replacen(
        &format!("version = \"{current}\""),
        &format!("version = \"{version}\""),
        1,
    );
    assert_ne!(text, replaced, "the version literal was found");
    std::fs::write(project.config_path(), replaced).expect("the configuration is writable");
}

/// Every regular file under `root`, with its bytes, for a write-nothing check.
fn snapshot(root: &Utf8Path) -> Vec<(Utf8PathBuf, Vec<u8>)> {
    let mut files: Vec<(Utf8PathBuf, Vec<u8>)> = Vec::new();
    collect(root, &mut files);
    files.sort();
    files
}

fn collect(directory: &Utf8Path, files: &mut Vec<(Utf8PathBuf, Vec<u8>)>) {
    for entry in std::fs::read_dir(directory).expect("a readable directory") {
        let entry = entry.expect("a readable directory entry");
        let path = Utf8PathBuf::from_path_buf(entry.path()).expect("a UTF-8 path");
        if path.is_dir() {
            collect(&path, files);
        } else if path.is_file() {
            files.push((path.clone(), std::fs::read(&path).expect("a readable file")));
        }
    }
}

fn set_note(project: &Project, id: &waymark_core::WorkItemId, note: &str) {
    edit::update_custom_fields(
        project,
        id.as_str(),
        &[FieldChangeEntry {
            key: "note".to_owned(),
            change: FieldChange::Set(FieldValue::String(note.to_owned())),
        }],
        &options(),
    )
    .expect("the note is declared and settable");
}

// -- generation and revision ------------------------------------------------

#[test]
fn the_configuration_carries_generation_and_revision_and_only_a_migration_moves_the_generation() {
    let (_dir, project) = fixtures::deterministic_project();
    assert!(config_text(&project).contains("version = \"1.0\""));

    // An ordinary mediated configuration write moves the minor only.
    let outcome = config::set_scalar(&project, Scalar::Operator, "frank", &options())
        .expect("the operator is a mediated write");
    assert_eq!(outcome.previous_revision, 0);
    assert_eq!(outcome.revision, 1);
    assert_eq!(outcome.config.version.generation, FORMAT_GENERATION);
    assert!(config_text(&project).contains("version = \"1.1\""));

    // Only the migration moves the major.
    set_config_version(&project, "0.2");
    let report = migrate::migrate_version(&project, FORMAT_GENERATION, &options())
        .expect("the generation migration runs");
    assert_eq!(report.previous_revision, 2);
    assert_eq!(report.revision, 3);
    assert!(report.config_changed);
    assert!(config_text(&project).contains("version = \"1.3\""));

    // And a later ordinary write leaves it alone.
    let outcome = config::set_scalar(&project, Scalar::Operator, "alice", &options())
        .expect("another mediated write");
    assert_eq!(outcome.config.version.generation, FORMAT_GENERATION);
    assert!(config_text(&project).contains("version = \"1.4\""));
}

#[test]
fn a_stale_item_is_read_only_for_mediated_writes_but_still_readable() {
    let (_dir, project) = project_with_fields(&[("note", FieldType::String, false)]);
    let (id, path) = create_one(&project, "Fix the thing");
    set_note(&project, &id, "hello");
    downgrade(&path);

    let error = edit::set_title(&project, id.as_str(), "Changed", &options())
        .expect_err("a stale item rejects a title write");
    assert_eq!(error.code(), Code::ItemNeedsMigration);
    assert_eq!(error.details().get("item_generation"), Some(&json!(0)));
    assert_eq!(
        error.details().get("project_generation"),
        Some(&json!(FORMAT_GENERATION))
    );

    let error = archive::archive(&project, id.as_str(), None, &options())
        .expect_err("archive is a mediated write and rejects a stale item");
    assert_eq!(error.code(), Code::ItemNeedsMigration);

    // Reads still include it, marked rather than hidden.
    let scoped = read::read_scoped(&project, id.as_str()).expect("the scoped read succeeds");
    assert_eq!(scoped.item.title, "Fix the thing");
    assert!(scoped.item.needs_migration());

    let listing = list::list(&project, &Query::new()).expect("the listing succeeds");
    let row = listing
        .items
        .iter()
        .find(|row| row.item.id == id)
        .expect("the stale item is listed");
    assert!(row.needs_migration);

    // Nothing was written by the refusals.
    assert_eq!(item_at(&path).title, "Fix the thing");
    assert_eq!(item_at(&path).schema, 0);
}

#[test]
fn a_stale_item_s_stored_fields_are_unchanged_by_the_refusal() {
    let (_dir, project) = project_with_fields(&[("note", FieldType::String, false)]);
    let (id, path) = create_one(&project, "Fix the thing");
    set_note(&project, &id, "hello");
    downgrade(&path);
    let before = std::fs::read(&path).expect("the document is readable");

    let _ = edit::set_title(&project, id.as_str(), "Changed", &options());
    let _ = archive::archive(&project, id.as_str(), None, &options());

    let after = std::fs::read(&path).expect("the document is still readable");
    assert_eq!(before, after, "a refused write leaves the bytes untouched");
}

// -- version migration ------------------------------------------------------

#[test]
fn a_version_migration_dry_run_checks_everything_and_writes_nothing() {
    let (_dir, project) = project_with_fields(&[("note", FieldType::String, false)]);
    let (first, first_path) = create_one(&project, "First");
    let (second, second_path) = create_one(&project, "Second");
    set_note(&project, &first, "one");
    set_note(&project, &second, "two");
    archive::archive(&project, second.as_str(), Some("duplicate"), &options())
        .expect("the second item archives");
    downgrade(&first_path);
    downgrade(&second_path);
    set_config_version(&project, "0.2");

    let before = snapshot(project.root());
    let report = migrate::migrate_version(&project, FORMAT_GENERATION, &dry_run())
        .expect("the dry run succeeds");
    assert!(report.dry_run);
    assert!(!report.written);
    assert_eq!(report.count, 2);
    assert!(report.config_changed);
    assert_eq!(report.previous_revision, 2);
    assert_eq!(report.revision, 3);
    let ids: Vec<&str> = report
        .affected
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    assert_eq!(ids.len(), 2);
    assert!(report
        .targets
        .iter()
        .all(|target| target.outcome == "skipped"));
    assert_eq!(snapshot(project.root()), before, "a dry run writes nothing");

    // No persisted plan, plan id, or handshake token appeared anywhere.
    let names: Vec<String> = before
        .iter()
        .map(|(path, _)| path.file_name().unwrap_or_default().to_owned())
        .collect();
    assert!(!names.iter().any(|name| name.contains("plan")));
}

#[test]
fn a_version_migration_rewrites_the_files_and_raises_the_configuration_revision() {
    let (_dir, project) = project_with_fields(&[("note", FieldType::String, false)]);
    let (first, first_path) = create_one(&project, "First");
    let (second, second_path) = create_one(&project, "Second");
    set_note(&project, &first, "one");
    set_note(&project, &second, "two");
    archive::archive(&project, second.as_str(), Some("duplicate"), &options())
        .expect("the second item archives");
    let first_before = item_at(&first_path);
    let second_before = item_at(&second_path);
    downgrade(&first_path);
    downgrade(&second_path);
    set_config_version(&project, "0.2");

    let report = migrate::migrate_version(&project, FORMAT_GENERATION, &options())
        .expect("the migration runs");
    assert!(!report.dry_run);
    assert!(report.written);
    assert_eq!(report.count, 2);
    assert!(report.config_changed);
    assert!(config_text(&project).contains("version = \"1.3\""));

    // The generation moved; nothing else did.
    let first_after = item_at(&first_path);
    assert_eq!(first_after.schema, FORMAT_GENERATION);
    assert_eq!(first_after.title, first_before.title);
    assert_eq!(first_after.body, first_before.body);
    assert_eq!(first_after.custom_fields, first_before.custom_fields);
    assert_eq!(
        first_after.updated_at, first_before.updated_at,
        "updated_at is not a content change"
    );
    assert_eq!(first_after.created_at, first_before.created_at);

    let second_after = item_at(&second_path);
    assert_eq!(second_after.schema, FORMAT_GENERATION);
    assert_eq!(second_after.archived_at, second_before.archived_at);
    assert_eq!(second_after.archive_reason.as_deref(), Some("duplicate"));
    assert_eq!(second_after.updated_at, second_before.updated_at);
    assert_eq!(second_after.custom_fields, second_before.custom_fields);

    // A migrated document is canonical and valid for the new generation.
    let canonical = std::fs::read_to_string(&first_path).expect("the document is readable");
    let parsed = waymark_core::parse::parse_canonical(canonical.as_bytes(), &first_path)
        .expect("the migrated document is canonical");
    assert_eq!(parsed.schema, FORMAT_GENERATION);
}

#[test]
fn a_version_migration_with_nothing_to_do_is_a_successful_no_op() {
    let (_dir, project) = project_with_fields(&[]);
    let (_id, _path) = create_one(&project, "Fresh");
    let before = snapshot(project.root());
    let report = migrate::migrate_version(&project, FORMAT_GENERATION, &options())
        .expect("a no-op migration succeeds");
    assert!(!report.written);
    assert!(!report.config_changed);
    assert_eq!(report.count, 0);
    assert_eq!(report.previous_revision, 0);
    assert_eq!(report.revision, 0);
    assert_eq!(snapshot(project.root()), before);
}

#[test]
fn a_target_generation_the_release_cannot_write_is_migration_unsupported_generation() {
    let (_dir, project) = project_with_fields(&[]);
    for target in [0, 2, 99] {
        let error = migrate::migrate_version(&project, target, &options())
            .expect_err("the release writes exactly one generation");
        assert_eq!(error.code(), Code::MigrationUnsupportedGeneration);
        assert_eq!(error.details().get("requested"), Some(&json!(target)));
        assert_eq!(
            error.details().get("supported"),
            Some(&json!(FORMAT_GENERATION))
        );
    }
}

// -- schema migration -------------------------------------------------------

#[test]
fn removing_a_field_deletes_it_from_every_item_including_archived_ones() {
    let (_dir, project) = project_with_fields(&[("note", FieldType::String, false)]);
    let (first, first_path) = create_one(&project, "First");
    let (second, second_path) = create_one(&project, "Second");
    set_note(&project, &first, "one");
    set_note(&project, &second, "two");
    archive::archive(&project, second.as_str(), None, &options())
        .expect("the second item archives");
    let before = snapshot(project.root());

    let dry = migrate::migrate_schema(
        &project,
        SchemaAction::RemoveField(field_key("note")),
        &dry_run(),
    )
    .expect("the dry run succeeds");
    assert!(dry.config_changed);
    assert_eq!(dry.count, 2);
    assert!(!dry.written);
    assert_eq!(snapshot(project.root()), before, "a dry run writes nothing");

    let report = migrate::migrate_schema(
        &project,
        SchemaAction::RemoveField(field_key("note")),
        &options(),
    )
    .expect("the removal runs");
    assert!(report.written);
    assert!(report.config_changed);
    assert_eq!(report.count, 2);
    assert_eq!(report.revision, 1);
    assert!(!config_text(&project).contains("[fields.note]"));

    for path in [&first_path, &second_path] {
        let item = item_at(path);
        assert!(!item.custom_fields.contains_key("note"), "{path}");
    }
    // The archived marker survives a schema rewrite.
    assert!(item_at(&second_path).is_archived());
}

#[test]
fn adding_a_required_field_supplies_its_default_so_no_item_is_left_invalid() {
    let (_dir, project) = project_with_fields(&[]);
    let (first, first_path) = create_one(&project, "First");
    let (_second, second_path) = create_one(&project, "Second");
    let first_id = first.as_str().to_owned();
    assert!(!first_id.is_empty());

    // A document may carry an undeclared key written by hand; the migration
    // keeps it and fills only the documents that have no value.
    let text = std::fs::read_to_string(&first_path).expect("the document is readable");
    let with_value = text.replacen("\n---\n", "\ncustom_fields:\n  effort: 5\n---\n", 1);
    assert_ne!(text, with_value);
    std::fs::write(&first_path, with_value).expect("the document is writable");

    let action = SchemaAction::AddRequiredField {
        key: field_key("effort"),
        ty: FieldType::Integer,
        default: Some(FieldValue::Integer(1)),
    };
    let before = snapshot(project.root());
    let dry = migrate::migrate_schema(&project, action.clone(), &dry_run())
        .expect("the dry run succeeds");
    assert_eq!(dry.count, 1, "only the item without a value is rewritten");
    assert!(dry.config_changed);
    assert_eq!(snapshot(project.root()), before);

    let report = migrate::migrate_schema(&project, action, &options()).expect("the add runs");
    assert!(report.written);
    assert!(report.config_changed);
    assert_eq!(report.revision, 1);

    let first_after = item_at(&first_path);
    assert_eq!(
        first_after.custom_fields.get("effort"),
        Some(&FieldValue::Integer(5)),
        "an existing value is preserved, never converted"
    );
    let second_after = item_at(&second_path);
    assert_eq!(
        second_after.custom_fields.get("effort"),
        Some(&FieldValue::Integer(1)),
        "the default is supplied where no value existed"
    );
    assert!(config_text(&project).contains("[fields.effort]"));
    assert!(config_text(&project).contains("required = true"));
}

#[test]
fn every_dry_run_reports_without_waiting_for_the_project_lock() {
    let (_dir, project) = project_with_fields(&[]);
    let (_id, _path) = create_one(&project, "First");

    // Hold the one write lock, then run every migration preview under an
    // immediate wait: a preview that took the lock would answer LOCK_TIMEOUT
    // instead of its report.
    with_project_lock(&project, LockWait::default_policy(), || {
        let dry = options().dry_run().with_lock_wait(LockWait::immediate());

        let version = migrate::migrate_version(&project, FORMAT_GENERATION, &dry)
            .expect("a version preview never waits on the write lock");
        assert!(!version.written);

        let schema = migrate::migrate_schema(
            &project,
            SchemaAction::AddRequiredField {
                key: field_key("effort"),
                ty: FieldType::Integer,
                default: Some(FieldValue::Integer(1)),
            },
            &dry,
        )
        .expect("a schema preview never waits on the write lock");
        assert!(!schema.written);
        assert!(schema.config_changed);

        let storage = migrate::migrate_storage(
            &project,
            &StorageAction::Root {
                path: "items".to_owned(),
            },
            &dry,
        )
        .expect("a storage preview never waits on the write lock");
        assert!(!storage.written);
        Ok(())
    })
    .expect("the lock is released");

    // None of the previews wrote anything.
    assert!(config_text(&project).contains("storage_root = \"work\""));
    assert!(!config_text(&project).contains("[fields.effort]"));
}

#[test]
fn a_required_field_without_a_default_cannot_leave_an_item_invalid() {
    let (_dir, project) = project_with_fields(&[]);
    let (_id, _path) = create_one(&project, "First");
    let error = migrate::migrate_schema(
        &project,
        SchemaAction::AddRequiredField {
            key: field_key("effort"),
            ty: FieldType::Integer,
            default: None,
        },
        &options(),
    )
    .expect_err("an item without a value would be left invalid");
    assert_eq!(error.code(), Code::ValidationError);
    assert!(!config_text(&project).contains("[fields.effort]"));
}

#[test]
fn schema_migration_refuses_a_field_type_change_with_the_same_command_as_config() {
    let (_dir, project) = project_with_fields(&[("effort", FieldType::Integer, false)]);
    let from_config = config::set_field(
        &project,
        &field_key("effort"),
        FieldAttribute::Type,
        "string",
        &options(),
    )
    .expect_err("a type change is migration territory");
    let from_migration = migrate::migrate_schema(
        &project,
        SchemaAction::AddRequiredField {
            key: field_key("effort"),
            ty: FieldType::String,
            default: Some(FieldValue::String("x".to_owned())),
        },
        &options(),
    )
    .expect_err("a type change is not a migration action");
    assert_eq!(from_config.code(), Code::ActionRequiresMigration);
    assert_eq!(from_migration.code(), Code::ActionRequiresMigration);
    assert_eq!(from_config.message(), from_migration.message());
    assert_eq!(
        from_config.details().get("required_command"),
        Some(&json!("waymark migrate schema"))
    );
    assert_eq!(
        from_migration.details().get("required_command"),
        from_config.details().get("required_command")
    );
}

#[test]
fn add_required_field_cannot_promote_an_existing_optional_field() {
    let (_dir, project) = project_with_fields(&[("effort", FieldType::Integer, false)]);
    let before = config_text(&project);

    // The same refusal `config set field effort required true` already gives:
    // promotion is a manual schema change, never a migration action.
    let from_config = config::set_field(
        &project,
        &field_key("effort"),
        FieldAttribute::Required,
        "true",
        &options(),
    )
    .expect_err("a requiredness change is migration territory");
    let from_migration = migrate::migrate_schema(
        &project,
        SchemaAction::AddRequiredField {
            key: field_key("effort"),
            ty: FieldType::Integer,
            default: Some(FieldValue::Integer(1)),
        },
        &options(),
    )
    .expect_err("the existing declaration cannot be promoted");

    assert_eq!(from_migration.code(), Code::ActionRequiresMigration);
    assert_eq!(from_migration.message(), from_config.message());
    assert_eq!(
        from_migration.details().get("required_command"),
        from_config.details().get("required_command")
    );
    assert_eq!(config_text(&project), before, "nothing is declared");
}

#[test]
fn removing_an_undeclared_field_is_a_validation_error() {
    let (_dir, project) = project_with_fields(&[]);
    let error = migrate::migrate_schema(
        &project,
        SchemaAction::RemoveField(field_key("note")),
        &options(),
    )
    .expect_err("there is no such declaration");
    assert_eq!(error.code(), Code::ValidationError);
}

// -- preconditions and execution failures ----------------------------------

#[test]
fn an_incomplete_projection_is_migration_precondition_failed_with_every_diagnostic() {
    let (_dir, project) = project_with_fields(&[]);
    let (_id, path) = create_one(&project, "First");
    downgrade(&path);
    let broken = path.parent().expect("a parent").join("wm-ffffffffffff.md");
    std::fs::write(&broken, "this is not a work item").expect("the broken file is writable");

    let before = snapshot(project.root());
    let error = migrate::migrate_version(&project, FORMAT_GENERATION, &options())
        .expect_err("an unparseable sibling blocks a migration");
    assert_eq!(error.code(), Code::MigrationPreconditionFailed);
    let diagnostics = error
        .details()
        .get("diagnostics")
        .and_then(|value| value.as_array())
        .expect("the diagnostics are reported");
    assert!(!diagnostics.is_empty());
    assert_eq!(snapshot(project.root()), before, "nothing was written");
}

#[test]
fn a_configuration_the_release_cannot_read_is_a_precondition_failure() {
    let (_dir, project) = project_with_fields(&[]);
    let (id, path) = create_one(&project, "First");
    downgrade(&path);
    set_config_version(&project, "9.0");

    let error = migrate::migrate_version(&project, FORMAT_GENERATION, &options())
        .expect_err("the configuration generation is unreadable");
    assert_eq!(error.code(), Code::MigrationPreconditionFailed);
    let diagnostics = format!("{:?}", error.details().get("diagnostics"));
    assert!(diagnostics.contains("config_unreadable"), "{diagnostics}");
    assert_eq!(item_at(&path).schema, 0, "the document was untouched");
    assert_eq!(item_at(&path).id, id);
}

#[test]
fn a_stale_plan_is_rechecked_and_never_applied() {
    let (_dir, project) = project_with_fields(&[]);
    let (_id, path) = create_one(&project, "Fix the thing");
    downgrade(&path);

    let plan = migrate::plan_version(&project, FORMAT_GENERATION).expect("the plan is built");
    // A hand edit between the preview and the execution must be detected.
    let text = std::fs::read_to_string(&path).expect("the document is readable");
    let edited = text.replacen("Fix the thing", "Edited by hand", 1);
    std::fs::write(&path, &edited).expect("the document is writable");

    let error =
        migrate::execute(&project, &plan, &options()).expect_err("the stale plan is refused");
    assert_eq!(error.code(), Code::MigrationPreconditionFailed);
    let diagnostics = format!("{:?}", error.details().get("diagnostics"));
    assert!(
        diagnostics.contains("fingerprint_disagreement"),
        "{diagnostics}"
    );
    assert_eq!(item_at(&path).schema, 0);
    assert_eq!(item_at(&path).title, "Edited by hand");
}

#[test]
fn the_command_surface_re_derives_the_plan_so_a_preview_followed_by_an_edit_succeeds_correctly() {
    let (_dir, project) = project_with_fields(&[]);
    let (_id, path) = create_one(&project, "Fix the thing");
    downgrade(&path);

    let preview = migrate::migrate_version(&project, FORMAT_GENERATION, &dry_run())
        .expect("the preview succeeds");
    assert_eq!(preview.count, 1);

    let text = std::fs::read_to_string(&path).expect("the document is readable");
    std::fs::write(&path, text.replacen("Fix the thing", "Edited by hand", 1))
        .expect("the document is writable");

    let report = migrate::migrate_version(&project, FORMAT_GENERATION, &options())
        .expect("the real run re-derives and succeeds");
    assert!(report.written);
    assert_eq!(report.count, 1);
    let after = item_at(&path);
    assert_eq!(after.schema, FORMAT_GENERATION);
    assert_eq!(
        after.title, "Edited by hand",
        "the new bytes are used, never overwritten by a stale plan"
    );
}

#[cfg(unix)]
#[test]
fn a_write_failure_reports_per_target_outcomes() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, project) = project_with_fields(&[]);
    let (_id, path) = create_one(&project, "First");
    downgrade(&path);
    let plan = migrate::plan_version(&project, FORMAT_GENERATION).expect("the plan is built");

    // Staging needs a temporary file beside the destination. Removing write
    // permission from the category directory makes the write fail after every
    // read-only check has passed.
    let directory = path.parent().expect("a parent").to_path_buf();
    let original = std::fs::metadata(&directory)
        .expect("the directory exists")
        .permissions();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o555))
        .expect("the directory is writable");
    let result = migrate::execute(&project, &plan, &options());
    std::fs::set_permissions(&directory, original).expect("the permissions are restored");

    let error = result.expect_err("the batch cannot stage a temporary file");
    assert_eq!(error.code(), Code::MigrationFailed);
    let per_target = error
        .details()
        .get("per_target")
        .and_then(|value| value.as_array())
        .expect("per-target outcomes are reported");
    assert_eq!(per_target.len(), 1);
    assert_eq!(per_target[0]["path"], json!(path.to_string()));
    assert_eq!(per_target[0]["outcome"], json!("failed"));
    assert_eq!(item_at(&path).schema, 0, "nothing was applied");
}

#[test]
fn a_migration_outside_a_git_repository_warns_that_recovery_is_unavailable() {
    let (_dir, project) = project_with_fields(&[]);
    let (_id, path) = create_one(&project, "First");
    downgrade(&path);

    if waymark_core::git::find_repository(&project).is_some() {
        // The test only has a meaning outside a repository; the workspace's own
        // checkout would make the temp directory a child of one.
        return;
    }

    let report = migrate::migrate_version(&project, FORMAT_GENERATION, &dry_run())
        .expect("the dry run succeeds");
    assert!(report.meta.warnings.iter().any(|warning: &Warning| {
        warning.code == Code::NotAGitRepository && warning.message.contains("Git")
    }));

    let real = migrate::migrate_version(&project, FORMAT_GENERATION, &options())
        .expect("the migration proceeds outside Git");
    assert!(real.written);
    assert!(real
        .meta
        .warnings
        .iter()
        .any(|warning| warning.code == Code::NotAGitRepository));
}

// -- no plan/apply handshake -------------------------------------------------

#[test]
fn migration_reports_expose_the_counts_and_per_target_outcomes_without_a_handshake() {
    let (_dir, project) = project_with_fields(&[("note", FieldType::String, false)]);
    let (id, path) = create_one(&project, "First");
    set_note(&project, &id, "one");
    downgrade(&path);

    let preview = migrate::migrate_version(&project, FORMAT_GENERATION, &dry_run())
        .expect("the preview succeeds");
    let data = preview.data_json();
    assert_eq!(data["migration"], json!("version"));
    assert_eq!(data["count"], json!(1));
    assert_eq!(data["written"], json!(false));
    assert_eq!(data["targets"][0]["outcome"], json!("skipped"));
    assert!(data.get("plan_id").is_none(), "no plan identifier exists");
    assert!(data.get("plan").is_none(), "no plan is persisted or echoed");
}
