//! The in-process projection: completeness, freshness, scoped reads, cache
//! rules, and snapshot stability.
//!
//! The projection is disposable: it is built from the documents, never written
//! back, and never consulted for authority. These tests drive the public seam —
//! [`waymark_core::projection`] and the read operations that sit on it.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use camino::Utf8PathBuf;
use waymark_core::config::CacheSettings;
use waymark_core::envelope::{Completeness, Freshness};
use waymark_core::error::Code;
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::parent::set_parent;
use waymark_core::ops::{create, read, OpOptions};
use waymark_core::projection::{CacheKey, LoadOptions, Projection};
use waymark_core::testing::fixtures;
use waymark_core::{Project, WorkItemId};

fn options() -> OpOptions {
    OpOptions::new().with_clock(fixtures::shared_clock())
}

fn project() -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let project = Project::init(&root, waymark_core::store::InitOptions::default())
        .unwrap()
        .with_clock(fixtures::shared_clock())
        .with_id_source(Box::new(waymark_core::testing::SequentialIds::new()));
    (dir, project)
}

fn make(project: &Project, title: &str) -> String {
    create::create(
        project,
        CreateRequest::new(title, "task", "frank"),
        &options(),
    )
    .unwrap()
    .item
    .id
    .as_str()
    .to_owned()
}

fn id(literal: &str) -> WorkItemId {
    WorkItemId::parse(literal).expect("a valid stable ID")
}

fn task_path(project: &Project, id: &str) -> Utf8PathBuf {
    project.root().join(format!("work/task/{id}.md"))
}

fn break_document(project: &Project, id: &str) {
    std::fs::write(task_path(project, id), "---\nid: not even close\n").unwrap();
}

/// Write a valid document straight to disk, outside any operation.
fn write_item(project: &Project, id: &str, title: &str) {
    let item = fixtures::work_item(id, "task", title).build();
    std::fs::write(
        task_path(project, id),
        waymark_core::serialize::to_document(&item),
    )
    .unwrap();
}

#[test]
fn a_projection_is_rebuilt_from_the_files_and_reports_complete_and_fresh() {
    let (_dir, project) = project();
    make(&project, "one");
    make(&project, "two");

    let projection = Projection::load(&project).unwrap();
    assert_eq!(projection.completeness(), Completeness::Complete);
    assert_eq!(projection.freshness(), Freshness::Fresh);
    assert!(projection.is_complete());
    assert_eq!(projection.stats().parse_count, 2);
    assert_eq!(projection.items().count(), 2);
    assert!(projection.require_complete().is_ok());

    // The projection is a snapshot, not an authority: a direct write does not
    // edit it, and the next build sees the file.
    let late = "wm-0000000000zz";
    write_item(&project, late, "late");
    assert!(projection.entry(&id(late)).is_none());
    assert_eq!(projection.items().count(), 2);

    let rebuilt = Projection::load(&project).unwrap();
    assert_eq!(rebuilt.items().count(), 3);
    assert!(rebuilt.entry(&id(late)).is_some());
    assert_eq!(
        rebuilt.meta().source,
        waymark_core::envelope::Source::MemoryProjection
    );
    assert_eq!(rebuilt.meta().completeness, Completeness::Complete);
    assert_eq!(rebuilt.meta().freshness, Freshness::Fresh);
}

#[test]
fn a_scoped_local_read_works_while_another_file_is_unparseable() {
    let (_dir, project) = project();
    let parent = make(&project, "parent");
    let child = make(&project, "child");
    let broken = make(&project, "broken");
    set_parent(&project, &child, &parent, &options()).unwrap();
    break_document(&project, &broken);

    // Loading does not fail: the projection is explicitly incomplete instead.
    let projection = Projection::load(&project).unwrap();
    assert_eq!(projection.completeness(), Completeness::Scoped);
    assert_eq!(projection.freshness(), Freshness::Fresh);
    assert!(!projection.is_complete());
    assert_eq!(projection.incomplete_paths().len(), 1);
    assert!(projection.incomplete_paths()[0].ends_with(format!("{broken}.md")));

    let error = projection.require_complete().unwrap_err();
    assert_eq!(error.code(), Code::IncompleteProjection);
    assert!(!error.retryable());
    assert_eq!(error.details()["completeness"], serde_json::json!("scoped"));
    assert!(error.details()["diagnostics"].as_array().unwrap().len() >= 2);

    // The good item is still locally readable, with its direct relationships.
    let scoped = projection.scoped_read(&id(&parent)).unwrap();
    assert_eq!(scoped.item.id, id(&parent));
    assert_eq!(scoped.meta.completeness, Completeness::Scoped);
    assert_eq!(
        scoped.meta.source,
        waymark_core::envelope::Source::DirectFile
    );
    assert_eq!(
        scoped.meta.diagnostics[0].code,
        Code::IncompleteProjection,
        "an incomplete local read says why it is scoped"
    );
    assert_eq!(scoped.children, vec![id(&child)]);
    assert!(
        scoped.readiness.is_none(),
        "derived readiness is unknown while a sibling cannot be read"
    );

    // Reading the broken document itself still fails with its own error.
    let error = projection.scoped_read(&id(&broken)).unwrap_err();
    assert!(matches!(
        error.code(),
        Code::ParseError | Code::ValidationError
    ));

    // And the read operation surfaces the scoped answer without scanning the
    // whole graph.
    let scoped = read::read_scoped(&project, &parent).unwrap();
    assert_eq!(scoped.meta.completeness, Completeness::Scoped);
    assert_eq!(scoped.children, vec![id(&child)]);
}

