//! The typed error contract: one stable code per recovery unit, its exit class,
//! its retryability, typed details, and an optional [`Location`].
//!
//! # Shape of the registry
//!
//! A code names a *recovery action*, not an error variant: `VALIDATION_ERROR`
//! covers every field-level complaint and carries the per-field issue list in
//! its details. Adding a code is warranted only when the recovery action
//! differs, so the table stays small and every addition is a visible contract
//! change.
//!
//! The table lives in exactly one `match` per attribute ([`Code::exit_class`],
//! [`Code::retryable`], [`Code::as_str`]) so a new variant cannot compile
//! without an answer. A later ticket freezes the full registry and pins it with
//! a golden snapshot; this ticket carries only the codes its operations can
//! actually produce, and the mapping is additive.
//!
//! # Details
//!
//! Details are typed per code: each constructor on [`WaymarkError`] fills the
//! keys the spec pins for that code, so an adapter never has to guess a shape.
//! The keys are `snake_case` and are part of the machine contract.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::id::{IdError, WorkItemId};
use crate::location::Location;

/// A registered error code.
///
/// The literal is the `SCREAMING_SNAKE` string on the wire; the enum variant
/// only exists to make exhaustiveness compile-checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Code {
    /// One or more field-level validation failures.
    ValidationError,
    /// No work item exists for the requested stable ID.
    ItemNotFound,
    /// No Waymark project was found while searching.
    ProjectNotFound,
    /// A filesystem operation failed.
    IoError,
    /// A document or its frontmatter cannot be parsed.
    ParseError,
    /// The project configuration fails schema validation.
    ConfigInvalid,
    /// An ID literal does not satisfy the stable-ID format.
    InvalidId,
    /// The target changed since the operation read it.
    WriteConflict,
    /// The lock wait expired; nothing was written.
    LockTimeout,
    /// The lock file cannot be opened, locked, or is not a regular file.
    LockFailed,
    /// A unique stable ID could not be allocated after bounded attempts.
    IdGenerationFailed,
    /// An unexpected program fault.
    InternalError,
    /// Bad flags or an unknown subcommand; adapter-level, exit class 1.
    UsageError,
}

impl Code {
    /// Every code this release can produce, in table order.
    pub const ALL: &'static [Code] = &[
        Code::ValidationError,
        Code::ItemNotFound,
        Code::ProjectNotFound,
        Code::IoError,
        Code::ParseError,
        Code::ConfigInvalid,
        Code::InvalidId,
        Code::WriteConflict,
        Code::LockTimeout,
        Code::LockFailed,
        Code::IdGenerationFailed,
        Code::InternalError,
        Code::UsageError,
    ];

    /// The stable on-the-wire literal.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ValidationError => "VALIDATION_ERROR",
            Self::ItemNotFound => "ITEM_NOT_FOUND",
            Self::ProjectNotFound => "PROJECT_NOT_FOUND",
            Self::IoError => "IO_ERROR",
            Self::ParseError => "PARSE_ERROR",
            Self::ConfigInvalid => "CONFIG_INVALID",
            Self::InvalidId => "INVALID_ID",
            Self::WriteConflict => "WRITE_CONFLICT",
            Self::LockTimeout => "LOCK_TIMEOUT",
            Self::LockFailed => "LOCK_FAILED",
            Self::IdGenerationFailed => "ID_GENERATION_FAILED",
            Self::InternalError => "INTERNAL_ERROR",
            Self::UsageError => "USAGE_ERROR",
        }
    }

    /// The process exit class.
    ///
    /// 1 usage, 2 not found, 3 invalid input or data, 4 conflict or state
    /// rejection, 5 filesystem/git/environment, 6 version or migration, 7
    /// internal. The adapter reads this and nothing else.
    pub fn exit_class(self) -> u8 {
        match self {
            Self::UsageError => 1,
            Self::ItemNotFound => 2,
            Self::ValidationError | Self::ParseError | Self::ConfigInvalid | Self::InvalidId => 3,
            Self::WriteConflict => 4,
            Self::ProjectNotFound | Self::IoError | Self::LockTimeout | Self::LockFailed => 5,
            Self::IdGenerationFailed | Self::InternalError => 7,
        }
    }

    /// Whether the same request may succeed on retry.
    ///
    /// Only lock contention is transient. A parse failure, a fingerprint
    /// conflict, and an invalid input are all terminal until the caller changes
    /// something.
    pub fn retryable(self) -> bool {
        matches!(self, Self::LockTimeout)
    }
}

impl std::fmt::Display for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

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
    /// A bare error with empty details and no location. Prefer the named
    /// constructors below, which fill the typed details the registry pins.
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Map::new(),
            location: None,
        }
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

    /// The project configuration is invalid. `issues` are fully rendered
    /// `key: reason` lines, one per problem.
    pub fn config_invalid(path: impl Into<String>, issues: Vec<String>) -> Self {
        let path = path.into();
        let message = match issues.len() {
            0 => format!("configuration at {path} is invalid"),
            1 => format!("configuration at {path}: {}", issues[0]),
            count => format!("configuration at {path}: {count} issues"),
        };
        Self::new(Code::ConfigInvalid, message).with_details(json!({
            "path": path,
            "issues": issues,
        }))
    }

    /// A document could not be parsed.
    pub fn parse(path: impl Into<String>, reason: impl Into<String>) -> Self {
        let path = path.into();
        let reason = reason.into();
        Self::new(Code::ParseError, format!("cannot parse {path}: {reason}"))
            .with_details(json!({"path": path, "reason": reason}))
            .with_location(Location::file(path))
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

    /// No item exists for this ID.
    pub fn item_not_found(id: &WorkItemId) -> Self {
        Self::new(Code::ItemNotFound, format!("no work item with id {id}"))
            .with_details(json!({"id": id.as_str()}))
            .with_location(Location::work_item(id))
    }

    /// No project was found while walking up from `searched`.
    pub fn project_not_found(searched: impl Into<String>) -> Self {
        let searched = searched.into();
        Self::new(
            Code::ProjectNotFound,
            format!("no Waymark project found in {searched} or any parent directory"),
        )
        .with_details(json!({"searched": searched}))
    }

    /// An ID literal was rejected.
    pub fn invalid_id(input: &str, source: &IdError) -> Self {
        Self::new(
            Code::InvalidId,
            format!("invalid stable ID {input:?}: {source}"),
        )
        .with_details(json!({"value": input, "reason": source.to_string()}))
    }

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

    /// An adapter-level usage failure. Deliberately outside the domain registry
    /// in the sense that no operation returns it, but it shares the exit table.
    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(Code::UsageError, message).with_details(json!({}))
    }

    /// Replace the details wholesale. Used by constructors above.
    pub fn with_details(mut self, details: Value) -> Self {
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
    pub fn exit_class(&self) -> u8 {
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
