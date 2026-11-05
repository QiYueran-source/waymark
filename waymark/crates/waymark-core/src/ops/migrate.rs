//! Version and schema migration.
//!
//! A migration is the only operation allowed to change a document's format
//! generation or to reshape declared custom fields across the whole project.
//! Because it can rewrite many files at once, it is shaped as one explicit
//! **plan** followed by one **execution**:
//!
//! - planning is read-only, takes the complete projection, revalidates every
//!   file, and produces the exact canonical bytes it would write;
//! - execution repeats every check (configuration readability, projection
//!   completeness and freshness, a fingerprint agreement for every planned
//!   target) and then writes the whole plan as one [`crate::write::replace_batch`];
//! - a dry run stops after planning and reports the same affected set with
//!   `written: false`. Nothing is persisted: there is no plan file, no plan id,
//!   and no plan/apply handshake. Re-derivation at execution time is the whole
//!   freshness story, so `migrate_version`/`migrate_schema` rebuild the plan
//!   immediately before executing it.
//!
//! # What each kind may change
//!
//! - **Version migration** rewrites every document whose generation is older
//!   than the requested target (which must be the release's own generation) and
//!   raises the configuration generation when it disagrees. It preserves the
//!   body, custom fields, relationships, archive markers, and `updated_at`: a
//!   generation rewrite is not a content change.
//! - **Schema migration** performs exactly one action. Removing a field deletes
//!   the declaration and the key from every document, archived ones included;
//!   adding a required field adds the declaration and supplies its default
//!   wherever a document has no value, so no document is left invalid. A
//!   configuration change increments the revision exactly once, and the
//!   configuration and the documents are written in one batch.
//! - **Storage migration** performs exactly one action: change the storage root
//!   and re-base every category directory, change one category path, or rename
//!   one category key. It prevalidates every target path before any write,
//!   relocates directories with a same-filesystem `rename(2)` (never a copy),
//!   and a key change also rewrites the persisted `category` value on every
//!   affected work item. A failed document/configuration batch restores the
//!   moves; an unrestorable restore is `ROLLBACK_FAILED`.
//!
//! Field type changes, key renames, list element type changes, and
//! optional-to-required changes made *outside* the migration verbs stay refused
//! by [`crate::ops::config`] with `ACTION_REQUIRES_MIGRATION`; this module
//! refuses the first three with the same code and the same recovery command
//! strings, because they are not a schema migration.
//!
//! # Outside Git
//!
//! A migration is permitted outside a Git repository. The report carries a
//! `NOT_A_GIT_REPOSITORY` warning in `_meta.warnings` saying that Git-based
//! recovery is unavailable; it is never an error.
//!
//! # Provisional decisions
//!
//! Recorded in `docs/implementation-decisions.md`: `updated_at` is preserved by
//! both migration kinds; a version migration from a project generation that is
//! already the target but with stale documents rewrites exactly those documents
//! and changes no configuration content, so the revision does not move.

use std::path::Path as StdPath;

use camino::{Utf8Path, Utf8PathBuf};
use serde_json::{json, Value};

use crate::config::{Config, FieldDecl, FieldType, Version, CONFIG_FILE};
use crate::envelope::{Meta, Warning};
use crate::error::{Code, TargetOutcome, ValidationIssue, WaymarkError};
use crate::location::Location;
use crate::model::{CategoryKey, FieldKey, FieldValue, WorkItem, FORMAT_GENERATION};
use crate::ops::OpOptions;
use crate::projection::Projection;
use crate::store::{Fingerprint, Project};

/// The kind of migration a plan or report describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationKind {
    /// Raise the format generation of the configuration and its documents.
    Version,
    /// Reshape one declared custom field across the configuration and its
    /// documents.
    Schema,
    /// Move storage directories, optionally rewriting persisted category values.
    Storage,
}

impl MigrationKind {
    /// The stable literal adapters print.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Version => "version",
            Self::Schema => "schema",
            Self::Storage => "storage",
        }
    }
}

/// The single action a schema migration may perform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaAction {
    /// Delete the declaration and the key from every work item, archived ones
    /// included.
    RemoveField(FieldKey),
    /// Add a required declaration and supply the default wherever an item has
    /// no value.
    AddRequiredField {
        /// Custom-field key.
        key: FieldKey,
        /// Declared value shape.
        ty: FieldType,
        /// Value written into every document that has no value for the key.
        default: Option<FieldValue>,
    },
}

impl SchemaAction {
    /// The recovery command the matching `ACTION_REQUIRES_MIGRATION` names.
    pub fn required_command(&self) -> &'static str {
        match self {
            Self::RemoveField(_) => "waymark migrate schema --remove-field",
            Self::AddRequiredField { .. } => "waymark migrate schema --add-required-field",
        }
    }
}

/// The single action a storage migration may perform.
///
/// Actions are never combined: a run changes the storage root, one category's
/// path, or one category's key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageAction {
    /// Move the storage root and re-base every category directory under it.
    Root {
        /// The new repository-relative storage root.
        path: String,
    },
    /// Move one category's directory and point its declaration at the new path.
    CategoryPath {
        /// The category to move.
        key: CategoryKey,
        /// The new repository-relative path.
        path: String,
    },
    /// Rename a category key, move its directory to the key's path, and rewrite
    /// the persisted `category` value on every affected work item.
    CategoryKey {
        /// The declared key to replace.
        from: CategoryKey,
        /// The new key.
        to: CategoryKey,
    },
}

/// One directory the plan relocates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedMove {
    /// The directory as it is now (absolute).
    pub from: Utf8PathBuf,
    /// The directory as the migration leaves it (absolute).
    pub to: Utf8PathBuf,
    /// A digest of the source directory the plan was built from.
    ///
    /// Execution recomputes it, so a hand edit anywhere under the source
    /// directory is detected before the rename.
    pub expected: Fingerprint,
}

/// A category-key change, which is the only relocation that also rewrites
/// persisted work-item values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryChange {
    /// The persisted value before the migration.
    pub from: CategoryKey,
    /// The persisted value after the migration.
    pub to: CategoryKey,
}

/// One directory the report says was (or would be) moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveOutcome {
    /// The directory before.
    pub from: String,
    /// The directory after.
    pub to: String,
    /// `applied`, `skipped`, or `failed`.
    pub outcome: String,
    /// The failure detail, when the directory failed to move.
    pub error: Option<String>,
}

/// One persisted `category` value a category-key migration rewrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewrittenCategory {
    /// The affected item.
    pub id: crate::id::WorkItemId,
    /// The document path before the move.
    pub path: String,
    /// The value before the migration.
    pub from: CategoryKey,
    /// The value after the migration.
    pub to: CategoryKey,
}

/// One item the plan will rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffectedItem {
    /// The item's stable ID.
    pub id: crate::id::WorkItemId,
    /// The document path.
    pub path: Utf8PathBuf,
}

/// One document the plan will replace.
#[derive(Debug, Clone)]
pub struct PlannedTarget {
    /// The item the document belongs to.
    pub id: crate::id::WorkItemId,
    /// The document path.
    pub path: Utf8PathBuf,
    /// The fingerprint the plan was built from.
    pub expected: Fingerprint,
    /// The exact canonical bytes to write.
    pub bytes: Vec<u8>,
}

/// The configuration document the plan will replace.
#[derive(Debug, Clone)]
pub struct PlannedConfig {
    /// `.waymark/config.toml`.
    pub path: Utf8PathBuf,
    /// The fingerprint the plan was built from.
    pub expected: Fingerprint,
    /// The exact canonical bytes to write.
    pub bytes: Vec<u8>,
    /// The version on disk when the plan was built.
    pub previous_version: Version,
    /// The version this plan writes.
    pub version: Version,
}

/// The complete, read-only description of one migration.
///
/// A plan is disposable and is never persisted. [`execute`] re-checks every
/// target fingerprint before writing, so a plan built for one store state can
/// never be applied to a different one.
#[derive(Debug, Clone)]
pub struct MigrationPlan {
    /// Which migration this is.
    pub kind: MigrationKind,
    /// Human-readable one-line description.
    pub description: String,
    /// Every work-item document to replace, sorted by path.
    pub targets: Vec<PlannedTarget>,
    /// Every directory a storage migration relocates, in application order.
    pub moves: Vec<PlannedMove>,
    /// The category-value change a storage migration rewrites, when it is a key
    /// change.
    pub category_change: Option<CategoryChange>,
    /// The configuration replacement, when configuration content changes.
    pub config: Option<PlannedConfig>,
    /// Warnings the run justifies (`NOT_A_GIT_REPOSITORY`).
    pub warnings: Vec<Warning>,
}

impl MigrationPlan {
    /// How many work-item documents the plan rewrites.
    pub fn count(&self) -> usize {
        self.targets.len()
    }

