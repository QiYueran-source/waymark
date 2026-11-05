//! The configuration surface end to end: reading, validating, and every medited
//! write, observed through the public API and the bytes on disk.

use camino::{Utf8Path, Utf8PathBuf};
use waymark_core::config::{FieldType, CONFIG_FILE};
use waymark_core::error::Code;
use waymark_core::model::FieldKey;
use waymark_core::ops::config::{self, FieldAttribute, InitRequest, Scalar};
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::{create, OpOptions};
use waymark_core::store::Fingerprint;
use waymark_core::testing::fixtures;
use waymark_core::{CategoryKey, FieldValue, Project};

fn options() -> OpOptions {
    OpOptions::new().with_clock(fixtures::shared_clock())
}

fn key(text: &str) -> CategoryKey {
    CategoryKey::parse(text).expect("a valid category key")
}

fn field(text: &str) -> FieldKey {
    FieldKey::parse(text).expect("a valid field key")
}

fn read_config(project: &Project) -> String {
    std::fs::read_to_string(project.config_path()).expect("the configuration is readable")
}

/// Every work-item file's path, bytes, and fingerprint.
fn item_state(project: &Project) -> Vec<(Utf8PathBuf, Vec<u8>)> {
    let inventory = project.inventory().expect("the project scans");
    inventory
        .entries
        .iter()
        .map(|entry| {
            (
                entry.path.clone(),
                std::fs::read(&entry.path).expect("an item is readable"),
            )
        })
        .collect()
}

fn create_one(project: &Project, category: &str) -> Utf8PathBuf {
    let created = create::create(
        project,
        CreateRequest::new("Fix the thing", category, "frank"),
        &options(),
    )
    .expect("creation succeeds");
    created.path
}

#[test]
fn show_reads_the_effective_configuration_from_disk() {
    let (_dir, project) = fixtures::deterministic_project();
    let shown = config::show(&project).expect("the configuration loads");
    assert_eq!(shown, *project.config());
    assert!(shown.categories.contains_key(&key("task")));
    assert_eq!(shown.version.revision, 0);
}

#[test]
fn a_fresh_configuration_has_no_findings() {
    let (_dir, project) = fixtures::deterministic_project();
    assert!(config::validate_config(&project).is_empty());
}

#[test]
fn a_stale_expected_fingerprint_is_a_write_conflict_instead_of_a_rebase() {
    let (_dir, project) = fixtures::deterministic_project();
    let original = Fingerprint::of(&std::fs::read(project.config_path()).expect("readable"));

    // A hand edit that leaves the revision counter alone changes the bytes; the
    // fingerprint is the conflict check, not the counter.
    let edited = format!("{}# hand edited\n", read_config(&project));
    std::fs::write(project.config_path(), &edited).expect("the configuration is writable");
    let edited_fingerprint = Fingerprint::of(edited.as_bytes());

    let error = config::set_category_name(
        &project,
        &key("task"),
        "Renamed",
        &options().with_expected_config_fingerprint(original.clone()),
    )
    .expect_err("the caller's expectation no longer holds");
    assert_eq!(error.code(), Code::WriteConflict);
    assert_eq!(
        error.details()["expected_fingerprint"],
        serde_json::json!(original.as_str())
    );
    assert_eq!(
        error.details()["actual_fingerprint"],
        serde_json::json!(edited_fingerprint.as_str())
    );
    assert_eq!(
        error.details()["path"],
        serde_json::json!(project.config_path())
    );
    assert_eq!(read_config(&project), edited, "the conflict writes nothing");

    // The current fingerprint is accepted: the option guards a concurrent edit,
    // it does not forbid writing at all.
    let written = config::set_category_name(
        &project,
        &key("task"),
        "Renamed",
        &options().with_expected_config_fingerprint(edited_fingerprint),
    )
    .expect("a current fingerprint writes");
    assert!(written.changed);
    assert!(read_config(&project).contains("name = \"Renamed\""));
}

#[test]
fn a_category_can_be_added_renamed_and_removed() {
    let (_dir, project) = fixtures::deterministic_project();
    let added = config::add_category(&project, &key("bug"), "work/bugs", None, &options())
        .expect("it adds");
    assert!(added.changed);
    assert_eq!(added.config.categories[&key("bug")].name, "Bug");
    assert_eq!(added.config.categories[&key("bug")].path, "work/bugs");
    assert_eq!(added.revision, 1);
    assert!(
        project.root().join("work/bugs").is_dir(),
        "adding a category also creates its directory"
    );

    let renamed =
        config::set_category_name(&project, &key("bug"), "Bugs", &options()).expect("it renames");
    assert_eq!(renamed.config.categories[&key("bug")].name, "Bugs");
    assert_eq!(renamed.revision, 2);

    let removed = config::remove_category(&project, &key("bug"), &options()).expect("it removes");
    assert!(!removed.config.categories.contains_key(&key("bug")));
    assert_eq!(removed.revision, 3);
}

