//! The configuration document: its canonical bytes, its validation, and the
//! round-trip invariant.

use std::collections::BTreeMap;

use camino::Utf8Path;
use waymark_core::config::{Category, Config, FieldDecl, FieldType, Version};
use waymark_core::error::Code;
use waymark_core::model::{CategoryKey, FieldKey, FieldValue};

/// The canonical document a fresh project gets. Pinned here, not in the library,
/// so a change to the writer is a visible change to this file.
const FRESH: &str = r#"version = "1.0"
storage_root = "work"

[categories.task]
path = "work/task"
name = "Task"
"#;

fn fresh() -> Config {
    Config::fresh()
}

#[test]
fn a_fresh_configuration_has_the_pinned_canonical_bytes() {
    assert_eq!(fresh().to_toml_string(), FRESH);
}

#[test]
fn rewriting_an_unchanged_configuration_is_byte_identical() {
    let config = fresh();
    let once = config.to_toml_string();
    let parsed = Config::parse(&once).expect("the canonical document parses");
    let twice = parsed.to_toml_string();
    assert_eq!(once, twice);
    assert_eq!(parsed, config);
}

#[test]
fn categories_and_fields_are_written_in_sorted_key_order() {
    let mut config = fresh();
    for (key, name) in [("zeta", "Zeta"), ("alpha", "Alpha"), ("beta", "Beta")] {
        config.categories.insert(
            CategoryKey::parse(key).unwrap(),
            Category {
                path: format!("work/{key}"),
                name: name.to_owned(),
            },
        );
    }
    for key in ["zeta_field", "alpha_field"] {
        config.fields.insert(
            FieldKey::parse(key).unwrap(),
            FieldDecl {
                ty: FieldType::String,
                required: false,
                default: None,
            },
        );
    }
    let document = config.to_toml_string();
    let alpha = document
        .find("[categories.alpha]")
        .expect("alpha is present");
    let beta = document.find("[categories.beta]").expect("beta is present");
    let zeta = document.find("[categories.zeta]").expect("zeta is present");
    assert!(alpha < beta && beta < zeta, "{document}");
    let alpha_field = document
        .find("[fields.alpha_field]")
        .expect("alpha_field is present");
    let zeta_field = document
        .find("[fields.zeta_field]")
        .expect("zeta_field is present");
    assert!(alpha_field < zeta_field, "{document}");
}

#[test]
fn field_declarations_round_trip_with_their_type_requiredness_and_default() {
    let mut config = fresh();
    config.fields.insert(
        FieldKey::parse("effort").unwrap(),
        FieldDecl {
            ty: FieldType::Integer,
            required: true,
            default: Some(FieldValue::Integer(3)),
        },
    );
    config.fields.insert(
        FieldKey::parse("tags").unwrap(),
        FieldDecl {
            ty: FieldType::List(Box::new(FieldType::String)),
            required: false,
            default: Some(FieldValue::List(vec![FieldValue::String("a".to_owned())])),
        },
    );
    let document = config.to_toml_string();
    assert!(document.contains("type = \"list:string\""), "{document}");
    assert!(document.contains("default = 3"), "{document}");
    assert_eq!(Config::parse(&document).unwrap(), config);
}

#[test]
fn field_type_spellings_round_trip_and_reject_unsupported_shapes() {
    for spelling in [
        "string",
        "integer",
        "boolean",
        "list:string",
        "list:integer",
        "list:boolean",
    ] {
        let parsed = FieldType::parse(spelling).expect("a pinned spelling");
        assert_eq!(parsed.as_str(), spelling);
    }
    for rejected in ["text", "float", "list:list:string", "list:", "string "] {
        let parsed = FieldType::parse(rejected);
        if rejected == "string " {
            // Surrounding whitespace is tolerated when reading, because TOML
            // already stripped the quoting.
            assert!(parsed.is_ok());
        } else {
            assert!(parsed.is_err(), "{rejected:?} must be rejected");
        }
    }
}

#[test]
fn an_unknown_top_level_key_is_reported_by_name() {
    let document = format!("{FRESH}\nunknown_key = 1\n");
    let error = Config::parse(&document).unwrap_err();
    assert_eq!(error.code(), Code::ConfigInvalid);
    assert!(error.message().contains("unknown_key"), "{error}");
}

#[test]
fn an_unknown_nested_key_is_reported_by_name() {
    let document = FRESH.replace("name = \"Task\"", "name = \"Task\"\n  colour = \"blue\"");
    let error = Config::parse(&document).unwrap_err();
    assert_eq!(error.code(), Code::ConfigInvalid);
    assert!(error.message().contains("colour"), "{error}");
}

