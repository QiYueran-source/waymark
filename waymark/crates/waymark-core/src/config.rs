//! The project configuration document, `.waymark/config.toml`: its typed shape,
//! its validation, and its deterministic writer.
//!
//! The document is the only configuration authority (spec §"Configuration").
//! Validation therefore rejects rather than repairs: one bad key makes the whole
//! configuration invalid, and because valid work-item files stay locally
//! readable, a broken configuration never destroys data.
//!
//! The writer is hand-rolled like the work-item one: fixed key order, categories
//! and fields sorted by key, one trailing newline, `\n` endings. Round-tripping
//! an unchanged [`Config`] is byte-identical.

use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};
use serde::Deserialize;

use crate::error::WaymarkError;
use crate::location::Location;
use crate::model::{CategoryKey, FieldKey, FieldValue, FORMAT_GENERATION};

/// The metadata directory that holds the configuration and the write lock.
pub const METADATA_DIR: &str = ".waymark";

/// The configuration file name inside [`METADATA_DIR`].
pub const CONFIG_FILE: &str = "config.toml";

/// The default storage root written by `init`.
pub const DEFAULT_STORAGE_ROOT: &str = "work";

/// The category key `init` creates when the caller names none.
pub const DEFAULT_CATEGORY_KEY: &str = "task";

/// The configuration revision this release writes into a fresh project.
pub const CURRENT_REVISION: u64 = 0;

/// The configuration version: format generation plus revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    /// Format generation (the major part). Only a version migration moves it.
    pub generation: u32,
    /// Monotonic configuration revision (the minor part).
    pub revision: u64,
}

impl Version {
    /// The version this release writes into a fresh project.
    pub fn current() -> Self {
        Self {
            generation: FORMAT_GENERATION,
            revision: CURRENT_REVISION,
        }
    }

    /// Parse the `major.minor` literal.
    pub fn parse(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        let Some((major, minor)) = trimmed.split_once('.') else {
            return Err(format!("{trimmed:?} is not a `major.minor` version"));
        };
        if major.is_empty() || minor.is_empty() {
            return Err(format!("{trimmed:?} is not a `major.minor` version"));
        }
        let generation = major
            .parse::<u32>()
            .map_err(|_| format!("{major:?} is not a numeric format generation"))?;
        let revision = minor
            .parse::<u64>()
            .map_err(|_| format!("{minor:?} is not a numeric configuration revision"))?;
        Ok(Self {
            generation,
            revision,
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.generation, self.revision)
    }
}

/// One configured category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    /// Repository-relative directory holding this category's documents.
    pub path: String,
    /// Human-readable name.
    pub name: String,
}

/// A declared custom-field type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    /// A string value.
    String,
    /// A whole number.
    Integer,
    /// A boolean value.
    Boolean,
    /// A homogeneous list of scalars.
    List(Box<FieldType>),
}

impl FieldType {
    /// The pinned spelling: `string`, `integer`, `boolean`, or `list:<scalar>`.
    pub fn as_str(&self) -> String {
        match self {
            Self::String => "string".to_owned(),
            Self::Integer => "integer".to_owned(),
            Self::Boolean => "boolean".to_owned(),
            Self::List(inner) => format!("list:{}", inner.as_str()),
        }
    }

    /// Parse the pinned spelling. Only scalar element types are allowed.
    pub fn parse(input: &str) -> Result<Self, String> {
        match input.trim() {
            "string" => Ok(Self::String),
            "integer" => Ok(Self::Integer),
            "boolean" => Ok(Self::Boolean),
            other => match other.strip_prefix("list:") {
                Some("string") => Ok(Self::List(Box::new(Self::String))),
                Some("integer") => Ok(Self::List(Box::new(Self::Integer))),
                Some("boolean") => Ok(Self::List(Box::new(Self::Boolean))),
                _ => Err(format!(
                    "{other:?} is not one of string, integer, boolean, list:string, list:integer, list:boolean"
                )),
            },
        }
    }