#[test]
fn a_metadata_only_change_retains_the_parsed_entry() {
    let (_dir, project) = project();
    let item = make(&project, "one");
    let path = task_path(&project, &item);

    let mut projection = Projection::load(&project).unwrap();
    let parsed = projection.stats().parse_count;
    assert_eq!(parsed, 1);
    let fingerprint = projection
        .fingerprint_of(&id(&item))
        .expect("the item is projected")
        .clone();

    // Same bytes, newer metadata: the inventory signal changes, the content
    // fingerprint does not.
    let bytes = std::fs::read(&path).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_modified(SystemTime::now() + Duration::from_secs(60))
        .unwrap();
    drop(file);

    projection.refresh(&project).unwrap();
    assert_eq!(
        projection.stats().parse_count,
        parsed,
        "a metadata-only change must not re-parse the document"
    );
    assert_eq!(projection.fingerprint_of(&id(&item)), Some(&fingerprint));
    assert_eq!(
        projection.entry(&id(&item)).unwrap().item.title,
        "one",
        "the parsed entry is retained"
    );

    // A content change does re-parse.
    let changed = fixtures::work_item(&item, "task", "renamed").build();
    std::fs::write(&path, waymark_core::serialize::to_document(&changed)).unwrap();
    projection.refresh(&project).unwrap();
    assert_eq!(projection.stats().parse_count, parsed + 1);
    assert_eq!(projection.entry(&id(&item)).unwrap().item.title, "renamed");
}

#[test]
fn a_concurrent_multi_file_write_ends_in_a_retryable_snapshot_unstable() {
    let (_dir, project) = project();
    make(&project, "first");

    // A controlled writer: every inventory pass sees two new documents, so no
    // snapshot can ever be verified as stable.
    let writer = Arc::new(AtomicUsize::new(0));
    let hook_project = project.clone();
    let hook_writer = Arc::clone(&writer);
    let hook = move |_pass: u32| {
        for _ in 0..2 {
            let n = hook_writer.fetch_add(1, Ordering::SeqCst) + 100;
            write_item(
                &hook_project,
                &format!("wm-{n:012}"),
                "written concurrently",
            );
        }
    };

    let error = Projection::load_with(
        &project,
        LoadOptions::new()
            .with_max_attempts(3)
            .with_inventory_hook(hook),
    )
    .expect_err("a moving project cannot produce a stable snapshot");
    assert_eq!(error.code(), Code::SnapshotUnstable);
    assert!(error.retryable(), "the reader may retry");
    assert_eq!(error.details()["retries"], serde_json::json!(3));

    // Once the writer stops, the same read succeeds and sees every document.
    let settled = Projection::load(&project).unwrap();
    assert_eq!(settled.items().count(), 1 + 2 * 2 * 3);
    assert!(settled.is_complete());
}