    /// Whether the plan changes configuration content.
    pub fn config_changed(&self) -> bool {
        self.config.is_some()
    }

    /// Whether the plan would write anything at all.
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty() && self.moves.is_empty() && self.config.is_none()
    }

    /// The affected items, in stable-ID order.
    pub fn affected(&self) -> Vec<AffectedItem> {
        let mut affected: Vec<AffectedItem> = self
            .targets
            .iter()
            .map(|target| AffectedItem {
                id: target.id.clone(),
                path: target.path.clone(),
            })
            .collect();
        affected.sort_by(|left, right| left.id.cmp(&right.id));
        affected
    }

    /// The configuration revision before and after the plan.
    pub fn revisions(&self, current: u64) -> (u64, u64) {
        match &self.config {
            Some(config) => (config.previous_version.revision, config.version.revision),
            None => (current, current),
        }
    }
}

/// What a migration produced, or would produce on a dry run.
#[derive(Debug, Clone)]
pub struct MigrationReport {
    /// Which migration produced it.
    pub kind: MigrationKind,
    /// Human-readable one-line description.
    pub description: String,
    /// Whether this was a dry run.
    pub dry_run: bool,
    /// Every affected item, in stable-ID order.
    pub affected: Vec<AffectedItem>,
    /// One outcome per target (documents first, then the configuration).
    pub targets: Vec<TargetOutcome>,
    /// Every directory the storage migration moved (or would move).
    pub moves: Vec<MoveOutcome>,
    /// Every persisted `category` value the migration rewrote (or would
    /// rewrite).
    pub rewritten: Vec<RewrittenCategory>,
    /// How many work-item documents were (or would be) rewritten.
    pub count: usize,
    /// Whether anything was written. Always `false` on a dry run.
    pub written: bool,
    /// Whether configuration content changed (or would change).
    pub config_changed: bool,
    /// The configuration revision before the migration.
    pub previous_revision: u64,
    /// The configuration revision after the migration.
    pub revision: u64,
    /// The `_meta` block the run justifies.
    pub meta: Meta,
}

impl MigrationReport {
    /// The machine form, for an adapter that emits the envelope.
    pub fn data_json(&self) -> Value {
        json!({
            "migration": self.kind.as_str(),
            "description": self.description,
            "dry_run": self.dry_run,
            "affected": self
                .affected
                .iter()
                .map(|item| json!({"id": item.id.as_str(), "path": item.path.to_string()}))
                .collect::<Vec<_>>(),
            "targets": self.targets,
            "moves": self
                .moves
                .iter()
                .map(|moved| {
                    let mut value = json!({
                        "from": moved.from,
                        "to": moved.to,
                        "outcome": moved.outcome,
                    });
                    if let Some(error) = &moved.error {
                        value["error"] = json!(error);
                    }
                    value
                })
                .collect::<Vec<_>>(),
            "rewritten": self
                .rewritten
                .iter()
                .map(|rewritten| json!({
                    "id": rewritten.id.as_str(),
                    "path": rewritten.path,
                    "from": rewritten.from.as_str(),
                    "to": rewritten.to.as_str(),
                }))
                .collect::<Vec<_>>(),
            "count": self.count,
            "written": self.written,
            "config_changed": self.config_changed,
            "previous_revision": self.previous_revision,
            "revision": self.revision,
        })
    }
}

/// One parsed, fingerprinted document of the read-only preflight scan.
#[derive(Debug, Clone)]
pub struct ScannedItem {
    /// The item's stable ID.
    pub id: crate::id::WorkItemId,
    /// The document path.
    pub path: Utf8PathBuf,
    /// The parsed document.
    pub item: WorkItem,
    /// The fingerprint of the bytes that were parsed.
    pub fingerprint: Fingerprint,
}

/// Everything a migration validates before it may plan or write.
///
/// This is the shared read-only gate phase B's storage migration also uses: it
/// loads the configuration afresh, requires a complete and fresh projection,
/// proves that every document still matches the fingerprint the projection
/// observed, and attaches the `NOT_A_GIT_REPOSITORY` warning. Any failure is
/// `MIGRATION_PRECONDITION_FAILED` carrying every diagnostic, before any write.
#[derive(Debug, Clone)]
pub struct Preflight {
    /// The configuration as read from disk.
    pub config: Config,
    /// The fingerprint of the configuration document the plan was based on.
    pub config_fingerprint: Fingerprint,
    /// Every document in the project, sorted by stable ID.
    pub items: Vec<ScannedItem>,
    /// The warnings a run starting from this state justifies.
    pub warnings: Vec<Warning>,
}

/// Read and validate the store for a migration. Read-only.
pub fn preflight(project: &Project) -> Result<Preflight, WaymarkError> {
    let config_path = project.config_path();
    let config = load_config_precondition(&config_path)?;

    let projection = Projection::load(project).map_err(|error| {
        WaymarkError::migration_precondition_failed(vec![json!({
            "kind": "projection_unreadable",
            "path": project.root().to_string(),
            "code": error.code().as_str(),
            "message": error.message(),
        })])
    })?;
    if !projection.is_complete() {
        let mut diagnostics: Vec<Value> = projection
            .diagnostics()
            .iter()
            .filter_map(|finding| serde_json::to_value(finding).ok())
            .collect();
        diagnostics.push(json!({
            "kind": "incomplete_projection",
            "completeness": projection.completeness().as_str(),
            "freshness": projection.freshness().as_str(),
            "paths": projection
                .incomplete_paths()
                .iter()
                .map(|path| path.to_string())
                .collect::<Vec<_>>(),
        }));
        return Err(WaymarkError::migration_precondition_failed(diagnostics));
    }

    let mut items: Vec<ScannedItem> = Vec::with_capacity(projection.entries().count());
    let mut drift: Vec<Value> = Vec::new();
    for entry in projection.entries() {
        let actual = project.fingerprint(&entry.path).map_err(|error| {
            WaymarkError::migration_precondition_failed(vec![json!({
                "kind": "unreadable_fingerprint",
                "path": entry.path.to_string(),
                "code": error.code().as_str(),
                "message": error.message(),
            })])
        })?;
        if actual != entry.fingerprint {
            drift.push(json!({
                "kind": "fingerprint_disagreement",
                "path": entry.path.to_string(),
                "expected": entry.fingerprint.as_str(),
                "actual": actual.as_str(),
            }));
        }
        items.push(ScannedItem {
            id: entry.id.clone(),
            path: entry.path.clone(),
            item: entry.item.clone(),
            fingerprint: entry.fingerprint.clone(),
        });
    }
    if !drift.is_empty() {
        return Err(WaymarkError::migration_precondition_failed(drift));
    }
    items.sort_by(|left, right| left.id.cmp(&right.id));

    let config_fingerprint = project.fingerprint(&config_path)?;
    let warnings = if crate::git::find_repository(project).is_none() {
        vec![Warning::from_error(&WaymarkError::not_a_git_repository())]
    } else {
        Vec::new()
    };

    Ok(Preflight {
        config,
        config_fingerprint,
        items,
        warnings,
    })
}

/// Translate an unreadable configuration into the migration precondition
/// failure, carrying every diagnostic the document offers.
fn load_config_precondition(path: &camino::Utf8Path) -> Result<Config, WaymarkError> {
    match Config::load(path) {
        Ok(config) => Ok(config),
        Err(error) => {
            let mut diagnostics = vec![json!({
                "kind": "config_unreadable",
                "path": path.to_string(),
                "code": error.code().as_str(),
                "message": error.message(),
            })];
            if let Ok(issues) = Config::issues_in_file(path) {
                for issue in issues {
                    diagnostics.push(json!({
                        "kind": "config_issue",
                        "key": issue.key,
                        "message": issue.reason,
                    }));
                }
            }
            Err(WaymarkError::migration_precondition_failed(diagnostics))
        }
    }
}

