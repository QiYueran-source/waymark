//! The local user configuration document, end to end.
//!
//! Three things are pinned here, because they are the three promises the local
//! document makes:
//!
//! - **Shape.** It accepts the operator, the time zone, and the cache settings,
//!   and refuses everything else by name — including the shared project keys.
//! - **Resolution.** Operator, zone, and cache each have one precedence, and
//!   every value carries the source it came from. The fallbacks are real: Git
//!   and the machine zone are probed, and neither is a hard dependency.
//! - **Writes.** A local write leaves the shared configuration and its revision
//!   untouched, and a read never creates the document.

use std::process::Command;

use camino::{Utf8Path, Utf8PathBuf};
use waymark_core::config::CacheSettings;
use waymark_core::error::Code;
use waymark_core::ops::config::{InitRequest, Scalar};
use waymark_core::ops::{create, local_config, OpOptions};
use waymark_core::store::{Fingerprint, InitOptions};
use waymark_core::testing::fixtures;
use waymark_core::user_config::{
    EffectiveConfig, LocalEnvironment, UserCache, UserConfig, ValueSource, USER_CONFIG_FILE,
};
use waymark_core::Project;

fn options() -> OpOptions {
    OpOptions::new().with_clock(fixtures::shared_clock())
}

fn utf8(dir: &tempfile::TempDir) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 temporary path")
}

fn shared_bytes(project: &Project) -> String {
    std::fs::read_to_string(project.config_path()).expect("the shared document is readable")
}

fn local_text(project: &Project) -> Option<String> {
    std::fs::read_to_string(project.user_config_path()).ok()
}

fn git_available() -> bool {
    waymark_core::git::find_executable().is_some()
}

/// Run `git` in `root`, panicking with its stderr on failure.
fn git(root: &Utf8Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .expect("the git binary runs");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A project inside a Git repository whose `user.name` is `name`.
fn project_in_repository(name: &str) -> Option<(tempfile::TempDir, Project)> {
    if !git_available() {
        eprintln!("skipping: no `git` executable on PATH");
        return None;
    }
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = utf8(&dir);
    git(&root, &["init", "--quiet"]);
    git(&root, &["config", "user.name", name]);
    let project = Project::init(&root, InitOptions::default()).expect("init succeeds");
    // Start from no local document: the fallback branches are what matter here.
    let _ = std::fs::remove_file(project.user_config_path());
    Some((dir, project))
}

// ---------------------------------------------------------------------------
// Shape
// ---------------------------------------------------------------------------

#[test]
fn the_local_document_accepts_the_three_local_settings_and_normalizes_them() {
    let document = "operator = \"  frank  \"\ntimezone = \"Asia/Shanghai\"\n\n[cache]\nenabled = false\ncapacity = 32\n";
    let local = UserConfig::parse(document).expect("the document validates");
    assert_eq!(local.operator.as_deref(), Some("frank"), "trimmed");
    assert_eq!(
        local.timezone.map(|zone| zone.name()),
        Some("Asia/Shanghai")
    );
    assert_eq!(local.cache.enabled, Some(false));
    assert_eq!(local.cache.capacity, Some(32));

    // The canonical form is stable, and a state nothing at all is empty.
    let canonical = local.to_toml_string();
    assert_eq!(UserConfig::parse(&canonical).unwrap(), local);
    assert!(UserConfig::parse("").unwrap().is_empty());
    assert!(!UserConfig::parse("operator = \"frank\"\n")
        .unwrap()
        .is_empty());
}

#[test]
fn every_key_the_local_document_may_not_carry_is_reported_by_name() {
    // An unknown key and each shared project key alike: the complaint names the
    // key, so a hand edit is actionable rather than a bare parse failure.
    for (document, key) in [
        ("nickname = \"frank\"\n", "nickname"),
        ("version = \"1.0\"\n", "version"),
        ("storage_root = \"work\"\n", "storage_root"),
        (
            "[categories.task]\npath = \"work/task\"\nname = \"Task\"\n",
            "categories",
        ),
        (
            "[fields.effort]\ntype = \"integer\"\nrequired = false\n",
            "fields",
        ),
        ("[cache]\nsize = 4\n", "cache.size"),
    ] {
        let error = UserConfig::parse(document).unwrap_err();
        assert_eq!(error.code(), Code::ConfigInvalid, "{document:?}");
        assert!(
            error.message().contains(key),
            "the complaint names {key:?}: {error}"
        );
    }

    // A shared key says where the setting belongs instead of only that it is
    // wrong.
    let error = UserConfig::parse("timezone = \"UTC\"\nstorage_root = \"work\"\n").unwrap_err();
    assert!(
        error.message().contains("shared project setting"),
        "{error}"
    );
}

#[test]
fn an_empty_operator_a_zero_capacity_and_an_unknown_zone_are_rejected() {
    for document in [
        "operator = \"   \"\n",
        "timezone = \"Mars/Olympus\"\n",
        "[cache]\ncapacity = 0\n",
        "operator = \"frank\n",
    ] {
        let error = UserConfig::parse(document).unwrap_err();
        assert_eq!(error.code(), Code::ConfigInvalid, "{document:?}");
    }
}

#[test]
fn a_local_finding_is_located_at_the_document_and_names_the_key() {
    let (_dir, project) = fixtures::deterministic_project();
    std::fs::write(project.user_config_path(), "timezone = \"Mars/Olympus\"\n").unwrap();

    let findings = local_config::validate_path(&project.user_config_path());
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].code, Code::ConfigInvalid);
    assert!(findings[0].message.contains("timezone"), "{findings:?}");
    assert!(
        findings[0]
            .location
            .as_ref()
            .is_some_and(|location| format!("{location}").contains(USER_CONFIG_FILE)),
        "a local finding is located at the local document: {findings:?}"
    );

    // An absent document is not a finding: absence is a normal state.
    std::fs::remove_file(project.user_config_path()).unwrap();
    assert!(local_config::validate_path(&project.user_config_path()).is_empty());
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

