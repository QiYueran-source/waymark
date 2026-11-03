//! The frozen error registry: one stable code per recovery unit, with its exit
//! class, retryability, typed details shape, allowed locations, and diagnostic
//! severity.
//!
//! # Shape of the registry
//!
//! A code names a *recovery action*, not an error variant: `VALIDATION_ERROR`
//! covers every field-level complaint and carries the per-field issue list in
//! its details, and `CYCLE_DETECTED` covers both dependency and parent cycles
//! with the kind in its details. Adding a code is warranted only when the
//! recovery action differs, so the table stays small and every addition is a
//! visible contract change.
//!
//! The table is pinned in the specification's registry table and in
//! `tests/golden/error_registry.json`. Every accessor reads the same [`Spec`]
//! through [`Code::spec`], which is an exhaustive `match` with no wildcard: a
//! new variant cannot compile until its exit class, retryability, finding
//! severity, details keys, and location kinds are all declared, and the golden
//! snapshot cannot change without the file changing in the same commit.
//!
//! # Details
//!
//! Details are typed per code: each constructor on [`WaymarkError`] fills exactly
//! the keys the registry declares for that code, so an adapter never has to
//! guess a shape. Keys are `snake_case` and are part of the machine contract.
//!
//! # Usage errors are adapter-level
//!
//! Bad flags and unknown subcommands are `USAGE_ERROR` at exit 1. That code is
//! deliberately **not** in [`Code`]: it never comes out of a domain operation,
//! and an adapter that invented a `Code` for it would be claiming a recovery
//! action the library does not have. The CLI owns it as a constant.

use std::str::FromStr;

use camino::Utf8Path;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::id::{IdError, WorkItemId};
use crate::location::{Location, LocationKind};

/// A registered error code.
///
/// The literal is the `SCREAMING_SNAKE` string on the wire; the enum variant
/// only exists to make the registry exhaustiveness compile-checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Code {
    /// No work item exists for the requested or referenced stable ID.
    ItemNotFound,
    /// No declared category has the given key.
    CategoryNotFound,
    /// One or more field-level validation failures.
    ValidationError,
    /// An ID literal does not satisfy the stable-ID format.
    InvalidId,
    /// A document or its frontmatter cannot be parsed.
    ParseError,
    /// Configuration fails schema validation.
    ConfigInvalid,
    /// Two or more files claim the same normalized ID.
    DuplicateId,
    /// The persisted ID disagrees with its filename stem.
    IdPathMismatch,
    /// A dependency or parent cycle was found.
    CycleDetected,
    /// A self-reference, duplicate edge, or wrong relation kind.
    InvalidRelationship,
    /// A complete, fresh projection is unavailable.
    IncompleteProjection,
    /// The target changed since the operation read it.
    WriteConflict,
    /// An archived item rejects a write.
    ItemArchived,
    /// Archive was requested for an already archived item.
    AlreadyArchived,
    /// Archive is blocked by unarchived children.
    HasActiveChildren,
    /// No Waymark project was found while searching.
    ProjectNotFound,
    /// A filesystem operation failed.
    IoError,
    /// A Git operation failed.
    GitError,
    /// The lock wait expired; nothing was written.
    LockTimeout,
    /// The lock file cannot be opened, locked, or is not a regular file.
    LockFailed,
    /// A reader could not obtain a stable snapshot after bounded retries.
    SnapshotUnstable,
    /// Data declares a generation the installed release cannot read.
    UnsupportedVersion,
    /// An item's generation is older than the project generation.
    ItemNeedsMigration,
    /// The operation requires an explicit migration first.
    ActionRequiresMigration,
    /// A migration target exists, overlaps, or is case-equivalent to an
    /// existing path.
    MigrationPathConflict,
    /// The installed release cannot write the requested generation.
    MigrationUnsupportedGeneration,
    /// Incomplete projection or fingerprint disagreement before migration.
    MigrationPreconditionFailed,
    /// Migration execution failed; per-target outcomes are reported.
    MigrationFailed,
    /// A multi-file operation partially applied and rollback did not complete.
    RollbackFailed,
    /// A unique stable ID could not be allocated after bounded attempts.
    IdGenerationFailed,
    /// An unexpected program fault.
    InternalError,
    /// The declared operator differs from the item owner.
    OwnerMismatch,
    /// A mediated write currently holds the project lock.
    LockHeld,
    /// One or more items need migration.
    MigrationAvailable,
    /// No Git repository is available for history or recovery.
    NotAGitRepository,
    /// A structured relationship references a missing target.
    DanglingReference,
    /// A persisted system timestamp is internally inconsistent.
    TimestampIncoherent,
}

/// The process exit class of a failure.
///
/// Class 1 (usage) is deliberately absent: `USAGE_ERROR` is adapter-level and
/// never reaches the domain. Classes 2 through 7 are the registry's whole range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitClass {
    /// No such work item or category.
    NotFound,
    /// Invalid input or data.
    InvalidInput,
    /// Conflict or state rejection.
    Conflict,
    /// Filesystem, Git, or environment failure.
    Environment,
    /// Version or migration failure.
    Version,
    /// Internal fault.
    Internal,
}

impl ExitClass {
    /// Every class the registry can assign.
    pub const ALL: &'static [ExitClass] = &[
        ExitClass::NotFound,
        ExitClass::InvalidInput,
        ExitClass::Conflict,
        ExitClass::Environment,
        ExitClass::Version,
        ExitClass::Internal,
    ];

    /// The numeric process exit code, 2 through 7.
    pub fn as_i32(self) -> i32 {
        match self {
            Self::NotFound => 2,
            Self::InvalidInput => 3,
            Self::Conflict => 4,
            Self::Environment => 5,
            Self::Version => 6,
            Self::Internal => 7,
        }
    }
}