/// Plan a version migration to `target_generation`. Read-only.
///
/// The target must be the generation this release writes; anything else is
/// `MIGRATION_UNSUPPORTED_GENERATION`.
pub fn plan_version(
    project: &Project,
    target_generation: u32,
) -> Result<MigrationPlan, WaymarkError> {
    if target_generation != FORMAT_GENERATION {
        return Err(WaymarkError::migration_unsupported_generation(
            target_generation,
            FORMAT_GENERATION,
        ));
    }

    let state = preflight(project)?;
    let current_generation = state.config.version.generation;

    let mut targets: Vec<PlannedTarget> = Vec::new();
    for item in &state.items {
        if !item.item.needs_migration_from(target_generation) {
            continue;
        }
        let mut migrated = item.item.clone();
        migrated.schema = target_generation;
        // A generation rewrite is not a content change: `updated_at`, the body,
        // custom fields, relationships, and archive markers are preserved.
        targets.push(PlannedTarget {
            id: item.id.clone(),
            path: item.path.clone(),
            expected: item.fingerprint.clone(),
            bytes: crate::serialize::to_document(&migrated).into_bytes(),
        });
    }
    targets.sort_by(|left, right| left.path.cmp(&right.path));

    let config = if current_generation == target_generation {
        None
    } else {
        let mut next = state.config.clone();
        next.version = Version {
            generation: target_generation,
            revision: state.config.version.revision.saturating_add(1),
        };
        validate_next_config(project, &next)?;
        Some(PlannedConfig {
            path: project.config_path(),
            expected: state.config_fingerprint.clone(),
            bytes: next.to_toml_string().into_bytes(),
            previous_version: state.config.version,
            version: next.version,
        })
    };

    Ok(MigrationPlan {
        kind: MigrationKind::Version,
        description: format!(
            "migrate format generation {current_generation} to {target_generation} ({} document(s))",
            targets.len()
        ),
        targets,
        moves: Vec::new(),
        category_change: None,
        config,
        warnings: state.warnings,
    })
}

/// Plan a schema migration. Read-only.
pub fn plan_schema(
    project: &Project,
    action: &SchemaAction,
) -> Result<MigrationPlan, WaymarkError> {
    let state = preflight(project)?;
    let current = &state.config;
    let mut next = current.clone();
    let mut targets: Vec<PlannedTarget> = Vec::new();
    let description;

    match action {
        SchemaAction::RemoveField(key) => {
            if !current.fields.contains_key(key) {
                return Err(schema_invalid(
                    format!("fields.{key}"),
                    format!("no custom field `{key}` is declared"),
                ));
            }
            next.fields.remove(key);
            description =
                format!("remove custom field `{key}` from the configuration and every work item");
            for item in &state.items {
                if current_value(&item.item, key).is_none() {
                    continue;
                }
                let mut rewritten = item.item.clone();
                rewritten.custom_fields.remove(key.as_str());
                targets.push(rewrite(item, &rewritten));
            }
        }
        SchemaAction::AddRequiredField { key, ty, default } => {
            if let Some(existing) = current.fields.get(key) {
                if existing.ty != *ty {
                    return Err(WaymarkError::action_requires_migration(
                        format!("changing the type of `{key}`"),
                        "waymark migrate schema",
                    ));
                }
                // A declaration that already exists can never be promoted to
                // required here: that makes every existing item invalid, which
                // the spec puts outside migration. The same refusal as
                // `config set field <key> required true` keeps them in step.
                if existing.required {
                    return Err(WaymarkError::action_requires_migration(
                        format!("changing the declared custom field `{key}`"),
                        "waymark migrate schema",
                    ));
                }
                return Err(WaymarkError::action_requires_migration(
                    format!("making `{key}` required"),
                    "waymark migrate schema --add-required-field",
                ));
            }
            // Only a genuinely new required field is a migration action.
            if let Some(value) = default {
                if !ty.accepts(value) {
                    return Err(schema_invalid(
                        format!("fields.{key}.default"),
                        format!("the default is not a {}", ty.as_str()),
                    ));
                }
            }
            next.fields.insert(
                key.clone(),
                FieldDecl {
                    ty: ty.clone(),
                    required: true,
                    default: default.clone(),
                },
            );
            description = format!("add required custom field `{key}` and its default");
            let fill = default.clone();
            let mut missing: Vec<String> = Vec::new();
            for item in &state.items {
                match current_value(&item.item, key) {
                    Some(value) if !ty.accepts(value) => {
                        return Err(schema_invalid(
                            format!("fields.{key}"),
                            format!(
                                "{} has a `{key}` value the declared type {} cannot hold; \
                                 convert the data explicitly and retry",
                                item.id,
                                ty.as_str()
                            ),
                        ));
                    }
                    Some(_) => {}
                    None => match &fill {
                        Some(value) => {
                            let mut rewritten = item.item.clone();
                            rewritten
                                .custom_fields
                                .insert(key.as_str().to_owned(), value.clone());
                            targets.push(rewrite(item, &rewritten));
                        }
                        None => missing.push(item.id.as_str().to_owned()),
                    },
                }
            }
            if !missing.is_empty() {
                return Err(schema_invalid(
                    format!("fields.{key}"),
                    format!(
                        "{} work item(s) have no `{key}` value and no default was supplied: {}",
                        missing.len(),
                        missing.join(", ")
                    ),
                ));
            }
        }
    }
    targets.sort_by(|left, right| left.path.cmp(&right.path));

    // A schema migration never changes the format generation.
    let mut probe = next.clone();
    probe.version = current.version;
    let config = if probe.to_toml_string() == current.to_toml_string() {
        None
    } else {
        next.version = Version {
            generation: current.version.generation,
            revision: current.version.revision.saturating_add(1),
        };
        validate_next_config(project, &next)?;
        Some(PlannedConfig {
            path: project.config_path(),
            expected: state.config_fingerprint.clone(),
            bytes: next.to_toml_string().into_bytes(),
            previous_version: current.version,
            version: next.version,
        })
    };

    Ok(MigrationPlan {
        kind: MigrationKind::Schema,
        description,
        targets,
        moves: Vec::new(),
        category_change: None,
        config,
        warnings: state.warnings,
    })
}

/// Execute a plan, repeating every check first. The caller holds the lock.
///
/// Every target fingerprint is compared with the store again, every planned
/// document is parsed again, and the planned configuration is validated again.
/// A disagreement writes nothing and fails with
/// `MIGRATION_PRECONDITION_FAILED`; a write failure is `MIGRATION_FAILED` with
/// one outcome per target, except an unrestorable rollback, which stays
/// `ROLLBACK_FAILED`.
pub fn execute(
    project: &Project,
    plan: &MigrationPlan,
    options: &OpOptions,
) -> Result<MigrationReport, WaymarkError> {
    let state = preflight(project)?;

    // 1. Fingerprint agreement: the plan must describe the bytes on disk now.
    let mut drift: Vec<Value> = Vec::new();
    for target in &plan.targets {
        drift.extend(fingerprint_drift(project, &target.path, &target.expected)?);
    }
    if let Some(config) = &plan.config {
        drift.extend(fingerprint_drift(project, &config.path, &config.expected)?);
    }
    if !drift.is_empty() {
        return Err(WaymarkError::migration_precondition_failed(drift));
    }

    // 2. Re-validate every planned document: canonical bytes that parse to the
    //    same ID are the last line of defence against a torn plan.
    for target in &plan.targets {
        let parsed =
            crate::parse::parse_document(&target.bytes, &target.path).map_err(|error| {
                WaymarkError::migration_precondition_failed(vec![json!({
                    "kind": "planned_document_invalid",
                    "path": target.path.to_string(),
                    "id": target.id.as_str(),
                    "code": error.code().as_str(),
                    "message": error.message(),
                })])
            })?;
        if parsed.id != target.id {
            return Err(WaymarkError::migration_precondition_failed(vec![json!({
                "kind": "planned_document_id_mismatch",
                "path": target.path.to_string(),
                "expected": target.id.as_str(),
                "found": parsed.id.as_str(),
            })]));
        }
    }

    // 3. Re-validate the planned configuration.
    if let Some(config) = &plan.config {
        let text = std::str::from_utf8(&config.bytes).map_err(|_| {
            WaymarkError::migration_precondition_failed(vec![json!({
                "kind": "planned_config_invalid",
                "path": config.path.to_string(),
                "message": "the planned configuration is not UTF-8",
            })])
        })?;
        Config::parse(text).map_err(|error| {
            WaymarkError::migration_precondition_failed(vec![json!({
                "kind": "planned_config_invalid",
                "path": config.path.to_string(),
                "code": error.code().as_str(),
                "message": error.message(),
            })])
        })?;
    }

    // 3b. A storage plan repeats its own read-only checks: every source
    //     directory must still be where (and what) the plan observed, and every
    //     target must still be free.
    if plan.kind == MigrationKind::Storage {
        recheck_storage(project, plan, &state.config)?;
    }

    let (previous_revision, revision) = plan.revisions(state.config.version.revision);
    let meta = Meta::memory_projection_complete().with_warnings(state.warnings.clone());

    if options.dry_run {
        return Ok(report(
            plan,
            true,
            false,
            (previous_revision, revision),
            meta,
            skipped_outcomes(plan, "skipped"),
            move_outcomes(plan, "skipped"),
        ));
    }
    if plan.is_empty() {
        return Ok(report(
            plan,
            false,
            false,
            (previous_revision, revision),
            meta,
            Vec::new(),
            Vec::new(),
        ));
    }

    // 4. A storage plan moves directories first and then, in one batch, writes
    //    the rewritten documents and the configuration. A failure after a move
    //    restores the move.
    if plan.kind == MigrationKind::Storage {
        return execute_storage(plan, previous_revision, revision, meta);
    }

    // 5. One batch: every document, then the configuration when it changed.
    let staged: Vec<StagedTarget> = plan
        .targets
        .iter()
        .map(|target| StagedTarget {
            id: target.id.clone(),
            path: target.path.clone(),
            expected: target.expected.clone(),
            bytes: target.bytes.clone(),
        })
        .collect();
    let batch: Vec<crate::write::BatchTarget<'_>> = staged
        .iter()
        .map(|target| crate::write::BatchTarget {
            id: &target.id,
            path: &target.path,
            expected: &target.expected,
            bytes: &target.bytes,
        })
        .collect();
    let config_target = plan.config.as_ref().map(|config| crate::write::FileTarget {
        path: &config.path,
        expected: &config.expected,
        bytes: &config.bytes,
    });

    match crate::write::replace_batch_with_file(&batch, config_target) {
        Ok(_) => Ok(report(
            plan,
            false,
            true,
            (previous_revision, revision),
            meta,
            applied_outcomes(plan),
            Vec::new(),
        )),
        Err(error) => Err(execution_error(plan, &error)),
    }
}