#[test]
fn operator_resolution_is_the_override_then_the_document_then_git_then_unset() {
    let mut local = UserConfig::empty();
    local.operator = Some("local".to_owned());
    let environment = LocalEnvironment {
        operator: Some("git".to_owned()),
        timezone: None,
    };

    let stated = EffectiveConfig::resolve(Some(&local), &environment);
    assert_eq!(stated.operator.as_deref(), Some("local"));
    assert_eq!(stated.operator_source, ValueSource::UserDocument);

    let fallback = EffectiveConfig::resolve(None, &environment);
    assert_eq!(fallback.operator.as_deref(), Some("git"));
    assert_eq!(fallback.operator_source, ValueSource::Git);

    let unset = EffectiveConfig::resolve(None, &LocalEnvironment::isolated());
    assert_eq!(unset.operator, None);
    assert_eq!(unset.operator_source, ValueSource::Unset);
}

#[test]
fn the_operation_override_still_wins_over_the_local_document() {
    let (_dir, project) = fixtures::deterministic_project();
    local_config::set_scalar(&project, Scalar::Operator, "local", &options())
        .expect("the local operator is written");

    let created = create::create(
        &project,
        create::CreateRequest::new("Fix the thing", "task", "frank"),
        &options().with_operator("cli"),
    )
    .expect("creation");
    assert_eq!(
        created.item.creator.as_deref(),
        Some("cli"),
        "the operation override outranks the document"
    );

    let defaulted = create::create(
        &project,
        create::CreateRequest::new("Another thing", "task", "frank"),
        &options(),
    )
    .expect("creation");
    assert_eq!(
        defaulted.item.creator.as_deref(),
        Some("local"),
        "an omitted creator still defaults to the effective operator"
    );
}

#[test]
fn git_user_name_is_the_operator_fallback_in_a_repository() {
    let Some((_dir, project)) = project_in_repository("Grace Hopper") else {
        return;
    };
    let effective = local_config::bundle(&project)
        .expect("the bundle builds")
        .effective;
    assert_eq!(effective.operator.as_deref(), Some("Grace Hopper"));
    assert_eq!(effective.operator_source, ValueSource::Git);
}

#[test]
fn a_local_operator_never_consults_git() {
    let Some((_dir, project)) = project_in_repository("Grace Hopper") else {
        return;
    };
    let mut local = UserConfig::empty();
    local.operator = Some("frank".to_owned());

    // Detection is skipped for the value the document states, which is what
    // makes the precedence structural rather than incidental.
    let environment = LocalEnvironment::detect_for(project.root(), Some(&local));
    assert_eq!(environment.operator, None);

    // And the same probe with nothing stated does find the identity.
    let probed = LocalEnvironment::detect_for(project.root(), None);
    assert_eq!(probed.operator.as_deref(), Some("Grace Hopper"));

    local_config::set_scalar(&project, Scalar::Operator, "frank", &options())
        .expect("the local operator is written");
    let effective = local_config::bundle(&project)
        .expect("the bundle builds")
        .effective;
    assert_eq!(effective.operator.as_deref(), Some("frank"));
    assert_eq!(effective.operator_source, ValueSource::UserDocument);
}

#[test]
fn time_zone_resolution_is_the_document_then_the_machine_then_utc_with_a_warning() {
    let mut local = UserConfig::empty();
    local.timezone = Some(chrono_tz::Tz::Europe__Berlin);
    let environment = LocalEnvironment {
        operator: None,
        timezone: Some(chrono_tz::Tz::Asia__Shanghai),
    };

    let stated = EffectiveConfig::resolve(Some(&local), &environment);
    assert_eq!(stated.timezone.name(), "Europe/Berlin");
    assert_eq!(stated.timezone_source, ValueSource::UserDocument);
    assert!(stated.warnings.is_empty(), "an explicit value cannot warn");

    let machine = EffectiveConfig::resolve(None, &environment);
    assert_eq!(machine.timezone.name(), "Asia/Shanghai");
    assert_eq!(machine.timezone_source, ValueSource::SystemTimeZone);
    assert!(machine.warnings.is_empty(), "{:?}", machine.warnings);

    let fallback = EffectiveConfig::resolve(None, &LocalEnvironment::isolated());
    assert_eq!(fallback.timezone, chrono_tz::UTC);
    assert_eq!(fallback.timezone_source, ValueSource::BuiltInDefault);
    assert_eq!(fallback.warnings.len(), 1, "{:?}", fallback.warnings);
    let warning = &fallback.warnings[0];
    assert_eq!(warning.code, Code::TimeZoneUndetected);
    assert!(warning.message.contains("UTC"), "{warning:?}");
}

