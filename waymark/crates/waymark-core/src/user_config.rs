//! The local user configuration document, `.waymark/user.toml`.
//!
//! The shared project configuration is the only authority for project
//! structure; this document holds the three settings that belong to one
//! collaborator: the advisory operator identity, the time zone used for display
//! and offset-less input, and the projection-cache sizing. It is ignored by the
//! metadata directory's ignore file and is never part of the shared baseline.
//!
//! Two properties shape the module:
//!
//! - **It is optional.** Every setting has a fallback, so an absent document is
//!   normal and reading never creates it.
//! - **It is validated, not repaired.** A malformed document, an unknown key, a
//!   shared project key, an empty operator, an unknown zone, or a zero cache
//!   capacity is a `CONFIG_INVALID` complaint located at the key that carries
//!   it. Nothing is silently defaulted.
//!
//! Resolution lives here too, because it is the one place that knows the
//! precedence. Adapters read [`EffectiveConfig`] and its [`ValueSource`]s; none
//! of them re-derives a fallback.

use camino::Utf8Path;
use chrono_tz::Tz;
use serde::Deserialize;

use crate::config::{quote, CacheSettings, ConfigIssue};
use crate::envelope::Warning;
use crate::error::{Code, WaymarkError};
use crate::location::Location;
use crate::time::parse_timezone;

/// The local user configuration file name inside
/// [`METADATA_DIR`](crate::config::METADATA_DIR).
pub const USER_CONFIG_FILE: &str = "user.toml";

/// The `.waymark/.gitignore` entry that keeps the local document out of the
/// shared baseline.
pub const USER_CONFIG_IGNORE_ENTRY: &str = USER_CONFIG_FILE;

/// One collaborator's local settings.
///
/// Every field is optional: an absent value falls back, and a value written here
/// stays exactly as written even when a later release changes its built-in
/// default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserConfig {
    /// Explicit operator identity; `None` means "use the fallback".
    pub operator: Option<String>,
    /// Explicit named zone; `None` means "use the machine-local zone".
    pub timezone: Option<Tz>,
    /// Explicit cache settings, field by field.
    pub cache: UserCache,
}

/// The cache settings this document states explicitly.
///
/// One field at a time, so a document that pins only the capacity keeps
/// following the built-in default for enablement — and so resolution can report
/// a source per field instead of inventing a shared one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UserCache {
    /// Whether the in-process projection cache is kept.
    pub enabled: Option<bool>,
    /// Entry budget for the projection cache.
    pub capacity: Option<usize>,
}

impl UserCache {
    /// Whether this document pins any cache field.
    pub fn is_absent(&self) -> bool {
        self.enabled.is_none() && self.capacity.is_none()
    }
}

impl UserConfig {
    /// An empty document: every setting falls back.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Parse and validate a local document.
    pub fn parse(text: &str) -> Result<Self, WaymarkError> {
        Self::parse_from(text, USER_CONFIG_FILE)
    }

    /// Load and validate the document at `path`.
    pub fn load(path: &Utf8Path) -> Result<Self, WaymarkError> {
        let text = std::fs::read_to_string(path)
            .map_err(|source| WaymarkError::io(path.to_string(), "read", &source))?;
        Self::parse_from(&text, path.as_str())
    }

    /// Load the document at `path`, treating an absent file as "not configured".
    ///
    /// An absent local document is the normal state, not a failure; only a file
    /// that exists and does not parse is an error.
    pub fn load_optional(path: &Utf8Path) -> Result<Option<Self>, WaymarkError> {
        if !path.exists() {
            return Ok(None);
        }
        Self::load(path).map(Some)
    }

    /// Every complaint about the document at `path`, per key.
    ///
    /// This is the finding-oriented counterpart of [`UserConfig::load`]: the
    /// caller reports one located finding per problem instead of one aggregate
    /// message.
    pub fn issues_in_file(path: &Utf8Path) -> Result<Vec<ConfigIssue>, WaymarkError> {
        let text = std::fs::read_to_string(path)
            .map_err(|source| WaymarkError::io(path.to_string(), "read", &source))?;
        Ok(Self::build(&text, path.as_str())?.1)
    }

