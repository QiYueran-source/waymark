//! The configuration surface: reading, validating, and the mediated writes.
//!
//! Configuration is the one document a project cannot work without, so this
//! module is deliberately narrow:
//!
//! - **Every write takes the project lock and re-reads the document from disk
//!   inside it.** The revision counter is *not* the conflict check; a hand edit
//!   that leaves the counter alone is still seen, because the content is what is
//!   compared. The counter is informational and only a mediated effective write
//!   moves it.
//! - **The whole resulting configuration is validated before the single
//!   replacement.** A write that would leave the document invalid writes
//!   nothing.
//! - **A write that changes nothing is not a write**: no replacement, no
//!   revision bump.
//! - **No configuration write touches a work-item file.** The only work-item
//!   read in this module is the read-only reference check that refuses removing a
//!   category still in use.
//!
//! What configuration may change is pinned by the spec: the operator, the time
//! zone, cache settings, adding a category, renaming a category's display name,
//! removing an unused category, adding an optional custom field, and relaxing
//! field metadata. A type change, a key rename, a required-field addition, and an
//! optional-to-required change are migration territory and are refused with
//! `ACTION_REQUIRES_MIGRATION`.

use camino::{Utf8Path, Utf8PathBuf};
use serde_json::json;

use crate::config::{
    default_category_name, Category, Config, ConfigIssue, FieldDecl, FieldType, Version,
    CONFIG_FILE,
};
use crate::envelope::Finding;
use crate::error::{Code, ValidationIssue, WaymarkError};
use crate::location::Location;
use crate::lock::with_project_lock;
use crate::model::{CategoryKey, FieldKey, FieldValue};
use crate::ops::OpOptions;
use crate::store::{Fingerprint, Project};

/// The result of one configuration operation.
#[derive(Debug, Clone)]
pub struct ConfigOutcome {
    /// The configuration as it now is, or — on a dry run — as it would be.
    pub config: Config,
    /// The revision the document had before the operation.
    pub previous_revision: u64,
    /// The revision the document has afterwards. Equal to
    /// [`Self::previous_revision`] when nothing changed.
    pub revision: u64,
    /// Whether the serialized content differs from what was on disk.
    pub changed: bool,
    /// The dotted keys the operation changed, in the order it decided them.
    ///
    /// Empty when [`Self::changed`] is false: a request that restates the
    /// current value changes no key.
    pub changed_keys: Vec<String>,
}

/// The inputs `init` needs to write a project's first configuration.
///
/// Every field is optional so the adapter passes only what the caller chose;
/// the domain owns the defaults (`work` for the storage root, `task` for the
/// one category) and the construction of the paths under the root.
#[derive(Debug, Clone, Default)]
pub struct InitRequest {
    /// Repository-relative storage root; defaults to `work`.
    pub storage_root: Option<String>,
    /// The one category to declare; defaults to `task`.
    pub category: Option<String>,
    /// Advisory operator identity to seed into the local document.
    pub operator: Option<String>,
    /// IANA time zone to seed into the local document.
    pub timezone: Option<String>,
}

impl InitRequest {
    /// A request with every default.
    pub fn new() -> Self {
        Self::default()
    }

    /// Use this repository-relative storage root.
    pub fn with_storage_root(mut self, storage_root: impl Into<String>) -> Self {
        self.storage_root = Some(storage_root.into());
        self
    }

    /// Declare this category key.
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Record this operator identity.
    pub fn with_operator(mut self, operator: impl Into<String>) -> Self {
        self.operator = Some(operator.into());
        self
    }

    /// Record this IANA time zone.
    pub fn with_timezone(mut self, timezone: impl Into<String>) -> Self {
        self.timezone = Some(timezone.into());
        self
    }

    /// Build the fresh configuration this request describes.
    pub fn config(&self) -> Result<Config, WaymarkError> {
        let key = CategoryKey::parse(
            self.category
                .as_deref()
                .unwrap_or(crate::config::DEFAULT_CATEGORY_KEY),
        )
        .map_err(|error| {
            WaymarkError::config_invalid("init", vec![format!("category: {error}")])
        })?;

        let mut config = Config::fresh();
        config.storage_root = self
            .storage_root
            .clone()
            .unwrap_or_else(|| crate::config::DEFAULT_STORAGE_ROOT.to_owned());
        config.categories.clear();
        config.categories.insert(
            key.clone(),
            Category {
                path: format!("{}/{}", config.storage_root, key.as_str()),
                name: default_category_name(&key),
            },
        );
        Ok(config)
    }