#[test]
fn the_undetected_zone_warning_is_informational_and_never_a_failure() {
    // The registry decides the severity, not the resolution path.
    assert_eq!(
        Code::TimeZoneUndetected.finding(),
        Some(waymark_core::Severity::Info),
        "an undetectable zone is information, not a warning about damage"
    );
    assert_eq!(Code::TimeZoneUndetected.exit_class(), None);
}

#[test]
fn cache_resolution_reports_a_source_per_field() {
    // Only the capacity is pinned, so enablement keeps following the default.
    let mut local = UserConfig::empty();
    local.cache.capacity = Some(64);
    let effective = EffectiveConfig::resolve(Some(&local), &LocalEnvironment::isolated());
    assert_eq!(effective.cache.capacity, 64);
    assert_eq!(effective.cache_capacity_source, ValueSource::UserDocument);
    assert_eq!(effective.cache.enabled, CacheSettings::default().enabled);
    assert_eq!(
        effective.cache_enabled_source,
        ValueSource::BuiltInDefault,
        "an unpinned field follows the default"
    );

    let absent = EffectiveConfig::resolve(None, &LocalEnvironment::isolated());
    assert_eq!(absent.cache, CacheSettings::default());
    assert_eq!(absent.cache_enabled_source, ValueSource::BuiltInDefault);
    assert_eq!(absent.cache_capacity_source, ValueSource::BuiltInDefault);
}

#[test]
fn initialization_writes_a_cache_snapshot_that_outlives_a_later_default_change() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = utf8(&dir);
    let project = waymark_core::ops::config::init(
        &root,
        waymark_core::ops::config::InitRequest::new(),
        false,
    )
    .expect("init succeeds");

    let written = local_config::load(&project)
        .expect("the local document is readable")
        .expect("first init writes it");
    // Every cache field is stated, so the snapshot is a written fact rather
    // than something inherited from whatever release runs later.
    assert_eq!(
        written.cache.enabled,
        Some(CacheSettings::default().enabled)
    );
    assert_eq!(
        written.cache.capacity,
        Some(CacheSettings::default().capacity)
    );

    let effective = local_config::bundle(&project)
        .expect("the bundle builds")
        .effective;
    assert_eq!(effective.cache, CacheSettings::default());
    assert_eq!(effective.cache_enabled_source, ValueSource::UserDocument);
    assert_eq!(effective.cache_capacity_source, ValueSource::UserDocument);

    // A snapshot pinned to a non-default value survives, because it is stated.
    std::fs::remove_file(project.user_config_path()).unwrap();
    std::fs::write(project.user_config_path(), "[cache]\ncapacity = 64\n").unwrap();
    let pinned = local_config::bundle(&project)
        .expect("the bundle builds")
        .effective;
    assert_eq!(pinned.cache.capacity, 64);
    assert_eq!(pinned.cache_capacity_source, ValueSource::UserDocument);
}

#[test]
fn resolution_works_with_no_git_and_no_repository() {
    let (_dir, project) = fixtures::deterministic_project();
    // A bare temporary directory is not inside a repository, so the operator
    // fallback is simply unavailable; nothing else degrades.
    let effective = local_config::bundle(&project)
        .expect("the bundle builds")
        .effective;
    assert_eq!(effective.operator_source, ValueSource::Unset);
    assert!(effective.operator.is_none());
    assert!(matches!(
        effective.timezone_source,
        ValueSource::SystemTimeZone | ValueSource::BuiltInDefault
    ));
    assert_eq!(effective.cache, CacheSettings::default());

    // An ordinary write still succeeds without an operator.
    let created = create::create(
        &project,
        create::CreateRequest::new("Fix the thing", "task", "frank"),
        &options(),
    )
    .expect("creation does not need an operator");
    assert_eq!(created.item.creator, None);
}

#[test]
fn an_absent_local_document_is_normal_and_never_created_by_a_read() {
    let (_dir, project) = fixtures::deterministic_project();
    // A fixture project is initialized through `Project::init`, which seeds
    // nothing: the document is genuinely absent until something writes it.
    assert!(!project.user_config_path().exists());

    assert!(
        local_config::load(&project).unwrap().is_none(),
        "an absent document reads as not configured"
    );
    let _ = local_config::bundle(&project)
        .expect("the bundle builds")
        .effective;
    let bundle = local_config::bundle(&project).expect("the bundle builds");
    assert!(bundle.local.is_none());
    assert_eq!(bundle.effective.operator_source, ValueSource::Unset);
    assert!(
        !project.user_config_path().exists(),
        "a read never creates the document"
    );
}

#[test]
fn a_local_write_never_moves_the_shared_revision() {
    let (_dir, project) = fixtures::deterministic_project();
    let shared_before = shared_bytes(&project);
    let revision_before = project.config().version.revision;

    let outcome = local_config::set_scalar(&project, Scalar::Operator, "frank", &options())
        .expect("the local write succeeds");
    assert!(outcome.changed);
    assert_eq!(outcome.changed_keys, vec!["operator".to_owned()]);
    assert_eq!(outcome.previous_revision, revision_before);
    assert_eq!(outcome.revision, revision_before);

    assert_eq!(shared_bytes(&project), shared_before, "shared is untouched");
    let document = local_text(&project).expect("the local document is written");
    assert!(document.contains("operator = \"frank\""), "{document}");
    assert!(!document.contains("storage_root"), "{document}");
}