    /// Parse one textual value token into a value of this type.
    ///
    /// This is the adapter-facing parser: it accepts the plain spellings a person
    /// types on a command line (`7`, `true`, `hello`, `["a","b"]`) and rejects a
    /// token whose shape the declared type cannot hold. The domain re-checks the
    /// result against the type; this only decides what the text *means*.
    pub fn parse_value(&self, raw: &str) -> Result<FieldValue, String> {
        match self {
            // A string is either a JSON string literal or a bare token taken
            // verbatim. A structured token for a string field is a mistake, not
            // content: it is almost always a value meant for a `list:*` field.
            Self::String if raw.starts_with('"') => match serde_json::from_str(raw) {
                Ok(serde_json::Value::String(text)) => Ok(FieldValue::String(text)),
                Ok(_) => Err("a string value must be a JSON string literal".to_owned()),
                Err(error) => Err(format!("not a valid JSON string literal: {error}")),
            },
            Self::String => match serde_json::from_str::<serde_json::Value>(raw) {
                Ok(serde_json::Value::Array(_) | serde_json::Value::Object(_)) => Err(
                    "a string value may not be a JSON array or object; quote it to keep it as text"
                        .to_owned(),
                ),
                _ => Ok(FieldValue::String(raw.to_owned())),
            },
            Self::Integer => {
                let digits = raw.strip_prefix('-').unwrap_or(raw);
                if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(format!("expected a decimal integer, found {raw:?}"));
                }
                raw.parse::<i64>()
                    .map(FieldValue::Integer)
                    .map_err(|_| format!("{raw:?} is outside the range of a 64-bit integer"))
            }
            Self::Boolean => match raw {
                "true" => Ok(FieldValue::Boolean(true)),
                "false" => Ok(FieldValue::Boolean(false)),
                other => Err(format!("expected `true` or `false`, found {other:?}")),
            },
            Self::List(element) => {
                let parsed: serde_json::Value = serde_json::from_str(raw)
                    .map_err(|error| format!("expected a JSON array, found {raw:?}: {error}"))?;
                let serde_json::Value::Array(items) = parsed else {
                    return Err(format!("expected a JSON array, found {raw:?}"));
                };
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(parse_list_element(element, item)?);
                }
                Ok(FieldValue::List(values))
            }
        }
    }

    /// Whether `value` conforms to this type.
    pub fn accepts(&self, value: &FieldValue) -> bool {
        match (self, value) {
            (Self::String, FieldValue::String(_)) => true,
            (Self::Integer, FieldValue::Integer(_)) => true,
            (Self::Boolean, FieldValue::Boolean(_)) => true,
            (Self::List(element), FieldValue::List(values)) => {
                values.iter().all(|value| element.accepts(value))
            }
            _ => false,
        }
    }
}

/// One declared custom field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDecl {
    /// Declared value shape.
    pub ty: FieldType,
    /// Whether every work item must carry a value.
    pub required: bool,
    /// Optional static default.
    pub default: Option<FieldValue>,
}

/// Projection cache sizing. v1 has no on-disk cache; these settings configure
/// the in-process projection only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheSettings {
    /// Whether the in-process projection is kept.
    pub enabled: bool,
    /// Entry budget for the projection.
    pub capacity: usize,
}

impl Default for CacheSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            capacity: 512,
        }
    }
}

/// The complete project configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Format generation plus configuration revision.
    pub version: Version,
    /// Repository-relative directory containing the category directories.
    pub storage_root: String,
    /// Categories keyed by their stable key.
    pub categories: BTreeMap<CategoryKey, Category>,
    /// Declared custom fields keyed by their stable key.
    pub fields: BTreeMap<FieldKey, FieldDecl>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: Version::current(),
            storage_root: DEFAULT_STORAGE_ROOT.to_owned(),
            categories: BTreeMap::new(),
            fields: BTreeMap::new(),
        }
    }
}

