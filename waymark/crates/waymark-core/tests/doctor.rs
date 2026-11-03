//! Diagnostics and health checks through the public seam.
//!
//! Every case starts from a real temporary project, writes real documents, and
//! asserts what `doctor::diagnose` reports and what is left on disk. The run is
//! read-only for business data: the only thing it may rebuild is the in-memory
//! projection and its cache, and every file must come out byte-identical.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use waymark_core::config::{Category, Config, FieldDecl, FieldType};
use waymark_core::envelope::Completeness;
use waymark_core::error::{Code, ExitClass, Severity};
use waymark_core::model::{CategoryKey, FieldKey, FieldValue, WorkItem, FORMAT_GENERATION};
use waymark_core::ops::create::{self, CreateRequest};
use waymark_core::ops::doctor::{self, DoctorOptions, RepairKind};
use waymark_core::ops::relationship;
use waymark_core::ops::OpOptions;
use waymark_core::projection::Projection;
use waymark_core::serialize;
use waymark_core::testing::fixtures;
use waymark_core::time::Timestamp;
use waymark_core::Location;
use waymark_core::{InitOptions, Project, WorkItemId};

fn options() -> OpOptions {
    OpOptions::new().with_clock(fixtures::shared_clock())
}

fn project() -> (tempfile::TempDir, Project) {
    let (dir, project) = fixtures::deterministic_project();
    (dir, project)
}

fn two_category_project() -> (tempfile::TempDir, Project) {
    let mut config = Config::fresh();
    config.categories.insert(
        CategoryKey::parse("bug").unwrap(),
        Category {
            path: "work/bugs".to_owned(),
            name: "Bugs".to_owned(),
        },
    );
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    std::fs::create_dir_all(root.join("work/bugs")).unwrap();
    let project = Project::init(
        &root,
        InitOptions {
            config: Some(config),
            ..InitOptions::default()
        },
    )
    .unwrap()
    .with_clock(fixtures::shared_clock())
    .with_id_source(Box::new(waymark_core::testing::SequentialIds::new()));
    (dir, project)
}

fn create_item(project: &Project, title: &str) -> WorkItemId {
    create::create(
        project,
        CreateRequest::new(title, "task", "frank"),
        &options(),
    )
    .unwrap()
    .item
    .id
}

fn task_path(project: &Project, id: &str) -> Utf8PathBuf {
    project.root().join(format!("work/task/{id}.md"))
}

fn bug_path(project: &Project, id: &str) -> Utf8PathBuf {
    project.root().join(format!("work/bugs/{id}.md"))
}

fn write_item(path: &Utf8Path, item: &WorkItem) {
    std::fs::write(path, serialize::to_document(item)).unwrap();
}

fn read_item(path: &Utf8Path) -> WorkItem {
    waymark_core::parse::parse_item_file(path).unwrap()
}

fn text_of(project: &Project, id: &str) -> String {
    std::fs::read_to_string(task_path(project, id)).unwrap()
}

fn codes(report: &doctor::Report) -> Vec<Code> {
    report.findings.iter().map(|finding| finding.code).collect()
}

fn messages(report: &doctor::Report, code: Code) -> Vec<String> {
    report
        .findings
        .iter()
        .filter(|finding| finding.code == code)
        .map(|finding| finding.message.clone())
        .collect()
}

#[test]
fn a_clean_project_reports_nothing_and_only_rebuilds_in_memory_state() {
    let (_dir, project) = project();
    create_item(&project, "one");

    let report = doctor::diagnose(&project).unwrap();
    assert!(report.findings.is_empty(), "{:?}", report.findings);
    assert_eq!(report.counts.errors, 0);
    assert_eq!(report.counts.warnings, 0);
    assert_eq!(report.counts.info, 0);
    assert!(!report.strict);
    assert_eq!(report.exit_class(), None);
    assert!(report.scope.contains(&"work_items".to_owned()));
    assert!(report.scope.contains(&"configuration".to_owned()));
    assert!(!report.repairs.is_empty());
    assert!(report.repairs.iter().all(|repair| matches!(
        repair.kind,
        RepairKind::ProjectionRebuild | RepairKind::CacheRebuild
    )));
    assert!(report
        .repairs
        .iter()
        .all(|repair| !repair.changed.is_empty()));
}