    /// Refuse a document that would not survive a reload.
    pub fn validate(&self) -> Result<(), WaymarkError> {
        Self::build(&self.to_toml_string(), "in-memory user configuration").map(|_| ())
    }

    /// Whether the document states nothing at all.
    pub fn is_empty(&self) -> bool {
        self.operator.is_none() && self.timezone.is_none() && self.cache.is_absent()
    }

    /// Render the canonical document: fixed key order, absent keys omitted, one
    /// trailing newline.
    pub fn to_toml_string(&self) -> String {
        let mut out = String::new();
        if let Some(operator) = &self.operator {
            out.push_str(&format!("operator = {}\n", quote(operator)));
        }
        if let Some(zone) = self.timezone {
            out.push_str(&format!("timezone = {}\n", quote(zone.name())));
        }
        if !self.cache.is_absent() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str("[cache]\n");
            if let Some(enabled) = self.cache.enabled {
                out.push_str(&format!("enabled = {enabled}\n"));
            }
            if let Some(capacity) = self.cache.capacity {
                out.push_str(&format!("capacity = {capacity}\n"));
            }
        }
        out
    }

    fn parse_from(text: &str, source: &str) -> Result<Self, WaymarkError> {
        match Self::build(text, source) {
            // The local document has no version: hand edits and seeded writes
            // are the only sources, so a complaint is always `CONFIG_INVALID`.
            Ok((config, issues)) if issues.is_empty() => Ok(config),
            Ok((_, issues)) => Err(invalid(
                source,
                issues.iter().map(ConfigIssue::render).collect(),
            )),
            Err(error) => Err(error),
        }
    }

    /// Parse `text` into the settings it states plus every complaint about it.
    ///
    /// Keys the document may not carry are removed before deserialization so
    /// that each one is reported against its own key instead of collapsing into
    /// one document-level parse failure.
    fn build(text: &str, source: &str) -> Result<(Self, Vec<ConfigIssue>), WaymarkError> {
        let malformed = |reason: String| invalid(source, vec![reason]);

        let value: toml::Value =
            toml::from_str(text).map_err(|error| malformed(error.to_string()))?;
        let table = value
            .as_table()
            .ok_or_else(|| malformed("the document must be a TOML table".to_owned()))?;

        let mut issues: Vec<ConfigIssue> = Vec::new();
        let mut allowed: toml::map::Map<String, toml::Value> = toml::map::Map::new();
        for (key, value) in table {
            if SHARED_KEYS.contains(&key.as_str()) {
                issues.push(ConfigIssue::key(
                    key,
                    format!("`{key}` is a shared project setting and cannot be set in the local user configuration"),
                ));
            } else if !KNOWN_LOCAL_KEYS.contains(&key.as_str()) {
                issues.push(ConfigIssue::key(
                    key,
                    "unknown local configuration key (expected operator, timezone, cache)",
                ));
            } else {
                allowed.insert(key.clone(), value.clone());
            }
        }
        report_nested_unknowns(&mut allowed, &mut issues);

        let raw: RawUserConfig = toml::Value::Table(allowed)
            .try_into()
            .map_err(|error: toml::de::Error| malformed(error.message().to_owned()))?;

        let operator = match raw.operator {
            Some(operator) if operator.0.trim().is_empty() => {
                issues.push(ConfigIssue::key(
                    "operator",
                    "the operator must not be empty; remove the key to fall back to Git",
                ));
                None
            }
            Some(operator) => Some(operator.0.trim().to_owned()),
            None => None,
        };

        let timezone = match raw.timezone {
            Some(zone) => match parse_timezone(zone.0.trim()) {
                Ok(zone) => Some(zone),
                Err(_) => {
                    issues.push(ConfigIssue::key(
                        "timezone",
                        format!("{:?} is not a known IANA time zone", zone.0),
                    ));
                    None
                }
            },
            None => None,
        };

        let cache = raw.cache.map_or_else(UserCache::default, |cache| {
            if cache.capacity == Some(0) {
                issues.push(ConfigIssue::key(
                    "cache.capacity",
                    "the cache capacity must be greater than zero",
                ));
            }
            UserCache {
                enabled: cache.enabled,
                capacity: cache.capacity,
            }
        });

        Ok((
            Self {
                operator,
                timezone,
                cache,
            },
            issues,
        ))
    }
}

