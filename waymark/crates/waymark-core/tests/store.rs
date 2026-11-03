//! The project store: `init`, discovery, path mapping, the inventory, and the
//! content fingerprint.

use camino::{Utf8Path, Utf8PathBuf};
use waymark_core::config::CONFIG_FILE;
use waymark_core::error::Code;
use waymark_core::model::CategoryKey;
use waymark_core::store::InitOptions;
use waymark_core::testing::fixtures;
use waymark_core::{Config, Project};

fn utf8(dir: &tempfile::TempDir) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 temporary path")
}

#[test]
fn init_creates_the_configuration_the_ignore_file_and_the_category_directory() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(&dir);
    let project = Project::init(&root, InitOptions::default()).expect("init succeeds");

    assert_eq!(project.root(), root);
    assert_eq!(project.metadata_dir(), root.join(".waymark"));
    assert_eq!(project.config_path(), root.join(".waymark/config.toml"));
    assert_eq!(project.lock_path(), root.join(".waymark/write.lock"));

    let config_bytes = std::fs::read_to_string(root.join(".waymark").join(CONFIG_FILE)).unwrap();
    assert_eq!(config_bytes, Config::fresh().to_toml_string());
    assert_eq!(config_bytes, config_bytes.trim_end().to_owned() + "\n");

    let ignore = std::fs::read_to_string(root.join(".waymark").join(".gitignore")).unwrap();
    assert!(ignore.lines().any(|line| line == "write.lock"), "{ignore}");

    assert!(
        root.join("work/task").is_dir(),
        "the category directory exists"
    );
}

#[test]
fn init_is_safe_to_re_run() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(&dir);
    let first = Project::init(&root, InitOptions::default()).unwrap();
    let before = std::fs::read_to_string(first.config_path()).unwrap();
    let second = Project::init(&root, InitOptions::default()).expect("a second init succeeds");
    let after = std::fs::read_to_string(second.config_path()).unwrap();
    assert_eq!(before, after);
}

#[test]
fn init_records_the_named_operator() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(&dir);
    let project = Project::init(
        &root,
        InitOptions {
            operator: Some("frank".to_owned()),
            ..InitOptions::default()
        },
    )
    .unwrap();
    assert_eq!(project.config().operator.as_deref(), Some("frank"));
    let reopened = Project::open(&root).unwrap();
    assert_eq!(reopened.config().operator.as_deref(), Some("frank"));
}

#[test]
fn discover_walks_up_from_a_nested_directory() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(&dir);
    Project::init(&root, InitOptions::default()).unwrap();
    let nested = root.join("work/task/deeper");
    std::fs::create_dir_all(&nested).unwrap();

    let found = Project::discover(&nested).expect("the project is found above the cwd");
    assert_eq!(found.root(), root);
}

#[test]
fn discover_reports_the_directory_it_searched_from() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(&dir);
    let error = Project::discover(&root.join("some/deeper/path")).unwrap_err();
    assert_eq!(error.code(), Code::ProjectNotFound);
    assert!(error.message().contains("some/deeper/path"), "{error}");
}

#[test]
fn opening_a_directory_without_a_project_is_project_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let error = Project::open(&utf8(&dir)).unwrap_err();
    assert_eq!(error.code(), Code::ProjectNotFound);
}

#[test]
fn an_existing_project_is_loaded_rather_than_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(&dir);
    let mut config = Config::fresh();
    config.operator = Some("someone".to_owned());
    Project::init(
        &root,
        InitOptions {
            config: Some(config),
            ..InitOptions::default()
        },
    )
    .unwrap();

    // A bare `init` must not clobber the operator that is already recorded.
    let project = Project::init(&root, InitOptions::default()).unwrap();
    assert_eq!(project.config().operator.as_deref(), Some("someone"));
}

#[test]
fn init_with_force_replaces_the_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(&dir);
    Project::init(&root, InitOptions::default()).unwrap();
    let project = Project::init(
        &root,
        InitOptions {
            force: true,
            operator: Some("someone".to_owned()),
            ..InitOptions::default()
        },
    )
    .unwrap();
    assert_eq!(project.config().operator.as_deref(), Some("someone"));
}

#[test]
fn a_document_path_is_derived_from_the_id_alone() {
    let (_dir, project) = fixtures::project();
    let id = waymark_core::WorkItemId::parse("wm-3f7k9q2m4x8c").unwrap();
    let category = CategoryKey::parse("task").unwrap();
    let path = project.path_for(&id, &category).unwrap();
    assert!(path.ends_with("work/task/wm-3f7k9q2m4x8c.md"));
}