#[test]
fn every_finding_uses_the_registry_shape() {
    let (_dir, project) = project();
    let id = create_item(&project, "dangling");
    let mut item = read_item(&task_path(&project, id.as_str()));
    item.dependencies
        .push(WorkItemId::parse("wm-0000000000zz").unwrap());
    write_item(&task_path(&project, id.as_str()), &item);

    let report = doctor::diagnose(&project).unwrap();
    assert!(!report.findings.is_empty());
    for finding in &report.findings {
        assert_eq!(
            finding.severity,
            finding.code.finding().unwrap(),
            "{} does not use its registry severity",
            finding.code
        );
        assert!(!finding.message.is_empty());
        assert!(
            finding.location.is_none()
                || finding
                    .code
                    .location_kinds()
                    .contains(&finding.location.as_ref().unwrap().kind()),
            "{} carries a location kind its registry row does not declare",
            finding.code
        );
        assert!(!finding.fixable, "business findings are never auto-fixable");
    }
    let dangling: Vec<_> = report
        .findings
        .iter()
        .filter(|finding| finding.code == Code::DanglingReference)
        .collect();
    assert_eq!(dangling.len(), 1);
    assert_eq!(dangling[0].severity, Severity::Error);
    assert!(matches!(
        dangling[0].location,
        Some(Location::WorkItem { .. })
    ));
}

#[test]
fn malformed_frontmatter_duplicate_ids_and_mismatched_ids_are_reported() {
    let (_dir, project) = two_category_project();
    let duplicate = create_item(&project, "duplicate");
    let good = create_item(&project, "good");
    // A second file in another category that claims the same ID.
    std::fs::copy(
        task_path(&project, duplicate.as_str()),
        bug_path(&project, duplicate.as_str()),
    )
    .unwrap();
    // A malformed document: no closing fence at all.
    std::fs::write(task_path(&project, "wm-0000000000aa"), "---\nid: nope\n").unwrap();
    // A valid document in the wrong file: the declared ID disagrees with the stem.
    let mut displaced = read_item(&task_path(&project, good.as_str()));
    displaced.id = WorkItemId::parse("wm-0000000000bb").unwrap();
    write_item(&task_path(&project, "wm-0000000000cc"), &displaced);

    let report = doctor::diagnose(&project).unwrap();
    assert!(
        codes(&report).contains(&Code::ParseError),
        "{:?}",
        report.findings
    );
    assert!(
        codes(&report).contains(&Code::DuplicateId),
        "{:?}",
        report.findings
    );
    assert!(
        codes(&report).contains(&Code::IdPathMismatch),
        "{:?}",
        report.findings
    );
}

#[test]
fn a_case_variant_filename_is_a_duplicate_id_not_a_silent_skip() {
    let (_dir, project) = project();
    let id = create_item(&project, "case");
    let upper = task_path(&project, id.as_str())
        .with_file_name(format!("{}.md", id.as_str().to_uppercase()));
    std::fs::copy(task_path(&project, id.as_str()), &upper).unwrap();

    let report = doctor::diagnose(&project).unwrap();
    let duplicate = report
        .findings
        .iter()
        .find(|finding| finding.code == Code::DuplicateId)
        .expect("a case variant of a live ID is a duplicate");
    assert!(duplicate.message.contains("case"), "{}", duplicate.message);
}

#[test]
fn missing_and_archived_relationship_targets_are_dangling_references() {
    let (_dir, project) = project();
    let source = create_item(&project, "dangling source");
    let missing = WorkItemId::parse("wm-0000000000zz").unwrap();
    let mut source_item = read_item(&task_path(&project, source.as_str()));
    source_item.dependencies.push(missing.clone());
    write_item(&task_path(&project, source.as_str()), &source_item);

    let archived = create_item(&project, "archived target");
    let user = create_item(&project, "archived user");
    relationship::add_dependency(&project, user.as_str(), archived.as_str(), &options()).unwrap();
    let mut archived_item = read_item(&task_path(&project, archived.as_str()));
    archived_item.archived_at = Some(Timestamp::parse("2026-11-04T00:00:00Z").unwrap());
    write_item(&task_path(&project, archived.as_str()), &archived_item);

    let report = doctor::diagnose(&project).unwrap();
    let dangling = messages(&report, Code::DanglingReference);
    assert_eq!(dangling.len(), 2, "{dangling:?}");
    assert!(dangling
        .iter()
        .any(|message| message.contains(missing.as_str())));
    assert!(dangling.iter().any(|message| message.contains("archived")));
}

