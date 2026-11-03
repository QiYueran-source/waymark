//! Canonical document bytes: the pinned key order, the omitted-when-unset rule,
//! the body newline rule, and the round-trip invariant.

use camino::Utf8Path;
use waymark_core::error::Code;
use waymark_core::model::{CategoryKey, FieldValue, Status, WorkItem};
use waymark_core::parse::parse_document;
use waymark_core::serialize::to_document;
use waymark_core::testing::fixtures;
use waymark_core::Timestamp;

fn full_item() -> WorkItem {
    let mut item = fixtures::work_item("wm-3f7k9q2m4x8c", "bug", "Fix the thing")
        .with_title("Fix the thing")
        .build();
    item.creator = Some("frank".to_owned());
    item.closed_at = Some(Timestamp::parse("2026-11-03T06:43:10Z").unwrap());
    item.parent = Some(waymark_core::WorkItemId::parse("wm-aaaaaaaaaaaa").unwrap());
    item.dependencies = vec![
        waymark_core::WorkItemId::parse("wm-aaaaaaaaaaaa").unwrap(),
        waymark_core::WorkItemId::parse("wm-bbbbbbbbbbbb").unwrap(),
    ];
    item.soft_links = vec![waymark_core::WorkItemId::parse("wm-cccccccccccc").unwrap()];
    item.custom_fields
        .insert("effort".to_owned(), FieldValue::Integer(3));
    item.custom_fields.insert(
        "tags".to_owned(),
        FieldValue::List(vec![
            FieldValue::String("a".to_owned()),
            FieldValue::String("b".to_owned()),
        ]),
    );
    item.archived_at = Some(Timestamp::parse("2026-11-04T00:00:00Z").unwrap());
    item.archive_reason = Some("duplicate".to_owned());
    item.body = "body text here\n".to_owned();
    item
}

const GOLDEN: &str = include_str!("golden/full_item.md");

#[test]
fn a_fully_populated_item_matches_the_pinned_golden_bytes() {
    assert_eq!(to_document(&full_item()), GOLDEN);
}

#[test]
fn reserializing_an_unchanged_item_is_byte_identical() {
    let item = full_item();
    let once = to_document(&item);
    let parsed = parse_document(once.as_bytes(), Utf8Path::new("wm-3f7k9q2m4x8c.md")).unwrap();
    let twice = to_document(&parsed);
    assert_eq!(once.as_bytes(), twice.as_bytes());
    assert_eq!(parsed, item);
}

#[test]
fn unset_optionals_are_omitted_rather_than_written_as_null() {
    let mut item = fixtures::minimal_item("wm-3f7k9q2m4x8c");
    item.title = "Just a title".to_owned();
    let document = to_document(&item);

    for absent in [
        "creator:",
        "closed_at:",
        "parent:",
        "archived_at:",
        "archive_reason:",
        "dependencies:",
        "soft_links:",
        "custom_fields:",
        "null",
        "~",
    ] {
        assert!(
            !document.contains(absent),
            "{absent:?} must be absent, document was:\n{document}"
        );
    }
    assert!(
        document.ends_with("---\n"),
        "an empty body adds no trailing blank line:\n{document}"
    );
}

#[test]
fn relationships_are_sorted_and_deduplicated() {
    let mut item = fixtures::minimal_item("wm-3f7k9q2m4x8c");
    item.dependencies = vec![
        "wm-cccccccccccc".parse().unwrap(),
        "wm-aaaaaaaaaaaa".parse().unwrap(),
        "wm-cccccccccccc".parse().unwrap(),
    ];
    let document = to_document(&item);
    assert!(
        document.contains("dependencies:\n  - wm-aaaaaaaaaaaa\n  - wm-cccccccccccc\n"),
        "got:\n{document}"
    );
}

#[test]
fn a_non_empty_body_ends_in_exactly_one_newline_and_keeps_its_leading_space() {
    let mut item = fixtures::minimal_item("wm-3f7k9q2m4x8c");
    item.body = "  indented first line\nsecond line\n\n\n".to_owned();
    let document = to_document(&item);
    assert!(
        document.ends_with("---\n  indented first line\nsecond line\n"),
        "got:\n{document}"
    );

    let parsed = parse_document(document.as_bytes(), Utf8Path::new("x.md")).unwrap();
    assert_eq!(parsed.body, "  indented first line\nsecond line\n");
    assert_eq!(to_document(&parsed).as_bytes(), document.as_bytes());
}

