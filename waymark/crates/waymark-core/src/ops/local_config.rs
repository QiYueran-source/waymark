//! The local user configuration surface: reading it, resolving it, and writing
//! it.
//!
//! This module is the one boundary adapters use. It owns three things so that no
//! adapter re-derives them:
//!
//! - **Reading.** [`load`] parses the local document if it exists and never
//!   creates it.
//! - **Resolving.** [`bundle`] and [`effective_lenient`] run
//!   [`EffectiveConfig::resolve`] with the environment the document actually
//!   needs; the first is strict, the second is for diagnostics.
//! - **Writing.** [`set_scalar`] updates one local setting under the project
//!   lock, leaving the shared configuration — and therefore its revision —
//!   untouched.
//!
//! A local-document problem is `CONFIG_INVALID` located at the document that
//! carries it, and its message names the key. The [`Bundle`] keeps the shared
//! and local documents side by side so a renderer can show both plus the resolved
//! values in one pass.

use camino::Utf8Path;

use crate::config::Config;
use crate::envelope::Finding;
use crate::error::WaymarkError;
use crate::ops::config::Scalar;
use crate::ops::OpOptions;
use crate::store::Project;
use crate::time::parse_timezone;
use crate::user_config::{EffectiveConfig, LocalEnvironment, UserCache, UserConfig};
use crate::{lock, write};

/// The local document as it exists on disk.
///
/// `None` means the project has none, which is the normal state; only a document
/// that exists and does not validate is an error.
pub fn load(project: &Project) -> Result<Option<UserConfig>, WaymarkError> {
    UserConfig::load_optional(&project.user_config_path())
}

/// The effective values, treating an unreadable local document as absent.
///
/// This is the lenient half of [`bundle`]: a broken document is absent here
/// instead of an error, because the diagnostic and advisory paths (`validate`,
/// `doctor`, the owner check) must keep working while the document is broken —
/// the command that reports the problem is the one that has to run. The
/// precedence is the same [`EffectiveConfig::resolve`] call; only the error
/// policy differs.
pub fn effective_lenient(project: &Project) -> EffectiveConfig {
    let local = load(project).ok().flatten();
    let environment = LocalEnvironment::detect_for(project.root(), local.as_ref());
    EffectiveConfig::resolve(local.as_ref(), &environment)
}

/// Every complaint about the local document at `path`, as findings.
///
/// An absent document produces no findings: absence is a normal state, not a
/// missing requirement. Every finding is located at the document itself and its
/// message names the offending key, so a person repairs the right file without
/// confusing it with the shared configuration.
pub fn validate_path(path: &Utf8Path) -> Vec<Finding> {
    use crate::error::Code;
    use crate::location::Location;

    if !path.exists() {
        return Vec::new();
    }
    let located = |message: String| {
        Finding::new(Code::ConfigInvalid, message).with_location(Location::file(path.to_string()))
    };
    match UserConfig::issues_in_file(path) {
        Ok(issues) => issues.iter().map(|issue| located(issue.render())).collect(),
        Err(error) => vec![located(error.message().to_owned())],
    }
}

/// Whether Git's index already tracks the local document.
///
/// A tracked local document is readable, but Waymark never writes it: every such
/// write would add a change to the shared baseline in every clone. `false` when
/// Git cannot answer, because an unknown answer must not block an ordinary
/// write.
pub fn tracked(project: &Project) -> bool {
    let probe = crate::git::GitProbe::detect_at(project.root());
    crate::git::tracks(&probe, &project.user_config_path())
}

/// The effective operator identity operations resolve without an explicit
/// override.
///
/// A broken local document must not block an operation, so this degrades to the
/// environment fallbacks exactly as [`effective_lenient`] does; the command that
/// reports the document is `validate` or `doctor`, not an ordinary write.
pub fn effective_operator(project: &Project) -> Option<String> {
    effective_lenient(project).operator
}

/// The shared configuration, the local document, and the resolved values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    /// The shared project configuration as it exists on disk.
    pub project: Config,
    /// The local document, or `None` when the project has none.
    pub local: Option<UserConfig>,
    /// The values operations actually use.
    pub effective: EffectiveConfig,
}