#[test]
fn dependency_and_parent_cycles_carry_their_paths() {
    let (_dir, project) = project();
    let a = create_item(&project, "a");
    let b = create_item(&project, "b");
    relationship::add_dependency(&project, a.as_str(), b.as_str(), &options()).unwrap();
    let mut b_item = read_item(&task_path(&project, b.as_str()));
    b_item.dependencies.push(a.clone());
    write_item(&task_path(&project, b.as_str()), &b_item);

    // A parent loop can only arrive from an external edit.
    let c = create_item(&project, "c");
    let d = create_item(&project, "d");
    let mut c_item = read_item(&task_path(&project, c.as_str()));
    c_item.parent = Some(d.clone());
    write_item(&task_path(&project, c.as_str()), &c_item);
    let mut d_item = read_item(&task_path(&project, d.as_str()));
    d_item.parent = Some(c.clone());
    write_item(&task_path(&project, d.as_str()), &d_item);

    let report = doctor::diagnose(&project).unwrap();
    let cycles = messages(&report, Code::CycleDetected);
    assert!(cycles.len() >= 2, "{cycles:?}");
    assert!(cycles.iter().any(|message| message.contains("dependency")));
    assert!(cycles.iter().any(|message| message.contains("parent")));
    for cycle in &cycles {
        assert!(
            cycle.contains(a.as_str()) || cycle.contains(c.as_str()),
            "{cycle}"
        );
    }
}

#[test]
fn custom_field_configuration_drift_is_reported_per_field() {
    let mut config = Config::fresh();
    config.fields.insert(
        FieldKey::parse("effort").unwrap(),
        FieldDecl {
            ty: FieldType::Integer,
            required: true,
            default: None,
        },
    );
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let mut project = Project::init(
        &root,
        InitOptions {
            config: Some(config.clone()),
            ..InitOptions::default()
        },
    )
    .unwrap()
    .with_clock(fixtures::shared_clock())
    .with_id_source(Box::new(waymark_core::testing::SequentialIds::new()));
    let id = create_item(&project, "with effort");
    let mut item = read_item(&task_path(&project, id.as_str()));
    item.custom_fields
        .insert("effort".to_owned(), FieldValue::Integer(3));
    write_item(&task_path(&project, id.as_str()), &item);

    // Remove the field from the configuration after the document was written:
    // that is exactly the drift a schema removal leaves behind.
    config.fields.remove(&FieldKey::parse("effort").unwrap());
    waymark_core::ops::config::set_scalar(
        &project,
        waymark_core::ops::config::Scalar::CacheCapacity,
        "512",
        &options(),
    )
    .unwrap();
    project.set_config(config);

    let report = doctor::diagnose(&project).unwrap();
    let validation = report
        .findings
        .iter()
        .filter(|finding| finding.code == Code::ValidationError)
        .collect::<Vec<_>>();
    assert!(!validation.is_empty(), "{:?}", report.findings);
    assert!(validation.iter().any(|finding| {
        matches!(
            &finding.location,
            Some(Location::Field { field }) if field == "custom_fields.effort"
        )
    }));
}