    /// The local document this request seeds, for an initialization that is
    /// actually creating one.
    pub fn local(&self, root: &Utf8Path) -> Result<crate::user_config::UserConfig, WaymarkError> {
        crate::ops::local_config::seeded(root, self.operator.as_deref(), self.timezone.as_deref())
    }
}

/// Create a project whose first configuration is the requested layout.
///
/// This is the one place an adapter-independent `init` lives: the category
/// path under the storage root, the display name, and the defaults are domain
/// facts, not CLI presentation.
pub fn init(root: &Utf8Path, request: InitRequest, force: bool) -> Result<Project, WaymarkError> {
    let config = request.config()?;
    let metadata = root.join(crate::config::METADATA_DIR);
    // Re-initialization is a no-op and must not build — let alone write — a
    // local document, so the seed is only resolved when this call is creating
    // both documents. An existing local document is never reseeded, not even by
    // `force`: it holds this collaborator's preferences, and `force` only
    // replaces the shared configuration.
    let local = if (metadata.join(CONFIG_FILE).exists() && !force)
        || metadata.join(crate::user_config::USER_CONFIG_FILE).exists()
    {
        None
    } else {
        Some(request.local(root)?)
    };
    Project::init(
        root,
        crate::store::InitOptions {
            force,
            config: Some(config),
            local,
        },
    )
}

/// One local scalar `config set` may address.
///
/// All four are local user settings: the shared configuration has no scalars.
/// They live here because `config set` is one command, and the adapter-facing
/// name list must stay in one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scalar {
    /// The advisory operator identity.
    Operator,
    /// The zone used for human display and offset-less input.
    Timezone,
    /// Whether the in-process projection is kept.
    CacheEnabled,
    /// The projection entry budget.
    CacheCapacity,
}

impl Scalar {
    /// Every scalar name `config set` accepts, in the order help lists them.
    pub const ALL: &'static [Scalar] = &[
        Scalar::Operator,
        Scalar::Timezone,
        Scalar::CacheEnabled,
        Scalar::CacheCapacity,
    ];

    /// Parse a scalar name from an adapter's argument.
    pub fn parse(name: &str) -> Result<Self, WaymarkError> {
        match name {
            "operator" => Ok(Self::Operator),
            "timezone" => Ok(Self::Timezone),
            "cache.enabled" => Ok(Self::CacheEnabled),
            "cache.capacity" => Ok(Self::CacheCapacity),
            other => Err(WaymarkError::validation(
                "config set",
                vec![ValidationIssue::new(
                    "name",
                    format!(
                        "{other:?} is not a configurable scalar (expected operator, timezone, cache.enabled, cache.capacity)"
                    ),
                )],
            )),
        }
    }

    /// The name the adapter prints and the configuration key it writes.
    pub fn name(self) -> &'static str {
        match self {
            Self::Operator => "operator",
            Self::Timezone => "timezone",
            Self::CacheEnabled => "cache.enabled",
            Self::CacheCapacity => "cache.capacity",
        }
    }
}

/// Read the effective configuration from disk.
///
/// A read never takes the write lock: the bytes on disk are the answer, and a
/// caller that needs a consistent read-modify-write cycle uses the write
/// operations below.
pub fn show(project: &Project) -> Result<Config, WaymarkError> {
    Config::load(&project.config_path())
}

/// Read both configuration documents and the values they resolve to.
///
/// This is the one read adapters use for `config show`: the shared document, the
/// local document, and the effective values with their sources, so no adapter
/// re-derives a fallback or re-parses the local document.
pub fn bundle(project: &Project) -> Result<crate::ops::local_config::Bundle, WaymarkError> {
    crate::ops::local_config::bundle(project)
}

/// Every complaint about both configuration documents, as findings.
///
/// Shared-document issues are reported first, then local-document issues, each
/// located at the document that carries it. A local-document problem is not a
/// shared-configuration problem: it never changes the shared revision and says
/// nothing about another clone.
pub fn validate_both(project: &Project) -> Vec<Finding> {
    validate_documents(&project.config_path())
}

