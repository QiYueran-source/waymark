//! Ordinary editing: title, body, and custom fields.
//!
//! Every case drives the public `waymark_core` API against a temporary project
//! and checks both the returned outcome and the document bytes, because an edit
//! is only real if the file says so — and an edit that changes nothing must leave
//! the file untouched.

use std::sync::Arc;

use camino::Utf8PathBuf;
use chrono::{DateTime, Utc};
use waymark_core::config::{Config, FieldDecl, FieldType};
use waymark_core::error::Code;
use waymark_core::model::FieldKey;
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::edit::{self, FieldChangeEntry};
use waymark_core::ops::{create, OpOptions};
use waymark_core::testing::{fixtures, SequentialIds};
use waymark_core::{FieldValue, FixedClock, InitOptions, Project, WorkItem};

/// The instant the fixtures start at.
fn base() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-03T05:43:10Z")
        .expect("a valid instant")
        .with_timezone(&Utc)
}

/// Options whose clock is frozen `seconds` after the fixture instant.
fn options_at(seconds: i64) -> OpOptions {
    OpOptions::new().with_clock(Arc::new(FixedClock::new(
        base() + chrono::Duration::seconds(seconds),
    )))
}

/// A project that declares every custom-field shape the edit operation accepts.
///
/// `effort` is required and `tags` is a list, so a test can exercise both the
/// required-clear rejection and homogeneous-list validation.
fn field_project() -> (tempfile::TempDir, Project) {
    let mut config = Config::fresh();
    for (key, ty, required) in [
        ("effort", FieldType::Integer, true),
        ("note", FieldType::String, false),
        ("done", FieldType::Boolean, false),
        ("tags", FieldType::List(Box::new(FieldType::String)), false),
    ] {
        config.fields.insert(
            FieldKey::parse(key).expect("a valid field key"),
            FieldDecl {
                ty,
                required,
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
    .expect("a project with declared fields initialises")
    .with_clock(Arc::new(fixtures::clock()))
    .with_id_source(Box::new(SequentialIds::new()));
    (dir, project)
}

/// Create one item carrying the required field and return its stable ID.
fn make(project: &Project, title: &str) -> String {
    let mut request = CreateRequest::new(title, "task", "frank")
        .with_field(FieldKey::parse("effort").unwrap(), FieldValue::Integer(1));
    request = request.with_body("original body\n");
    let created = create::create(project, request, &options_at(0)).expect("creation succeeds");
    let id = created.item.id.clone();
    edit::update_custom_fields(
        project,
        id.as_str(),
        &[FieldChangeEntry::set("effort", FieldValue::Integer(1))],
        &options_at(0),
    )
    .expect("the required field is settable");
    id.as_str().to_owned()
}

fn document(project: &Project, id: &str) -> String {
    let entry = project
        .locate(&waymark_core::WorkItemId::parse(id).expect("a valid id"))
        .expect("the item exists");
    std::fs::read_to_string(&entry.path).expect("the document is readable")
}

fn keys(values: &[FieldKey]) -> Vec<&str> {
    values.iter().map(FieldKey::as_str).collect()
}

#[test]
fn title_replacement_trims_surroundings_and_keeps_internal_whitespace() {
    let (_dir, project) = field_project();
    let id = make(&project, "Fix the thing");
    let before = document(&project, &id);

    let outcome = edit::set_title(&project, &id, "  Fix  the   thing  ", &options_at(60))
        .expect("a single-line title is accepted");

    assert!(outcome.changed);
    assert!(outcome.written);
    assert_eq!(outcome.item.title, "Fix  the   thing");
    assert_eq!(
        outcome.item.id.as_str(),
        id,
        "the identity field is never editable"
    );
    assert_eq!(
        outcome.item.updated_at.to_string(),
        "2026-11-03T05:44:10Z",
        "a real title change moves updated_at"
    );
    let after = document(&project, &id);
    assert_ne!(after, before);
    assert!(
        after.contains("\ntitle: Fix  the   thing\n"),
        "the internal run of spaces is preserved: {after}"
    );
}

#[test]
fn an_empty_or_multi_line_title_is_rejected_without_writing() {
    let (_dir, project) = field_project();
    let id = make(&project, "Fix the thing");
    let before = document(&project, &id);

    for rejected in ["", "   ", "\t\n ", "two\nlines", "carriage\rreturn"] {
        let error = edit::set_title(&project, &id, rejected, &options_at(60))
            .expect_err("a blank or multi-line title is not a title");
        assert_eq!(error.code(), Code::ValidationError, "{rejected:?}");
        let issues = error.details()["issues"]
            .as_array()
            .expect("a validation error carries issues");
        assert_eq!(issues[0]["path"], serde_json::json!("title"));
    }

    assert_eq!(document(&project, &id), before, "nothing was written");
}

#[test]
fn body_replacement_canonicalizes_to_one_trailing_newline_and_never_trims() {
    let (_dir, project) = field_project();
    let id = make(&project, "Fix the thing");

    let outcome = edit::replace_body(
        &project,
        &id,
        Some("  leading spaces stay\n\ninternal blank line\n\n\n"),
        &options_at(60),
    )
    .expect("a body replaces the old one");

    assert!(outcome.changed && outcome.written);
    assert_eq!(
        outcome.item.body,
        "  leading spaces stay\n\ninternal blank line\n"
    );
    assert!(document(&project, &id).ends_with("  leading spaces stay\n\ninternal blank line\n"));
}

#[test]
fn an_absent_body_leaves_the_body_alone_and_an_empty_one_clears_it() {
    let (_dir, project) = field_project();
    let id = make(&project, "Fix the thing");
    let before = document(&project, &id);

    let unchanged = edit::replace_body(&project, &id, None, &options_at(60)).expect("a no-op");
    assert!(!unchanged.changed && !unchanged.written);
    assert_eq!(unchanged.item.body, "original body\n");
    assert_eq!(
        document(&project, &id),
        before,
        "no write, no timestamp move"
    );

    let cleared =
        edit::replace_body(&project, &id, Some(""), &options_at(60)).expect("an empty body clears");
    assert!(cleared.changed && cleared.written);
    assert_eq!(cleared.item.body, "");
    assert!(
        !document(&project, &id).contains("original body"),
        "the old body is gone"
    );

    let repeat = edit::replace_body(&project, &id, Some(""), &options_at(120))
        .expect("clearing an already empty body is not a change");
    assert!(!repeat.changed && !repeat.written);
    assert_eq!(repeat.item.updated_at.to_string(), "2026-11-03T05:44:10Z");
}

#[test]
fn custom_fields_accept_mixed_set_and_clear_entries_in_one_write() {
    let (_dir, project) = field_project();
    let id = make(&project, "Fix the thing");
    edit::update_custom_fields(
        &project,
        &id,
        &[
            FieldChangeEntry::set("note", FieldValue::String("note".to_owned())),
            FieldChangeEntry::set("effort", FieldValue::Integer(1)),
        ],
        &options_at(0),
    )
    .expect("a first change set");

    let outcome = edit::update_custom_fields(
        &project,
        &id,
        &[
            FieldChangeEntry::set("effort", FieldValue::Integer(3)),
            FieldChangeEntry::set(
                "tags",
                FieldValue::List(vec![FieldValue::String("a".to_owned())]),
            ),
            FieldChangeEntry::set("done", FieldValue::Boolean(true)),
            FieldChangeEntry::clear("note"),
        ],
        &options_at(60),
    )
    .expect("mixed set and clear entries are one validated change set");

    assert!(outcome.changed && outcome.written);
    assert_eq!(
        keys(&outcome.changed_fields),
        ["done", "effort", "note", "tags"],
        "a cleared key that had a value is a changed field"
    );
    assert_eq!(
        outcome.item.custom_fields.get("effort"),
        Some(&FieldValue::Integer(3))
    );
    assert_eq!(
        outcome.item.custom_fields.get("done"),
        Some(&FieldValue::Boolean(true))
    );
    assert!(!outcome.item.custom_fields.contains_key("note"));

    let written: WorkItem = waymark_core::parse::parse_item_file(
        &project
            .locate(&waymark_core::WorkItemId::parse(&id).unwrap())
            .unwrap()
            .path,
    )
    .expect("the written document parses");
    assert_eq!(written.custom_fields, outcome.item.custom_fields);
}

#[test]
fn an_invalid_change_set_is_rejected_whole_and_nothing_is_written() {
    let (_dir, project) = field_project();
    let id = make(&project, "Fix the thing");
    let before = document(&project, &id);

    let cases: Vec<(&str, Vec<FieldChangeEntry>)> = vec![
        ("null", vec![FieldChangeEntry::null("note")]),
        (
            "unknown",
            vec![FieldChangeEntry::set(
                "nope",
                FieldValue::String("x".to_owned()),
            )],
        ),
        (
            "duplicate",
            vec![
                FieldChangeEntry::set("note", FieldValue::String("a".to_owned())),
                FieldChangeEntry::set("note", FieldValue::String("b".to_owned())),
            ],
        ),
        (
            "conflict",
            vec![
                FieldChangeEntry::set("note", FieldValue::String("a".to_owned())),
                FieldChangeEntry::clear("note"),
            ],
        ),
        ("required", vec![FieldChangeEntry::clear("effort")]),
        (
            "type",
            vec![FieldChangeEntry::set(
                "effort",
                FieldValue::String("3".to_owned()),
            )],
        ),
        (
            "list-element",
            vec![FieldChangeEntry::set(
                "tags",
                FieldValue::List(vec![
                    FieldValue::String("a".to_owned()),
                    FieldValue::Integer(1),
                ]),
            )],
        ),
    ];

    for (name, changes) in cases {
        // A valid entry in the same set must not be written either: the whole
        // change set is validated before the single write.
        let mut all = changes.clone();
        all.push(FieldChangeEntry::set("done", FieldValue::Boolean(true)));
        let error =
            edit::update_custom_fields(&project, &id, &all, &options_at(60)).expect_err(name);
        assert_eq!(error.code(), Code::ValidationError, "case {name}: {error}");
        let issues = error.details()["issues"]
            .as_array()
            .expect("per-field issues");
        assert!(!issues.is_empty(), "case {name}");
        assert_eq!(document(&project, &id), before, "case {name} wrote nothing");
    }
}

#[test]
fn a_request_with_no_effective_change_performs_no_write() {
    let (_dir, project) = field_project();
    let id = make(&project, "Fix the thing");
    let before = document(&project, &id);
    let entry = project
        .locate(&waymark_core::WorkItemId::parse(&id).unwrap())
        .unwrap();
    let parsed = waymark_core::parse::parse_item_file(&entry.path).unwrap();

    let title = edit::set_title(&project, &id, parsed.title.as_str(), &options_at(60))
        .expect("the same title is not an error");
    assert!(!title.changed && !title.written);
    assert_eq!(title.item.updated_at, parsed.updated_at);

    let body = edit::replace_body(&project, &id, Some(parsed.body.as_str()), &options_at(60))
        .expect("the same body is not an error");
    assert!(!body.changed && !body.written);
    assert_eq!(body.item.updated_at, parsed.updated_at);

    let fields = edit::update_custom_fields(
        &project,
        &id,
        &[
            FieldChangeEntry::set("effort", FieldValue::Integer(1)),
            FieldChangeEntry::clear("note"),
        ],
        &options_at(60),
    )
    .expect("setting the same value and clearing an unset one is not an error");
    assert!(!fields.changed && !fields.written);
    assert!(fields.changed_fields.is_empty());
    assert_eq!(fields.item.updated_at, parsed.updated_at);

    assert_eq!(
        document(&project, &id),
        before,
        "an unchanged request is byte-identical"
    );
}

#[test]
fn an_owner_mismatch_warns_and_still_writes() {
    let (_dir, project) = field_project();
    let id = make(&project, "Fix the thing");

    let outcome = edit::set_title(
        &project,
        &id,
        "Renamed",
        &options_at(60).with_operator("alice"),
    )
    .expect("a mismatch never blocks an edit");

    assert!(outcome.written);
    assert_eq!(outcome.item.title, "Renamed");
    assert_eq!(
        outcome.meta.warnings.len(),
        1,
        "{:?}",
        outcome.meta.warnings
    );
    assert_eq!(outcome.meta.warnings[0].code, Code::OwnerMismatch);
    assert!(outcome.meta.warnings[0].message.contains("alice"));
}

#[test]
fn an_archived_item_rejects_an_edit() {
    let (_dir, project) = field_project();
    let id = make(&project, "Fix the thing");
    let entry = project
        .locate(&waymark_core::WorkItemId::parse(&id).unwrap())
        .unwrap();
    let mut item = waymark_core::parse::parse_item_file(&entry.path).unwrap();
    item.archived_at = Some(waymark_core::testing::instant());
    std::fs::write(&entry.path, waymark_core::serialize::to_document(&item)).unwrap();
    let before = document(&project, &id);

    let error = edit::replace_body(&project, &id, Some("new"), &options_at(60))
        .expect_err("an archived item rejects edits");

    assert_eq!(error.code(), Code::ItemArchived);
    assert_eq!(error.details()["operation"], serde_json::json!("body"));
    assert_eq!(document(&project, &id), before);
}

#[test]
fn an_edited_document_re_serializes_byte_identically() {
    let (_dir, project) = field_project();
    let id = make(&project, "Fix the thing");
    edit::set_title(&project, &id, "Renamed", &options_at(60)).unwrap();
    edit::replace_body(&project, &id, Some("body\n"), &options_at(120)).unwrap();

    let entry = project
        .locate(&waymark_core::WorkItemId::parse(&id).unwrap())
        .unwrap();
    let bytes = std::fs::read_to_string(&entry.path).unwrap();
    let parsed = waymark_core::parse::parse_item_file(&entry.path).unwrap();
    assert_eq!(
        waymark_core::serialize::to_document(&parsed),
        bytes,
        "an unchanged item re-serializes to the same bytes"
    );

    // Repeating the edit in the other order is still a no-op on the same bytes.
    edit::replace_body(&project, &id, Some("body\n"), &options_at(180)).unwrap();
    edit::set_title(&project, &id, "Renamed", &options_at(240)).unwrap();
    assert_eq!(std::fs::read_to_string(&entry.path).unwrap(), bytes);
}