/// Where an effective value came from.
///
/// One literal per source, stable on the wire: an adapter renders this instead of
/// deciding for itself which fallback produced the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValueSource {
    /// An explicit value in the local user configuration document.
    UserDocument,
    /// Git's effective `user.name`.
    Git,
    /// The current machine-local time zone.
    SystemTimeZone,
    /// A built-in program default.
    BuiltInDefault,
    /// No value is available at all.
    Unset,
}

impl ValueSource {
    /// The stable on-the-wire literal.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserDocument => "local_document",
            Self::Git => "git",
            Self::SystemTimeZone => "system_time_zone",
            Self::BuiltInDefault => "built_in_default",
            Self::Unset => "unset",
        }
    }
}

impl std::fmt::Display for ValueSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The ambient facts the fallbacks read.
///
/// Injectable so a test can pin them: the real environment supplies Git's
/// `user.name` and the machine-local zone, and neither is a hard dependency of
/// any operation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalEnvironment {
    /// Git's effective `user.name`, when Git and a repository are available.
    pub operator: Option<String>,
    /// The machine-local zone, when it can be determined.
    pub timezone: Option<Tz>,
}

impl LocalEnvironment {
    /// An environment with no fallbacks available.
    pub fn isolated() -> Self {
        Self::default()
    }

    /// Detect only the fallbacks `user` actually needs.
    ///
    /// A setting the local document states explicitly never consults its
    /// fallback, so no lookup runs for it: a machine with an unreadable Git
    /// identity or an odd zone pays nothing for a value the user already chose.
    pub fn detect_for(root: &Utf8Path, user: Option<&UserConfig>) -> Self {
        let operator_stated = user.is_some_and(|user| user.operator.is_some());
        let timezone_stated = user.is_some_and(|user| user.timezone.is_some());
        let mut environment = Self::default();
        if !operator_stated {
            let probe = crate::git::GitProbe::detect_at(root);
            environment.operator = crate::git::user_name(&probe);
        }
        if !timezone_stated {
            environment.timezone = crate::time::detect_machine_timezone();
        }
        environment
    }
}

/// The values an operation actually uses, with the source of each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveConfig {
    /// The advisory operator identity, or `None` when nothing supplies one.
    pub operator: Option<String>,
    /// Where [`Self::operator`] came from; [`ValueSource::Unset`] when absent.
    pub operator_source: ValueSource,
    /// The zone used for display and offset-less input.
    pub timezone: Tz,
    /// Where [`Self::timezone`] came from.
    pub timezone_source: ValueSource,
    /// The resolved projection-cache settings.
    pub cache: CacheSettings,
    /// Where `cache.enabled` came from.
    pub cache_enabled_source: ValueSource,
    /// Where `cache.capacity` came from.
    pub cache_capacity_source: ValueSource,
    /// Conditions worth reporting about the resolution itself.
    ///
    /// Today that is one case: an undetectable machine zone falls back to UTC and
    /// says so, so a surprising value is visible instead of silent.
    pub warnings: Vec<Warning>,
}