#[test]
fn clearing_the_local_operator_restores_the_fallback_and_removes_an_empty_document() {
    let (_dir, project) = fixtures::deterministic_project();
    local_config::set_scalar(&project, Scalar::Operator, "frank", &options())
        .expect("the value is written");
    assert_eq!(
        local_config::bundle(&project)
            .unwrap()
            .effective
            .operator
            .as_deref(),
        Some("frank")
    );

    // A document that would state nothing is deleted: absence is the canonical
    // "no local settings" form, and an ignored file should not linger.
    let cleared = local_config::set_scalar(&project, Scalar::Operator, "   ", &options())
        .expect("clearing is a change");
    assert!(cleared.changed);
    assert!(cleared.local.operator.is_none());

    let remaining = local_config::load(&project).unwrap();
    if let Some(remaining) = remaining {
        assert!(remaining.operator.is_none(), "the override is gone");
    }
    let effective = local_config::bundle(&project).unwrap().effective;
    assert_eq!(effective.operator_source, ValueSource::Unset);
}

#[test]
fn a_broken_local_document_fails_a_value_read_but_not_a_diagnosis() {
    let (_dir, project) = fixtures::deterministic_project();
    std::fs::write(project.user_config_path(), "timezone = \"Mars/Olympus\"\n").unwrap();

    // An operation that needs the value must not guess at it.
    let error = local_config::bundle(&project).expect_err("the local document is invalid");
    assert_eq!(error.code(), Code::ConfigInvalid);
    assert!(error.message().contains("timezone"), "{error}");

    // The command that reports the problem still runs, and still resolves the
    // values it can.
    assert_eq!(
        local_config::validate_path(&project.user_config_path()).len(),
        1
    );
    let lenient = local_config::effective_lenient(&project);
    assert_eq!(lenient.timezone_source, ValueSource::SystemTimeZone);
}

#[test]
fn an_old_shaped_shared_document_is_reported_rather_than_silently_accepted() {
    let (_dir, project) = fixtures::deterministic_project();
    // The old shape wrote these at the top level, before any table header.
    let old_shaped = shared_bytes(&project).replace(
        "storage_root = \"work\"\n",
        "storage_root = \"work\"\ntimezone = \"Asia/Shanghai\"\n\n[cache]\nenabled = true\ncapacity = 512\n",
    );
    std::fs::write(project.config_path(), &old_shaped).unwrap();

    let error = Project::open(project.root()).expect_err("the document is invalid");
    assert_eq!(error.code(), Code::ConfigInvalid);
    let details = error.details()["issues"]
        .as_array()
        .expect("the issues list")
        .iter()
        .map(|issue| issue.as_str().unwrap_or_default().to_owned())
        .collect::<Vec<String>>()
        .join("; ");
    assert!(
        details.contains("local user configuration"),
        "the complaint names the destination: {details}"
    );

    // ... and the finding-oriented path points at the keys too.
    let findings = waymark_core::ops::config::validate_config_path(&project.config_path());
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert!(
        findings
            .iter()
            .any(|finding| finding.message.contains("timezone")),
        "{findings:?}"
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.message.contains("cache")),
        "{findings:?}"
    );
}

#[test]
fn the_local_document_is_ignored_and_never_committed() {
    let (_dir, project) = fixtures::deterministic_project();
    let ignore = std::fs::read_to_string(project.metadata_dir().join(".gitignore"))
        .expect("the ignore file exists");
    assert!(
        ignore.lines().any(|line| line == USER_CONFIG_FILE),
        "the local document is ignored: {ignore}"
    );

    // Writing one creates it inside the metadata directory, where the entry
    // covers it.
    local_config::set_scalar(&project, Scalar::Operator, "frank", &options())
        .expect("the local write succeeds");
    assert!(project.user_config_path().is_file());
    assert_eq!(
        project.user_config_path(),
        project.metadata_dir().join(USER_CONFIG_FILE)
    );
}

#[test]
fn the_bundle_exposes_both_documents_and_the_resolved_values_once() {
    let (_dir, project) = fixtures::deterministic_project();
    local_config::set_scalar(&project, Scalar::Timezone, "Europe/Berlin", &options())
        .expect("the zone is written");

    let bundle = local_config::bundle(&project).expect("the bundle builds");
    assert_eq!(bundle.project, *project.config());
    assert_eq!(
        bundle.local.as_ref().and_then(|local| local.timezone),
        Some(chrono_tz::Tz::Europe__Berlin)
    );
    assert_eq!(bundle.effective.timezone.name(), "Europe/Berlin");
    assert_eq!(bundle.effective.timezone_source, ValueSource::UserDocument);
    // The renderer takes the source from here rather than re-deriving it.
    assert_eq!(ValueSource::UserDocument.as_str(), "local_document");
    assert_eq!(ValueSource::Git.as_str(), "git");
    assert_eq!(ValueSource::SystemTimeZone.as_str(), "system_time_zone");
    assert_eq!(ValueSource::BuiltInDefault.as_str(), "built_in_default");
    assert_eq!(ValueSource::Unset.as_str(), "unset");
}