/// Plan and execute a version migration.
///
/// A real run takes the project lock and rebuilds the plan inside it right
/// before execution, so no preview can be applied to newer bytes. A dry run
/// performs the same read-only plan and checks without taking the lock, because
/// previews never block on the write lock.
pub fn migrate_version(
    project: &Project,
    target_generation: u32,
    options: &OpOptions,
) -> Result<MigrationReport, WaymarkError> {
    if target_generation != FORMAT_GENERATION {
        return Err(WaymarkError::migration_unsupported_generation(
            target_generation,
            FORMAT_GENERATION,
        ));
    }
    if options.dry_run {
        let plan = plan_version(project, target_generation)?;
        return execute(project, &plan, options);
    }
    crate::lock::with_project_lock(project, options.lock_wait, || {
        let plan = plan_version(project, target_generation)?;
        execute(project, &plan, options)
    })
}

/// Plan and execute a schema migration.
///
/// A dry run is lock-free: it runs every check and reports, and writes nothing.
pub fn migrate_schema(
    project: &Project,
    action: SchemaAction,
    options: &OpOptions,
) -> Result<MigrationReport, WaymarkError> {
    if options.dry_run {
        let plan = plan_schema(project, &action)?;
        return execute(project, &plan, options);
    }
    crate::lock::with_project_lock(project, options.lock_wait, || {
        let plan = plan_schema(project, &action)?;
        execute(project, &plan, options)
    })
}

/// Plan and execute a storage migration.
///
/// A dry run is lock-free: it runs every check and reports, and writes nothing.
pub fn migrate_storage(
    project: &Project,
    action: &StorageAction,
    options: &OpOptions,
) -> Result<MigrationReport, WaymarkError> {
    if options.dry_run {
        let plan = plan_storage(project, action)?;
        return execute(project, &plan, options);
    }
    crate::lock::with_project_lock(project, options.lock_wait, || {
        let plan = plan_storage(project, action)?;
        execute(project, &plan, options)
    })
}

/// Plan a storage migration. Read-only.
///
/// A storage plan performs the shared [`preflight`] gate and then its own path
/// checks: every target must be repository-relative, must not escape the root,
/// must not overlap an existing category directory by equality, ancestry, or
/// descent, must not be case-equivalent to one, must not exist as a non-empty
/// directory, and must not traverse a symbolic link. Every diagnostic is
/// collected before the run aborts.
pub fn plan_storage(
    project: &Project,
    action: &StorageAction,
) -> Result<MigrationPlan, WaymarkError> {
    let state = preflight(project)?;
    let current = &state.config;
    let root = project.root();

    let mut next = current.clone();
    let mut relative_moves: Vec<(String, String)> = Vec::new();
    let mut diagnostics: Vec<Value> = Vec::new();
    let mut conflicts: Vec<(String, String)> = Vec::new();
    let mut category_change: Option<CategoryChange> = None;
    let description;

    match action {
        StorageAction::Root { path } => {
            let Some(new_root) = checked_relative_path("storage_root", path, &mut diagnostics)
            else {
                return Err(storage_error(conflicts, diagnostics));
            };
            for (key, category) in &current.categories {
                let Some(new_path) =
                    rebase_category(&category.path, &current.storage_root, &new_root)
                else {
                    diagnostics.push(json!({
                        "kind": "category_outside_storage_root",
                        "category": key.as_str(),
                        "path": category.path,
                        "storage_root": current.storage_root,
                        "message": "the category path is not under the storage root, so its new path cannot be derived",
                    }));
                    continue;
                };
                if new_path != category.path {
                    relative_moves.push((category.path.clone(), new_path.clone()));
                }
                if let Some(entry) = next.categories.get_mut(key) {
                    entry.path = new_path;
                }
            }
            next.storage_root = new_root.clone();
            description = format!(
                "move the storage root from {} to {new_root} ({} category director{})",
                current.storage_root,
                relative_moves.len(),
                if relative_moves.len() == 1 {
                    "y"
                } else {
                    "ies"
                }
            );
        }
        StorageAction::CategoryPath { key, path } => {
            let Some(category) = current.categories.get(key) else {
                return Err(WaymarkError::category_not_found(key.as_str()));
            };
            let Some(new_path) =
                checked_relative_path(&format!("categories.{key}.path"), path, &mut diagnostics)
            else {
                return Err(storage_error(conflicts, diagnostics));
            };
            if new_path != category.path {
                relative_moves.push((category.path.clone(), new_path.clone()));
            }
            if let Some(entry) = next.categories.get_mut(key) {
                entry.path = new_path.clone();
            }
            description = format!("move category {key} from {} to {new_path}", category.path);
        }
        StorageAction::CategoryKey { from, to } => {
            if from == to {
                return Err(WaymarkError::validation(
                    "action",
                    vec![ValidationIssue::new(
                        "action",
                        format!("category key `{from}` is already the requested key"),
                    )],
                ));
            }
            let Some(category) = current.categories.get(from) else {
                return Err(WaymarkError::category_not_found(from.as_str()));
            };
            if let Some(existing) = current.categories.get(to) {
                diagnostics.push(json!({
                    "kind": "category_exists",
                    "category": to.as_str(),
                    "path": existing.path,
                    "message": "the target category key is already declared",
                }));
                return Err(storage_error(conflicts, diagnostics));
            }
            let new_path = key_path(&category.path, to);
            if new_path != category.path {
                relative_moves.push((category.path.clone(), new_path.clone()));
            }
            let mut renamed = category.clone();
            renamed.path = new_path.clone();
            next.categories.remove(from);
            next.categories.insert(to.clone(), renamed);
            category_change = Some(CategoryChange {
                from: from.clone(),
                to: to.clone(),
            });
            description = format!(
                "rename category {from} to {to}, move {} to {new_path}, and rewrite the persisted category value",
                category.path
            );
        }
    }

    // Absolute moves plus the source digest the plan is built from.
    let mut moves: Vec<PlannedMove> = Vec::new();
    for (from, to) in &relative_moves {
        let from_path = crate::config::resolve_relative(root, from);
        let to_path = crate::config::resolve_relative(root, to);
        match directory_digest(&from_path) {
            Ok(expected) => moves.push(PlannedMove {
                from: from_path,
                to: to_path,
                expected,
            }),
            Err(error) => {
                let kind = if error.kind() == std::io::ErrorKind::NotFound {
                    "source_missing"
                } else {
                    "source_unreadable"
                };
                diagnostics.push(json!({
                    "kind": kind,
                    "path": from_path.to_string(),
                    "message": error.to_string(),
                }));
            }
        }
    }

    // Every category directory that stays where it is: a moving target must not
    // collide with one of them.
    let moved_sources: std::collections::BTreeSet<&str> = relative_moves
        .iter()
        .map(|(from, _)| from.as_str())
        .collect();
    let remaining: Vec<Utf8PathBuf> = current
        .categories
        .values()
        .filter(|category| !moved_sources.contains(category.path.as_str()))
        .map(|category| crate::config::resolve_relative(root, &category.path))
        .collect();
    check_move_paths(root, &moves, &remaining, &mut diagnostics, &mut conflicts);

    // A category-key migration rewrites the persisted value on every affected
    // document, archived ones included.
    let mut targets: Vec<PlannedTarget> = Vec::new();
    if let Some(change) = &category_change {
        for scanned in &state.items {
            if scanned.item.category != change.from {
                continue;
            }
            let mut rewritten = scanned.item.clone();
            rewritten.category = change.to.clone();
            targets.push(PlannedTarget {
                id: scanned.id.clone(),
                path: scanned.path.clone(),
                expected: scanned.fingerprint.clone(),
                bytes: crate::serialize::to_document(&rewritten).into_bytes(),
            });
        }
        targets.sort_by(|left, right| left.path.cmp(&right.path));
    }

    // The configuration change and the moves must agree; a content no-op keeps
    // the revision where it is.
    next.version = current.version;
    let config = if next.to_toml_string() == current.to_toml_string() {
        None
    } else {
        next.version = Version {
            generation: current.version.generation,
            revision: current.version.revision.saturating_add(1),
        };
        for issue in next.issues_at(root) {
            diagnostics.push(json!({
                "kind": "config_issue",
                "key": issue.key,
                "message": issue.reason,
            }));
        }
        if !diagnostics.is_empty() {
            return Err(storage_error(conflicts, diagnostics));
        }
        Some(PlannedConfig {
            path: project.config_path(),
            expected: state.config_fingerprint.clone(),
            bytes: next.to_toml_string().into_bytes(),
            previous_version: current.version,
            version: next.version,
        })
    };

    if !diagnostics.is_empty() || !conflicts.is_empty() {
        return Err(storage_error(conflicts, diagnostics));
    }

    Ok(MigrationPlan {
        kind: MigrationKind::Storage,
        description,
        targets,
        moves,
        category_change,
        config,
        warnings: state.warnings,
    })
}