#[test]
fn empty_relationship_lists_are_accepted_when_a_hand_edit_writes_them() {
    let document = r#"---
schema: 1
id: wm-3f7k9q2m4x8c
title: Hand edited
category: bug
status: open
owner: frank
created_at: 2026-11-03T05:43:10Z
updated_at: 2026-11-03T05:43:10Z
dependencies: []
soft_links: []
custom_fields: {}
---
"#;
    let parsed = parse_document(document.as_bytes(), Utf8Path::new("x.md")).unwrap();
    assert!(parsed.dependencies.is_empty());
    assert!(parsed.soft_links.is_empty());
    assert!(parsed.custom_fields.is_empty());
    // And the canonical form drops them again.
    assert!(!to_document(&parsed).contains("dependencies:"));
}

#[test]
fn an_unknown_frontmatter_key_is_a_locatable_validation_error() {
    let document = r#"---
schema: 1
id: wm-3f7k9q2m4x8c
title: Hand edited
category: bug
status: open
owner: frank
created_at: 2026-11-03T05:43:10Z
updated_at: 2026-11-03T05:43:10Z
priority: 2
---
"#;
    let err = parse_document(
        document.as_bytes(),
        Utf8Path::new("work/bugs/wm-3f7k9q2m4x8c.md"),
    )
    .unwrap_err();
    assert_eq!(err.code(), Code::ValidationError);
    assert!(
        err.details()["issues"]
            .as_array()
            .expect("a validation error carries an issue list")
            .iter()
            .any(|issue| issue["path"] == "priority"),
        "the unknown key must be named in the issue list: {err:?}"
    );
    assert!(
        err.message().contains("priority"),
        "the unknown key must be named: {err}"
    );
    match err.location() {
        Some(waymark_core::Location::Field { field }) => assert_eq!(field, "priority"),
        other => panic!("expected a field location, got {other:?}"),
    }
}

#[test]
fn a_document_without_frontmatter_fences_is_a_parse_error() {
    let err = parse_document(b"no frontmatter here\n", Utf8Path::new("x.md")).unwrap_err();
    assert_eq!(err.code(), Code::ParseError);
    assert!(err.message().contains("---"), "{err}");
}

#[test]
fn a_timestamp_without_an_offset_is_a_validation_error() {
    let document = r#"---
schema: 1
id: wm-3f7k9q2m4x8c
title: t
category: bug
status: open
owner: frank
created_at: 2026-11-03T05:43:10
updated_at: 2026-11-03T05:43:10Z
---
"#;
    let err = parse_document(document.as_bytes(), Utf8Path::new("x.md")).unwrap_err();
    assert_eq!(err.code(), Code::ValidationError);
    assert!(
        err.message().contains("created_at"),
        "the offending field must be named: {err}"
    );
}

#[test]
fn an_unreadable_schema_generation_is_unsupported_version() {
    let document = r#"---
schema: 2
id: wm-3f7k9q2m4x8c
title: t
category: bug
status: open
owner: frank
created_at: 2026-11-03T05:43:10Z
updated_at: 2026-11-03T05:43:10Z
---
"#;
    let err = parse_document(document.as_bytes(), Utf8Path::new("x.md")).unwrap_err();
    assert_eq!(err.code(), Code::UnsupportedVersion);
    assert!(err.message().contains("generation"), "{err}");
}

#[test]
fn the_category_key_grammar_is_enforced_on_read() {
    let document = r#"---
schema: 1
id: wm-3f7k9q2m4x8c
title: t
category: Bug Report
status: open
owner: frank
created_at: 2026-11-03T05:43:10Z
updated_at: 2026-11-03T05:43:10Z
---
"#;
    let err = parse_document(document.as_bytes(), Utf8Path::new("x.md")).unwrap_err();
    assert_eq!(err.code(), Code::ValidationError);
    assert!(err.message().contains("category"), "{err}");
}

#[test]
fn relationship_ids_must_be_parseable_stable_ids() {
    let mut item = fixtures::minimal_item("wm-3f7k9q2m4x8c");
    item.dependencies = vec![waymark_core::WorkItemId::parse("wm-bbbbbbbbbbbb").unwrap()];
    let document = to_document(&item).replace("wm-bbbbbbbbbbbb", "nonsense");
    let err = parse_document(document.as_bytes(), Utf8Path::new("x.md")).unwrap_err();
    assert_eq!(err.code(), Code::ValidationError);
    assert!(err.message().contains("dependencies"), "{err}");
}

#[test]
fn status_literals_are_the_three_persisted_strings() {
    assert_eq!(Status::Open.as_str(), "open");
    assert_eq!(Status::InProgress.as_str(), "in_progress");
    assert_eq!(Status::Closed.as_str(), "closed");

    let mut item = fixtures::minimal_item("wm-3f7k9q2m4x8c");
    item.status = Status::InProgress;
    let document = to_document(&item);
    assert!(document.contains("\nstatus: in_progress\n"), "{document}");
    let parsed = parse_document(document.as_bytes(), Utf8Path::new("x.md")).unwrap();
    assert_eq!(parsed.status, Status::InProgress);
}