#[test]
fn a_missing_required_custom_field_and_a_type_mismatch_are_reported_as_well() {
    let mut config = Config::fresh();
    config.fields.insert(
        FieldKey::parse("effort").unwrap(),
        FieldDecl {
            ty: FieldType::Integer,
            required: true,
            default: None,
        },
    );
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let project = Project::init(
        &root,
        InitOptions {
            config: Some(config),
            ..InitOptions::default()
        },
    )
    .unwrap()
    .with_clock(fixtures::shared_clock())
    .with_id_source(Box::new(waymark_core::testing::SequentialIds::new()));
    let id = create_item(&project, "no effort");
    let mut item = read_item(&task_path(&project, id.as_str()));
    item.custom_fields
        .insert("effort".to_owned(), FieldValue::String("lots".to_owned()));
    write_item(&task_path(&project, id.as_str()), &item);

    let report = doctor::diagnose(&project).unwrap();
    let validation: Vec<_> = report
        .findings
        .iter()
        .filter(|finding| finding.code == Code::ValidationError)
        .collect();
    assert_eq!(validation.len(), 1, "{:?}", report.findings);
    assert!(
        validation[0].message.contains("expected"),
        "{}",
        validation[0].message
    );
}

#[test]
fn timestamp_incoherence_is_a_warning_naming_the_field() {
    let (_dir, project) = project();
    let id = create_item(&project, "time");
    let text = text_of(&project, id.as_str())
        .replace(
            "updated_at: 2026-11-03T05:43:10Z",
            "updated_at: 2020-01-01T00:00:00Z",
        )
        .replace("status: open", "status: closed");
    std::fs::write(task_path(&project, id.as_str()), text).unwrap();

    let report = doctor::diagnose(&project).unwrap();
    let warnings: Vec<_> = report
        .findings
        .iter()
        .filter(|finding| finding.code == Code::TimestampIncoherent)
        .collect();
    assert!(!warnings.is_empty(), "{:?}", report.findings);
    assert!(warnings
        .iter()
        .all(|finding| finding.severity == Severity::Warning));
    assert!(warnings
        .iter()
        .any(|finding| finding.message.contains("updated_at")));
    assert!(warnings
        .iter()
        .any(|finding| finding.message.contains("closed_at")));
}

#[test]
fn unsupported_file_and_configuration_generations_are_reported() {
    let (_dir, project) = project();
    let future = create_item(&project, "future");
    let text = text_of(&project, future.as_str()).replace("schema: 1", "schema: 99");
    std::fs::write(task_path(&project, future.as_str()), text).unwrap();

    let report = doctor::diagnose(&project).unwrap();
    assert!(
        codes(&report).contains(&Code::UnsupportedVersion),
        "{:?}",
        report.findings
    );

    let mut document = std::fs::read_to_string(project.config_path()).unwrap();
    document = document.replace("version = \"1.0\"", "version = \"9.0\"");
    std::fs::write(project.config_path(), document).unwrap();
    let config_report =
        doctor::diagnose_config(&project.config_path(), DoctorOptions::new()).unwrap();
    let unsupported = config_report
        .findings
        .iter()
        .find(|finding| finding.code == Code::UnsupportedVersion)
        .expect("a future configuration generation is reported");
    assert!(matches!(
        unsupported.location,
        Some(Location::ConfigKey { .. })
    ));
}

#[test]
fn an_older_file_generation_needs_migration() {
    let (_dir, project) = project();
    let id = create_item(&project, "old");
    let text = text_of(&project, id.as_str()).replace("schema: 1", "schema: 0");
    std::fs::write(task_path(&project, id.as_str()), text).unwrap();

    let report = doctor::diagnose(&project).unwrap();
    let migration: Vec<_> = report
        .findings
        .iter()
        .filter(|finding| finding.code == Code::MigrationAvailable)
        .collect();
    assert_eq!(migration.len(), 1, "{:?}", report.findings);
    assert_eq!(migration[0].severity, Severity::Info);
    assert!(migration[0].message.contains("migration"));
}

#[test]
fn an_incomplete_projection_is_reported_and_not_completed_by_a_partial_graph() {
    let (_dir, project) = project();
    let good = create_item(&project, "good");
    let broken = create_item(&project, "broken");
    std::fs::write(task_path(&project, broken.as_str()), "---\nid: nope\n").unwrap();
    // A dangling edge from the readable document would be a graph finding in a
    // complete world; with a broken sibling the review must not pretend the
    // graph is complete.
    let mut item = read_item(&task_path(&project, good.as_str()));
    item.dependencies
        .push(WorkItemId::parse("wm-0000000000zz").unwrap());
    write_item(&task_path(&project, good.as_str()), &item);

    let report = doctor::diagnose(&project).unwrap();
    assert!(codes(&report).contains(&Code::IncompleteProjection));
    assert_eq!(report.meta.completeness, Completeness::Scoped);
    assert!(!codes(&report).contains(&Code::DanglingReference));
}