/// Validate one requested repository-relative path.
fn checked_relative_path(field: &str, raw: &str, diagnostics: &mut Vec<Value>) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        diagnostics.push(json!({
            "kind": "invalid_path",
            "field": field,
            "message": "a path is required",
        }));
        return None;
    }
    if value.starts_with('/') || value.starts_with('\\') || value.contains(":\\") {
        diagnostics.push(json!({
            "kind": "absolute_path",
            "field": field,
            "path": value,
            "message": "a path must be repository-relative, not absolute",
        }));
        return None;
    }
    if value.contains('\\') {
        diagnostics.push(json!({
            "kind": "invalid_path",
            "field": field,
            "path": value,
            "message": "a path must use POSIX separators",
        }));
        return None;
    }
    if value.split('/').any(|component| component == "..") {
        diagnostics.push(json!({
            "kind": "escaping_path",
            "field": field,
            "path": value,
            "message": "a path must not escape the repository root",
        }));
        return None;
    }
    if value != "." && value.split('/').any(str::is_empty) {
        diagnostics.push(json!({
            "kind": "invalid_path",
            "field": field,
            "path": value,
            "message": "a path must not contain an empty component",
        }));
        return None;
    }
    Some(value.to_owned())
}

/// The new storage-root-relative path of an existing category path.
fn rebase_category(path: &str, old_root: &str, new_root: &str) -> Option<String> {
    let old_root = old_root.trim_end_matches('/');
    if old_root.is_empty() || old_root == "." {
        return Some(join_relative(new_root, path));
    }
    if path == old_root {
        return Some(new_root.to_owned());
    }
    let rest = path.strip_prefix(&format!("{old_root}/"))?;
    Some(join_relative(new_root, rest))
}

fn join_relative(root: &str, rest: &str) -> String {
    if rest.is_empty() {
        root.to_owned()
    } else if root == "." {
        rest.to_owned()
    } else {
        format!("{}/{rest}", root.trim_end_matches('/'))
    }
}

/// A category key change renames the directory's final component to the key.
///
/// The result is a configuration value, not a filesystem path, so it is built
/// from the string form: a `Utf8Path` join would use the platform separator and
/// the configuration validator accepts `/` only.
fn key_path(old_path: &str, new_key: &CategoryKey) -> String {
    match old_path.rsplit_once('/') {
        Some((parent, _)) if !parent.is_empty() => format!("{parent}/{}", new_key.as_str()),
        _ => new_key.to_string(),
    }
}

/// Read-only checks for every planned relocation.
fn check_move_paths(
    root: &Utf8Path,
    moves: &[PlannedMove],
    remaining: &[Utf8PathBuf],
    diagnostics: &mut Vec<Value>,
    conflicts: &mut Vec<(String, String)>,
) {
    for moved in moves {
        if !moved.to.starts_with(root) {
            diagnostics.push(json!({
                "kind": "escaping_path",
                "path": moved.to.to_string(),
                "message": "a target must stay inside the repository root",
            }));
        }
        check_symlink_components(root, &moved.to, diagnostics);
        check_target_parent(&moved.to, diagnostics);
        check_existing_target(moved, diagnostics, conflicts);
        check_cross_device(moved, diagnostics);
        check_case_components(root, &moved.to, conflicts);
    }

    for (index, moved) in moves.iter().enumerate() {
        if is_prefix(&moved.to, &moved.from) || is_prefix(&moved.from, &moved.to) {
            conflicts.push((
                moved.to.to_string(),
                format!(
                    "the source directory {} (a directory cannot move inside or over itself)",
                    moved.from
                ),
            ));
        }
        if moved.to.as_str().eq_ignore_ascii_case(moved.from.as_str()) {
            conflicts.push((
                moved.to.to_string(),
                format!("case-equivalently the source directory {}", moved.from),
            ));
        }
        for (other_index, other) in moves.iter().enumerate() {
            if index == other_index {
                continue;
            }
            if moved.to == other.to
                || is_prefix(&moved.to, &other.to)
                || is_prefix(&other.to, &moved.to)
            {
                conflicts.push((
                    moved.to.to_string(),
                    format!("another target directory in this run ({})", other.to),
                ));
            }
            if moved.to == other.from {
                conflicts.push((
                    moved.to.to_string(),
                    format!("the existing category path {}", other.from),
                ));
            } else if is_prefix(&moved.to, &other.from) || is_prefix(&other.from, &moved.to) {
                conflicts.push((
                    moved.to.to_string(),
                    format!(
                        "the existing category path {} (ancestry or descent)",
                        other.from
                    ),
                ));
            }
            if moved.to.as_str().eq_ignore_ascii_case(other.from.as_str()) {
                conflicts.push((
                    moved.to.to_string(),
                    format!(
                        "case-equivalently the existing category path {}",
                        other.from
                    ),
                ));
            }
            if moved.to.as_str().eq_ignore_ascii_case(other.to.as_str()) {
                conflicts.push((
                    moved.to.to_string(),
                    format!(
                        "case-equivalently another target directory in this run ({})",
                        other.to
                    ),
                ));
            }
        }
        for existing in remaining {
            if moved.to == *existing {
                conflicts.push((
                    moved.to.to_string(),
                    format!("the existing category path {existing}"),
                ));
            } else if is_prefix(&moved.to, existing) || is_prefix(existing, &moved.to) {
                conflicts.push((
                    moved.to.to_string(),
                    format!("the existing category path {existing} (ancestry or descent)"),
                ));
            }
            if moved.to.as_str().eq_ignore_ascii_case(existing.as_str()) {
                conflicts.push((
                    moved.to.to_string(),
                    format!("case-equivalently the existing category path {existing}"),
                ));
            }
        }
    }
}

/// Whether `inner` is strictly inside `outer`.
fn is_prefix(outer: &Utf8Path, inner: &Utf8Path) -> bool {
    inner.as_str().len() > outer.as_str().len() && inner.starts_with(outer)
}

/// Every symbolic link that joining `target` from `root` traverses.
fn check_symlink_components(root: &Utf8Path, target: &Utf8Path, diagnostics: &mut Vec<Value>) {
    let Ok(relative) = target.strip_prefix(root) else {
        return;
    };
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let name = component.as_str();
        if name.is_empty() {
            continue;
        }
        current.push(name);
        if let Ok(metadata) = std::fs::symlink_metadata(&current) {
            if metadata.file_type().is_symlink() {
                diagnostics.push(json!({
                    "kind": "symlink_in_path",
                    "path": target.to_string(),
                    "component": current.to_string(),
                    "message": "a migration path must not traverse a symbolic link",
                }));
                return;
            }
        }
    }
}