impl Config {
    /// A fresh configuration with one category, as `init` writes it.
    pub fn fresh() -> Self {
        let mut config = Self::default();
        let key = CategoryKey::parse(DEFAULT_CATEGORY_KEY).expect("the default key is valid");
        config.categories.insert(
            key.clone(),
            Category {
                path: format!("{DEFAULT_STORAGE_ROOT}/{DEFAULT_CATEGORY_KEY}"),
                name: default_category_name(&key),
            },
        );
        config
    }

    /// The directory a category's documents live in, given the project root.
    pub fn category_dir(&self, root: &Utf8Path, category: &CategoryKey) -> Option<Utf8PathBuf> {
        self.categories
            .get(category)
            .map(|entry| resolve_relative(root, &entry.path))
    }

    /// Parse and validate a configuration document without filesystem checks.
    pub fn parse(text: &str) -> Result<Self, WaymarkError> {
        Self::parse_with_root(text, None, CONFIG_FILE)
    }

    /// Load and fully validate the configuration at `path`, including the
    /// symlink-traversal check that needs the repository root.
    pub fn load(path: &Utf8Path) -> Result<Self, WaymarkError> {
        let text = std::fs::read_to_string(path)
            .map_err(|source| WaymarkError::io(path.to_string(), "read", &source))?;
        let root = path
            .parent()
            .and_then(Utf8Path::parent)
            .map(Utf8Path::to_path_buf);
        Self::parse_with_root(&text, root.as_deref(), path.as_str())
    }

    fn parse_with_root(
        text: &str,
        root: Option<&Utf8Path>,
        source: &str,
    ) -> Result<Self, WaymarkError> {
        let invalid = |issues: Vec<String>| {
            WaymarkError::config_invalid(source, issues)
                .with_location(Location::config_key(CONFIG_FILE))
        };
        match Self::build(text, root, source) {
            Ok((config, issues)) => {
                if issues.is_empty() {
                    Ok(config)
                } else {
                    Err(invalid(issues.iter().map(ConfigIssue::render).collect()))
                }
            }
            // `build` already reports its hard failures with the right code
            // (malformed TOML is `CONFIG_INVALID`; an unreadable generation is
            // `UNSUPPORTED_VERSION`), so it is not wrapped again here.
            Err(error) => Err(error),
        }
    }