/// How a diagnostic finding is classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// The data is wrong and a project-wide operation must fail.
    Error,
    /// The data is usable but suspicious.
    Warning,
    /// Context only.
    Info,
}

impl Severity {
    /// The lower-case literal used on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

/// The registry row behind one [`Code`].
#[derive(Debug, Clone, Copy)]
struct Spec {
    /// The exit class; `None` for the finding-only codes.
    exit: Option<ExitClass>,
    /// Whether the same request may succeed on retry.
    retryable: bool,
    /// The severity when the code appears as a diagnostic finding, if it may.
    finding: Option<Severity>,
    /// The details keys the code may carry, in contract order.
    details: &'static [&'static str],
    /// The subset of `details` a constructor may omit, because the specification
    /// writes the key with a `?`.
    optional_details: &'static [&'static str],
    /// The location kinds the code may carry, in contract order.
    locations: &'static [LocationKind],
}

use LocationKind::{ConfigKey, Field, File, Relationship, WorkItem};

const fn spec(
    exit: Option<ExitClass>,
    retryable: bool,
    finding: Option<Severity>,
    details: &'static [&'static str],
    locations: &'static [LocationKind],
) -> Spec {
    Spec {
        exit,
        retryable,
        finding,
        details,
        optional_details: &[],
        locations,
    }
}

/// A row whose details include at least one key the specification writes with a
/// `?`, so a constructor may omit it.
const fn spec_optional(
    exit: Option<ExitClass>,
    retryable: bool,
    finding: Option<Severity>,
    details: &'static [&'static str],
    optional_details: &'static [&'static str],
    locations: &'static [LocationKind],
) -> Spec {
    Spec {
        exit,
        retryable,
        finding,
        details,
        optional_details,
        locations,
    }
}