impl EffectiveConfig {
    /// Resolve the effective values for `user` in `environment`.
    ///
    /// The precedence is local-first: an explicit local value wins, then the
    /// environment's fallback, then the built-in default. An explicit local value
    /// never consults its fallback, so detection that is unavailable or slow
    /// cannot change a value the user chose.
    pub fn resolve(user: Option<&UserConfig>, environment: &LocalEnvironment) -> Self {
        let (operator, operator_source) = match user.and_then(|user| user.operator.clone()) {
            Some(operator) => (Some(operator), ValueSource::UserDocument),
            None => match environment.operator.clone() {
                Some(operator) => (Some(operator), ValueSource::Git),
                None => (None, ValueSource::Unset),
            },
        };

        let mut warnings = Vec::new();
        let (timezone, timezone_source) = match user.and_then(|user| user.timezone) {
            Some(zone) => (zone, ValueSource::UserDocument),
            None => match environment.timezone {
                Some(zone) => (zone, ValueSource::SystemTimeZone),
                None => {
                    warnings.push(Warning::new(
                        Code::TimeZoneUndetected,
                        "no machine-local time zone could be detected; the effective time zone falls back to UTC",
                    ));
                    (chrono_tz::UTC, ValueSource::BuiltInDefault)
                }
            },
        };

        let (cache, cache_enabled_source, cache_capacity_source) = resolve_cache(user);

        Self {
            operator,
            operator_source,
            timezone,
            timezone_source,
            cache,
            cache_enabled_source,
            cache_capacity_source,
            warnings,
        }
    }
}

/// Resolve the projection-cache settings and the source of each field.
///
/// Cache settings have no environment fallback: an explicit local value wins and
/// everything else is the built-in default, field by field. A document that pins
/// only the capacity therefore keeps following the default for enablement, and a
/// value written by `init` stays a written value even after a release changes the
/// default.
pub fn resolve_cache(user: Option<&UserConfig>) -> (CacheSettings, ValueSource, ValueSource) {
    let defaults = CacheSettings::default();
    let stated = user.map(|user| user.cache).unwrap_or_default();
    let (enabled, enabled_source) = match stated.enabled {
        Some(enabled) => (enabled, ValueSource::UserDocument),
        None => (defaults.enabled, ValueSource::BuiltInDefault),
    };
    let (capacity, capacity_source) = match stated.capacity {
        Some(capacity) => (capacity, ValueSource::UserDocument),
        None => (defaults.capacity, ValueSource::BuiltInDefault),
    };
    (
        CacheSettings { enabled, capacity },
        enabled_source,
        capacity_source,
    )
}

/// A `CONFIG_INVALID` about the local document, located at the document.
fn invalid(source: &str, issues: Vec<String>) -> WaymarkError {
    WaymarkError::config_invalid(source, issues)
        .with_location(Location::config_key(USER_CONFIG_FILE))
}

const KNOWN_LOCAL_KEYS: &[&str] = &["operator", "timezone", "cache"];

const SHARED_KEYS: &[&str] = &["version", "storage_root", "categories", "fields"];

const KNOWN_CACHE_KEYS: &[&str] = &["enabled", "capacity"];

/// Report every unknown key inside `[cache]` and drop it, so each one becomes
/// its own located complaint instead of one document-level parse failure.
fn report_nested_unknowns(
    table: &mut toml::map::Map<String, toml::Value>,
    issues: &mut Vec<ConfigIssue>,
) {
    let Some(cache) = table.get_mut("cache").and_then(toml::Value::as_table_mut) else {
        return;
    };
    let unknown: Vec<String> = cache
        .keys()
        .filter(|key| !KNOWN_CACHE_KEYS.contains(&key.as_str()))
        .cloned()
        .collect();
    for key in unknown {
        cache.remove(&key);
        issues.push(ConfigIssue::key(
            format!("cache.{key}"),
            "unknown cache key (expected enabled, capacity)",
        ));
    }
}

// ---------------------------------------------------------------------------
// The raw TOML shape.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawUserConfig {
    #[serde(default)]
    operator: Option<Raw<String>>,
    #[serde(default)]
    timezone: Option<Raw<String>>,
    #[serde(default)]
    cache: Option<RawCache>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCache {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    capacity: Option<usize>,
}

/// A leaf whose wrong TOML type reports its own key.
#[derive(Debug)]
struct Raw<T>(T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Raw<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self)
    }
}