#[cfg(unix)]
#[test]
fn a_failed_configuration_write_rolls_back_the_category_directory() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, project) = fixtures::deterministic_project();
    // Make sure the lock file and the ignore entry already exist: only the
    // configuration replacement may fail, not lock acquisition.
    config::set_category_name(&project, &key("task"), "Renamed", &options())
        .expect("the shared write succeeds");
    let config_before = read_config(&project);

    let metadata_dir = project.root().join(".waymark");
    let original = std::fs::metadata(&metadata_dir)
        .expect("the metadata directory exists")
        .permissions();
    let mut read_only = original.clone();
    read_only.set_mode(0o555);
    std::fs::set_permissions(&metadata_dir, read_only).expect("the directory is made read-only");

    let error = config::add_category(&project, &key("bug"), "work/bugs", None, &options())
        .expect_err("the configuration cannot be replaced");

    std::fs::set_permissions(&metadata_dir, original).expect("the directory is writable again");

    assert_eq!(error.code(), Code::IoError);
    assert!(
        !project.root().join("work/bugs").exists(),
        "the directory created before the failed replacement is rolled back"
    );
    assert_eq!(
        read_config(&project),
        config_before,
        "the config is untouched"
    );
}

#[test]
fn adding_an_existing_category_is_refused_rather_than_repointed() {
    let (_dir, project) = fixtures::deterministic_project();
    let error =
        config::add_category(&project, &key("task"), "work/other", None, &options()).unwrap_err();
    assert_eq!(error.code(), Code::ValidationError);
}

#[test]
fn an_invalid_category_path_is_refused_by_validation() {
    let (_dir, project) = fixtures::deterministic_project();
    for path in ["/absolute/bugs", "../bugs", "work/task/child", "work\\bugs"] {
        let error = config::add_category(&project, &key("bug"), path, None, &options())
            .expect_err("the path is refused");
        assert_eq!(error.code(), Code::ConfigInvalid, "{path}: {error}");
    }
}