#[test]
fn malformed_toml_is_a_config_invalid_error() {
    let error = Config::parse("version = ").unwrap_err();
    assert_eq!(error.code(), Code::ConfigInvalid);
}

#[test]
fn a_category_key_that_breaks_the_grammar_is_rejected() {
    for key in ["Bug", "bug report", "1bug", "bug_report", ""] {
        let document = FRESH.replace("[categories.task]", &format!("[categories.\"{key}\"]"));
        let error = Config::parse(&document).unwrap_err();
        assert_eq!(error.code(), Code::ConfigInvalid, "key {key:?}");
    }
}

#[test]
fn duplicate_category_paths_are_rejected() {
    let mut config = fresh();
    config.categories.insert(
        CategoryKey::parse("bug").unwrap(),
        Category {
            path: "work/task".to_owned(),
            name: "Bugs".to_owned(),
        },
    );
    let error = config.validate().unwrap_err();
    assert_eq!(error.code(), Code::ConfigInvalid);
    assert!(error.message().contains("work/task"), "{error}");
}

#[test]
fn overlapping_category_paths_are_rejected() {
    let mut config = fresh();
    config.categories.insert(
        CategoryKey::parse("bug").unwrap(),
        Category {
            path: "work/task/bugs".to_owned(),
            name: "Bugs".to_owned(),
        },
    );
    let error = config.validate().unwrap_err();
    assert_eq!(error.code(), Code::ConfigInvalid);
    assert!(error.message().contains("overlap"), "{error}");
}

#[test]
fn an_absolute_or_escaping_path_is_rejected() {
    for path in ["/work/task", "../outside", "work/../outside", "work//task"] {
        let mut config = fresh();
        config.storage_root = path.to_owned();
        let error = config.validate().unwrap_err();
        assert_eq!(error.code(), Code::ConfigInvalid, "path {path:?}");
    }
}

#[test]
fn a_backslash_separator_is_rejected() {
    let mut config = fresh();
    config.storage_root = "work\\tasks".to_owned();
    let error = config.validate().unwrap_err();
    assert_eq!(error.code(), Code::ConfigInvalid);
    assert!(error.message().contains("POSIX"), "{error}");
}

#[test]
fn a_symlinked_category_directory_is_refused_when_loading() {
    let dir = tempfile::tempdir().unwrap();
    let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    std::fs::create_dir_all(root.join(".waymark")).unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.join("linked")).unwrap();

    let mut config = fresh();
    config.categories.insert(
        CategoryKey::parse("bug").unwrap(),
        Category {
            path: "linked/bugs".to_owned(),
            name: "Bugs".to_owned(),
        },
    );
    std::fs::write(
        root.join(".waymark").join("config.toml"),
        config.to_toml_string(),
    )
    .unwrap();

    let error = Config::load(&root.join(".waymark").join("config.toml")).unwrap_err();
    assert_eq!(error.code(), Code::ConfigInvalid);
    assert!(error.message().contains("symbolic link"), "{error}");
}

#[test]
fn an_empty_display_name_is_rejected() {
    let document = FRESH.replace("name = \"Task\"", "name = \"  \"");
    let error = Config::parse(&document).unwrap_err();
    assert_eq!(error.code(), Code::ConfigInvalid);
    assert!(error.message().contains("name"), "{error}");
}

#[test]
fn the_local_settings_are_refused_by_name_with_their_destination() {
    // The three local settings are not merely unknown: they are recognised and
    // reported as belonging to the local document, so an old-shaped repository
    // gets an actionable message instead of a generic one.
    for (key, line) in [
        ("operator", "operator = \"frank\"\n"),
        ("timezone", "timezone = \"UTC\"\n"),
        ("cache", "[cache]\nenabled = true\ncapacity = 512\n"),
    ] {
        // Inserted at the first top-level key, because a key appended after a
        // table header would belong to that table instead.
        let document = FRESH.replace(
            "storage_root = \"work\"\n",
            &format!("storage_root = \"work\"\n{line}"),
        );
        let error = Config::parse(&document).unwrap_err();
        assert_eq!(error.code(), Code::ConfigInvalid, "key {key:?}");
        assert!(
            error.message().contains(key),
            "the complaint names {key:?}: {error}"
        );
        assert!(
            error.message().contains("local user configuration"),
            "the complaint names the destination: {error}"
        );
    }
}