/// Every complaint about both configuration documents, found from the shared
/// document's path alone.
///
/// The path form exists because a diagnostic command must work when the shared
/// document cannot be opened as a project at all; the local document is its
/// sibling in the same metadata directory. Every adapter that reviews
/// configuration calls this, so which documents are covered cannot diverge.
pub fn validate_documents(config_path: &Utf8Path) -> Vec<Finding> {
    let mut findings = validate_config_path(config_path);
    findings.extend(local_findings_for(config_path));
    findings
}

/// The local document's findings, found from the shared document's path.
///
/// Composition lives here so every review of configuration covers the same
/// documents; a diagnostic that maps a shared failure to a specific code can
/// reuse this instead of repeating the sibling-path rule.
pub fn local_findings_for(config_path: &Utf8Path) -> Vec<Finding> {
    match config_path.parent() {
        Some(metadata) => crate::ops::local_config::validate_path(
            &metadata.join(crate::user_config::USER_CONFIG_FILE),
        ),
        None => Vec::new(),
    }
}

/// The configuration file of the nearest project at or above `start`, found
/// without parsing it.
///
/// [`Project::discover`](crate::store::Project::discover) opens and validates
/// the document, so an invalid configuration makes discovery fail — and a
/// diagnostic command still has to locate the file it is complaining about. The
/// walk is the same one discovery uses; only the validation is skipped.
pub fn locate_config_path(start: &Utf8Path) -> Result<Utf8PathBuf, WaymarkError> {
    let mut current = Some(absolute(start));
    for _ in 0..crate::store::MAX_DISCOVERY_DEPTH {
        let Some(directory) = current else { break };
        let candidate = directory
            .join(crate::config::METADATA_DIR)
            .join(CONFIG_FILE);
        if candidate.exists() {
            return Ok(candidate);
        }
        current = directory.parent().map(Utf8Path::to_path_buf);
    }
    Err(WaymarkError::project_not_found(start.to_string()))
}