/// A target that already exists is allowed only as an empty directory.
fn check_existing_target(
    moved: &PlannedMove,
    diagnostics: &mut Vec<Value>,
    conflicts: &mut Vec<(String, String)>,
) {
    match std::fs::symlink_metadata(&moved.to) {
        Ok(metadata) if metadata.file_type().is_symlink() => {}
        Ok(metadata) if metadata.is_dir() => {
            let non_empty = std::fs::read_dir(&moved.to)
                .map(|mut entries| entries.next().is_some())
                .unwrap_or(true);
            if non_empty {
                conflicts.push((
                    moved.to.to_string(),
                    "an existing non-empty directory".to_owned(),
                ));
            }
        }
        Ok(_) => conflicts.push((moved.to.to_string(), "an existing file".to_owned())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => diagnostics.push(json!({
            "kind": "target_unreadable",
            "path": moved.to.to_string(),
            "message": error.to_string(),
        })),
    }
}

/// The nearest existing ancestor must be a directory the rename can land in.
fn check_target_parent(target: &Utf8Path, diagnostics: &mut Vec<Value>) {
    let Some(parent) = target.parent() else {
        return;
    };
    let Some(ancestor) = nearest_existing(parent) else {
        return;
    };
    if let Ok(metadata) = std::fs::metadata(&ancestor) {
        if !metadata.is_dir() {
            diagnostics.push(json!({
                "kind": "target_parent_not_a_directory",
                "path": target.to_string(),
                "ancestor": ancestor.to_string(),
                "message": "the target's nearest existing ancestor is not a directory",
            }));
        }
    }
}

/// Whether a failed rename says "the target is on another filesystem".
///
/// Every platform has its own errno for it, and only Unix defines `EXDEV`, so
/// the mapping is per-platform instead of reaching for a libc constant that
/// does not exist everywhere.
#[cfg(unix)]
fn is_cross_device(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(libc::EXDEV)
}

/// `ERROR_NOT_SAME_DEVICE` (17), Windows' cross-volume rename failure.
#[cfg(windows)]
fn is_cross_device(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(17)
}

/// A pure relocation is `rename(2)`; a cross-filesystem target cannot be one.
#[cfg(unix)]
fn check_cross_device(moved: &PlannedMove, diagnostics: &mut Vec<Value>) {
    use std::os::unix::fs::MetadataExt;

    let Ok(source) = std::fs::metadata(&moved.from) else {
        return;
    };
    let Some(parent) = moved.to.parent() else {
        return;
    };
    let Some(ancestor) = nearest_existing(parent) else {
        return;
    };
    let Ok(target) = std::fs::metadata(&ancestor) else {
        return;
    };
    if source.dev() != target.dev() {
        diagnostics.push(json!({
            "kind": "cross_device",
            "path": moved.to.to_string(),
            "source": moved.from.to_string(),
            "message": "a storage migration is a same-filesystem directory rename and never copies",
        }));
    }
}

#[cfg(not(unix))]
fn check_cross_device(_moved: &PlannedMove, _diagnostics: &mut Vec<Value>) {}

/// A target path component that differs only in case from an existing entry.
fn check_case_components(
    root: &Utf8Path,
    target: &Utf8Path,
    conflicts: &mut Vec<(String, String)>,
) {
    let Ok(relative) = target.strip_prefix(root) else {
        return;
    };
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let name = component.as_str();
        if name.is_empty() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&current) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(entry_name) = entry.file_name().into_string() else {
                continue;
            };
            if entry_name != name && entry_name.eq_ignore_ascii_case(name) {
                conflicts.push((
                    target.to_string(),
                    format!(
                        "case-equivalently the existing path {}",
                        current.join(&entry_name)
                    ),
                ));
                return;
            }
        }
        current.push(name);
    }
}

fn nearest_existing(path: &Utf8Path) -> Option<Utf8PathBuf> {
    for ancestor in path.ancestors() {
        if std::fs::symlink_metadata(ancestor).is_ok() {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

/// A digest of every entry under a directory, used to detect a hand edit
/// between planning and executing.
fn directory_digest(directory: &Utf8Path) -> std::io::Result<Fingerprint> {
    let mut bytes: Vec<u8> = Vec::new();
    append_directory_digest(directory.as_std_path(), directory.as_std_path(), &mut bytes)?;
    Ok(Fingerprint::of(&bytes))
}

fn append_directory_digest(
    base: &StdPath,
    directory: &StdPath,
    out: &mut Vec<u8>,
) -> std::io::Result<()> {
    let mut entries: Vec<std::fs::DirEntry> =
        std::fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(base).unwrap_or(&path);
        let metadata = std::fs::symlink_metadata(&path)?;
        out.extend_from_slice(relative.to_string_lossy().as_bytes());
        out.push(0);
        if metadata.file_type().is_symlink() {
            out.push(b'L');
            let linked = std::fs::read_link(&path)?;
            out.extend_from_slice(linked.to_string_lossy().as_bytes());
            out.push(0);
        } else if metadata.is_dir() {
            out.push(b'D');
            out.push(0);
            append_directory_digest(base, &path, out)?;
        } else if metadata.is_file() {
            out.push(b'F');
            out.extend_from_slice(Fingerprint::of(&std::fs::read(&path)?).as_str().as_bytes());
            out.push(0);
        } else {
            out.push(b'O');
            out.push(0);
        }
    }
    Ok(())
}

/// The path a rewritten document lands on after the moves.
fn destination_for(plan: &MigrationPlan, path: &Utf8Path) -> Utf8PathBuf {
    for moved in &plan.moves {
        if let Ok(rest) = path.strip_prefix(&moved.from) {
            return moved.to.join(rest);
        }
    }
    path.to_path_buf()
}

/// Repeat the storage plan's read-only checks against the current filesystem.
fn recheck_storage(
    project: &Project,
    plan: &MigrationPlan,
    current: &Config,
) -> Result<(), WaymarkError> {
    let root = project.root();
    let mut diagnostics: Vec<Value> = Vec::new();
    let mut conflicts: Vec<(String, String)> = Vec::new();

    let moved_sources: std::collections::BTreeSet<&str> =
        plan.moves.iter().map(|moved| moved.from.as_str()).collect();
    let remaining: Vec<Utf8PathBuf> = current
        .categories
        .values()
        .filter(|category| {
            !moved_sources.contains(crate::config::resolve_relative(root, &category.path).as_str())
        })
        .map(|category| crate::config::resolve_relative(root, &category.path))
        .collect();
    check_move_paths(
        root,
        &plan.moves,
        &remaining,
        &mut diagnostics,
        &mut conflicts,
    );

    for moved in &plan.moves {
        match directory_digest(&moved.from) {
            Ok(actual) => {
                if actual != moved.expected {
                    diagnostics.push(json!({
                        "kind": "fingerprint_disagreement",
                        "path": moved.from.to_string(),
                        "expected": moved.expected.as_str(),
                        "actual": actual.as_str(),
                    }));
                }
            }
            Err(error) => {
                let kind = if error.kind() == std::io::ErrorKind::NotFound {
                    "source_missing"
                } else {
                    "source_unreadable"
                };
                diagnostics.push(json!({
                    "kind": kind,
                    "path": moved.from.to_string(),
                    "message": error.to_string(),
                }));
            }
        }
        check_existing_target(moved, &mut diagnostics, &mut conflicts);
    }

    if diagnostics.is_empty() && conflicts.is_empty() {
        Ok(())
    } else {
        Err(storage_error(conflicts, diagnostics))
    }
}

/// The one error a failed storage prevalidation returns.
fn storage_error(conflicts: Vec<(String, String)>, diagnostics: Vec<Value>) -> WaymarkError {
    if let Some((target, _)) = conflicts.first() {
        let mut conflict = conflicts
            .iter()
            .map(|(path, reason)| format!("{path}: {reason}"))
            .collect::<Vec<_>>()
            .join("; ");
        if !diagnostics.is_empty() {
            let extra = diagnostics
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("; ");
            conflict.push_str("; additionally: ");
            conflict.push_str(&extra);
        }
        return WaymarkError::migration_path_conflict(target.clone(), conflict);
    }
    WaymarkError::migration_precondition_failed(diagnostics)
}

/// The directories an `apply_move` must create, outermost first.
fn collect_missing_ancestors(directory: Option<&Utf8Path>) -> Vec<std::path::PathBuf> {
    let Some(directory) = directory else {
        return Vec::new();
    };
    let mut missing: Vec<std::path::PathBuf> = Vec::new();
    for ancestor in directory.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(ancestor.as_std_path().to_path_buf());
            }
            Err(_) => break,
        }
    }
    missing.reverse();
    missing
}

/// Rename one directory, creating a missing target parent first.
fn apply_move(
    moved: &PlannedMove,
    created: &mut Vec<std::path::PathBuf>,
) -> Result<(), WaymarkError> {
    for directory in collect_missing_ancestors(moved.to.parent()) {
        std::fs::create_dir(&directory).map_err(|source| {
            WaymarkError::io(directory.display().to_string(), "create", &source)
        })?;
        created.push(directory);
    }
    std::fs::rename(moved.from.as_std_path(), moved.to.as_std_path()).map_err(|source| {
        if is_cross_device(&source) {
            WaymarkError::io(
                moved.to.to_string(),
                "rename onto another filesystem (a storage migration never copies)",
                &source,
            )
        } else {
            WaymarkError::io(moved.to.to_string(), "move", &source)
        }
    })
}