#[test]
fn an_unused_category_is_removable_and_a_used_one_is_refused_without_touching_its_items() {
    let (_dir, project) = fixtures::deterministic_project();
    config::add_category(&project, &key("bug"), "work/bugs", None, &options()).unwrap();
    let project = Project::open(project.root()).expect("the project reopens");
    let item_path = create_one(&project, "bug");
    let before = item_state(&project);
    let config_before = read_config(&project);

    let error = config::remove_category(&project, &key("bug"), &options())
        .expect_err("the category is in use");
    assert_eq!(error.code(), Code::ValidationError);
    let id = item_path.file_stem().unwrap_or_default();
    assert!(error.message().contains(id), "{error}");
    assert!(error.details()["issues"]
        .as_array()
        .expect("a per-issue list")
        .iter()
        .any(|issue| issue["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains(item_path.as_str()))));

    assert_eq!(item_state(&project), before, "no item may change");
    assert_eq!(
        read_config(&project),
        config_before,
        "no config write either"
    );

    // Removing the declaration is only possible once no item uses it.
    std::fs::remove_file(&item_path).unwrap();
    config::remove_category(&project, &key("bug"), &options()).expect("now it is removable");
}

#[test]
fn adding_an_optional_field_rewrites_no_item_and_leaves_it_unset() {
    let (_dir, project) = fixtures::deterministic_project();
    let _item = create_one(&project, "task");
    let before = item_state(&project);
    let config_before = read_config(&project);

    let outcome = config::add_field(
        &project,
        &field("effort"),
        FieldType::Integer,
        false,
        Some("3"),
        &options(),
    )
    .expect("it declares");
    assert!(outcome.changed);
    assert_eq!(
        outcome.config.fields[&field("effort")].default,
        Some(FieldValue::Integer(3))
    );

    assert_eq!(
        item_state(&project),
        before,
        "an optional field rewrites nothing"
    );
    assert_ne!(read_config(&project), config_before);
}

#[test]
fn a_required_field_addition_is_migration_territory() {
    let (_dir, project) = fixtures::deterministic_project();
    let error = config::add_field(
        &project,
        &field("effort"),
        FieldType::Integer,
        true,
        Some("3"),
        &options(),
    )
    .unwrap_err();
    assert_eq!(error.code(), Code::ActionRequiresMigration);
}

#[test]
fn a_default_must_match_its_declared_type() {
    let (_dir, project) = fixtures::deterministic_project();
    let error = config::add_field(
        &project,
        &field("effort"),
        FieldType::Integer,
        false,
        Some("three"),
        &options(),
    )
    .unwrap_err();
    assert_eq!(error.code(), Code::ValidationError);
    assert!(config::show(&project).unwrap().fields.is_empty());
}

#[test]
fn a_default_can_be_changed_and_cleared() {
    let (_dir, project) = fixtures::deterministic_project();
    config::add_field(
        &project,
        &field("effort"),
        FieldType::Integer,
        false,
        None,
        &options(),
    )
    .unwrap();
    let set = config::set_field(
        &project,
        &field("effort"),
        FieldAttribute::Default,
        "8",
        &options(),
    )
    .expect("it writes");
    assert_eq!(
        set.config.fields[&field("effort")].default,
        Some(FieldValue::Integer(8))
    );

    let cleared = config::set_field(
        &project,
        &field("effort"),
        FieldAttribute::Default,
        "null",
        &options(),
    )
    .expect("it clears");
    assert_eq!(cleared.config.fields[&field("effort")].default, None);

    let error = config::set_field(
        &project,
        &field("effort"),
        FieldAttribute::Default,
        "eight",
        &options(),
    )
    .unwrap_err();
    assert_eq!(error.code(), Code::ValidationError);
}

#[test]
fn relaxing_required_is_allowed_and_tightening_it_is_a_migration() {
    let (_dir, project) = fixtures::deterministic_project();
    let mut declared = config::show(&project).unwrap();
    declared.fields.insert(
        field("effort"),
        waymark_core::config::FieldDecl {
            ty: FieldType::Integer,
            required: true,
            default: None,
        },
    );
    std::fs::write(project.config_path(), declared.to_toml_string()).unwrap();

    let relaxed = config::set_field(
        &project,
        &field("effort"),
        FieldAttribute::Required,
        "false",
        &options(),
    )
    .expect("relaxing is a configuration write");
    assert!(!relaxed.config.fields[&field("effort")].required);

    let error = config::set_field(
        &project,
        &field("effort"),
        FieldAttribute::Required,
        "true",
        &options(),
    )
    .unwrap_err();
    assert_eq!(error.code(), Code::ActionRequiresMigration);
}

#[test]
fn a_type_change_is_a_migration_and_changes_nothing() {
    let (_dir, project) = fixtures::deterministic_project();
    config::add_field(
        &project,
        &field("effort"),
        FieldType::Integer,
        false,
        None,
        &options(),
    )
    .unwrap();
    let before = read_config(&project);
    let error = config::set_field(
        &project,
        &field("effort"),
        FieldAttribute::Type,
        "string",
        &options(),
    )
    .unwrap_err();
    assert_eq!(error.code(), Code::ActionRequiresMigration);
    assert_eq!(read_config(&project), before);
}

#[test]
fn an_unknown_field_key_is_refused_not_created() {
    let (_dir, project) = fixtures::deterministic_project();
    let error = config::set_field(
        &project,
        &field("missing"),
        FieldAttribute::Default,
        "1",
        &options(),
    )
    .unwrap_err();
    assert_eq!(error.code(), Code::ValidationError);
}

#[test]
fn every_configuration_write_leaves_work_item_files_byte_identical() {
    let (_dir, project) = fixtures::deterministic_project();
    let _first = create_one(&project, "task");
    config::add_category(&project, &key("bug"), "work/bugs", Some("Bugs"), &options()).unwrap();
    let project = Project::open(project.root()).unwrap();
    let _second = create_one(&project, "bug");
    let before = item_state(&project);

    let scalar = |project: &Project, scalar, value: &str| {
        waymark_core::ops::local_config::set_scalar(project, scalar, value, &options())
            .expect("the local scalar writes")
    };
    scalar(&project, Scalar::Operator, "frank");
    scalar(&project, Scalar::Timezone, "Asia/Shanghai");
    scalar(&project, Scalar::CacheCapacity, "128");
    config::set_category_name(&project, &key("bug"), "Defects", &options()).unwrap();
    config::add_field(
        &project,
        &field("effort"),
        FieldType::Integer,
        false,
        Some("1"),
        &options(),
    )
    .unwrap();
    config::set_field(
        &project,
        &field("effort"),
        FieldAttribute::Default,
        "2",
        &options(),
    )
    .unwrap();
    config::set_field(
        &project,
        &field("effort"),
        FieldAttribute::Required,
        "false",
        &options(),
    )
    .unwrap();

    assert_eq!(
        item_state(&project),
        before,
        "no configuration write touches an item"
    );
}

#[test]
fn a_hand_edited_document_is_seen_inside_the_lock_by_content() {
    let (_dir, project) = fixtures::deterministic_project();
    // A hand edit that changes the bytes but leaves the counter alone: the
    // content is what the engine re-reads inside the lock.
    let text = read_config(&project).replace("name = \"Task\"", "name = \"Hand\"");
    std::fs::write(project.config_path(), &text).unwrap();

    let outcome = config::set_category_name(&project, &key("task"), "Renamed", &options())
        .expect("it writes");
    assert_eq!(outcome.config.categories[&key("task")].name, "Renamed");
    assert_eq!(
        outcome.previous_revision, 0,
        "a hand edit does not move the counter"
    );
    assert_eq!(outcome.revision, 1);
}

#[test]
fn a_dry_run_reports_the_change_without_writing() {
    let (_dir, project) = fixtures::deterministic_project();
    let before = read_config(&project);
    let outcome =
        config::set_category_name(&project, &key("task"), "Renamed", &options().dry_run()).unwrap();
    assert!(outcome.changed);
    assert_eq!(outcome.revision, 1);
    assert_eq!(read_config(&project), before);
}

#[test]
fn validation_reports_one_located_finding_per_issue() {
    let (_dir, project) = fixtures::deterministic_project();
    let mut document = config::show(&project).unwrap();
    document.categories.get_mut(&key("task")).unwrap().name = "  ".to_owned();
    document.fields.insert(
        field("effort"),
        waymark_core::config::FieldDecl {
            ty: FieldType::String,
            required: false,
            default: Some(FieldValue::Integer(3)),
        },
    );
    std::fs::write(project.config_path(), document.to_toml_string()).unwrap();

    let findings = config::validate_config(&project);
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert!(findings
        .iter()
        .all(|finding| finding.code == Code::ConfigInvalid));
    let keys: Vec<String> = findings
        .iter()
        .filter_map(|finding| match &finding.location {
            Some(waymark_core::Location::ConfigKey { key }) => Some(key.clone()),
            _ => None,
        })
        .collect();
    assert!(
        keys.contains(&"categories.task.name".to_owned()),
        "{keys:?}"
    );
    assert!(
        keys.contains(&"fields.effort.default".to_owned()),
        "{keys:?}"
    );
}

#[test]
fn an_unreadable_document_is_one_finding_not_a_panic() {
    let (_dir, project) = fixtures::deterministic_project();
    std::fs::write(project.config_path(), "this is not toml = = =").unwrap();
    let findings = config::validate_config(&project);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, Code::ConfigInvalid);
}

#[test]
fn the_show_path_is_the_configuration_file() {
    let (_dir, project) = fixtures::deterministic_project();
    let expected = project.metadata_dir().join(CONFIG_FILE);
    assert_eq!(project.config_path(), expected);
    assert_eq!(Utf8Path::new(&project.config_path()), expected.as_path());
}

/// Write `document` as the project configuration and report the findings.
fn findings_for(project: &Project, document: &str) -> Vec<String> {
    std::fs::write(project.config_path(), document).expect("the document is writable");
    config::validate_config_path(&project.config_path())
        .into_iter()
        .map(|finding| finding.message)
        .collect()
}

#[test]
fn every_configuration_defect_is_a_located_config_invalid_finding() {
    let (_dir, project) = fixtures::deterministic_project();
    let canonical = config::show(&project).unwrap().to_toml_string();

    // Each case is a minimal edit that must produce at least one finding whose
    // message names the offending key.
    let cases: Vec<(&str, String, &str)> = vec![
        (
            "key grammar",
            canonical.replace("[categories.task]", "[categories.Task]"),
            "categories.Task",
        ),
        (
            "field key grammar",
            format!("{canonical}\n[fields.\"Effort!\"]\ntype = \"string\"\nrequired = false\n"),
            "fields.Effort!",
        ),
        (
            "absolute path",
            canonical.replace("path = \"work/task\"", "path = \"/tmp/task\""),
            "categories.task.path",
        ),
        (
            "escaping path",
            canonical.replace("path = \"work/task\"", "path = \"../task\""),
            "categories.task.path",
        ),
        (
            "empty display name",
            canonical.replace("name = \"Task\"", "name = \"\""),
            "categories.task.name",
        ),
        (
            "unknown field type",
            format!(
                "{canonical}\n[fields.effort]\ntype = \"decimal\"\nrequired = false\n"
            ),
            "fields.effort.type",
        ),
        (
            "default of the wrong type",
            format!(
                "{canonical}\n[fields.effort]\ntype = \"integer\"\nrequired = false\ndefault = \"three\"\n"
            ),
            "fields.effort.default",
        ),
    ];
    for (label, document, expected) in cases {
        let findings = findings_for(&project, &document);
        assert!(
            findings.iter().any(|message| message.contains(expected)),
            "{label}: expected a finding naming {expected}, got {findings:?}"
        );
    }
}

#[test]
fn overlapping_category_paths_are_a_finding() {
    let (_dir, project) = fixtures::deterministic_project();
    let canonical = config::show(&project).unwrap().to_toml_string();
    let document =
        format!("{canonical}\n[categories.bug]\npath = \"work/task/bugs\"\nname = \"Bugs\"\n");
    let findings = findings_for(&project, &document);
    assert!(
        findings.iter().any(|message| message.contains("overlap")),
        "{findings:?}"
    );
}

// A symlink is a Unix-only fixture: on Windows the equivalent
// reparse-point behaviour needs the platform effort that owns it.
#[cfg(unix)]
#[test]
fn a_symlinked_category_path_is_a_finding() {
    let (_dir, project) = fixtures::deterministic_project();
    let canonical = config::show(&project).unwrap().to_toml_string();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), project.root().join("linked")).unwrap();
    let document =
        format!("{canonical}\n[categories.bug]\npath = \"linked/bugs\"\nname = \"Bugs\"\n");
    let findings = findings_for(&project, &document);
    assert!(
        findings
            .iter()
            .any(|message| message.contains("symbolic link")),
        "{findings:?}"
    );
}