    /// Parse `text` into a configuration plus every complaint about it.
    ///
    /// A document whose TOML cannot be read at all is the only hard failure: the
    /// error message is the complaint. Everything else yields a usable-in-memory
    /// configuration (with a bad value replaced by its fallback) and one issue per
    /// key, which is what `validate` needs to report locatable findings.
    fn build(
        text: &str,
        root: Option<&Utf8Path>,
        source: &str,
    ) -> Result<(Self, Vec<ConfigIssue>), WaymarkError> {
        let malformed = |reason: String| {
            WaymarkError::config_invalid(source, vec![reason])
                .with_location(Location::config_key(CONFIG_FILE))
        };

        let value: toml::Value =
            toml::from_str(text).map_err(|error| malformed(error.to_string()))?;
        let table = value
            .as_table()
            .ok_or_else(|| malformed("the document must be a TOML table".to_owned()))?;

        let mut issues: Vec<ConfigIssue> = Vec::new();
        let mut allowed: toml::map::Map<String, toml::Value> = toml::map::Map::new();
        for (key, entry) in table {
            if LOCAL_ONLY_KEYS.contains(&key.as_str()) {
                issues.push(ConfigIssue::key(
                    key,
                    format!(
                        "`{key}` belongs to the local user configuration and cannot be set in the shared project configuration"
                    ),
                ));
            } else if !KNOWN_TOP_LEVEL.contains(&key.as_str()) {
                issues.push(ConfigIssue::key(
                    key,
                    "unknown configuration key (expected version, storage_root, categories, fields)",
                ));
            } else {
                allowed.insert(key.clone(), entry.clone());
            }
        }
        report_nested_unknowns(&allowed, &mut issues);

        let raw: RawConfig = toml::Value::Table(allowed)
            .try_into()
            .map_err(|error: toml::de::Error| malformed(error.message().to_owned()))?;

        let version = match Version::parse(&raw.version.0) {
            Ok(version) if crate::model::is_readable_generation(version.generation) => version,
            Ok(version) => {
                // The generation is a compatibility boundary, not a value
                // typo: `UNSUPPORTED_VERSION` names the recovery action.
                return Err(WaymarkError::unsupported_version(
                    version.generation.to_string(),
                    crate::model::readable_generations(),
                )
                .with_location(Location::config_key("version")));
            }
            Err(reason) => {
                issues.push(ConfigIssue::key("version", reason));
                Version::current()
            }
        };

        issues.extend(validate_relative_path("storage_root", &raw.storage_root.0));
        issues.extend(check_symlink_free(root, &raw.storage_root.0));

        let mut categories: BTreeMap<CategoryKey, Category> = BTreeMap::new();
        for (key, entry) in raw.categories {
            let parsed = match CategoryKey::parse(&key) {
                Ok(parsed) => Some(parsed),
                Err(error) => {
                    issues.push(ConfigIssue::key(
                        format!("categories.{key}"),
                        format!("not a valid category key: {error}"),
                    ));
                    None
                }
            };
            if entry.name.0.trim().is_empty() {
                issues.push(ConfigIssue::key(
                    format!("categories.{key}.name"),
                    "a category display name must not be empty",
                ));
            }
            issues.extend(
                validate_relative_path("path", &entry.path.0)
                    .into_iter()
                    .map(|issue| issue.with_key(format!("categories.{key}.path"))),
            );
            issues.extend(check_symlink_free(root, &entry.path.0));
            if let Some(parsed) = parsed {
                categories.insert(
                    parsed,
                    Category {
                        path: entry.path.0.trim().to_owned(),
                        name: entry.name.0.trim().to_owned(),
                    },
                );
            }
        }
        issues.extend(check_overlaps(&categories));

        let mut fields: BTreeMap<FieldKey, FieldDecl> = BTreeMap::new();
        for (key, entry) in raw.fields {
            if crate::model::RESERVED_FIELD_NAMES.contains(&key.as_str()) {
                issues.push(ConfigIssue::key(
                    format!("fields.{key}"),
                    "a custom field may not shadow a reserved core or computed name",
                ));
            }
            let parsed_key = match FieldKey::parse(&key) {
                Ok(parsed) => Some(parsed),
                Err(error) => {
                    issues.push(ConfigIssue::key(
                        format!("fields.{key}"),
                        format!("not a valid custom-field key: {error}"),
                    ));
                    None
                }
            };
            let ty = match FieldType::parse(&entry.ty.0) {
                Ok(ty) => ty,
                Err(reason) => {
                    issues.push(ConfigIssue::key(format!("fields.{key}.type"), reason));
                    continue;
                }
            };
            let default = entry.default.map(|default| default.0);
            if let Some(value) = &default {
                if !ty.accepts(value) {
                    issues.push(ConfigIssue::key(
                        format!("fields.{key}.default"),
                        format!("the default is not a {}", ty.as_str()),
                    ));
                }
            }
            if let Some(parsed_key) = parsed_key {
                fields.insert(
                    parsed_key,
                    FieldDecl {
                        ty,
                        required: entry.required,
                        default,
                    },
                );
            }
        }

        Ok((
            Self {
                version,
                storage_root: raw.storage_root.0.trim().to_owned(),
                categories,
                fields,
            },
            issues,
        ))
    }

    /// Every complaint in this document, addressed by key, including the
    /// filesystem checks that need `root` (symbolic links in a path).
    ///
    /// The returned list is empty exactly when [`Config::load`] would succeed.
    /// It is the finding-oriented counterpart of the aggregate `CONFIG_INVALID`
    /// error: one entry per key, so a caller can locate each issue instead of
    /// parsing one rendered message.
    pub fn issues_at(&self, root: &Utf8Path) -> Vec<ConfigIssue> {
        match Self::build(&self.to_toml_string(), Some(root), CONFIG_FILE) {
            Ok((_, issues)) => issues,
            Err(error) => vec![ConfigIssue::key("", error.message())],
        }
    }