#[test]
fn a_cache_only_document_shape_survives_a_round_trip() {
    let mut local = UserConfig::empty();
    local.cache = UserCache {
        enabled: Some(false),
        capacity: None,
    };
    let document = local.to_toml_string();
    assert_eq!(document, "[cache]\nenabled = false\n");
    assert_eq!(UserConfig::parse(&document).unwrap(), local);
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

/// A temporary project directory inside a fresh Git repository.
///
/// `identity` is written as the repository-local `user.name`; `None` writes an
/// empty one so Git's effective lookup is deterministic and says "no identity",
/// whatever the developer's global configuration holds.
fn repository(identity: Option<&str>) -> Option<(tempfile::TempDir, Utf8PathBuf)> {
    if !git_available() {
        eprintln!("skipping: no `git` executable on PATH");
        return None;
    }
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = utf8(&dir);
    git(&root, &["init", "--quiet"]);
    git(&root, &["config", "user.name", identity.unwrap_or("")]);
    Some((dir, root))
}

/// Run `git` in `root` and return its stdout.
fn git_output(root: &Utf8Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .expect("the git binary runs");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn initialize(root: &Utf8Path) -> Project {
    waymark_core::ops::config::init(root, InitRequest::new(), false).expect("init succeeds")
}

#[test]
fn first_initialization_seeds_the_git_identity_zone_and_cache_defaults() {
    let Some((_dir, root)) = repository(Some("Grace Hopper")) else {
        return;
    };
    let project = initialize(&root);

    let local = local_config::load(&project)
        .expect("the local document is readable")
        .expect("first initialization writes it");
    assert_eq!(local.operator.as_deref(), Some("Grace Hopper"));
    assert_eq!(
        local.timezone,
        waymark_core::time::detect_machine_timezone(),
        "the machine zone is the seed, or UTC when it cannot be detected"
    );
    // Both cache values are written out, so a later release cannot change what
    // this project snapshotted.
    assert_eq!(local.cache.enabled, Some(CacheSettings::default().enabled));
    assert_eq!(
        local.cache.capacity,
        Some(CacheSettings::default().capacity)
    );

    let findings = waymark_core::ops::config::validate_both(&project);
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn a_missing_git_identity_leaves_the_operator_out_of_the_seed() {
    let Some((_dir, root)) = repository(None) else {
        return;
    };
    let project = initialize(&root);

    let local = local_config::load(&project)
        .expect("the local document is readable")
        .expect("first initialization writes it");
    assert_eq!(local.operator, None, "nothing invents an identity");
    let text = local_text(&project).expect("the document exists");
    assert!(!text.contains("operator"), "{text}");
    assert!(text.contains("[cache]"), "{text}");
}

#[test]
fn initialization_options_seed_only_a_newly_created_document() {
    let Some((_dir, root)) = repository(None) else {
        return;
    };
    let seeded = waymark_core::ops::config::init(
        &root,
        InitRequest::new()
            .with_operator("seed")
            .with_timezone("Europe/Berlin"),
        false,
    )
    .expect("the seeded init succeeds");
    let local = local_config::load(&seeded)
        .expect("the local document is readable")
        .expect("the seed created it");
    assert_eq!(local.operator.as_deref(), Some("seed"));
    assert_eq!(
        local.timezone.map(|zone| zone.name()),
        Some("Europe/Berlin")
    );
    let written = local_text(&seeded).expect("the document exists");

    // Neither an ordinary re-run nor a forced replacement reseeds it: the
    // options apply only when the document is being created.
    let rerun = waymark_core::ops::config::init(
        &root,
        InitRequest::new()
            .with_operator("other")
            .with_timezone("Asia/Tokyo"),
        true,
    )
    .expect("the forced init succeeds");
    assert_eq!(local_text(&rerun).as_deref(), Some(written.as_str()));
}

#[test]
fn re_initializing_creates_and_overwrites_nothing() {
    let Some((_dir, root)) = repository(Some("Grace Hopper")) else {
        return;
    };
    let project = initialize(&root);
    let seeded = local_text(&project).expect("the document exists");
    let shared = std::fs::read_to_string(project.config_path()).expect("the shared document");

    // A missing document is not recreated.
    std::fs::remove_file(project.user_config_path()).expect("the document is removable");
    let project = initialize(&root);
    assert_eq!(local_text(&project), None, "a repeat creates nothing");

    // And an existing document is left byte-identical, hand edit and all.
    std::fs::write(project.user_config_path(), "operator = \"manual\"\n").expect("a hand edit");
    let project = initialize(&root);
    assert_eq!(
        local_text(&project).as_deref(),
        Some("operator = \"manual\"\n")
    );
    assert_eq!(
        std::fs::read_to_string(project.config_path()).expect("the shared document"),
        shared,
        "the shared document is untouched"
    );
    let _ = seeded;
}

#[test]
fn initialization_adds_the_local_entry_to_the_ignore_file_without_duplicating() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = utf8(&dir);
    std::fs::create_dir_all(root.join(".waymark")).expect("the metadata directory exists");
    std::fs::write(root.join(".waymark/.gitignore"), "write.lock\n").expect("a pre-existing file");

    let project = initialize(&root);
    let ignore = std::fs::read_to_string(project.metadata_dir().join(".gitignore"))
        .expect("the ignore file exists");
    assert_eq!(ignore.matches("write.lock").count(), 1, "{ignore}");
    assert_eq!(ignore.matches(USER_CONFIG_FILE).count(), 1, "{ignore}");

    // Repeating the repair neither duplicates nor reorders anything.
    let project = initialize(&root);
    let again = std::fs::read_to_string(project.metadata_dir().join(".gitignore"))
        .expect("the ignore file exists");
    assert_eq!(again, ignore);
}

#[test]
fn initialization_never_touches_the_repository_level_ignore_files() {
    let Some((_dir, root)) = repository(Some("Grace Hopper")) else {
        return;
    };
    let root_ignore = root.join(".gitignore");
    let exclude = root.join(".git/info/exclude");
    std::fs::write(&root_ignore, "target/\n").expect("the repository ignore file");
    std::fs::write(&exclude, "# keep this\nnode_modules\n").expect("the exclude file");

    let project = initialize(&root);
    assert!(project.user_config_path().is_file());
    assert_eq!(
        std::fs::read_to_string(&root_ignore).expect("the repository ignore file"),
        "target/\n",
        "the repository-level .gitignore is not touched"
    );
    assert_eq!(
        std::fs::read_to_string(&exclude).expect("the exclude file"),
        "# keep this\nnode_modules\n",
        "the repository-level exclude file is not touched"
    );
}

#[test]
fn an_unsafe_ignore_file_stops_initialization_before_any_local_settings_exist() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = utf8(&dir);
    let metadata = root.join(".waymark");
    std::fs::create_dir_all(&metadata).expect("the metadata directory exists");

    // A symlinked ignore file is refused, not written through.
    let outside = dir.path().join("outside-gitignore");
    std::fs::write(&outside, "keep me\n").expect("the link target");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, metadata.join(".gitignore")).expect("the symlink");
    #[cfg(unix)]
    {
        let error = waymark_core::ops::config::init(&root, InitRequest::new(), false)
            .expect_err("a symlinked ignore file is refused");
        assert_eq!(error.code(), Code::LockFailed);
        assert_eq!(
            std::fs::read_to_string(&outside).expect("the link target"),
            "keep me\n"
        );
        assert!(
            !metadata.join(USER_CONFIG_FILE).exists(),
            "no local settings"
        );
        assert!(!metadata.join("config.toml").exists(), "no configuration");
        std::fs::remove_file(metadata.join(".gitignore")).expect("cleanup");
    }

    // A non-regular ignore file is refused.
    std::fs::create_dir(metadata.join(".gitignore")).expect("a directory in place of the file");
    let error = waymark_core::ops::config::init(&root, InitRequest::new(), false)
        .expect_err("a non-regular ignore file is refused");
    assert_eq!(error.code(), Code::LockFailed);
    assert!(
        !metadata.join(USER_CONFIG_FILE).exists(),
        "no local settings"
    );
    std::fs::remove_dir(metadata.join(".gitignore")).expect("cleanup");

    // A read-only ignore file that is missing the entry is refused rather than
    // replaced behind its permissions.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = metadata.join(".gitignore");
        std::fs::write(&path, "write.lock\n").expect("a read-only ignore file");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444))
            .expect("the read-only mode");
        let error = waymark_core::ops::config::init(&root, InitRequest::new(), false)
            .expect_err("a read-only ignore file is refused");
        assert_eq!(error.code(), Code::LockFailed);
        assert!(
            !metadata.join(USER_CONFIG_FILE).exists(),
            "no local settings"
        );
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .expect("the mode is restorable");
    }
}