#[test]
fn an_invalid_configuration_cannot_be_opened_as_a_project() {
    let (_dir, project) = fixtures::deterministic_project();
    std::fs::write(
        project.config_path(),
        config::show(&project)
            .unwrap()
            .to_toml_string()
            .replace("path = \"work/task\"", "path = \"/tmp/elsewhere\""),
    )
    .unwrap();
    let error = Project::open(project.root()).expect_err("the document is invalid");
    assert_eq!(error.code(), Code::ConfigInvalid);
    // ... and the finding-oriented path still points at the key.
    let findings = config::validate_config_path(&project.config_path());
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].message.contains("categories.task.path"));
}

/// `init` builds the domain configuration in core: the adapter supplies the
/// request, not the category records or the paths.
#[test]
fn init_builds_the_requested_configuration_in_core() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 path");

    let project = config::init(
        &root,
        InitRequest::new()
            .with_storage_root("docs/work")
            .with_category("bug")
            .with_operator("frank")
            .with_timezone("Asia/Shanghai"),
        false,
    )
    .expect("init succeeds");

    let config = project.config();
    assert_eq!(config.storage_root, "docs/work");
    assert_eq!(config.categories.len(), 1);
    // The seeds are local, never shared.
    let local = waymark_core::ops::local_config::load(&project)
        .expect("the local document is readable")
        .expect("the local document exists");
    assert_eq!(local.operator.as_deref(), Some("frank"));
    assert_eq!(
        local.timezone.map(|zone| zone.name()),
        Some("Asia/Shanghai")
    );
    let bug = config.categories.get(&key("bug")).expect("bug is declared");
    assert_eq!(bug.path, "docs/work/bug");
    assert_eq!(bug.name, "Bug");
    assert!(root.join("docs/work/bug").is_dir());

    // An empty request uses the domain defaults, not an adapter's copy.
    let default_root = root.join("default");
    let project =
        config::init(&default_root, InitRequest::new(), false).expect("the default init succeeds");
    let config = project.config();
    assert_eq!(config.storage_root, "work");
    assert_eq!(config.categories.len(), 1);
    let task = config
        .categories
        .get(&key("task"))
        .expect("task is declared");
    assert_eq!(task.path, "work/task");
    assert!(default_root.join("work/task").is_dir());
}