fn absolute(path: &Utf8Path) -> Utf8PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    let Ok(cwd) = std::env::current_dir() else {
        return path.to_path_buf();
    };
    Utf8PathBuf::from_path_buf(cwd)
        .map(|base| base.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

/// Every configuration complaint, as `CONFIG_INVALID` findings.
///
/// One finding per key, each carrying a `config_key` location, so a caller can
/// point at the offending line instead of parsing one aggregate message. A
/// document whose TOML cannot be read at all yields a single finding.
pub fn validate_config(project: &Project) -> Vec<Finding> {
    validate_config_path(&project.config_path())
}

/// Every configuration complaint about the document at `path`, as findings.
///
/// The path form exists because a document that is *semantically* invalid cannot
/// be opened as a [`Project`] at all: `Project::open` validates it and fails. A
/// diagnostic command must still be able to point at the offending keys, so it
/// resolves the file and validates it here.
pub fn validate_config_path(path: &Utf8Path) -> Vec<Finding> {
    match Config::issues_in_file(path) {
        Ok(issues) if issues.is_empty() => Vec::new(),
        Ok(issues) => issues
            .iter()
            .map(|issue| {
                Finding::new(Code::ConfigInvalid, issue.render()).with_location(issue.location())
            })
            .collect(),
        Err(error) => vec![Finding::new(Code::ConfigInvalid, error.message())
            .with_location(Location::config_key(CONFIG_FILE))],
    }
}

/// Add a category.
///
/// The display name falls back to [`default_category_name`] when omitted. The
/// path is validated as part of the resulting document (relative, no escape, no
/// overlap with a sibling category, no symbolic link), and the operation refuses
/// a key that is already declared rather than silently re-pointing it.
pub fn add_category(
    project: &Project,
    key: &CategoryKey,
    path: &str,
    name: Option<&str>,
    options: &OpOptions,
) -> Result<ConfigOutcome, WaymarkError> {
    let name = name.map_or_else(|| default_category_name(key), str::to_owned);
    // The declaration is what makes a category exist; the directory is where its
    // documents will go. It is created **inside** the same critical section,
    // before the configuration is replaced, and removed again if the replacement
    // fails: a reported failure leaves neither a declaration nor a directory.
    // Creating a directory is never a work-item write, and an existing directory
    // (or one another process creates first) is fine.
    mediate_with(
        project,
        options,
        |next| {
            let category = next
                .categories
                .get(key)
                .expect("the replacement declares the category");
            let directory = project.root().join(&category.path);
            let created = create_directory_tree(&directory)?;
            Ok(Some(Box::new(move || remove_directory_tree(&created))))
        },
        |current| {
            if current.categories.contains_key(key) {
                return Err(invalid(
                    format!("categories.{key}"),
                    format!("the category `{key}` already exists"),
                ));
            }
            let mut next = current.clone();
            next.categories.insert(
                key.clone(),
                Category {
                    path: path.trim().to_owned(),
                    name,
                },
            );
            Ok((next, vec![format!("categories.{key}")]))
        },
    )
}

/// Create `directory` and any missing ancestors, returning every directory this
/// call actually created, outermost first.
///
/// Each level is created separately so the returned list names only our own
/// directories: a directory another process created first is not ours to remove
/// if the configuration replacement then fails.
fn create_directory_tree(directory: &Utf8Path) -> Result<Vec<Utf8PathBuf>, WaymarkError> {
    let mut created = Vec::new();
    for path in missing_directories(directory) {
        match std::fs::create_dir(&path) {
            Ok(()) => created.push(path),
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists && path.is_dir() => {}
            Err(source) => {
                remove_directory_tree(&created);
                return Err(WaymarkError::io(path.to_string(), "create", &source));
            }
        }
    }
    Ok(created)
}

/// The missing ancestors of `directory`, outermost first, including itself.
fn missing_directories(directory: &Utf8Path) -> Vec<Utf8PathBuf> {
    let mut missing = Vec::new();
    let mut cursor = Some(directory);
    while let Some(path) = cursor {
        if path.exists() {
            break;
        }
        missing.push(path.to_path_buf());
        cursor = path.parent();
    }
    missing.reverse();
    missing
}

/// Remove the directories this operation created, deepest first.
///
/// `remove_dir` only removes an empty directory, so a concurrent writer's file is
/// never deleted; a directory that cannot be removed is simply left behind.
fn remove_directory_tree(created: &[Utf8PathBuf]) {
    for path in created.iter().rev() {
        let _ = std::fs::remove_dir(path);
    }
}

/// Change a category's display name. Items are untouched.
pub fn set_category_name(
    project: &Project,
    key: &CategoryKey,
    name: &str,
    options: &OpOptions,
) -> Result<ConfigOutcome, WaymarkError> {
    let name = name.trim().to_owned();
    mediate(project, options, |current| {
        let Some(existing) = current.categories.get(key) else {
            return Err(WaymarkError::category_not_found(key.as_str()));
        };
        let mut next = current.clone();
        next.categories.insert(
            key.clone(),
            Category {
                path: existing.path.clone(),
                name,
            },
        );
        Ok((next, vec![format!("categories.{key}.name")]))
    })
}

/// Remove a category that no work item uses.
///
/// The reference check is read-only: the inventory is scanned against the
/// freshly read configuration and every item found in the category is named in
/// the refusal. No work-item file is opened for writing, and none is modified.
pub fn remove_category(
    project: &Project,
    key: &CategoryKey,
    options: &OpOptions,
) -> Result<ConfigOutcome, WaymarkError> {
    mediate(project, options, |current| {
        if !current.categories.contains_key(key) {
            return Err(WaymarkError::category_not_found(key.as_str()));
        }
        let mut scoped = project.clone();
        scoped.set_config(current.clone());
        let inventory = scoped.inventory()?;
        let users: Vec<&crate::store::InventoryEntry> = inventory
            .entries
            .iter()
            .filter(|entry| &entry.category == key)
            .collect();
        if !users.is_empty() {
            let issues = users
                .iter()
                .map(|entry| {
                    ValidationIssue::new(
                        key.as_str(),
                        format!("the category still holds {} at {}", entry.id, entry.path),
                    )
                })
                .collect();
            return Err(
                WaymarkError::validation(format!("categories.{key}"), issues)
                    .with_location(Location::config_key(format!("categories.{key}"))),
            );
        }
        let mut next = current.clone();
        next.categories.remove(key);
        Ok((next, vec![format!("categories.{key}")]))
    })
}

/// Declare a custom field.
///
/// Only an **optional** field may be added: a required field would make every
/// existing item schema-invalid, which is a migration. `default` is a textual
/// token interpreted against `ty`; a token the type cannot hold is rejected
/// before anything is written.
pub fn add_field(
    project: &Project,
    key: &FieldKey,
    ty: FieldType,
    required: bool,
    default: Option<&str>,
    options: &OpOptions,
) -> Result<ConfigOutcome, WaymarkError> {
    if required {
        return Err(WaymarkError::action_requires_migration(
            format!("adding the required custom field `{key}`"),
            "waymark migrate schema --add-required-field",
        ));
    }
    let default = match default {
        Some(raw) => Some(
            parse_default(&ty, raw)
                .map_err(|reason| invalid(format!("fields.{key}.default"), reason))?,
        ),
        None => None,
    };
    mediate(project, options, |current| {
        if current.fields.contains_key(key) {
            return Err(invalid(
                format!("fields.{key}"),
                format!("the custom field `{key}` is already declared"),
            ));
        }
        let mut next = current.clone();
        next.fields.insert(
            key.clone(),
            FieldDecl {
                ty,
                required: false,
                default,
            },
        );
        Ok((next, vec![format!("fields.{key}")]))
    })
}

/// Attributes `config set field` may change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldAttribute {
    /// The declared value shape. Changes are migrations.
    Type,
    /// Requiredness. Adding it is a migration; relaxing it is not.
    Required,
    /// The static default, or its absence.
    Default,
}