/// Read both documents and resolve them together.
pub fn bundle(project: &Project) -> Result<Bundle, WaymarkError> {
    let project_config = Config::load(&project.config_path())?;
    let local = load(project)?;
    let environment = LocalEnvironment::detect_for(project.root(), local.as_ref());
    let effective = EffectiveConfig::resolve(local.as_ref(), &environment);
    Ok(Bundle {
        project: project_config,
        local,
        effective,
    })
}

/// The result of one local configuration write.
///
/// The shared configuration cannot change here, so both revisions are the
/// revision that was on disk: a caller can assert the local write did not move it
/// without reading the shared document again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalOutcome {
    /// The local document as written.
    pub local: UserConfig,
    /// The project's shared configuration revision before the write.
    pub previous_revision: u64,
    /// The project's shared configuration revision after the write.
    pub revision: u64,
    /// Whether the document actually changed.
    pub changed: bool,
    /// The keys this write intended to change.
    pub changed_keys: Vec<String>,
}

/// Set one local scalar.
///
/// An empty operator clears the key: absence is how the document expresses "use
/// the fallback", because an empty operator is what validation rejects. The
/// shared configuration is never read for a value and never written, so its
/// revision does not move.
pub fn set_scalar(
    project: &Project,
    scalar: Scalar,
    value: &str,
    options: &OpOptions,
) -> Result<LocalOutcome, WaymarkError> {
    // Parse before taking the lock, so a malformed value never needs the write
    // path at all.
    let parsed = parse_scalar(scalar, value)?;
    let revision = project.config().version.revision;

    lock::with_project_lock(project, options.lock_wait, || {
        let path = project.user_config_path();
        // The fingerprint guards the exact bytes on disk, so a concurrent hand
        // edit becomes a conflict instead of being silently rebased away. An
        // absent document is "missing", the same shape a shared write reports.
        if let Some(expected) = &options.expected_config_fingerprint {
            let actual = match std::fs::read(&path) {
                Ok(bytes) => Some(crate::store::Fingerprint::of(&bytes)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(source) => return Err(WaymarkError::io(path.to_string(), "read", &source)),
            };
            if actual.as_ref() != Some(expected) {
                return Err(local_conflict(
                    &path,
                    expected,
                    actual.as_ref().map_or("missing", |actual| actual.as_str()),
                ));
            }
        }
        let current = UserConfig::load_optional(&path)?;
        let mut next = current.clone().unwrap_or_else(UserConfig::empty);

        match parsed {
            LocalScalar::Operator(operator) => next.operator = operator,
            LocalScalar::Timezone(zone) => next.timezone = zone,
            LocalScalar::CacheEnabled(enabled) => next.cache.enabled = enabled,
            LocalScalar::CacheCapacity(capacity) => next.cache.capacity = capacity,
        }

        // A document that states nothing is deleted rather than written empty:
        // an absent document is the canonical "no local settings" form, and it
        // is what an ignored file should look like.
        let changed = next != current.clone().unwrap_or_else(UserConfig::empty);
        if !changed {
            return Ok(LocalOutcome {
                local: next,
                previous_revision: revision,
                revision,
                changed: false,
                changed_keys: Vec::new(),
            });
        }

        next.validate()?;
        // A tracked document belongs to the shared baseline by mistake. Writing
        // it would deepen that mistake in every clone, so refuse and name the
        // manual repair instead of working around it.
        if tracked(project) {
            return Err(WaymarkError::config_invalid(
                path.to_string(),
                vec![format!(
                    "the local user configuration is tracked by Git; untrack it with \
                     `git rm --cached {}/{}` before Waymark writes it",
                    crate::config::METADATA_DIR,
                    crate::user_config::USER_CONFIG_FILE
                )],
            ));
        }
        if !options.dry_run {
            // The ignore entry is enforced here rather than at lock acquisition:
            // it is this write that must not leave an unignored preferences
            // file, and a dry run must not touch the ignore file at all. Every
            // local write, including a clearing one, also needs the ignore file
            // writable, even when every entry is already present: one rule is
            // easier to predict than one that depends on what the document
            // happens to contain.
            crate::store::ensure_local_gitignore(&project.metadata_dir())?;
            if next.is_empty() {
                remove_if_present(&path)?;
            } else {
                write::replace_unchecked(&path, next.to_toml_string().as_bytes())?;
            }
        }

        Ok(LocalOutcome {
            local: next,
            previous_revision: revision,
            revision,
            changed: true,
            changed_keys: vec![scalar.name().to_owned()],
        })
    })
}

/// A local write whose expected document bytes are no longer on disk.
fn local_conflict(
    path: &Utf8Path,
    expected: &crate::store::Fingerprint,
    actual: &str,
) -> WaymarkError {
    use crate::error::Code;
    use crate::location::Location;

    WaymarkError::new(
        Code::WriteConflict,
        format!("{path} changed on disk since it was read; reread and retry"),
    )
    .with_details(serde_json::json!({
        "path": path.to_string(),
        "expected_fingerprint": expected.as_str(),
        "actual_fingerprint": actual,
    }))
    .with_location(Location::file(path.to_string()))
}

fn remove_if_present(path: &Utf8Path) -> Result<(), WaymarkError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(WaymarkError::io(path.to_string(), "remove", &source)),
    }
}