#[test]
fn the_category_key_of_a_document_round_trips() {
    let mut item = fixtures::minimal_item("wm-3f7k9q2m4x8c");
    item.category = CategoryKey::parse("bug-report").unwrap();
    let parsed = parse_document(to_document(&item).as_bytes(), Utf8Path::new("x.md")).unwrap();
    assert_eq!(parsed.category.as_str(), "bug-report");
}

#[test]
fn yaml_hostile_scalars_round_trip_through_the_writer() {
    // Each of these is a value a YAML serializer may quote, escape, or fold —
    // and each has burned a hand-rolled writer before.
    let hostile = [
        "Fix: the thing",
        "a - b",
        "  leading spaces",
        "trailing spaces  ",
        "#not-a-comment",
        "'single quoted'",
        "\"double quoted\"",
        "R&D",
        "*emphasis*",
        "&anchor",
        "- dash start",
        "? question",
        "| pipe",
        "> fold",
        "% percent",
        "@ at",
        "`tick`",
        "null",
        "true",
        "123",
        "1.5",
        "unicode ✓ 中文",
        "back\\slash",
    ];
    for value in hostile {
        let mut item = fixtures::minimal_item("wm-3f7k9q2m4x8c");
        item.title = value.to_owned();
        let document = to_document(&item);
        let parsed = parse_document(document.as_bytes(), Utf8Path::new("x.md"))
            .unwrap_or_else(|error| panic!("{value:?} did not parse: {error}\n{document}"));
        assert_eq!(parsed.title, value, "{value:?} did not round-trip");
        assert_eq!(parsed, item, "{value:?} changed the item");
    }
}

#[test]
fn a_value_survives_a_round_trip_in_every_free_text_slot() {
    let mut item = fixtures::minimal_item("wm-3f7k9q2m4x8c");
    item.title = "title: with a colon".to_owned();
    item.owner = "#hashtag".to_owned();
    item.creator = Some("*starred*".to_owned());
    item.archive_reason = Some("&why".to_owned());
    item.custom_fields.insert(
        "note".to_owned(),
        FieldValue::String("multi: word".to_owned()),
    );
    let document = to_document(&item);
    let parsed = parse_document(document.as_bytes(), Utf8Path::new("x.md")).unwrap();
    assert_eq!(parsed, item);
}

#[test]
fn a_body_that_looks_like_frontmatter_is_not_treated_as_one() {
    let mut item = fixtures::minimal_item("wm-3f7k9q2m4x8c");
    item.body = "---\nschema: 9\n---\nsecond fence\n".to_owned();
    let document = to_document(&item);
    let parsed = parse_document(document.as_bytes(), Utf8Path::new("x.md")).unwrap();
    assert_eq!(parsed.body, item.body);
    assert_eq!(parsed.schema, 1);
}

#[test]
fn a_yaml_alias_in_frontmatter_is_refused() {
    let document = "---\nschema: 1\nid: wm-3f7k9q2m4x8c\ntitle: *alias\ncategory: bug\nstatus: open\nowner: frank\ncreated_at: 2026-11-03T05:43:10Z\nupdated_at: 2026-11-03T05:43:10Z\n---\n";
    let error = parse_document(document.as_bytes(), Utf8Path::new("x.md")).unwrap_err();
    assert_eq!(error.code(), Code::ParseError);
    assert!(error.message().contains("anchor"), "{error}");
}

#[test]
fn an_anchor_in_a_quoted_scalar_is_plain_text() {
    // The writer may emit a value that starts with an anchor character once it
    // is quoted; refusing to read it back would be a write that cannot be read.
    let mut item = fixtures::minimal_item("wm-3f7k9q2m4x8c");
    item.title = "*not an alias*".to_owned();
    let document = to_document(&item);
    let parsed = parse_document(document.as_bytes(), Utf8Path::new("x.md")).unwrap();
    assert_eq!(parsed.title, "*not an alias*");
}

#[test]
fn a_duplicate_frontmatter_key_is_refused() {
    let document = "---\nschema: 1\nid: wm-3f7k9q2m4x8c\ntitle: one\ntitle: two\ncategory: bug\nstatus: open\nowner: frank\ncreated_at: 2026-11-03T05:43:10Z\nupdated_at: 2026-11-03T05:43:10Z\n---\n";
    // The YAML layer refuses a duplicate key outright, which is the strictest
    // possible answer: it can never silently pick one of the two values.
    let error = parse_document(document.as_bytes(), Utf8Path::new("x.md")).unwrap_err();
    assert_eq!(error.code(), Code::ParseError);
    assert!(error.message().contains("title"), "{error}");
}