impl Code {
    /// Every code in the registry, in table order.
    pub const ALL: &'static [Code] = &[
        Code::ItemNotFound,
        Code::CategoryNotFound,
        Code::ValidationError,
        Code::InvalidId,
        Code::ParseError,
        Code::ConfigInvalid,
        Code::DuplicateId,
        Code::IdPathMismatch,
        Code::CycleDetected,
        Code::InvalidRelationship,
        Code::IncompleteProjection,
        Code::WriteConflict,
        Code::ItemArchived,
        Code::AlreadyArchived,
        Code::HasActiveChildren,
        Code::ProjectNotFound,
        Code::IoError,
        Code::GitError,
        Code::LockTimeout,
        Code::LockFailed,
        Code::SnapshotUnstable,
        Code::UnsupportedVersion,
        Code::ItemNeedsMigration,
        Code::ActionRequiresMigration,
        Code::MigrationPathConflict,
        Code::MigrationUnsupportedGeneration,
        Code::MigrationPreconditionFailed,
        Code::MigrationFailed,
        Code::RollbackFailed,
        Code::IdGenerationFailed,
        Code::InternalError,
        Code::OwnerMismatch,
        Code::LockHeld,
        Code::MigrationAvailable,
        Code::NotAGitRepository,
        Code::DanglingReference,
        Code::TimestampIncoherent,
    ];

    /// The registry row for this code.
    ///
    /// Exhaustive `match`, no wildcard: adding a variant is a compile error until
    /// its five attributes are declared, which is the registry's first pin.
    fn spec(self) -> Spec {
        match self {
            Self::ItemNotFound => spec_optional(
                Some(ExitClass::NotFound),
                false,
                None,
                &["id", "relation"],
                &["relation"],
                &[WorkItem, Relationship],
            ),
            Self::CategoryNotFound => spec(
                Some(ExitClass::NotFound),
                false,
                None,
                &["key"],
                &[Field, ConfigKey],
            ),
            Self::ValidationError => spec(
                Some(ExitClass::InvalidInput),
                false,
                Some(Severity::Error),
                &["target", "issues"],
                &[WorkItem, File, Field, ConfigKey, Relationship],
            ),
            Self::InvalidId => spec(
                Some(ExitClass::InvalidInput),
                false,
                None,
                &["value", "reason"],
                &[Field, File],
            ),
            Self::ParseError => spec(
                Some(ExitClass::InvalidInput),
                false,
                Some(Severity::Error),
                &["path", "reason"],
                &[File],
            ),
            Self::ConfigInvalid => spec(
                Some(ExitClass::InvalidInput),
                false,
                Some(Severity::Error),
                &["path", "issues"],
                &[ConfigKey, File],
            ),
            Self::DuplicateId => spec(
                Some(ExitClass::InvalidInput),
                false,
                Some(Severity::Error),
                &["id", "paths"],
                &[File, WorkItem],
            ),
            Self::IdPathMismatch => spec(
                Some(ExitClass::InvalidInput),
                false,
                Some(Severity::Error),
                &["id", "path", "stem"],
                &[File, WorkItem],
            ),
            Self::CycleDetected => spec(
                Some(ExitClass::InvalidInput),
                false,
                Some(Severity::Error),
                &["kind", "cycle"],
                &[WorkItem, Relationship],
            ),
            Self::InvalidRelationship => spec(
                Some(ExitClass::InvalidInput),
                false,
                None,
                &["relation", "from", "to", "reason"],
                &[WorkItem, Relationship],
            ),
            Self::IncompleteProjection => spec(
                Some(ExitClass::InvalidInput),
                false,
                Some(Severity::Error),
                &["completeness", "freshness", "diagnostics"],
                &[File, WorkItem],
            ),
            Self::WriteConflict => spec(
                Some(ExitClass::Conflict),
                false,
                None,
                &["id", "path", "expected_fingerprint", "actual_fingerprint"],
                &[WorkItem, File],
            ),
            Self::ItemArchived => spec(
                Some(ExitClass::Conflict),
                false,
                None,
                &["id", "operation"],
                &[WorkItem],
            ),
            Self::AlreadyArchived => {
                spec(Some(ExitClass::Conflict), false, None, &["id"], &[WorkItem])
            }
            Self::HasActiveChildren => spec(
                Some(ExitClass::Conflict),
                false,
                None,
                &["id", "children"],
                &[WorkItem, Relationship],
            ),
            Self::ProjectNotFound => spec(
                Some(ExitClass::Environment),
                false,
                None,
                &["searched"],
                &[],
            ),
            Self::IoError => spec(
                Some(ExitClass::Environment),
                false,
                Some(Severity::Error),
                &["path", "operation"],
                &[File],
            ),
            Self::GitError => spec(
                Some(ExitClass::Environment),
                false,
                None,
                &["operation", "reason"],
                &[],
            ),
            Self::LockTimeout => spec(
                Some(ExitClass::Environment),
                true,
                None,
                &["lock_path", "waited_ms"],
                &[],
            ),
            Self::LockFailed => spec(
                Some(ExitClass::Environment),
                false,
                None,
                &["lock_path", "reason"],
                &[],
            ),
            Self::SnapshotUnstable => {
                spec(Some(ExitClass::Environment), true, None, &["retries"], &[])
            }
            Self::UnsupportedVersion => spec(
                Some(ExitClass::Version),
                false,
                Some(Severity::Error),
                &["found", "supported"],
                &[File, WorkItem, ConfigKey],
            ),
            Self::ItemNeedsMigration => spec(
                Some(ExitClass::Version),
                false,
                Some(Severity::Error),
                &["id", "item_generation", "project_generation"],
                &[WorkItem, File],
            ),
            Self::ActionRequiresMigration => spec(
                Some(ExitClass::Version),
                false,
                None,
                &["action", "required_command"],
                &[WorkItem, ConfigKey],
            ),
            Self::MigrationPathConflict => spec(
                Some(ExitClass::Version),
                false,
                None,
                &["target", "conflict"],
                &[File],
            ),
            Self::MigrationUnsupportedGeneration => spec(
                Some(ExitClass::Version),
                false,
                None,
                &["requested", "supported"],
                &[],
            ),
            Self::MigrationPreconditionFailed => spec(
                Some(ExitClass::Version),
                false,
                None,
                &["diagnostics"],
                &[File, WorkItem, ConfigKey],
            ),
            Self::MigrationFailed => spec(
                Some(ExitClass::Version),
                false,
                None,
                &["per_target"],
                &[File, WorkItem],
            ),
            Self::RollbackFailed => spec(
                Some(ExitClass::Internal),
                false,
                None,
                &["applied", "failed", "rollback_failed"],
                &[File],
            ),
            Self::IdGenerationFailed => {
                spec(Some(ExitClass::Internal), false, None, &["attempts"], &[])
            }
            Self::InternalError => spec(Some(ExitClass::Internal), false, None, &[], &[]),
            Self::OwnerMismatch => spec(
                None,
                false,
                Some(Severity::Warning),
                &["item", "operator", "owner"],
                &[WorkItem],
            ),
            Self::LockHeld => spec(None, false, Some(Severity::Info), &["lock_path"], &[]),
            Self::MigrationAvailable => {
                spec(None, false, Some(Severity::Info), &["items"], &[WorkItem])
            }
            Self::NotAGitRepository => spec(None, false, Some(Severity::Info), &[], &[]),
            Self::DanglingReference => spec(
                None,
                false,
                Some(Severity::Error),
                &["from", "relation", "missing"],
                &[WorkItem, Relationship],
            ),
            Self::TimestampIncoherent => spec(
                None,
                false,
                Some(Severity::Warning),
                &["id", "field", "value", "reason"],
                &[WorkItem, Field],
            ),
        }
    }

    /// The stable on-the-wire literal.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ItemNotFound => "ITEM_NOT_FOUND",
            Self::CategoryNotFound => "CATEGORY_NOT_FOUND",
            Self::ValidationError => "VALIDATION_ERROR",
            Self::InvalidId => "INVALID_ID",
            Self::ParseError => "PARSE_ERROR",
            Self::ConfigInvalid => "CONFIG_INVALID",
            Self::DuplicateId => "DUPLICATE_ID",
            Self::IdPathMismatch => "ID_PATH_MISMATCH",
            Self::CycleDetected => "CYCLE_DETECTED",
            Self::InvalidRelationship => "INVALID_RELATIONSHIP",
            Self::IncompleteProjection => "INCOMPLETE_PROJECTION",
            Self::WriteConflict => "WRITE_CONFLICT",
            Self::ItemArchived => "ITEM_ARCHIVED",
            Self::AlreadyArchived => "ALREADY_ARCHIVED",
            Self::HasActiveChildren => "HAS_ACTIVE_CHILDREN",
            Self::ProjectNotFound => "PROJECT_NOT_FOUND",
            Self::IoError => "IO_ERROR",
            Self::GitError => "GIT_ERROR",
            Self::LockTimeout => "LOCK_TIMEOUT",
            Self::LockFailed => "LOCK_FAILED",
            Self::SnapshotUnstable => "SNAPSHOT_UNSTABLE",
            Self::UnsupportedVersion => "UNSUPPORTED_VERSION",
            Self::ItemNeedsMigration => "ITEM_NEEDS_MIGRATION",
            Self::ActionRequiresMigration => "ACTION_REQUIRES_MIGRATION",
            Self::MigrationPathConflict => "MIGRATION_PATH_CONFLICT",
            Self::MigrationUnsupportedGeneration => "MIGRATION_UNSUPPORTED_GENERATION",
            Self::MigrationPreconditionFailed => "MIGRATION_PRECONDITION_FAILED",
            Self::MigrationFailed => "MIGRATION_FAILED",
            Self::RollbackFailed => "ROLLBACK_FAILED",
            Self::IdGenerationFailed => "ID_GENERATION_FAILED",
            Self::InternalError => "INTERNAL_ERROR",
            Self::OwnerMismatch => "OWNER_MISMATCH",
            Self::LockHeld => "LOCK_HELD",
            Self::MigrationAvailable => "MIGRATION_AVAILABLE",
            Self::NotAGitRepository => "NOT_A_GIT_REPOSITORY",
            Self::DanglingReference => "DANGLING_REFERENCE",
            Self::TimestampIncoherent => "TIMESTAMP_INCOHERENT",
        }
    }

    /// Resolve a wire literal back to its code.
    ///
    /// Exact match only: the literals are part of the contract, so a lower-case
    /// or near-miss spelling is an unknown code rather than a silent
    /// normalization.
    ///
    /// The inherent spelling is kept because every adapter calls it directly; it
    /// is also reachable as [`FromStr::from_str`], which the two delegate to each
    /// other.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(literal: &str) -> Result<Code, UnknownCode> {
        match literal {
            "ITEM_NOT_FOUND" => Ok(Self::ItemNotFound),
            "CATEGORY_NOT_FOUND" => Ok(Self::CategoryNotFound),
            "VALIDATION_ERROR" => Ok(Self::ValidationError),
            "INVALID_ID" => Ok(Self::InvalidId),
            "PARSE_ERROR" => Ok(Self::ParseError),
            "CONFIG_INVALID" => Ok(Self::ConfigInvalid),
            "DUPLICATE_ID" => Ok(Self::DuplicateId),
            "ID_PATH_MISMATCH" => Ok(Self::IdPathMismatch),
            "CYCLE_DETECTED" => Ok(Self::CycleDetected),
            "INVALID_RELATIONSHIP" => Ok(Self::InvalidRelationship),
            "INCOMPLETE_PROJECTION" => Ok(Self::IncompleteProjection),
            "WRITE_CONFLICT" => Ok(Self::WriteConflict),
            "ITEM_ARCHIVED" => Ok(Self::ItemArchived),
            "ALREADY_ARCHIVED" => Ok(Self::AlreadyArchived),
            "HAS_ACTIVE_CHILDREN" => Ok(Self::HasActiveChildren),
            "PROJECT_NOT_FOUND" => Ok(Self::ProjectNotFound),
            "IO_ERROR" => Ok(Self::IoError),
            "GIT_ERROR" => Ok(Self::GitError),
            "LOCK_TIMEOUT" => Ok(Self::LockTimeout),
            "LOCK_FAILED" => Ok(Self::LockFailed),
            "SNAPSHOT_UNSTABLE" => Ok(Self::SnapshotUnstable),
            "UNSUPPORTED_VERSION" => Ok(Self::UnsupportedVersion),
            "ITEM_NEEDS_MIGRATION" => Ok(Self::ItemNeedsMigration),
            "ACTION_REQUIRES_MIGRATION" => Ok(Self::ActionRequiresMigration),
            "MIGRATION_PATH_CONFLICT" => Ok(Self::MigrationPathConflict),
            "MIGRATION_UNSUPPORTED_GENERATION" => Ok(Self::MigrationUnsupportedGeneration),
            "MIGRATION_PRECONDITION_FAILED" => Ok(Self::MigrationPreconditionFailed),
            "MIGRATION_FAILED" => Ok(Self::MigrationFailed),
            "ROLLBACK_FAILED" => Ok(Self::RollbackFailed),
            "ID_GENERATION_FAILED" => Ok(Self::IdGenerationFailed),
            "INTERNAL_ERROR" => Ok(Self::InternalError),
            "OWNER_MISMATCH" => Ok(Self::OwnerMismatch),
            "LOCK_HELD" => Ok(Self::LockHeld),
            "MIGRATION_AVAILABLE" => Ok(Self::MigrationAvailable),
            "NOT_A_GIT_REPOSITORY" => Ok(Self::NotAGitRepository),
            "DANGLING_REFERENCE" => Ok(Self::DanglingReference),
            "TIMESTAMP_INCOHERENT" => Ok(Self::TimestampIncoherent),
            other => Err(UnknownCode(other.to_owned())),
        }
    }

    /// The process exit class, or `None` for a finding-only code.
    ///
    /// The CLI and the MCP server read this and nothing else; a warning or an
    /// informational finding never has an exit class, so it can never change a
    /// process exit code.
    pub fn exit_class(self) -> Option<ExitClass> {
        self.spec().exit
    }

    /// Whether the same request may succeed on retry.
    pub fn retryable(self) -> bool {
        self.spec().retryable
    }

    /// The details keys this code may carry, in contract order.
    pub fn details_keys(self) -> &'static [&'static str] {
        self.spec().details
    }

    /// The subset of [`details_keys`](Code::details_keys) a constructor may omit.
    ///
    /// Only `ITEM_NOT_FOUND`'s `relation` is optional today: a direct lookup has
    /// no relation to name, while a relationship resolution does. The distinction
    /// is data, not a convention, so the anti-drift test can check that every
    /// constructor fills exactly the keys its instance owes.
    pub fn optional_details_keys(self) -> &'static [&'static str] {
        self.spec().optional_details
    }

    /// The location kinds this code may carry, in contract order.
    pub fn location_kinds(self) -> &'static [LocationKind] {
        self.spec().locations
    }

    /// The severity when this code appears as a diagnostic finding, if it may.
    pub fn finding(self) -> Option<Severity> {
        self.spec().finding
    }
}