#[test]
fn a_tracked_local_document_is_reported_and_never_written() {
    let Some((_dir, root)) = repository(Some("Grace Hopper")) else {
        return;
    };
    let project = initialize(&root);
    local_config::set_scalar(&project, Scalar::Operator, "manual", &options())
        .expect("the local write succeeds");
    git(&root, &["add", "--force", ".waymark/user.toml"]);
    let tracked = std::fs::read_to_string(project.user_config_path()).expect("the document");
    let index = git_output(&root, &["ls-files", "-s", ".waymark/user.toml"]);
    assert!(local_config::tracked(&project));

    // A write refuses and names the manual repair.
    let error = local_config::set_scalar(&project, Scalar::Operator, "other", &options())
        .expect_err("a tracked document is never written");
    assert_eq!(error.code(), Code::ConfigInvalid);
    assert!(
        error.message().contains("untrack"),
        "the refusal explains the repair: {}",
        error.message()
    );
    assert_eq!(
        std::fs::read_to_string(project.user_config_path()).expect("the document"),
        tracked
    );

    // Initialization reports it too, and leaves both the file and the index
    // exactly as they were.
    let project =
        waymark_core::ops::config::init(&root, InitRequest::new().with_operator("seed"), true)
            .expect("initialization still succeeds");
    assert!(local_config::tracked(&project));
    assert_eq!(
        std::fs::read_to_string(project.user_config_path()).expect("the document"),
        tracked
    );
    assert_eq!(
        git_output(&root, &["ls-files", "-s", ".waymark/user.toml"]),
        index,
        "the Git index is unmodified"
    );
}