/// One local scalar value, already validated.
///
/// `None` means "clear this override", which for every local setting restores
/// its fallback: Git for the operator, the machine zone, the built-in defaults
/// for the cache. There is no sentinel value that opts out instead.
enum LocalScalar {
    Operator(Option<String>),
    Timezone(Option<chrono_tz::Tz>),
    CacheEnabled(Option<bool>),
    CacheCapacity(Option<usize>),
}

fn parse_scalar(scalar: Scalar, value: &str) -> Result<LocalScalar, WaymarkError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        // An empty value is the uniform way to drop an override, for every local
        // setting rather than only the operator.
        return Ok(match scalar {
            Scalar::Operator => LocalScalar::Operator(None),
            Scalar::Timezone => LocalScalar::Timezone(None),
            Scalar::CacheEnabled => LocalScalar::CacheEnabled(None),
            Scalar::CacheCapacity => LocalScalar::CacheCapacity(None),
        });
    }
    match scalar {
        Scalar::Operator => Ok(LocalScalar::Operator(Some(trimmed.to_owned()))),
        Scalar::Timezone => {
            let zone = parse_timezone(trimmed).map_err(|_| {
                crate::ops::config::invalid(
                    "timezone",
                    format!("{value:?} is not a known IANA time zone"),
                )
            })?;
            Ok(LocalScalar::Timezone(Some(zone)))
        }
        Scalar::CacheEnabled => match trimmed {
            "true" => Ok(LocalScalar::CacheEnabled(Some(true))),
            "false" => Ok(LocalScalar::CacheEnabled(Some(false))),
            other => Err(crate::ops::config::invalid(
                "cache.enabled",
                format!("expected `true` or `false`, found {other:?}"),
            )),
        },
        Scalar::CacheCapacity => match trimmed.parse::<usize>() {
            Ok(capacity) if capacity > 0 => Ok(LocalScalar::CacheCapacity(Some(capacity))),
            _ => Err(crate::ops::config::invalid(
                "cache.capacity",
                format!("expected a positive whole number, found {value:?}"),
            )),
        },
    }
}

/// The local document a fresh initialization seeds.
///
/// First initialization writes the effective Git `user.name`, the machine-local
/// zone, and the current cache defaults, so a new project is usable with no
/// further setup. A missing Git name leaves the operator unset rather than
/// inventing one, and a seed overrides the detected value only because
/// initialization is the one moment the document is created.
pub fn seeded(
    root: &Utf8Path,
    operator: Option<&str>,
    timezone: Option<&str>,
) -> Result<UserConfig, WaymarkError> {
    let environment = LocalEnvironment::detect_for(root, None);
    let defaults = crate::config::CacheSettings::default();
    let mut local = UserConfig {
        operator: environment.operator,
        timezone: environment.timezone,
        // The defaults are written out rather than left implicit, so a release
        // that later changes one cannot silently resize an existing project's
        // cache: the snapshot stays what it was when the project was created.
        cache: UserCache {
            enabled: Some(defaults.enabled),
            capacity: Some(defaults.capacity),
        },
    };
    if let Some(operator) = operator {
        let trimmed = operator.trim();
        local.operator = (!trimmed.is_empty()).then(|| trimmed.to_owned());
    }
    if let Some(timezone) = timezone {
        local.timezone = Some(parse_timezone(timezone.trim()).map_err(|error| {
            WaymarkError::config_invalid("init", vec![format!("timezone: {error}")])
        })?);
    }
    local.validate()?;
    Ok(local)
}