#[test]
fn an_undeclared_category_is_a_config_invalid_error() {
    let (_dir, project) = fixtures::project();
    let id = waymark_core::WorkItemId::parse("wm-3f7k9q2m4x8c").unwrap();
    let category = CategoryKey::parse("bug").unwrap();
    let error = project.path_for(&id, &category).unwrap_err();
    assert_eq!(error.code(), Code::ConfigInvalid);
    assert!(error.message().contains("bug"), "{error}");
}

#[test]
fn the_inventory_lists_documents_and_reports_the_files_it_skips() {
    let (_dir, project) = fixtures::project();
    let category = CategoryKey::parse("task").unwrap();
    let directory = project.category_dir(&category).unwrap();

    let id = waymark_core::WorkItemId::parse("wm-3f7k9q2m4x8c").unwrap();
    waymark_core::serialize::write_item_file(
        &fixtures::minimal_item("wm-3f7k9q2m4x8c"),
        &directory.join(id.file_name()),
    )
    .unwrap();
    std::fs::write(directory.join("notes.txt"), b"not an item").unwrap();
    std::fs::write(directory.join("wm-short.md"), b"not an id").unwrap();
    std::fs::write(directory.join("WM-3F7K9Q2M4X8C.md"), b"uppercase stem").unwrap();

    let inventory = project.inventory().unwrap();
    assert_eq!(inventory.entries.len(), 1);
    assert_eq!(inventory.entries[0].id, id);
    assert_eq!(inventory.entries[0].category, category);
    let skipped: Vec<String> = inventory
        .problems
        .iter()
        .map(|problem| problem.path.file_name().unwrap().to_owned())
        .collect();
    assert!(
        skipped.contains(&"WM-3F7K9Q2M4X8C.md".to_owned()),
        "{skipped:?}"
    );
    assert!(skipped.contains(&"wm-short.md".to_owned()), "{skipped:?}");
    assert!(
        !skipped.iter().any(|name| name == "notes.txt"),
        "{skipped:?}"
    );
}

#[test]
fn a_missing_category_directory_reads_as_empty() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(&dir);
    let project = Project::init(&root, InitOptions::default()).unwrap();
    std::fs::remove_dir_all(root.join("work/task")).unwrap();
    let inventory = project.inventory().unwrap();
    assert!(inventory.entries.is_empty());
}