impl FromStr for Code {
    type Err = UnknownCode;

    fn from_str(literal: &str) -> Result<Self, Self::Err> {
        Code::from_str(literal)
    }
}

impl std::fmt::Display for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A wire literal that names no registry code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownCode(String);

impl UnknownCode {
    /// The literal that was not recognised.
    pub fn literal(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for UnknownCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown error code `{}`", self.0)
    }
}

impl std::error::Error for UnknownCode {}

/// One field-level complaint inside a [`Code::ValidationError`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    /// The field path or operation argument name.
    pub path: String,
    /// Why the value was rejected, in human-readable prose.
    pub reason: String,
}

impl ValidationIssue {
    /// A new issue.
    pub fn new(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            reason: reason.into(),
        }
    }
}

impl std::fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.reason)
    }
}

/// One target of a failed multi-document operation, inside
/// [`Code::MigrationFailed`] details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetOutcome {
    /// The target path.
    pub path: String,
    /// What happened to it, in one word (`applied`, `failed`, `skipped`).
    pub outcome: String,
    /// The failure detail, when the target failed.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<String>,
}

impl TargetOutcome {
    /// A target and its outcome.
    pub fn new(path: impl Into<String>, outcome: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            outcome: outcome.into(),
            error: None,
        }
    }

    /// Record why the target failed.
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }
}