#[test]
fn the_bounded_cache_holds_derived_data_only_and_evicts_nothing_from_the_projection() {
    let (_dir, project) = project();
    let parent = make(&project, "parent");
    let child = make(&project, "child");
    let sibling = make(&project, "sibling");
    set_parent(&project, &child, &parent, &options()).unwrap();

    let mut projection = Projection::load(&project).unwrap();
    assert_eq!(projection.graph().children(&id(&parent)), vec![id(&child)]);

    projection.reconfigure_cache(CacheSettings {
        enabled: true,
        capacity: 2,
    });
    let stamp = projection.stamp();

    let mut keys = Vec::new();
    for item in [&parent, &child, &sibling] {
        let item = id(item);
        let fingerprint = projection
            .fingerprint_of(&item)
            .expect("the item is projected")
            .clone();
        let key = CacheKey::body(&item, &fingerprint);
        assert!(projection.cache_get(&key).is_none(), "cold cache");
        projection.cache_put(key.clone(), format!("body of {item}"));
        keys.push(key);
    }
    // Two entries fit; the third evicted the oldest.
    assert_eq!(projection.cache_len(), 2);
    assert!(projection.stats().cache_evictions >= 1);
    assert!(projection.cache_get(&keys[0]).is_none());
    assert_eq!(
        projection.cache_get(&keys[2]),
        Some(format!("body of {sibling}").as_str())
    );

    // Relationship nodes are not cache entries: they survive every eviction.
    assert_eq!(projection.items().count(), 3);
    assert_eq!(projection.graph().len(), 3);
    assert_eq!(projection.graph().children(&id(&parent)), vec![id(&child)]);

    // Query data is bound to the whole-projection fingerprint.
    let query_key = CacheKey::query("list:all", &stamp);
    projection.cache_put(query_key.clone(), "[]".to_owned());
    assert_eq!(projection.cache_get(&query_key), Some("[]"));
    let changed = fixtures::work_item(&sibling, "task", "renamed").build();
    std::fs::write(
        task_path(&project, &sibling),
        waymark_core::serialize::to_document(&changed),
    )
    .unwrap();
    projection.refresh(&project).unwrap();
    assert!(
        projection.cache_get(&query_key).is_none(),
        "changed files invalidate query data"
    );
}

#[test]
fn a_body_cache_entry_is_bound_to_the_content_fingerprint() {
    let (_dir, project) = project();
    let item = make(&project, "one");
    let path = task_path(&project, &item);

    let mut projection = Projection::load(&project).unwrap();
    let old_fingerprint = projection.fingerprint_of(&id(&item)).unwrap().clone();
    let old_key = CacheKey::body(&id(&item), &old_fingerprint);
    projection.cache_put(old_key.clone(), "cached body".to_owned());
    assert_eq!(projection.cache_get(&old_key), Some("cached body"));

    let changed = fixtures::work_item(&item, "task", "renamed").build();
    std::fs::write(&path, waymark_core::serialize::to_document(&changed)).unwrap();
    projection.refresh(&project).unwrap();

    assert!(
        projection.cache_get(&old_key).is_none(),
        "the old fingerprint cannot serve a changed file"
    );
    let new_fingerprint = projection.fingerprint_of(&id(&item)).unwrap();
    assert_ne!(&old_fingerprint, new_fingerprint);
    let new_key = CacheKey::body(&id(&item), new_fingerprint);
    assert!(projection.cache_get(&new_key).is_none());
}

#[test]
fn a_cache_only_configuration_change_clears_the_cache_but_not_the_projection() {
    let (_dir, project) = project();
    let item = make(&project, "one");
    let mut projection = Projection::load(&project).unwrap();
    let fingerprint = projection.fingerprint_of(&id(&item)).unwrap().clone();
    let key = CacheKey::detail(&id(&item), &fingerprint);
    projection.cache_put(key.clone(), "detail".to_owned());
    assert_eq!(projection.cache_len(), 1);

    projection.reconfigure_cache(CacheSettings {
        enabled: true,
        capacity: 8,
    });
    assert_eq!(projection.cache_len(), 0, "evictable data is cleared");
    assert!(projection.entry(&id(&item)).is_some());
    assert_eq!(projection.items().count(), 1);
    assert_eq!(projection.stats().parse_count, 1);

    projection.reconfigure_cache(CacheSettings {
        enabled: false,
        capacity: 8,
    });
    projection.cache_put(key.clone(), "detail".to_owned());
    assert_eq!(projection.cache_len(), 0, "a disabled cache stores nothing");
}

#[test]
fn a_projection_loaded_with_a_configured_cache_uses_that_capacity() {
    let (_dir, project) = project();
    let mut config = project.config().clone();
    config.cache = CacheSettings {
        enabled: true,
        capacity: 1,
    };
    let mut project = project;
    project.set_config(config);
    make(&project, "one");
    let item = id(&make(&project, "two"));

    let mut projection = Projection::load(&project).unwrap();
    let fingerprint = projection
        .fingerprint_of(&item)
        .expect("the new item is projected")
        .clone();
    projection.cache_put(CacheKey::body(&item, &fingerprint), "a".to_owned());
    projection.cache_put(CacheKey::detail(&item, &fingerprint), "b".to_owned());
    assert_eq!(
        projection.cache_len(),
        1,
        "the configured capacity bounds it"
    );

    // The defaulted load policy retries a bounded number of times.
    assert_eq!(LoadOptions::default().max_attempts, 3);
}