    /// Every complaint about the document at `path`, including the filesystem
    /// checks. A document that is not readable TOML at all is an error whose
    /// message is the single complaint.
    pub fn issues_in_file(path: &Utf8Path) -> Result<Vec<ConfigIssue>, WaymarkError> {
        let text = std::fs::read_to_string(path)
            .map_err(|source| WaymarkError::io(path.to_string(), "read", &source))?;
        let root = path
            .parent()
            .and_then(Utf8Path::parent)
            .map(Utf8Path::to_path_buf);
        Self::build(&text, root.as_deref(), path.as_str()).map(|(_, issues)| issues)
    }

    /// Re-validate a configuration changed in memory, without filesystem checks.
    pub fn validate(&self) -> Result<(), WaymarkError> {
        Self::parse_with_root(&self.to_toml_string(), None, "in-memory configuration").map(|_| ())
    }

    /// Render the canonical configuration document.
    pub fn to_toml_string(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("version = {}\n", quote(&self.version.to_string())));
        out.push_str(&format!("storage_root = {}\n", quote(&self.storage_root)));

        for (key, category) in &self.categories {
            out.push_str(&format!("\n[categories.{key}]\n"));
            out.push_str(&format!("path = {}\n", quote(&category.path)));
            out.push_str(&format!("name = {}\n", quote(&category.name)));
        }

        for (key, field) in &self.fields {
            out.push_str(&format!("\n[fields.{key}]\n"));
            out.push_str(&format!("type = {}\n", quote(&field.ty.as_str())));
            out.push_str(&format!("required = {}\n", field.required));
            if let Some(default) = &field.default {
                out.push_str(&format!("default = {}\n", render_field_value(default)));
            }
        }

        out
    }
}

/// The default display name for a category key: the key with its first letter
/// upper-cased (`task` → `Task`).
///
/// The specification defines no naming convention. Deriving a name from hyphens
/// would be a guess that shows up in generated documents, so this is the
/// smallest transform that reads as a name; `config add category` always takes an
/// explicit name.
pub fn default_category_name(key: &CategoryKey) -> String {
    let mut chars = key.as_str().chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The keys the shared document may carry.
///
/// `operator`, `timezone`, and `cache` are deliberately absent: they belong to
/// the local user configuration, and a shared document that still states one is
/// reported as invalid rather than read or silently ignored, so a project never
/// carries a stale shared copy of a collaborator's preferences.
const KNOWN_TOP_LEVEL: &[&str] = &["version", "storage_root", "categories", "fields"];

/// The keys that moved to the local document, reported with their destination.
const LOCAL_ONLY_KEYS: &[&str] = &["operator", "timezone", "cache"];

const KNOWN_CATEGORY_KEYS: &[&str] = &["path", "name"];
const KNOWN_FIELD_KEYS: &[&str] = &["type", "required", "default"];

fn report_nested_unknowns(
    table: &toml::map::Map<String, toml::Value>,
    issues: &mut Vec<ConfigIssue>,
) {
    if let Some(categories) = table.get("categories").and_then(toml::Value::as_table) {
        for (key, value) in categories {
            for nested in value.as_table().map_or_else(Vec::new, |entry| {
                entry.keys().cloned().collect::<Vec<String>>()
            }) {
                if !KNOWN_CATEGORY_KEYS.contains(&nested.as_str()) {
                    issues.push(ConfigIssue::key(
                        format!("categories.{key}.{nested}"),
                        "unknown category key (expected path, name)",
                    ));
                }
            }
        }
    }
    if let Some(fields) = table.get("fields").and_then(toml::Value::as_table) {
        for (key, value) in fields {
            for nested in value.as_table().map_or_else(Vec::new, |entry| {
                entry.keys().cloned().collect::<Vec<String>>()
            }) {
                if !KNOWN_FIELD_KEYS.contains(&nested.as_str()) {
                    issues.push(ConfigIssue::key(
                        format!("fields.{key}.{nested}"),
                        "unknown field key (expected type, required, default)",
                    ));
                }
            }
        }
    }
}

/// One configuration complaint, addressed by the key it is about.
///
/// The renderer flattens it into the `key: reason` line `CONFIG_INVALID`
/// details carry; the pair is public so `validate` can turn each complaint into
/// its own `Finding` with a `config_key` location instead of one aggregate
/// error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigIssue {
    /// The dotted configuration key, for example `categories.bug.path`.
    pub key: String,
    /// Why the value was rejected.
    pub reason: String,
}