impl FieldAttribute {
    /// Every attribute name the command accepts.
    pub const ALL: &'static [FieldAttribute] = &[
        FieldAttribute::Type,
        FieldAttribute::Required,
        FieldAttribute::Default,
    ];

    /// Parse an attribute name from an adapter's argument.
    pub fn parse(name: &str) -> Result<Self, WaymarkError> {
        match name {
            "type" => Ok(Self::Type),
            "required" => Ok(Self::Required),
            "default" => Ok(Self::Default),
            other => Err(invalid(
                "attribute",
                format!("{other:?} is not a field attribute (expected type, required, default)"),
            )),
        }
    }

    /// The attribute's command-line spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::Type => "type",
            Self::Required => "required",
            Self::Default => "default",
        }
    }
}

/// Change one attribute of a declared custom field.
///
/// Permitted: clearing or changing the default, and relaxing `required` from
/// true to false. Refused as migration territory, with
/// `ACTION_REQUIRES_MIGRATION`: changing the type, renaming the key (there is no
/// rename here — an unknown key is a different field), and moving a field from
/// optional to required.
///
/// A `default` value of `null` clears the default; any other token is parsed
/// against the declared type.
pub fn set_field(
    project: &Project,
    key: &FieldKey,
    attribute: FieldAttribute,
    value: &str,
    options: &OpOptions,
) -> Result<ConfigOutcome, WaymarkError> {
    mediate(project, options, |current| {
        let Some(existing) = current.fields.get(key) else {
            return Err(invalid(
                format!("fields.{key}"),
                format!("no custom field `{key}` is declared"),
            ));
        };
        let mut next = current.clone();
        let mut declaration = existing.clone();
        let changed_key = match attribute {
            FieldAttribute::Type => {
                return Err(WaymarkError::action_requires_migration(
                    format!("changing the type of `{key}`"),
                    "waymark migrate schema",
                ))
            }
            FieldAttribute::Required => match value.trim() {
                "false" => {
                    declaration.required = false;
                    format!("fields.{key}.required")
                }
                "true" if existing.required => format!("fields.{key}.required"),
                "true" => {
                    return Err(WaymarkError::action_requires_migration(
                        format!("making `{key}` required"),
                        "waymark migrate schema --add-required-field",
                    ))
                }
                other => {
                    return Err(invalid(
                        format!("fields.{key}.required"),
                        format!("expected `true` or `false`, found {other:?}"),
                    ))
                }
            },
            FieldAttribute::Default => {
                if value.trim() == "null" {
                    declaration.default = None;
                } else {
                    declaration.default = Some(
                        parse_default(&existing.ty, value)
                            .map_err(|reason| invalid(format!("fields.{key}.default"), reason))?,
                    );
                }
                format!("fields.{key}.default")
            }
        };
        next.fields.insert(key.clone(), declaration);
        Ok((next, vec![changed_key]))
    })
}

/// Parse a textual default against a declared type.
fn parse_default(ty: &FieldType, raw: &str) -> Result<FieldValue, String> {
    let value = ty.parse_value(raw)?;
    if !ty.accepts(&value) {
        return Err(format!("the default is not a {}", ty.as_str()));
    }
    Ok(value)
}