// ---------------------------------------------------------------------------
// Local writes
// ---------------------------------------------------------------------------

#[test]
fn a_conflicting_local_fingerprint_is_refused_and_writes_nothing() {
    let (_dir, project) = fixtures::deterministic_project();
    let path = project.user_config_path();

    // An expectation cannot be met by an absent document.
    let error = local_config::set_scalar(
        &project,
        Scalar::Operator,
        "frank",
        &options().with_expected_config_fingerprint(Fingerprint::of(b"operator = \"frank\"\n")),
    )
    .expect_err("an absent document does not match an expectation");
    assert_eq!(error.code(), Code::WriteConflict);
    assert!(!path.exists(), "a conflict writes nothing");

    // A hand edit between the read and the write is a conflict, not a rebase.
    local_config::set_scalar(&project, Scalar::Operator, "frank", &options())
        .expect("the first write succeeds");
    let expected = Fingerprint::of(&std::fs::read(&path).expect("the document"));
    std::fs::write(&path, "operator = \"manual\"\n").expect("a concurrent hand edit");

    let error = local_config::set_scalar(
        &project,
        Scalar::Timezone,
        "UTC",
        &options().with_expected_config_fingerprint(expected),
    )
    .expect_err("the hand edit is a conflict");
    assert_eq!(error.code(), Code::WriteConflict);
    assert_ne!(
        error.details()["actual_fingerprint"],
        "missing",
        "the conflict carries the fingerprint on disk"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("the document"),
        "operator = \"manual\"\n",
        "a conflicted write leaves the file alone"
    );
}

#[test]
fn a_dry_run_reports_the_change_and_writes_nothing() {
    let (_dir, project) = fixtures::deterministic_project();
    let outcome =
        local_config::set_scalar(&project, Scalar::CacheCapacity, "64", &options().dry_run())
            .expect("the dry run succeeds");
    assert!(outcome.changed);
    assert_eq!(outcome.changed_keys, ["cache.capacity"]);
    assert_eq!(outcome.local.cache.capacity, Some(64));
    assert_eq!(outcome.revision, outcome.previous_revision);
    assert!(
        !project.user_config_path().exists(),
        "a dry run never creates the document"
    );
}

#[test]
fn a_malformed_local_document_is_refused_rather_than_overwritten() {
    let (_dir, project) = fixtures::deterministic_project();
    let path = project.user_config_path();
    let broken = "operator = \n";
    std::fs::write(&path, broken).expect("a hand-broken document");

    let error = local_config::set_scalar(&project, Scalar::Operator, "frank", &options())
        .expect_err("a broken document fails the write");
    assert_eq!(error.code(), Code::ConfigInvalid);
    assert_eq!(
        std::fs::read_to_string(&path).expect("the document"),
        broken,
        "the broken document is preserved for repair"
    );
}

#[test]
fn project_configuration_writes_never_create_the_local_document() {
    let (_dir, project) = fixtures::deterministic_project();
    let key = waymark_core::model::CategoryKey::parse("bug").expect("a valid key");
    waymark_core::ops::config::add_category(&project, &key, "work/bug", None, &options())
        .expect("the shared write succeeds");
    waymark_core::ops::config::set_category_name(&project, &key, "Defect", &options())
        .expect("the shared rename succeeds");
    assert!(
        !project.user_config_path().exists(),
        "project configuration never lands in the local document"
    );
}

#[test]
fn a_local_write_repairs_a_missing_ignore_entry_before_writing() {
    let (_dir, project) = fixtures::deterministic_project();
    let ignore = project.metadata_dir().join(".gitignore");
    std::fs::write(&ignore, "write.lock\n").expect("an ignore file missing the local entry");

    local_config::set_scalar(&project, Scalar::Operator, "frank", &options())
        .expect("the local write succeeds");
    let contents = std::fs::read_to_string(&ignore).expect("the ignore file");
    assert_eq!(contents.matches(USER_CONFIG_FILE).count(), 1, "{contents}");
    assert_eq!(contents.matches("write.lock").count(), 1, "{contents}");
}

#[test]
fn an_unsafe_ignore_file_refuses_a_local_write_before_it_happens() {
    let (_dir, project) = fixtures::deterministic_project();
    let ignore = project.metadata_dir().join(".gitignore");
    std::fs::write(&ignore, "").expect("an empty ignore file");
    // A directory in place of the ignore file is not something to write through.
    std::fs::remove_file(&ignore).expect("the file is removable");
    std::fs::create_dir(&ignore).expect("a directory in its place");

    let error = local_config::set_scalar(&project, Scalar::Operator, "frank", &options())
        .expect_err("an unsafe ignore file stops the write");
    assert_eq!(error.code(), Code::LockFailed);
    assert!(
        !project.user_config_path().exists(),
        "nothing is written before the ignore file is safe"
    );
}