impl ConfigIssue {
    pub(crate) fn key(key: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            reason: reason.into(),
        }
    }

    fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    /// The location this complaint points at.
    pub fn location(&self) -> Location {
        Location::config_key(self.key.clone())
    }

    /// The `key: reason` line, as `CONFIG_INVALID` details carry it.
    pub fn render(&self) -> String {
        if self.key.is_empty() {
            self.reason.clone()
        } else {
            format!("{}: {}", self.key, self.reason)
        }
    }
}

/// Parse one JSON token as a scalar of `element`, for a list default.
fn parse_list_element(element: &FieldType, value: serde_json::Value) -> Result<FieldValue, String> {
    match (element, value) {
        (FieldType::String, serde_json::Value::String(text)) => Ok(FieldValue::String(text)),
        (FieldType::Integer, serde_json::Value::Number(number)) => number
            .as_i64()
            .map(FieldValue::Integer)
            .ok_or_else(|| format!("{number} is not a whole number")),
        (FieldType::Boolean, serde_json::Value::Bool(flag)) => Ok(FieldValue::Boolean(flag)),
        (FieldType::List(_), _) => Err("nested lists are not valid custom-field values".to_owned()),
        (element, other) => Err(format!(
            "expected a `{}` element, found {other}",
            element.as_str()
        )),
    }
}

/// Validate a repository-relative path: no absolute prefix, no `..`, no empty
/// component, POSIX separators.
fn validate_relative_path(key: &str, raw: &str) -> Vec<ConfigIssue> {
    let value = raw.trim();
    let mut issues = Vec::new();
    if value.is_empty() {
        issues.push(ConfigIssue::key(key, "a path is required"));
        return issues;
    }
    if value.starts_with('/') || value.starts_with('\\') || value.contains(":\\") {
        issues.push(ConfigIssue::key(
            key,
            "a path must be repository-relative, not absolute",
        ));
    }
    if value.contains('\\') {
        issues.push(ConfigIssue::key(key, "a path must use POSIX separators"));
    }
    if value.split('/').any(|component| component == "..") {
        issues.push(ConfigIssue::key(
            key,
            "a path must not escape the repository root",
        ));
    }
    // An absolute path's leading component is empty by construction; reporting
    // both would describe one mistake twice. The absolute-path issue is the
    // better description, so the empty-component check only speaks when nothing
    // else already rejected the path.
    if issues.is_empty() && value != "." && value.split('/').any(str::is_empty) {
        issues.push(ConfigIssue::key(
            key,
            "a path must not contain an empty component",
        ));
    }
    issues
}

/// Refuse a path whose existing components under `root` include a symlink.
fn check_symlink_free(root: Option<&Utf8Path>, relative: &str) -> Vec<ConfigIssue> {
    let Some(root) = root else {
        return Vec::new();
    };
    let mut issues = Vec::new();
    let mut current = root.to_path_buf();
    for component in relative.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        current = current.join(component);
        if let Ok(metadata) = std::fs::symlink_metadata(&current) {
            if metadata.file_type().is_symlink() {
                issues.push(ConfigIssue::key(
                    relative,
                    format!("a path must not traverse the symbolic link {current}"),
                ));
            }
        }
    }
    issues
}