#[test]
fn a_version_this_release_cannot_read_is_rejected_as_unsupported() {
    let document = FRESH.replace("version = \"1.0\"", "version = \"2.0\"");
    let error = Config::parse(&document).unwrap_err();
    assert_eq!(error.code(), Code::UnsupportedVersion);
    assert!(error.message().contains("generation"), "{error}");
}

#[test]
fn the_previous_generation_is_still_readable() {
    let document = FRESH.replace("version = \"1.0\"", "version = \"0.4\"");
    let config = Config::parse(&document).expect("the previous generation is readable");
    assert_eq!(config.version.generation, 0);
    assert_eq!(config.version.revision, 4);
}

#[test]
fn a_generation_older_than_the_previous_is_unsupported() {
    let document = FRESH.replace("version = \"1.0\"", "version = \"9.0\"");
    let error = Config::parse(&document).unwrap_err();
    assert_eq!(error.code(), Code::UnsupportedVersion);
    assert!(
        error.details().get("supported").is_some(),
        "the readable window is reported: {error}"
    );
}

#[test]
fn an_unparseable_version_is_rejected() {
    for version in ["1", "one.two", "1.x", "", "1.2.3"] {
        let document = FRESH.replace("version = \"1.0\"", &format!("version = \"{version}\""));
        let error = Config::parse(&document).unwrap_err();
        assert_eq!(error.code(), Code::ConfigInvalid, "version {version:?}");
    }
}

#[test]
fn a_version_round_trips_through_its_literal() {
    let version = Version {
        generation: 1,
        revision: 7,
    };
    assert_eq!(version.to_string(), "1.7");
    assert_eq!(Version::parse("1.7").unwrap(), version);
    assert_eq!(
        Version::current(),
        Version {
            generation: 1,
            revision: 0
        }
    );
}

#[test]
fn a_default_that_does_not_match_the_declared_type_is_rejected() {
    let document = format!(
        "{FRESH}\n[fields.effort]\ntype = \"integer\"\nrequired = false\ndefault = \"three\"\n"
    );
    let error = Config::parse(&document).unwrap_err();
    assert_eq!(error.code(), Code::ConfigInvalid);
    assert!(error.message().contains("default"), "{error}");
}

#[test]
fn a_field_may_not_shadow_a_core_name() {
    let document = format!("{FRESH}\n[fields.title]\ntype = \"string\"\nrequired = false\n");
    let error = Config::parse(&document).unwrap_err();
    assert_eq!(error.code(), Code::ConfigInvalid);
    assert!(error.message().contains("reserved"), "{error}");
}

#[test]
fn the_declared_categories_are_what_the_configuration_holds() {
    let config = Config::parse(FRESH).unwrap();
    assert_eq!(config.categories.len(), 1);
    assert_eq!(
        config.categories[&CategoryKey::parse("task").unwrap()].path,
        "work/task"
    );
    assert_eq!(config.storage_root, "work");
    assert!(config.fields.is_empty());
}

#[test]
fn the_raw_tables_are_ordered_by_key_when_read() {
    let document = format!(
        "{FRESH}\n[fields.beta]\ntype = \"string\"\nrequired = false\n\n[fields.alpha]\ntype = \"string\"\nrequired = false\n"
    );
    let config = Config::parse(&document).unwrap();
    let keys: Vec<&str> = config.fields.keys().map(FieldKey::as_str).collect();
    assert_eq!(keys, vec!["alpha", "beta"]);
}

#[test]
fn a_configuration_file_that_does_not_exist_is_an_io_error() {
    let error = Config::load(Utf8Path::new("/nonexistent/.waymark/config.toml")).unwrap_err();
    assert_eq!(error.code(), Code::IoError);
}

#[test]
fn a_configuration_document_without_a_category_still_parses() {
    let document = "version = \"1.0\"\nstorage_root = \"work\"\n";
    let config = Config::parse(document).unwrap();
    assert!(config.categories.is_empty());
    assert_eq!(config.to_toml_string(), document);
}

#[test]
fn a_quoted_string_survives_the_writer() {
    let mut config = fresh();
    config.storage_root = "a \"quoted\" operator".to_owned();
    let document = config.to_toml_string();
    assert_eq!(
        Config::parse(&document).unwrap().storage_root,
        config.storage_root
    );
}

#[test]
fn categories_are_stored_as_a_map_keyed_by_their_validated_key() {
    let config: BTreeMap<CategoryKey, Category> = Config::parse(FRESH).unwrap().categories;
    assert!(config.contains_key(&CategoryKey::parse("task").unwrap()));
}