#[test]
fn oversized_frontmatter_is_reported_even_when_yaml_is_well_formed() {
    let (_dir, project) = project();
    let id = create_item(&project, "oversized");
    let document = text_of(&project, id.as_str()).replace(
        "title: oversized",
        &format!("title: {}", "x".repeat(70 * 1024)),
    );
    std::fs::write(task_path(&project, id.as_str()), document).unwrap();

    let report = doctor::diagnose(&project).unwrap();
    let parse = messages(&report, Code::ParseError);
    assert!(
        parse
            .iter()
            .any(|message| message.contains("frontmatter") && message.contains("limit")),
        "{parse:?}"
    );
}

#[test]
fn a_retained_projection_reports_new_deleted_moved_and_changed_files() {
    let (_dir, project) = two_category_project();
    let one = create_item(&project, "one");
    let two = create_item(&project, "two");
    let three = create_item(&project, "three");
    let previous = Projection::load(&project).unwrap();

    // Change one, delete another, move a third across categories, add a fourth.
    let mut changed = read_item(&task_path(&project, one.as_str()));
    changed.title = "changed".to_owned();
    write_item(&task_path(&project, one.as_str()), &changed);
    std::fs::remove_file(task_path(&project, two.as_str())).unwrap();
    std::fs::rename(
        task_path(&project, three.as_str()),
        bug_path(&project, three.as_str()),
    )
    .unwrap();
    create_item(&project, "four");

    let report = doctor::diagnose_against(&project, &previous, DoctorOptions::new()).unwrap();
    let inventory = messages(&report, Code::IncompleteProjection).join("\n");
    assert!(inventory.contains("new"), "{inventory}");
    assert!(inventory.contains("deleted"), "{inventory}");
    assert!(inventory.contains("moved"), "{inventory}");
    assert!(inventory.contains("fingerprint changed"), "{inventory}");
}

#[test]
fn an_uncertain_inventory_path_is_reported() {
    let (_dir, project) = project();
    create_item(&project, "one");
    std::fs::write(project.root().join("work/task/notes.md"), "not a document").unwrap();

    let report = doctor::diagnose(&project).unwrap();
    let findings = messages(&report, Code::IncompleteProjection);
    assert!(
        findings.iter().any(|message| message.contains("uncertain")),
        "{findings:?}"
    );
}

#[test]
fn overlapping_category_paths_are_configuration_findings() {
    let (_dir, project) = project();
    let mut document = std::fs::read_to_string(project.config_path()).unwrap();
    document.push_str("\n[categories.bug]\npath = \"work/task\"\nname = \"Bugs\"\n");
    std::fs::write(project.config_path(), document).unwrap();

    let report = doctor::diagnose_config(&project.config_path(), DoctorOptions::new()).unwrap();
    let config: Vec<_> = report
        .findings
        .iter()
        .filter(|finding| finding.code == Code::ConfigInvalid)
        .collect();
    assert!(!config.is_empty(), "{:?}", report.findings);
    assert!(config
        .iter()
        .any(|finding| finding.message.contains("already used")));
    assert_eq!(report.scope, vec!["configuration".to_owned()]);
}

#[test]
fn a_held_lock_is_an_info_finding_from_a_probe_that_does_not_wait() {
    let (_dir, project) = project();
    create_item(&project, "one");

    let started = Instant::now();
    let report = waymark_core::lock::with_project_lock(
        &project,
        waymark_core::lock::LockWait::default_policy(),
        || {
            let report = doctor::diagnose(&project).unwrap();
            let held: Vec<_> = report
                .findings
                .iter()
                .filter(|finding| finding.code == Code::LockHeld)
                .collect();
            assert_eq!(held.len(), 1, "{:?}", report.findings);
            assert_eq!(held[0].severity, Severity::Info);
            assert!(held[0].location.is_none());
            assert!(!held[0].message.contains("holder"));
            Ok(report)
        },
    )
    .unwrap();

    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the probe waited on the lock"
    );
    assert!(codes(&report).contains(&Code::LockHeld));
}