/// Restore every applied rename. Returns the targets that were not restored.
fn restore_moves(plan: &MigrationPlan, applied: &[usize]) -> Vec<String> {
    let mut failures: Vec<String> = Vec::new();
    for index in applied.iter().rev() {
        let moved = &plan.moves[*index];
        if std::fs::rename(moved.to.as_std_path(), moved.from.as_std_path()).is_err() {
            failures.push(moved.to.to_string());
        }
    }
    failures
}

/// Remove the directories `apply_move` created, deepest first.
fn remove_created_directories(created: &[std::path::PathBuf]) -> Vec<String> {
    let mut directories: Vec<&std::path::PathBuf> = created.iter().collect();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    let mut failures: Vec<String> = Vec::new();
    for directory in directories {
        match std::fs::remove_dir(directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => failures.push(directory.display().to_string()),
        }
    }
    failures
}

/// Apply the moves, then the rewritten documents and the configuration as one
/// batch. A failure after a move restores the moves.
fn execute_storage(
    plan: &MigrationPlan,
    previous_revision: u64,
    revision: u64,
    meta: Meta,
) -> Result<MigrationReport, WaymarkError> {
    let mut created: Vec<std::path::PathBuf> = Vec::new();
    let mut applied: Vec<usize> = Vec::new();
    for (index, moved) in plan.moves.iter().enumerate() {
        if let Err(error) = apply_move(moved, &mut created) {
            let mut not_restored = restore_moves(plan, &applied);
            not_restored.extend(remove_created_directories(&created));
            if !not_restored.is_empty() {
                return Err(storage_rollback_failed(
                    plan,
                    &applied,
                    Some(index),
                    not_restored,
                    None,
                ));
            }
            return Err(storage_move_failed(plan, index, &error));
        }
        applied.push(index);
    }

    let staged: Vec<StagedTarget> = plan
        .targets
        .iter()
        .map(|target| StagedTarget {
            id: target.id.clone(),
            path: destination_for(plan, &target.path),
            expected: target.expected.clone(),
            bytes: target.bytes.clone(),
        })
        .collect();
    let batch: Vec<crate::write::BatchTarget<'_>> = staged
        .iter()
        .map(|target| crate::write::BatchTarget {
            id: &target.id,
            path: &target.path,
            expected: &target.expected,
            bytes: &target.bytes,
        })
        .collect();
    let config_target = plan.config.as_ref().map(|config| crate::write::FileTarget {
        path: &config.path,
        expected: &config.expected,
        bytes: &config.bytes,
    });

    match crate::write::replace_batch_with_file(&batch, config_target) {
        Ok(_) => Ok(report(
            plan,
            false,
            true,
            (previous_revision, revision),
            meta,
            storage_applied_outcomes(plan),
            move_outcomes(plan, "applied"),
        )),
        Err(error) => {
            let mut not_restored = restore_moves(plan, &applied);
            not_restored.extend(remove_created_directories(&created));
            if error.code() == Code::RollbackFailed || !not_restored.is_empty() {
                return Err(storage_rollback_failed(
                    plan,
                    &applied,
                    None,
                    not_restored,
                    Some(&error),
                ));
            }
            Err(storage_batch_failed(plan, &error))
        }
    }
}

/// The per-target outcomes of a successful storage run.
fn storage_applied_outcomes(plan: &MigrationPlan) -> Vec<TargetOutcome> {
    let mut outcomes: Vec<TargetOutcome> = plan
        .moves
        .iter()
        .map(|moved| TargetOutcome::new(moved.to.to_string(), "applied"))
        .collect();
    for target in &plan.targets {
        outcomes.push(TargetOutcome::new(
            destination_for(plan, &target.path).to_string(),
            "applied",
        ));
    }
    if let Some(config) = &plan.config {
        outcomes.push(TargetOutcome::new(config.path.to_string(), "applied"));
    }
    outcomes
}

/// The failure of one directory rename, after the applied renames were
/// restored.
fn storage_move_failed(
    plan: &MigrationPlan,
    failed_index: usize,
    error: &WaymarkError,
) -> WaymarkError {
    let mut outcomes: Vec<TargetOutcome> = plan
        .moves
        .iter()
        .enumerate()
        .map(|(index, moved)| {
            if index == failed_index {
                TargetOutcome::new(moved.to.to_string(), "failed").with_error(error.message())
            } else {
                TargetOutcome::new(moved.to.to_string(), "skipped")
            }
        })
        .collect();
    for target in &plan.targets {
        outcomes.push(TargetOutcome::new(
            destination_for(plan, &target.path).to_string(),
            "skipped",
        ));
    }
    if let Some(config) = &plan.config {
        outcomes.push(TargetOutcome::new(config.path.to_string(), "skipped"));
    }
    WaymarkError::migration_failed(outcomes)
}

/// The failure of the document/configuration batch, after the moves were
/// restored.
fn storage_batch_failed(plan: &MigrationPlan, error: &WaymarkError) -> WaymarkError {
    let failed_path = destination_failed_path(plan, error);
    let mut outcomes: Vec<TargetOutcome> = plan
        .moves
        .iter()
        .map(|moved| TargetOutcome::new(moved.to.to_string(), "skipped"))
        .collect();
    let mut matched = false;
    for target in &plan.targets {
        let path = destination_for(plan, &target.path);
        let failed = failed_path.as_deref() == Some(path.as_str());
        matched |= failed;
        outcomes.push(target_outcome(&path, failed, error));
    }
    if let Some(config) = &plan.config {
        let failed = failed_path.as_deref() == Some(config.path.as_str());
        matched |= failed;
        outcomes.push(target_outcome(&config.path, failed, error));
    }
    if !matched {
        for outcome in &mut outcomes {
            outcome.outcome = "failed".to_owned();
            outcome.error = Some(error.message().to_owned());
        }
    }
    WaymarkError::migration_failed(outcomes)
}

/// The disk path a batch failure names, translated to the post-move location.
fn destination_failed_path(plan: &MigrationPlan, error: &WaymarkError) -> Option<String> {
    if let Some(Location::WorkItem { id }) = error.location() {
        if let Some(target) = plan.targets.iter().find(|target| &target.id == id) {
            return Some(destination_for(plan, &target.path).to_string());
        }
    }
    if let Some(Location::File { path, .. }) = error.location() {
        return Some(path.clone());
    }
    error
        .details()
        .get("path")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// A rollback that could not complete is never downgraded. `ROLLBACK_FAILED`
/// names the applied, failed, and not-restored paths, merging the batch
/// engine's own report when one exists.
fn storage_rollback_failed(
    plan: &MigrationPlan,
    applied: &[usize],
    failed_index: Option<usize>,
    not_restored: Vec<String>,
    engine: Option<&WaymarkError>,
) -> WaymarkError {
    let mut applied_paths: Vec<String> = applied
        .iter()
        .map(|index| plan.moves[*index].to.to_string())
        .collect();
    let mut failed_paths: Vec<String> = failed_index
        .map(|index| vec![plan.moves[index].to.to_string()])
        .unwrap_or_default();
    let mut rollback_failed = not_restored;
    if let Some(error) = engine {
        if error.code() == Code::RollbackFailed {
            extend_paths(&mut applied_paths, error, "applied");
            extend_paths(&mut failed_paths, error, "failed");
            extend_paths(&mut rollback_failed, error, "rollback_failed");
        }
    }
    WaymarkError::rollback_failed(applied_paths, failed_paths, rollback_failed)
}

fn extend_paths(target: &mut Vec<String>, error: &WaymarkError, key: &str) {
    if let Some(values) = error.details().get(key).and_then(Value::as_array) {
        for value in values {
            if let Some(text) = value.as_str() {
                if !target.iter().any(|known| known == text) {
                    target.push(text.to_owned());
                }
            }
        }
    }
}

/// One owned document staged for the batch.
struct StagedTarget {
    id: crate::id::WorkItemId,
    path: Utf8PathBuf,
    expected: Fingerprint,
    bytes: Vec<u8>,
}

/// One planned rewrite of `item`.
fn rewrite(scanned: &ScannedItem, item: &WorkItem) -> PlannedTarget {
    PlannedTarget {
        id: scanned.id.clone(),
        path: scanned.path.clone(),
        expected: scanned.fingerprint.clone(),
        bytes: crate::serialize::to_document(item).into_bytes(),
    }
}

/// The value an item declares for `key`, or `None`.
fn current_value<'a>(item: &'a WorkItem, key: &FieldKey) -> Option<&'a FieldValue> {
    item.custom_fields.get(key.as_str())
}