/// A `VALIDATION_ERROR` about one configuration key.
pub(crate) fn invalid(key: impl Into<String>, reason: impl Into<String>) -> WaymarkError {
    let key = key.into();
    WaymarkError::validation(key.clone(), vec![ValidationIssue::new(key.clone(), reason)])
        .with_location(Location::config_key(key))
}

/// The locked read-modify-validate-write cycle every configuration write uses.
///
/// `body` receives the configuration **as it is on disk right now** and returns
/// the complete replacement plus the keys it intended to change. The replacement
/// is compared by canonical serialization: when the bytes are identical nothing
/// is written and the revision does not move.
fn mediate(
    project: &Project,
    options: &OpOptions,
    body: impl FnOnce(&Config) -> Result<(Config, Vec<String>), WaymarkError>,
) -> Result<ConfigOutcome, WaymarkError> {
    mediate_with(project, options, |_| Ok(None), body)
}

/// Undo work a configuration write did before its replacement failed.
type Rollback = Box<dyn FnOnce()>;

/// The locked read-modify-validate-write cycle with a pre-replacement hook.
///
/// `before_write` receives the validated replacement and runs **inside the same
/// critical section**, immediately before the one atomic configuration
/// replacement. It may create the things the new configuration implies (a
/// category directory) and returns the undo for them; the undo runs when the
/// replacement itself fails, so a reported failure leaves nothing behind. A dry
/// run never calls it.
fn mediate_with(
    project: &Project,
    options: &OpOptions,
    before_write: impl FnOnce(&Config) -> Result<Option<Rollback>, WaymarkError>,
    body: impl FnOnce(&Config) -> Result<(Config, Vec<String>), WaymarkError>,
) -> Result<ConfigOutcome, WaymarkError> {
    let config_path = project.config_path();
    with_project_lock(project, options.lock_wait, || {
        // Fingerprint the exact bytes on disk, then parse them: a caller that
        // supplied an expectation is guarding against a concurrent hand edit,
        // not against this module's canonical rendering.
        let raw = std::fs::read(&config_path)
            .map_err(|source| WaymarkError::io(config_path.to_string(), "read", &source))?;
        let current = Config::load(&config_path)?;
        if let Some(expected) = &options.expected_config_fingerprint {
            let actual = Fingerprint::of(&raw);
            if &actual != expected {
                return Err(config_conflict(&config_path, expected, &actual));
            }
        }
        let previous_revision = current.version.revision;
        let current_bytes = current.to_toml_string();

        let (mut next, changed_keys) = body(&current)?;
        next.version = Version {
            generation: current.version.generation,
            revision: previous_revision,
        };

        if next.to_toml_string() == current_bytes {
            return Ok(ConfigOutcome {
                config: current,
                previous_revision,
                revision: previous_revision,
                changed: false,
                changed_keys: Vec::new(),
            });
        }

        next.version.revision = previous_revision.saturating_add(1);
        let issues = next.issues_at(project.root());
        if !issues.is_empty() {
            return Err(config_invalid_error(&config_path, &issues));
        }
        let bytes = next.to_toml_string();
        if !options.dry_run {
            let rollback = before_write(&next)?;
            if let Err(error) = crate::write::replace_unchecked(&config_path, bytes.as_bytes()) {
                if let Some(rollback) = rollback {
                    rollback();
                }
                return Err(error);
            }
        }
        Ok(ConfigOutcome {
            config: next,
            previous_revision,
            revision: previous_revision.saturating_add(1),
            changed: true,
            changed_keys,
        })
    })
}

/// The `WRITE_CONFLICT` a stale expected configuration fingerprint produces.
///
/// The details mirror the work-item conflict shape: the path and the two
/// fingerprints. A configuration document has no stable item ID, so there is no
/// `id` key.
fn config_conflict(path: &Utf8Path, expected: &Fingerprint, actual: &Fingerprint) -> WaymarkError {
    WaymarkError::new(
        Code::WriteConflict,
        format!("{path} changed on disk since it was read; reread and retry"),
    )
    .with_details(json!({
        "path": path.to_string(),
        "expected_fingerprint": expected.as_str(),
        "actual_fingerprint": actual.as_str(),
    }))
    .with_location(Location::config_key(CONFIG_FILE))
}

fn config_invalid_error(path: &Utf8Path, issues: &[ConfigIssue]) -> WaymarkError {
    WaymarkError::config_invalid(
        path.to_string(),
        issues.iter().map(ConfigIssue::render).collect(),
    )
    .with_location(Location::config_key(CONFIG_FILE))
}