#[test]
fn a_missing_required_key_is_named() {
    for key in [
        "schema",
        "id",
        "title",
        "category",
        "status",
        "owner",
        "created_at",
        "updated_at",
    ] {
        let mut item = fixtures::minimal_item("wm-3f7k9q2m4x8c");
        item.owner = "frank".to_owned();
        let document = to_document(&item);
        let without: String = document
            .lines()
            .filter(|line| !line.starts_with(&format!("{key}:")))
            .collect::<Vec<_>>()
            .join("\n");
        let error = parse_document(without.as_bytes(), Utf8Path::new("x.md"))
            .expect_err(&format!("{key} must be required"));
        assert_eq!(error.code(), Code::ValidationError, "key {key}");
        assert!(error.message().contains(key), "key {key}: {error}");
    }
}

#[test]
fn a_hand_edit_that_uses_block_scalars_still_reads() {
    let document = "---\nschema: 1\nid: wm-3f7k9q2m4x8c\ntitle: >-\n  folded\n  title\ncategory: bug\nstatus: open\nowner: frank\ncreated_at: 2026-11-03T05:43:10Z\nupdated_at: 2026-11-03T05:43:10Z\n---\n";
    let parsed = parse_document(document.as_bytes(), Utf8Path::new("x.md")).unwrap();
    assert_eq!(parsed.title, "folded title");
    // And the writer re-emits one canonical line.
    assert!(to_document(&parsed).contains("\ntitle: folded title\n"));
}

#[test]
fn empty_optional_custom_fields_map_is_accepted() {
    let document = "---\nschema: 1\nid: wm-3f7k9q2m4x8c\ntitle: t\ncategory: bug\nstatus: open\nowner: frank\ncreated_at: 2026-11-03T05:43:10Z\nupdated_at: 2026-11-03T05:43:10Z\ncustom_fields: {}\n---\n";
    let parsed = parse_document(document.as_bytes(), Utf8Path::new("x.md")).unwrap();
    assert!(parsed.custom_fields.is_empty());
}

#[test]
fn a_nested_custom_field_list_is_refused() {
    let document = "---\nschema: 1\nid: wm-3f7k9q2m4x8c\ntitle: t\ncategory: bug\nstatus: open\nowner: frank\ncreated_at: 2026-11-03T05:43:10Z\nupdated_at: 2026-11-03T05:43:10Z\ncustom_fields:\n  tags:\n    - - nested\n---\n";
    let error = parse_document(document.as_bytes(), Utf8Path::new("x.md")).unwrap_err();
    assert_eq!(error.code(), Code::ValidationError);
    assert!(error.message().contains("custom_fields"), "{error}");
}

#[test]
fn a_null_custom_field_value_is_refused() {
    let document = "---\nschema: 1\nid: wm-3f7k9q2m4x8c\ntitle: t\ncategory: bug\nstatus: open\nowner: frank\ncreated_at: 2026-11-03T05:43:10Z\nupdated_at: 2026-11-03T05:43:10Z\ncustom_fields:\n  note: null\n---\n";
    let error = parse_document(document.as_bytes(), Utf8Path::new("x.md")).unwrap_err();
    assert_eq!(error.code(), Code::ValidationError);
    assert!(error.message().contains("null"), "{error}");
}

#[test]
fn custom_field_values_of_every_type_round_trip() {
    let mut item = fixtures::minimal_item("wm-3f7k9q2m4x8c");
    item.custom_fields
        .insert("text".to_owned(), FieldValue::String("a: b".to_owned()));
    item.custom_fields
        .insert("count".to_owned(), FieldValue::Integer(-7));
    item.custom_fields
        .insert("done".to_owned(), FieldValue::Boolean(true));
    item.custom_fields.insert(
        "tags".to_owned(),
        FieldValue::List(vec![
            FieldValue::String("x".to_owned()),
            FieldValue::String("y".to_owned()),
        ]),
    );
    item.custom_fields
        .insert("empty".to_owned(), FieldValue::List(Vec::new()));
    let document = to_document(&item);
    assert!(document.contains("\n  empty: []\n"), "{document}");
    let parsed = parse_document(document.as_bytes(), Utf8Path::new("x.md")).unwrap();
    assert_eq!(parsed, item);
}

#[test]
fn a_crlf_document_is_read_and_rewritten_with_lf() {
    let mut item = fixtures::minimal_item("wm-3f7k9q2m4x8c");
    item.body = "body\n".to_owned();
    let lf = to_document(&item);
    let crlf = lf.replace('\n', "\r\n");
    let parsed = parse_document(crlf.as_bytes(), Utf8Path::new("x.md")).unwrap();
    assert_eq!(parsed, item);
    assert_eq!(to_document(&parsed), lf);
}