#[test]
fn a_case_variant_document_makes_the_projection_scoped_and_names_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let mut config = waymark_core::config::Config::fresh();
    config.categories.insert(
        waymark_core::model::CategoryKey::parse("bug").unwrap(),
        waymark_core::config::Category {
            path: "work/bugs".to_owned(),
            name: "Bugs".to_owned(),
        },
    );
    let project = Project::init(
        &root,
        waymark_core::store::InitOptions {
            config: Some(config),
            ..Default::default()
        },
    )
    .unwrap()
    .with_clock(fixtures::shared_clock())
    .with_id_source(Box::new(waymark_core::testing::SequentialIds::new()));
    let item = make(&project, "case");
    let lowercase = task_path(&project, &item);
    // Separate categories avoid aliasing on case-insensitive filesystems.
    let uppercase = root.join(format!("work/bugs/{}.md", item.to_uppercase()));
    let original_bytes = std::fs::read(&lowercase).unwrap();
    std::fs::copy(&lowercase, &uppercase).unwrap();
    assert_eq!(std::fs::read(&lowercase).unwrap(), original_bytes);
    assert_eq!(std::fs::read(&uppercase).unwrap(), original_bytes);

    // The inventory cannot classify the case variant as a document, but it names
    // an ID the projection also holds. That is an ambiguity, not noise.
    let projection = Projection::load(&project).unwrap();
    assert_eq!(projection.completeness(), Completeness::Scoped);
    assert!(!projection.is_complete());
    assert!(projection
        .problems()
        .iter()
        .any(|problem| problem.path == uppercase));
    assert!(projection
        .incomplete_paths()
        .iter()
        .any(|path| *path == uppercase.as_path()));

    let error = projection.require_complete().unwrap_err();
    assert_eq!(error.code(), Code::IncompleteProjection);
    let diagnostics = error.details()["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics.iter().any(|finding| {
            finding["code"] == serde_json::json!("DUPLICATE_ID")
                && finding["message"]
                    .as_str()
                    .is_some_and(|message| message.contains(uppercase.as_str()))
        }),
        "the diagnostic must name the case-variant path: {diagnostics:?}"
    );

    // Every global read refuses the partial world instead of publishing it.
    let error = waymark_core::ops::list::list(&project, &waymark_core::query::Query::new())
        .expect_err("a global read requires a complete projection");
    assert_eq!(error.code(), Code::IncompleteProjection);

    // The lowercase document is still locally readable: a scoped read names the
    // item it was asked for and does not choose between the two files.
    let scoped = projection.scoped_read(&id(&item)).unwrap();
    assert_eq!(scoped.item.id, id(&item));
    assert_eq!(scoped.meta.completeness, Completeness::Scoped);
}

#[test]
fn an_uppercase_stem_without_a_live_document_is_scoped() {
    let (_dir, project) = project();
    let item = make(&project, "upper");
    let lowercase = task_path(&project, &item);
    let uppercase = lowercase.with_file_name(format!("{}.md", item.to_uppercase()));
    std::fs::rename(&lowercase, &uppercase).unwrap();

    let projection = Projection::load(&project).unwrap();
    assert_eq!(projection.completeness(), Completeness::Scoped);
    assert_eq!(projection.items().count(), 0);
    assert!(projection.diagnostics().iter().any(|finding| {
        finding.code == Code::ParseError && finding.message.contains(uppercase.as_str())
    }));
}

#[test]
fn a_stray_readme_is_inventory_noise_not_an_incomplete_projection() {
    let (_dir, project) = project();
    make(&project, "one");
    std::fs::write(
        project.root().join("work/task/README.md"),
        "project notes, not a work item\n",
    )
    .unwrap();

    let projection = Projection::load(&project).unwrap();
    assert_eq!(
        projection.completeness(),
        Completeness::Complete,
        "a non-item file must not make the world partial"
    );
    assert!(projection.is_complete());
    assert!(projection.require_complete().is_ok());
    assert!(projection.diagnostics().is_empty());
    assert!(projection.incomplete_paths().is_empty());
    assert!(projection
        .problems()
        .iter()
        .any(|problem| problem.path.ends_with("README.md")));

    let listing =
        waymark_core::ops::list::list(&project, &waymark_core::query::Query::new()).unwrap();
    assert_eq!(listing.len(), 1, "the real document is still listed");
}