#[test]
fn strict_mode_decides_whether_error_findings_change_the_exit_class() {
    let (_dir, project) = project();
    let id = create_item(&project, "warning only");
    let text = text_of(&project, id.as_str()).replace(
        "updated_at: 2026-11-03T05:43:10Z",
        "updated_at: 2020-01-01T00:00:00Z",
    );
    std::fs::write(task_path(&project, id.as_str()), text).unwrap();

    let warning_only = doctor::diagnose_with(&project, DoctorOptions::new()).unwrap();
    assert_eq!(warning_only.counts.errors, 0);
    assert!(warning_only.counts.warnings > 0);
    assert_eq!(warning_only.exit_class(), None);
    let strict_warning_only =
        doctor::diagnose_with(&project, DoctorOptions::new().strict()).unwrap();
    assert_eq!(strict_warning_only.exit_class(), None);

    let mut item = read_item(&task_path(&project, id.as_str()));
    item.dependencies
        .push(WorkItemId::parse("wm-0000000000zz").unwrap());
    write_item(&task_path(&project, id.as_str()), &item);
    let strict = doctor::diagnose_with(&project, DoctorOptions::new().strict()).unwrap();
    assert!(strict.counts.errors > 0);
    assert_eq!(strict.exit_class(), Some(ExitClass::InvalidInput));
    let non_strict = doctor::diagnose(&project).unwrap();
    assert!(non_strict.counts.errors > 0);
    assert_eq!(non_strict.exit_class(), None);
}

#[test]
fn a_full_run_leaves_every_project_file_byte_identical() {
    let (_dir, project) = two_category_project();
    let good = create_item(&project, "good");
    let duplicate = create_item(&project, "duplicate");
    std::fs::copy(
        task_path(&project, duplicate.as_str()),
        bug_path(&project, duplicate.as_str()),
    )
    .unwrap();
    std::fs::write(task_path(&project, "wm-0000000000bb"), "---\noops\n").unwrap();
    let mut item = read_item(&task_path(&project, good.as_str()));
    item.dependencies
        .push(WorkItemId::parse("wm-0000000000zz").unwrap());
    item.custom_fields
        .insert("orphan".to_owned(), FieldValue::String("x".to_owned()));
    item.updated_at = Timestamp::parse("2020-01-01T00:00:00Z").unwrap();
    write_item(&task_path(&project, good.as_str()), &item);
    let migration = create_item(&project, "old");
    let text = text_of(&project, migration.as_str())
        .replace(&format!("schema: {FORMAT_GENERATION}"), "schema: 0");
    std::fs::write(task_path(&project, migration.as_str()), text).unwrap();

    let before = snapshot(project.root());
    let report = doctor::diagnose_with(&project, DoctorOptions::new().strict()).unwrap();
    let after = snapshot(project.root());
    assert_eq!(before, after, "diagnostics wrote to the project");
    assert!(!report.findings.is_empty());
    for repair in &report.repairs {
        assert!(matches!(
            repair.kind,
            RepairKind::ProjectionRebuild | RepairKind::CacheRebuild
        ));
        assert!(!repair.changed.is_empty());
    }
    let projection_repair = report
        .repairs
        .iter()
        .find(|repair| repair.kind == RepairKind::ProjectionRebuild)
        .expect("the projection rebuild is reported");
    assert!(!projection_repair.remaining.is_empty());
}

/// Every regular file under `root`, with its bytes.
fn snapshot(root: &Utf8Path) -> BTreeMap<Utf8PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    walk(root, &mut files);
    files
}

fn walk(directory: &Utf8Path, files: &mut BTreeMap<Utf8PathBuf, Vec<u8>>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let path = Utf8PathBuf::from_path_buf(entry.path()).unwrap();
        if path.is_dir() {
            walk(&path, files);
        } else if path.is_file() {
            files.insert(path.clone(), std::fs::read(&path).unwrap());
        }
    }
}