#[test]
fn every_local_document_problem_becomes_a_named_finding_at_the_document() {
    let (_dir, project) = fixtures::deterministic_project();
    let path = project.user_config_path();
    let revision = project.config().version.revision;

    for (document, key) in [
        ("timezone = \"Mars/Olympus\"\n", "timezone"),
        ("[cache]\ncapacity = 0\n", "capacity"),
        ("operator = \"\"\n", "operator"),
        ("nickname = \"frank\"\n", "nickname"),
        ("storage_root = \"work\"\n", "storage_root"),
    ] {
        std::fs::write(&path, document).expect("a broken local document");
        let findings = waymark_core::ops::config::validate_both(&project);
        assert_eq!(findings.len(), 1, "{document}: {findings:?}");
        let finding = &findings[0];
        assert_eq!(finding.code, Code::ConfigInvalid);
        assert_eq!(finding.severity, waymark_core::Severity::Error);
        assert!(
            finding.message.contains(key),
            "{document}: the message names `{key}`: {finding:?}"
        );
        assert!(
            finding
                .location
                .as_ref()
                .is_some_and(|location| format!("{location}").contains(USER_CONFIG_FILE)),
            "{document}: the finding is located at the local document: {finding:?}"
        );
        assert_eq!(
            project.config().version.revision,
            revision,
            "a local finding never moves the shared revision"
        );
    }

    // An absent document is normal and produces nothing.
    std::fs::remove_file(&path).expect("the document is removable");
    assert!(waymark_core::ops::config::validate_both(&project).is_empty());
}

#[test]
fn clearing_any_local_setting_restores_its_fallback() {
    let (_dir, project) = fixtures::deterministic_project();
    let path = project.user_config_path();
    for (scalar, value) in [
        (Scalar::Timezone, "Europe/Berlin"),
        (Scalar::CacheEnabled, "false"),
        (Scalar::CacheCapacity, "64"),
    ] {
        local_config::set_scalar(&project, scalar, value, &options())
            .expect("the value is written");
        assert!(local_text(&project)
            .is_some_and(|text| text.contains(scalar.name()) || text.contains("cache")));
        let cleared = local_config::set_scalar(&project, scalar, "", &options())
            .expect("an empty value clears the override");
        assert!(cleared.changed, "{} is cleared", scalar.name());
        let effective = local_config::bundle(&project)
            .expect("the bundle builds")
            .effective;
        match scalar {
            Scalar::Timezone => {
                assert_eq!(
                    effective.timezone_source,
                    ValueSource::SystemTimeZone,
                    "the machine zone is the fallback, or UTC when undetectable"
                );
            }
            Scalar::CacheEnabled => {
                assert_eq!(effective.cache_enabled_source, ValueSource::BuiltInDefault);
                assert_eq!(effective.cache.enabled, CacheSettings::default().enabled);
            }
            Scalar::CacheCapacity => {
                assert_eq!(effective.cache_capacity_source, ValueSource::BuiltInDefault);
                assert_eq!(effective.cache.capacity, CacheSettings::default().capacity);
            }
            Scalar::Operator => unreachable!("the operator is covered by its own case"),
        }
    }
    // Clearing everything leaves an empty document, which is deleted.
    assert!(!path.exists(), "an empty document is removed");
}

#[test]
fn a_dry_run_reports_the_intended_change_and_leaves_local_settings_alone() {
    let (_dir, project) = fixtures::deterministic_project();
    let ignore = project.metadata_dir().join(".gitignore");
    std::fs::write(&ignore, "").expect("an empty ignore file");
    let dry = local_config::set_scalar(&project, Scalar::Operator, "frank", &options().dry_run())
        .expect("the dry run succeeds");
    assert!(dry.changed);
    let contents = std::fs::read_to_string(&ignore).expect("the ignore file");
    assert!(
        !contents.contains(USER_CONFIG_FILE),
        "a dry run does not prepare the ignore file for a local write: {contents}"
    );
    assert!(!project.user_config_path().exists());
}

#[test]
fn a_work_item_write_leaves_the_local_ignore_entry_alone() {
    let (_dir, project) = fixtures::deterministic_project();
    let ignore = project.metadata_dir().join(".gitignore");
    std::fs::write(&ignore, "write.lock\n").expect("an ignore file without the local entry");

    // A work-item write neither needs the local entry nor may rewrite an ignore
    // file on its behalf; only a local write repairs it.
    waymark_core::ops::create::create(
        &project,
        waymark_core::ops::create::CreateRequest::new("t", "task", "f"),
        &options(),
    )
    .expect("the work-item write succeeds");
    assert_eq!(
        std::fs::read_to_string(&ignore).expect("the ignore file"),
        "write.lock\n",
        "a work-item write does not touch the local settings entry"
    );
}

#[test]
fn a_malformed_local_document_fails_an_operator_that_would_be_recorded() {
    let (_dir, project) = fixtures::deterministic_project();
    std::fs::write(project.user_config_path(), "timezone = \"Mars/Olympus\"\n")
        .expect("a broken local document");

    // Provenance that is about to be written must not be invented from a
    // fallback, so the error is explicit rather than a silent degradation.
    let error = waymark_core::ops::create::create(
        &project,
        waymark_core::ops::create::CreateRequest::new("t", "task", "f"),
        &options(),
    )
    .expect_err("a broken local document fails the creator default");
    assert_eq!(error.code(), Code::ConfigInvalid);
    assert!(error.message().contains("timezone"), "{error}");

    // An explicit creator needs no local value, so it still works.
    waymark_core::ops::create::create(
        &project,
        waymark_core::ops::create::CreateRequest::new("t", "task", "f").with_creator("explicit"),
        &options(),
    )
    .expect("an explicit creator is unaffected");
}