/// Reject two category paths that are equal or where one contains the other.
fn check_overlaps(categories: &BTreeMap<CategoryKey, Category>) -> Vec<ConfigIssue> {
    let mut issues = Vec::new();
    let entries: Vec<(&CategoryKey, &Category)> = categories.iter().collect();
    for (index, (left_key, left)) in entries.iter().enumerate() {
        for (right_key, right) in entries.iter().skip(index + 1) {
            if left.path == right.path {
                issues.push(ConfigIssue::key(
                    format!("categories.{right_key}.path"),
                    format!(
                        "the path {:?} is already used by category {left_key}",
                        right.path
                    ),
                ));
            } else if is_path_prefix(&left.path, &right.path) {
                issues.push(ConfigIssue::key(
                    format!("categories.{right_key}.path"),
                    format!(
                        "the path {:?} overlaps category {left_key} at {:?}",
                        right.path, left.path
                    ),
                ));
            } else if is_path_prefix(&right.path, &left.path) {
                issues.push(ConfigIssue::key(
                    format!("categories.{left_key}.path"),
                    format!(
                        "the path {:?} overlaps category {right_key} at {:?}",
                        left.path, right.path
                    ),
                ));
            }
        }
    }
    issues
}

fn is_path_prefix(candidate: &str, other: &str) -> bool {
    let candidate = candidate.trim_end_matches('/');
    other.starts_with(candidate) && other.as_bytes().get(candidate.len()) == Some(&b'/')
}

/// Render a TOML basic string with the escapes the format requires.
pub(crate) fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other if other.is_control() => {
                out.push_str(&format!("\\u{:04X}", other as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Render a custom-field default as a TOML value.
fn render_field_value(value: &FieldValue) -> String {
    match value {
        FieldValue::String(text) => quote(text),
        FieldValue::Integer(number) => number.to_string(),
        FieldValue::Boolean(flag) => flag.to_string(),
        FieldValue::List(values) => {
            let rendered: Vec<String> = values.iter().map(render_field_value).collect();
            format!("[{}]", rendered.join(", "))
        }
    }
}

// ---------------------------------------------------------------------------
// The raw TOML shape.
//
// Each leaf is wrapped in [`Raw`] for one reason: a TOML type error then names
// the offending key instead of becoming an opaque document-level parse failure.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    version: Raw<String>,
    storage_root: Raw<String>,
    #[serde(default)]
    categories: BTreeMap<String, RawCategory>,
    #[serde(default)]
    fields: BTreeMap<String, RawField>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCategory {
    path: Raw<String>,
    name: Raw<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawField {
    #[serde(rename = "type")]
    ty: Raw<String>,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    default: Option<RawDefault>,
}

/// A leaf whose wrong TOML type reports its own key.
#[derive(Debug)]
struct Raw<T>(T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Raw<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self)
    }
}

/// A `default` value converted from TOML into the field-value model.
#[derive(Debug)]
struct RawDefault(FieldValue);

impl<'de> Deserialize<'de> for RawDefault {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = toml::Value::deserialize(deserializer)?;
        Ok(Self(value.into_field_value()))
    }
}

/// Convert a TOML value into the field-value model.
///
/// Shapes TOML can express but a field type cannot (floats, datetimes, tables)
/// become strings, so the type check reports them against their key instead of
/// the document failing to parse with no key at all.
trait IntoFieldValue {
    fn into_field_value(self) -> FieldValue;
}

impl IntoFieldValue for toml::Value {
    fn into_field_value(self) -> FieldValue {
        match self {
            toml::Value::String(text) => FieldValue::String(text),
            toml::Value::Integer(number) => FieldValue::Integer(number),
            toml::Value::Boolean(flag) => FieldValue::Boolean(flag),
            toml::Value::Array(values) => {
                FieldValue::List(values.into_iter().map(Self::into_field_value).collect())
            }
            other => FieldValue::String(other.to_string()),
        }
    }
}

/// Resolve a configuration-relative path against `root`.
pub fn resolve_relative(root: &Utf8Path, relative: &str) -> Utf8PathBuf {
    let mut path = root.to_path_buf();
    for component in relative.split('/') {
        match component {
            "" | "." => {}
            other => path.push(other),
        }
    }
    path
}