/// Re-validate a configuration before planning its write.
fn validate_next_config(project: &Project, config: &Config) -> Result<(), WaymarkError> {
    let issues = config.issues_at(project.root());
    if issues.is_empty() {
        return Ok(());
    }
    Err(WaymarkError::config_invalid(
        project.config_path().to_string(),
        issues.iter().map(|issue| issue.render()).collect(),
    )
    .with_location(Location::config_key(CONFIG_FILE)))
}

/// The drift diagnostics for one planned target, if its bytes moved on.
fn fingerprint_drift(
    project: &Project,
    path: &camino::Utf8Path,
    expected: &Fingerprint,
) -> Result<Vec<Value>, WaymarkError> {
    let actual = project.fingerprint(path).map_err(|error| {
        WaymarkError::migration_precondition_failed(vec![json!({
            "kind": "unreadable_fingerprint",
            "path": path.to_string(),
            "code": error.code().as_str(),
            "message": error.message(),
        })])
    })?;
    if &actual == expected {
        Ok(Vec::new())
    } else {
        Ok(vec![json!({
            "kind": "fingerprint_disagreement",
            "path": path.to_string(),
            "expected": expected.as_str(),
            "actual": actual.as_str(),
        })])
    }
}

fn schema_invalid(key: impl Into<String>, reason: impl Into<String>) -> WaymarkError {
    let key = key.into();
    WaymarkError::validation(
        format!("migrate schema {key}"),
        vec![ValidationIssue::new(key.clone(), reason)],
    )
    .with_location(Location::config_key(key))
}

/// The `_meta` outcomes for a dry run: every target skipped.
fn skipped_outcomes(plan: &MigrationPlan, outcome: &str) -> Vec<TargetOutcome> {
    let mut outcomes: Vec<TargetOutcome> = plan
        .targets
        .iter()
        .map(|target| TargetOutcome::new(target.path.to_string(), outcome))
        .collect();
    if let Some(config) = &plan.config {
        outcomes.push(TargetOutcome::new(config.path.to_string(), outcome));
    }
    outcomes
}

fn applied_outcomes(plan: &MigrationPlan) -> Vec<TargetOutcome> {
    skipped_outcomes(plan, "applied")
}

/// Every directory the report says was (or would be) moved.
fn move_outcomes(plan: &MigrationPlan, outcome: &str) -> Vec<MoveOutcome> {
    plan.moves
        .iter()
        .map(|moved| MoveOutcome {
            from: moved.from.to_string(),
            to: moved.to.to_string(),
            outcome: outcome.to_owned(),
            error: None,
        })
        .collect()
}

/// The persisted values the plan rewrites, in target order.
fn rewritten_categories(plan: &MigrationPlan) -> Vec<RewrittenCategory> {
    let Some(change) = &plan.category_change else {
        return Vec::new();
    };
    plan.targets
        .iter()
        .map(|target| RewrittenCategory {
            id: target.id.clone(),
            path: target.path.to_string(),
            from: change.from.clone(),
            to: change.to.clone(),
        })
        .collect()
}

fn report(
    plan: &MigrationPlan,
    dry_run: bool,
    written: bool,
    revisions: (u64, u64),
    meta: Meta,
    targets: Vec<TargetOutcome>,
    moves: Vec<MoveOutcome>,
) -> MigrationReport {
    let (previous_revision, revision) = revisions;
    MigrationReport {
        kind: plan.kind,
        description: plan.description.clone(),
        dry_run,
        affected: plan.affected(),
        targets,
        moves,
        rewritten: rewritten_categories(plan),
        count: plan.count(),
        written,
        config_changed: plan.config_changed(),
        previous_revision,
        revision,
        meta,
    }
}

/// Translate a batch failure into `MIGRATION_FAILED` with per-target outcomes.
///
/// A rollback that could not complete is already its own code and is passed
/// through untouched: it names the applied, failed, and not-restored paths.
fn execution_error(plan: &MigrationPlan, error: &WaymarkError) -> WaymarkError {
    if error.code() == Code::RollbackFailed {
        return error.clone();
    }
    let failed_path = failed_path(plan, error);
    let mut outcomes: Vec<TargetOutcome> = Vec::new();
    let mut matched = false;
    for target in &plan.targets {
        let failed = failed_path.as_deref() == Some(target.path.as_str());
        matched |= failed;
        outcomes.push(target_outcome(&target.path, failed, error));
    }
    if let Some(config) = &plan.config {
        let failed = failed_path.as_deref() == Some(config.path.as_str());
        matched |= failed;
        outcomes.push(target_outcome(&config.path, failed, error));
    }
    if !matched {
        // The engine could not name a path (for example a staging failure): the
        // honest report is that every target failed to apply.
        outcomes = outcomes
            .into_iter()
            .map(|outcome| outcome.with_error(error.message()))
            .collect();
        for outcome in &mut outcomes {
            outcome.outcome = "failed".to_owned();
        }
    }
    WaymarkError::migration_failed(outcomes)
}

fn target_outcome(path: &camino::Utf8Path, failed: bool, error: &WaymarkError) -> TargetOutcome {
    if failed {
        TargetOutcome::new(path.to_string(), "failed").with_error(error.message())
    } else {
        TargetOutcome::new(path.to_string(), "skipped")
    }
}

/// The path a batch failure names, if it names one.
fn failed_path(plan: &MigrationPlan, error: &WaymarkError) -> Option<String> {
    if let Some(Location::WorkItem { id }) = error.location() {
        if let Some(target) = plan.targets.iter().find(|target| &target.id == id) {
            return Some(target.path.to_string());
        }
    }
    if let Some(Location::File { path, .. }) = error.location() {
        return Some(path.clone());
    }
    error
        .details()
        .get("path")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_plan() -> MigrationPlan {
        MigrationPlan {
            kind: MigrationKind::Version,
            description: String::new(),
            targets: Vec::new(),
            moves: Vec::new(),
            category_change: None,
            config: None,
            warnings: Vec::new(),
        }
    }

    #[test]
    fn an_unrestorable_rollback_is_never_downgraded_to_migration_failed() {
        // The batch engine's `ROLLBACK_FAILED` is the one failure a migration
        // must not relabel: it names the applied, failed, and not-restored
        // paths, which is the recovery action.
        let error = WaymarkError::rollback_failed(
            vec!["/p/a.md".to_owned()],
            vec!["/p/b.md".to_owned()],
            vec!["/p/a.md".to_owned()],
        );
        let mapped = execution_error(&empty_plan(), &error);
        assert_eq!(mapped.code(), Code::RollbackFailed);
        assert_eq!(mapped.details(), error.details());
    }

    #[test]
    fn an_unrestorable_directory_restore_is_reported_as_rollback_failed() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 path");
        let from = root.join("from");
        let to = root.join("to");
        std::fs::create_dir(&from).expect("the source directory");
        std::fs::rename(&from, &to).expect("the move");
        // A non-empty directory at the original location makes the restore fail.
        std::fs::create_dir(&from).expect("the blocking directory");
        std::fs::write(from.join("blocker"), b"x").expect("the blocking file");

        let plan = MigrationPlan {
            kind: MigrationKind::Storage,
            description: String::new(),
            targets: Vec::new(),
            moves: vec![PlannedMove {
                from: from.clone(),
                to: to.clone(),
                expected: Fingerprint::of(b""),
            }],
            category_change: None,
            config: None,
            warnings: Vec::new(),
        };

        let not_restored = restore_moves(&plan, &[0]);
        assert_eq!(not_restored, vec![to.to_string()]);
        let error = storage_rollback_failed(&plan, &[0], None, not_restored, None);
        assert_eq!(error.code(), Code::RollbackFailed);
        assert_eq!(
            error.details()["rollback_failed"]
                .as_array()
                .expect("the not-restored targets")
                .len(),
            1
        );
    }

    #[test]
    fn an_unidentified_execution_failure_marks_every_target_failed() {
        let error = WaymarkError::io(
            "/p",
            "create a temporary file in",
            &std::io::Error::other("read-only"),
        );
        let plan = MigrationPlan {
            kind: MigrationKind::Version,
            description: String::new(),
            targets: vec![PlannedTarget {
                id: crate::id::WorkItemId::parse("wm-000000000001").expect("a valid ID"),
                path: camino::Utf8PathBuf::from("/p/a.md"),
                expected: Fingerprint::of(b"a"),
                bytes: b"b".to_vec(),
            }],
            moves: Vec::new(),
            category_change: None,
            config: None,
            warnings: Vec::new(),
        };
        let mapped = execution_error(&plan, &error);
        assert_eq!(mapped.code(), Code::MigrationFailed);
        let per_target = mapped
            .details()
            .get("per_target")
            .and_then(serde_json::Value::as_array)
            .expect("per-target outcomes");
        assert_eq!(per_target.len(), 1);
        assert_eq!(per_target[0]["outcome"], serde_json::json!("failed"));
        assert!(per_target[0]["error"].as_str().is_some());
    }
}
