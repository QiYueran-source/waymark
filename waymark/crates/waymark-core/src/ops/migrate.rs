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

use camino::Utf8PathBuf;
use serde_json::{json, Value};

use crate::config::{Config, FieldDecl, FieldType, Version, CONFIG_FILE};
use crate::envelope::{Meta, Warning};
use crate::error::{Code, TargetOutcome, ValidationIssue, WaymarkError};
use crate::location::Location;
use crate::model::{FieldKey, FieldValue, WorkItem, FORMAT_GENERATION};
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
    /// Move storage directories (phase B; only the shared scaffold lives here).
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
        self.targets.is_empty() && self.config.is_none()
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
            let existing_default = current
                .fields
                .get(key)
                .and_then(|decl| decl.default.clone());
            if let Some(existing) = current.fields.get(key) {
                if existing.ty != *ty {
                    return Err(WaymarkError::action_requires_migration(
                        format!("changing the type of `{key}`"),
                        "waymark migrate schema",
                    ));
                }
                let mut declaration: FieldDecl = existing.clone();
                declaration.required = true;
                if default.is_some() {
                    declaration.default = default.clone();
                }
                next.fields.insert(key.clone(), declaration);
            } else {
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
            }
            description = format!("add required custom field `{key}` and its default");
            let fill = default.clone().or(existing_default);
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

    let (previous_revision, revision) = plan.revisions(state.config.version.revision);
    let meta = Meta::memory_projection_complete().with_warnings(state.warnings.clone());

    if options.dry_run {
        return Ok(report(
            plan,
            true,
            false,
            previous_revision,
            revision,
            meta,
            skipped_outcomes(plan, "skipped"),
        ));
    }
    if plan.is_empty() {
        return Ok(report(
            plan,
            false,
            false,
            previous_revision,
            revision,
            meta,
            Vec::new(),
        ));
    }

    // 4. One batch: every document, then the configuration when it changed.
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
            previous_revision,
            revision,
            meta,
            applied_outcomes(plan),
        )),
        Err(error) => Err(execution_error(plan, &error)),
    }
}

/// Plan and execute a version migration under the project lock.
///
/// This is the command surface: the plan is rebuilt inside the lock right
/// before execution, so no preview can be applied to newer bytes.
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
    crate::lock::with_project_lock(project, options.lock_wait, || {
        let plan = plan_version(project, target_generation)?;
        execute(project, &plan, options)
    })
}

/// Plan and execute a schema migration under the project lock.
pub fn migrate_schema(
    project: &Project,
    action: SchemaAction,
    options: &OpOptions,
) -> Result<MigrationReport, WaymarkError> {
    crate::lock::with_project_lock(project, options.lock_wait, || {
        let plan = plan_schema(project, &action)?;
        execute(project, &plan, options)
    })
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

fn report(
    plan: &MigrationPlan,
    dry_run: bool,
    written: bool,
    previous_revision: u64,
    revision: u64,
    meta: Meta,
    targets: Vec<TargetOutcome>,
) -> MigrationReport {
    MigrationReport {
        kind: plan.kind,
        description: plan.description.clone(),
        dry_run,
        affected: plan.affected(),
        targets,
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