/// A registered failure with typed details and an optional location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaymarkError {
    code: Code,
    message: String,
    #[serde(default)]
    details: Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    location: Option<Box<Location>>,
}

impl WaymarkError {
    /// A bare error with empty details and no location.
    ///
    /// Public because a code with no details still needs a message, but prefer
    /// the named constructors below: they are the only way to fill the typed
    /// details a code with keys declares, because [`with_details`](Self) is
    /// crate-private.
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Map::new(),
            location: None,
        }
    }

    // -- invalid input and data (class 3) ---------------------------------

    /// No work item exists for this ID.
    pub fn item_not_found(id: &WorkItemId) -> Self {
        Self::new(Code::ItemNotFound, format!("no work item with id {id}"))
            .with_details(json!({"id": id.as_str()}))
            .with_location(Location::work_item(id))
    }

    /// A structured relationship names an item that does not exist.
    pub fn referenced_item_not_found(id: &WorkItemId, relation: impl Into<String>) -> Self {
        let relation = relation.into();
        Self::new(
            Code::ItemNotFound,
            format!("{relation} references the missing work item {id}"),
        )
        .with_details(json!({"id": id.as_str(), "relation": relation}))
        .with_location(Location::work_item(id))
    }

    /// No declared category has this key.
    pub fn category_not_found(key: impl Into<String>) -> Self {
        let key = key.into();
        Self::new(Code::CategoryNotFound, format!("no category `{key}`"))
            .with_details(json!({"key": key.clone()}))
            .with_location(Location::config_key(format!("categories.{key}")))
    }

    /// A validation failure carrying the per-field issue list.
    pub fn validation(target: impl Into<String>, issues: Vec<ValidationIssue>) -> Self {
        let target = target.into();
        let message = if issues.len() == 1 {
            format!("{}: {}", target, issues[0])
        } else {
            format!("{}: {} validation issues", target, issues.len())
        };
        Self::new(Code::ValidationError, message)
            .with_details(json!({"target": target, "issues": issues}))
    }

    /// An ID literal was rejected.
    pub fn invalid_id(input: &str, source: &IdError) -> Self {
        Self::new(
            Code::InvalidId,
            format!("invalid stable ID {input:?}: {source}"),
        )
        .with_details(json!({"value": input, "reason": source.to_string()}))
    }

    /// A document could not be parsed.
    pub fn parse(path: impl Into<String>, reason: impl Into<String>) -> Self {
        let path = path.into();
        let reason = reason.into();
        Self::new(Code::ParseError, format!("cannot parse {path}: {reason}"))
            .with_details(json!({"path": path, "reason": reason}))
            .with_location(Location::file(path))
    }

    /// The project configuration is invalid. `issues` are fully rendered
    /// `key: reason` lines, one per problem.
    pub fn config_invalid(path: impl Into<String>, issues: Vec<String>) -> Self {
        let path = path.into();
        let message = match issues.len() {
            0 => format!("configuration at {path} is invalid"),
            1 => format!("configuration at {path}: {}", issues[0]),
            count => format!("configuration at {path}: {count} issues"),
        };
        Self::new(Code::ConfigInvalid, message)
            .with_details(json!({"path": path, "issues": issues}))
    }

    /// Two or more files claim the same normalized ID.
    pub fn duplicate_id(id: &WorkItemId, paths: Vec<String>) -> Self {
        Self::new(
            Code::DuplicateId,
            format!(
                "{id} is claimed by {} files: {}",
                paths.len(),
                paths.join(", ")
            ),
        )
        .with_details(json!({"id": id.as_str(), "paths": paths}))
        .with_location(Location::work_item(id))
    }

    /// The persisted ID disagrees with the file name stem.
    pub fn id_path_mismatch(id: &WorkItemId, path: &Utf8Path, stem: impl Into<String>) -> Self {
        let path = path.to_string();
        let stem = stem.into();
        Self::new(
            Code::IdPathMismatch,
            format!("the document at {path} declares {id} but its file name is {stem}"),
        )
        .with_details(json!({"id": id.as_str(), "path": path, "stem": stem}))
        .with_location(Location::file(path))
    }

    /// A dependency or parent cycle was found. `kind` is `dependency` or
    /// `parent`; `cycle` is the closed path.
    pub fn cycle_detected(kind: impl Into<String>, cycle: Vec<WorkItemId>) -> Self {
        let kind = kind.into();
        let ids: Vec<&str> = cycle.iter().map(WorkItemId::as_str).collect();
        Self::new(
            Code::CycleDetected,
            format!("{kind} cycle: {}", ids.join(" -> ")),
        )
        .with_details(json!({
            "kind": kind,
            "cycle": ids,
        }))
    }

    /// A self-reference, duplicate edge, or wrong relation kind.
    pub fn invalid_relationship(
        relation: impl Into<String>,
        from: &WorkItemId,
        to: &WorkItemId,
        reason: impl Into<String>,
    ) -> Self {
        let relation = relation.into();
        let reason = reason.into();
        Self::new(
            Code::InvalidRelationship,
            format!("invalid {relation} from {from} to {to}: {reason}"),
        )
        .with_details(json!({
            "relation": relation,
            "from": from.as_str(),
            "to": to.as_str(),
            "reason": reason,
        }))
        .with_location(Location::relationship(from, relation, to))
    }

    /// A complete, fresh projection is unavailable.
    ///
    /// The diagnostics are the findings that blocked it; they are kept as JSON
    /// because a finding's shape belongs to the envelope, and an error must not
    /// depend on it.
    pub fn incomplete_projection(
        completeness: impl Into<String>,
        freshness: impl Into<String>,
        diagnostics: Vec<Value>,
    ) -> Self {
        let completeness = completeness.into();
        let freshness = freshness.into();
        Self::new(
            Code::IncompleteProjection,
            format!(
                "a complete, fresh projection is required but the projection is {completeness} \
                 and {freshness}"
            ),
        )
        .with_details(json!({
            "completeness": completeness,
            "freshness": freshness,
            "diagnostics": diagnostics,
        }))
    }

    // -- conflict and state rejection (class 4) ---------------------------

    /// The target changed since the operation read it.
    pub fn write_conflict(
        id: &WorkItemId,
        path: impl Into<String>,
        expected: &str,
        actual: &str,
    ) -> Self {
        let path = path.into();
        Self::new(
            Code::WriteConflict,
            format!("{id} changed on disk since it was read; reread and retry"),
        )
        .with_details(json!({
            "id": id.as_str(),
            "path": path,
            "expected_fingerprint": expected,
            "actual_fingerprint": actual,
        }))
        .with_location(Location::work_item(id))
    }

    /// An archived item rejects a write.
    pub fn item_archived(id: &WorkItemId, operation: impl Into<String>) -> Self {
        let operation = operation.into();
        Self::new(
            Code::ItemArchived,
            format!("{id} is archived and rejects {operation}"),
        )
        .with_details(json!({"id": id.as_str(), "operation": operation}))
        .with_location(Location::work_item(id))
    }

    /// Archive was requested for an already archived item.
    pub fn already_archived(id: &WorkItemId) -> Self {
        Self::new(Code::AlreadyArchived, format!("{id} is already archived"))
            .with_details(json!({"id": id.as_str()}))
            .with_location(Location::work_item(id))
    }

    /// Archive is blocked by unarchived children.
    pub fn has_active_children(id: &WorkItemId, children: &[WorkItemId]) -> Self {
        let ids: Vec<&str> = children.iter().map(WorkItemId::as_str).collect();
        Self::new(
            Code::HasActiveChildren,
            format!(
                "cannot archive {id}: {} unarchived child item(s)",
                children.len()
            ),
        )
        .with_details(json!({"id": id.as_str(), "children": ids}))
        .with_location(Location::work_item(id))
    }

    // -- filesystem, Git, environment (class 5) ---------------------------

    /// No project was found while walking up from `searched`.
    pub fn project_not_found(searched: impl Into<String>) -> Self {
        let searched = searched.into();
        Self::new(
            Code::ProjectNotFound,
            format!("no Waymark project found in {searched} or any parent directory"),
        )
        .with_details(json!({"searched": searched}))
    }

    /// A filesystem operation failed.
    pub fn io(
        path: impl Into<String>,
        operation: impl Into<String>,
        source: &std::io::Error,
    ) -> Self {
        let path = path.into();
        let operation = operation.into();
        Self::new(
            Code::IoError,
            format!("cannot {operation} {path}: {source}"),
        )
        .with_details(json!({"path": path, "operation": operation}))
        .with_location(Location::file(path))
    }

    /// A Git operation failed.
    pub fn git_error(operation: impl Into<String>, reason: impl Into<String>) -> Self {
        let operation = operation.into();
        let reason = reason.into();
        Self::new(Code::GitError, format!("Git {operation} failed: {reason}"))
            .with_details(json!({"operation": operation, "reason": reason}))
    }

    /// The lock wait expired.
    pub fn lock_timeout(path: impl Into<String>, waited_ms: u64) -> Self {
        let path = path.into();
        Self::new(
            Code::LockTimeout,
            format!("timed out after {waited_ms} ms waiting for the project write lock at {path}"),
        )
        .with_details(json!({"lock_path": path, "waited_ms": waited_ms}))
    }

    /// The lock file could not be opened, locked, or is not a regular file.
    pub fn lock_failed(path: impl Into<String>, reason: impl Into<String>) -> Self {
        let path = path.into();
        let reason = reason.into();
        Self::new(
            Code::LockFailed,
            format!("cannot take the project write lock at {path}: {reason}"),
        )
        .with_details(json!({"lock_path": path, "reason": reason}))
    }

    /// A reader could not obtain a stable snapshot after bounded retries.
    pub fn snapshot_unstable(retries: u32) -> Self {
        Self::new(
            Code::SnapshotUnstable,
            format!("the project changed during {retries} read attempts; retry"),
        )
        .with_details(json!({"retries": retries}))
    }

    // -- version and migration (class 6) ----------------------------------

    /// Data declares a generation the installed release cannot read.
    pub fn unsupported_version(found: impl Into<String>, supported: impl Into<String>) -> Self {
        let found = found.into();
        let supported = supported.into();
        Self::new(
            Code::UnsupportedVersion,
            format!("format generation {found} is not supported; this release reads {supported}"),
        )
        .with_details(json!({"found": found, "supported": supported}))
    }

    /// An item's generation is older than the project generation.
    pub fn item_needs_migration(
        id: &WorkItemId,
        item_generation: u32,
        project_generation: u32,
    ) -> Self {
        Self::new(
            Code::ItemNeedsMigration,
            format!(
                "{id} uses format generation {item_generation} but the project uses \
                 {project_generation}; run the version migration first"
            ),
        )
        .with_details(json!({
            "id": id.as_str(),
            "item_generation": item_generation,
            "project_generation": project_generation,
        }))
        .with_location(Location::work_item(id))
    }

    /// The operation requires an explicit migration first.
    pub fn action_requires_migration(
        action: impl Into<String>,
        required_command: impl Into<String>,
    ) -> Self {
        let action = action.into();
        let required_command = required_command.into();
        Self::new(
            Code::ActionRequiresMigration,
            format!("{action} requires `{required_command}` first"),
        )
        .with_details(json!({
            "action": action,
            "required_command": required_command,
        }))
    }

    /// A migration target exists, overlaps, or is case-equivalent to an
    /// existing path.
    pub fn migration_path_conflict(target: impl Into<String>, conflict: impl Into<String>) -> Self {
        let target = target.into();
        let conflict = conflict.into();
        Self::new(
            Code::MigrationPathConflict,
            format!("migration target {target} conflicts with {conflict}"),
        )
        .with_details(json!({"target": target, "conflict": conflict}))
    }

    /// The installed release cannot write the requested generation.
    pub fn migration_unsupported_generation(requested: u32, supported: u32) -> Self {
        Self::new(
            Code::MigrationUnsupportedGeneration,
            format!(
                "this release cannot write format generation {requested}; it writes {supported}"
            ),
        )
        .with_details(json!({"requested": requested, "supported": supported}))
    }

    /// Incomplete projection or fingerprint disagreement before migration.
    pub fn migration_precondition_failed(diagnostics: Vec<Value>) -> Self {
        Self::new(
            Code::MigrationPreconditionFailed,
            format!(
                "migration preconditions failed with {} diagnostic(s); nothing was written",
                diagnostics.len()
            ),
        )
        .with_details(json!({"diagnostics": diagnostics}))
    }

    /// Migration execution failed; per-target outcomes are reported.
    pub fn migration_failed(per_target: Vec<TargetOutcome>) -> Self {
        let failed = per_target
            .iter()
            .filter(|outcome| outcome.outcome == "failed")
            .count();
        Self::new(
            Code::MigrationFailed,
            format!(
                "migration failed for {failed} of {} target(s); previously applied targets were \
                 rolled back where possible",
                per_target.len()
            ),
        )
        .with_details(json!({"per_target": per_target}))
    }

    // -- internal (class 7) ----------------------------------------------

    /// A multi-file operation partially applied and rollback did not complete.
    pub fn rollback_failed(
        applied: Vec<String>,
        failed: Vec<String>,
        rollback_failed: Vec<String>,
    ) -> Self {
        Self::new(
            Code::RollbackFailed,
            format!(
                "rollback did not complete: {} applied, {} failed, {} not restored",
                applied.len(),
                failed.len(),
                rollback_failed.len()
            ),
        )
        .with_details(json!({
            "applied": applied,
            "failed": failed,
            "rollback_failed": rollback_failed,
        }))
    }

    /// A unique ID could not be allocated.
    pub fn id_generation_failed(attempts: u32) -> Self {
        Self::new(
            Code::IdGenerationFailed,
            format!("could not allocate a unique stable ID after {attempts} attempts"),
        )
        .with_details(json!({"attempts": attempts}))
    }

    /// An unexpected program fault.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(Code::InternalError, message).with_details(json!({}))
    }

    // -- finding-only codes ----------------------------------------------
    //
    // These never fail an operation: they have no exit class. They are built
    // here so the details shape has one constructor, and an adapter turns them
    // into a `Warning` or a `Finding` (see `envelope`).

    /// The declared operator differs from the item owner.
    pub fn owner_mismatch(
        item: &WorkItemId,
        operator: impl Into<String>,
        owner: impl Into<String>,
    ) -> Self {
        let operator = operator.into();
        let owner = owner.into();
        Self::new(
            Code::OwnerMismatch,
            format!("operator {operator} is not the owner {owner} of {item}"),
        )
        .with_details(json!({
            "item": item.as_str(),
            "operator": operator,
            "owner": owner,
        }))
        .with_location(Location::work_item(item))
    }

    /// A mediated write currently holds the project lock.
    pub fn lock_held(lock_path: impl Into<String>) -> Self {
        let lock_path = lock_path.into();
        Self::new(
            Code::LockHeld,
            format!("another process holds the project lock at {lock_path}"),
        )
        .with_details(json!({"lock_path": lock_path}))
    }

    /// One or more items need migration.
    pub fn migration_available(items: &[WorkItemId]) -> Self {
        let ids: Vec<&str> = items.iter().map(WorkItemId::as_str).collect();
        Self::new(
            Code::MigrationAvailable,
            format!("{} item(s) need a format migration", items.len()),
        )
        .with_details(json!({"items": ids}))
    }

    /// No Git repository is available for history or recovery.
    pub fn not_a_git_repository() -> Self {
        Self::new(
            Code::NotAGitRepository,
            "no Git repository is available for history or recovery",
        )
        .with_details(json!({}))
    }

    /// A structured relationship references a missing target.
    pub fn dangling_reference(
        from: &WorkItemId,
        relation: impl Into<String>,
        missing: &[WorkItemId],
    ) -> Self {
        let relation = relation.into();
        let ids: Vec<&str> = missing.iter().map(WorkItemId::as_str).collect();
        Self::new(
            Code::DanglingReference,
            format!(
                "{from} {relation} {} missing item(s): {}",
                missing.len(),
                ids.join(", ")
            ),
        )
        .with_details(json!({
            "from": from.as_str(),
            "relation": relation,
            "missing": ids,
        }))
        .with_location(Location::work_item(from))
    }

    /// A persisted system timestamp is internally inconsistent.
    pub fn timestamp_incoherent(
        id: &WorkItemId,
        field: impl Into<String>,
        value: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        let field = field.into();
        let value = value.into();
        let reason = reason.into();
        Self::new(
            Code::TimestampIncoherent,
            format!("{id} has an incoherent {field} ({value}): {reason}"),
        )
        .with_details(json!({
            "id": id.as_str(),
            "field": field,
            "value": value,
            "reason": reason,
        }))
        .with_location(Location::work_item(id))
    }

    // -- building ---------------------------------------------------------

    /// Replace the details wholesale.
    ///
    /// Crate-private on purpose: details are a machine contract pinned per code,
    /// so only the constructors above — each one tested against
    /// `details_keys()` — may build them. An adapter that wants a detail it
    /// cannot get is asking for a new constructor, which is a visible change to
    /// this file.
    pub(crate) fn with_details(mut self, details: Value) -> Self {
        self.details = match details {
            Value::Object(map) => map,
            other => {
                let mut map = Map::new();
                map.insert("detail".to_owned(), other);
                map
            }
        };
        self
    }

    /// Attach a location.
    ///
    /// The location is boxed because it is the largest part of the error and
    /// every `Result` in the crate carries this type: keeping it inline costs
    /// 128 bytes on every success path.
    pub fn with_location(mut self, location: Location) -> Self {
        self.location = Some(Box::new(location));
        self
    }

    /// Attach a field location, for a failure about one named argument or
    /// document field.
    pub fn at_field(self, field: impl Into<String>) -> Self {
        self.with_location(Location::field(field))
    }

    /// The registered code.
    pub fn code(&self) -> Code {
        self.code
    }

    /// The human-readable message. Never parsed for classification.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The typed details object.
    pub fn details(&self) -> &Map<String, Value> {
        &self.details
    }

    /// The optional location.
    pub fn location(&self) -> Option<&Location> {
        self.location.as_deref()
    }

    /// Whether the same request may succeed on retry.
    pub fn retryable(&self) -> bool {
        self.code.retryable()
    }

    /// The process exit class for this failure.
    ///
    /// `None` only if a finding-only code was used as a failure, which is a
    /// programming error: an adapter maps it to the internal class rather than
    /// pretending a warning failed the command.
    pub fn exit_class(&self) -> Option<ExitClass> {
        self.code.exit_class()
    }

    /// The error's machine shape, `{v, code, message, details, location?,
    /// retryable}`. This is what the envelope embeds.
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert("v".to_owned(), json!(1));
        object.insert("code".to_owned(), json!(self.code.as_str()));
        object.insert("message".to_owned(), json!(self.message));
        object.insert("details".to_owned(), Value::Object(self.details.clone()));
        if let Some(location) = &self.location {
            object.insert("location".to_owned(), json!(location));
        }
        object.insert("retryable".to_owned(), json!(self.retryable()));
        Value::Object(object)
    }
}

impl std::fmt::Display for WaymarkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for WaymarkError {}