#[test]
fn the_fingerprint_is_the_sha256_of_the_bytes() {
    use waymark_core::store::Fingerprint;
    // The empty-input digest is a fixed, well-known value: a fingerprint that
    // silently changed algorithm would invalidate every persisted cache.
    assert_eq!(
        Fingerprint::of(b"").as_str(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        Fingerprint::of(b"abc").as_str(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(Fingerprint::of(b"abc"), Fingerprint::of(b"abc"));
    assert_ne!(Fingerprint::of(b"abc"), Fingerprint::of(b"abd"));
}

#[test]
fn the_project_fingerprint_matches_the_file_bytes() {
    let (_dir, project) = fixtures::project();
    let category = CategoryKey::parse("task").unwrap();
    let directory = project.category_dir(&category).unwrap();
    let path = directory.join("wm-3f7k9q2m4x8c.md");
    std::fs::write(&path, b"some bytes").unwrap();
    assert_eq!(
        project.fingerprint(&path).unwrap().as_str(),
        waymark_core::store::Fingerprint::of(b"some bytes").as_str()
    );
}

#[test]
fn locating_an_absent_id_is_item_not_found() {
    let (_dir, project) = fixtures::project();
    let id = waymark_core::WorkItemId::parse("wm-3f7k9q2m4x8c").unwrap();
    let error = project.locate(&id).unwrap_err();
    assert_eq!(error.code(), Code::ItemNotFound);
}

#[test]
fn two_files_claiming_one_id_are_reported_rather_than_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(&dir);
    let mut config = Config::fresh();
    let other = CategoryKey::parse("bug").unwrap();
    config.categories.insert(
        other.clone(),
        waymark_core::config::Category {
            path: "work/bug".to_owned(),
            name: "Bugs".to_owned(),
        },
    );
    let project = Project::init(
        &root,
        InitOptions {
            config: Some(config),
            ..InitOptions::default()
        },
    )
    .unwrap();

    let id = waymark_core::WorkItemId::parse("wm-3f7k9q2m4x8c").unwrap();
    for category in [CategoryKey::parse("task").unwrap(), other] {
        let directory = project.category_dir(&category).unwrap();
        std::fs::write(directory.join(id.file_name()), b"---\n").unwrap();
    }

    let error = project.locate(&id).unwrap_err();
    assert_eq!(error.code(), Code::ValidationError);
    assert!(error.message().contains("wm-3f7k9q2m4x8c"), "{error}");
}

#[test]
fn the_configured_storage_root_is_honoured() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(&dir);
    let mut config = Config::fresh();
    config.storage_root = "items".to_owned();
    config.categories.insert(
        CategoryKey::parse("task").unwrap(),
        waymark_core::config::Category {
            path: "items/tasks".to_owned(),
            name: "Tasks".to_owned(),
        },
    );
    let project = Project::init(
        &root,
        InitOptions {
            config: Some(config),
            ..InitOptions::default()
        },
    )
    .unwrap();
    assert!(root.join("items/tasks").is_dir());
    let id = waymark_core::WorkItemId::parse("wm-3f7k9q2m4x8c").unwrap();
    assert!(project
        .path_for(&id, &CategoryKey::parse("task").unwrap())
        .unwrap()
        .ends_with("items/tasks/wm-3f7k9q2m4x8c.md"));
}

#[test]
fn a_path_maps_back_to_its_category() {
    let (_dir, project) = fixtures::project();
    let id = waymark_core::WorkItemId::parse("wm-3f7k9q2m4x8c").unwrap();
    let category = CategoryKey::parse("task").unwrap();
    let path = project.path_for(&id, &category).unwrap();
    assert_eq!(project.category_for_path(&path), Some(category));
    assert_eq!(
        project.category_for_path(Utf8Path::new("/elsewhere/item.md")),
        None
    );
}

#[test]
fn an_id_is_allocated_against_the_inventory_and_never_collides() {
    use waymark_core::testing::SequentialIds;
    let (_dir, project) = fixtures::project();
    let project = project.with_id_source(Box::new(SequentialIds::new()));
    let inventory = project.inventory().unwrap();
    let first = project.allocate_id(&inventory).unwrap();
    let second = project.allocate_id(&inventory).unwrap();
    assert_ne!(first, second);
    assert_eq!(first.as_str(), "wm-000000000000");
    assert_eq!(second.as_str(), "wm-000000000001");
}

#[test]
fn a_constant_id_source_exhausts_its_budget_rather_than_looping_forever() {
    use waymark_core::testing::ConstantIds;
    let (_dir, project) = fixtures::project();
    let project = project.with_id_source(Box::new(ConstantIds::new("000000000000")));

    let category = CategoryKey::parse("task").unwrap();
    let directory = project.category_dir(&category).unwrap();
    std::fs::write(directory.join("wm-000000000000.md"), b"taken").unwrap();

    let error = project
        .allocate_id(&project.inventory().unwrap())
        .unwrap_err();
    assert_eq!(error.code(), Code::IdGenerationFailed);
}

#[test]
fn a_symlinked_metadata_gitignore_is_not_written_through() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(&dir);
    // Build `.waymark/` by hand so the ignore file can be a symlink before init
    // gets a chance to create it.
    std::fs::create_dir_all(root.join(".waymark")).unwrap();
    let outside = dir.path().join("outside-gitignore");
    std::fs::write(&outside, b"keep me\n").unwrap();
    std::os::unix::fs::symlink(&outside, root.join(".waymark/.gitignore")).unwrap();

    let error = Project::init(&root, InitOptions::default()).unwrap_err();
    assert_eq!(error.code(), Code::LockFailed);
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "keep me\n");
}

#[test]
fn an_existing_gitignore_entry_is_not_duplicated() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(&dir);
    std::fs::create_dir_all(root.join(".waymark")).unwrap();
    std::fs::write(root.join(".waymark/.gitignore"), "cache\nwrite.lock\n").unwrap();
    Project::init(&root, InitOptions::default()).unwrap();
    let ignore = std::fs::read_to_string(root.join(".waymark/.gitignore")).unwrap();
    assert_eq!(ignore.matches("write.lock").count(), 1, "{ignore}");
    assert!(ignore.contains("cache"), "{ignore}");
}
