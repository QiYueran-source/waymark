//! The versioned machine envelope: `{v, ok, data, _meta}` on success and
//! `{v, ok: false, error}` on failure.
//!
//! The envelope is built in the library, not in an adapter, so the CLI, the MCP
//! server, and any future surface cannot drift in shape. Adapters only decide
//! *where* to write it.
//!
//! # Warnings and findings never fail a command
//!
//! Both reuse the error registry: a [`Warning`] is a non-fatal note, and a
//! [`Finding`] is a diagnostic with a severity. The registry gives the
//! finding-only codes no exit class at all, so no adapter can turn a warning into
//! a non-zero exit even by accident. Construct one from a [`WaymarkError`] built
//! by the corresponding constructor (for example
//! [`WaymarkError::owner_mismatch`](crate::error::WaymarkError::owner_mismatch)).

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::error::{Code, Severity, WaymarkError};
use crate::location::Location;

/// The envelope contract generation.
pub const ENVELOPE_VERSION: u32 = 1;

/// Where the returned data came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Read straight from a document for this call.
    DirectFile,
    /// Served from an in-memory projection.
    MemoryProjection,
}

/// How much of the project the answer covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Completeness {
    /// Only the named items were examined.
    Scoped,
    /// The whole project was examined.
    Complete,
}

impl Completeness {
    /// An incomplete answer.
    pub fn incomplete() -> Self {
        Self::Scoped
    }

    /// The `snake_case` literal.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scoped => "scoped",
            Self::Complete => "complete",
        }
    }
}

/// How current the answer is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    /// The data matched the files when it was produced.
    Fresh,
    /// Something could have changed since.
    Stale,
    /// Freshness cannot be established.
    Unknown,
}

impl Freshness {
    /// The `snake_case` literal.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
        }
    }
}

/// A non-fatal note attached to a successful answer.
///
/// The code comes from the registry, but it never has an exit class: a warning is
/// an observation, not a failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    /// The registry code, for example `OWNER_MISMATCH`.
    pub code: Code,
    /// Human-readable prose.
    pub message: String,
    /// Where the condition was observed.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub location: Option<Location>,
}

impl Warning {
    /// A warning with no location.
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            location: None,
        }
    }

    /// Attach a location.
    pub fn with_location(mut self, location: Location) -> Self {
        self.location = Some(location);
        self
    }

    /// Build a warning from a finding-only [`WaymarkError`].
    ///
    /// # Panics
    ///
    /// Panics if the error's code has an exit class: such an error is a failure
    /// and must be returned, not reported as a warning.
    pub fn from_error(error: &WaymarkError) -> Self {
        assert!(
            error.code().exit_class().is_none(),
            "{} is a failure code and cannot be a warning",
            error.code()
        );
        let warning = Self::new(error.code(), error.message());
        match error.location() {
            Some(location) => warning.with_location(location.clone()),
            None => warning,
        }
    }
}

/// A diagnostic observation attached to a machine answer.
///
/// Findings reuse the error registry and the envelope's [`Warning`] codes: there
/// is no second taxonomy. `fixable` says whether a command can repair it, which
/// is what `validate` and `doctor` report on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// How serious the observation is.
    pub severity: Severity,
    /// The registry code.
    pub code: Code,
    /// Where it was observed.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub location: Option<Location>,
    /// Human-readable prose.
    pub message: String,
    /// Whether a command can repair it.
    pub fixable: bool,
}

impl Finding {
    /// A finding at registry severity, not fixable.
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            severity: code.finding().unwrap_or(Severity::Info),
            code,
            location: None,
            message: message.into(),
            fixable: false,
        }
    }

    /// Attach a location.
    pub fn with_location(mut self, location: Location) -> Self {
        self.location = Some(location);
        self
    }

    /// Mark whether a command can repair it.
    pub fn fixable(mut self, fixable: bool) -> Self {
        self.fixable = fixable;
        self
    }

    /// Build a finding from an error whose code the registry lists as a finding.
    ///
    /// # Panics
    ///
    /// Panics if the code may not appear as a finding.
    pub fn from_error(error: &WaymarkError) -> Self {
        assert!(
            error.code().finding().is_some(),
            "{} is not a finding code",
            error.code()
        );
        let finding = Self::new(error.code(), error.message());
        match error.location() {
            Some(location) => finding.with_location(location.clone()),
            None => finding,
        }
    }
}

/// The `_meta` block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    /// Where the data came from.
    pub source: Source,
    /// How much of the project the answer covers.
    pub completeness: Completeness,
    /// How current it is.
    pub freshness: Freshness,
    /// Diagnostic findings.
    #[serde(default)]
    pub diagnostics: Vec<Finding>,
    /// Non-fatal notes.
    #[serde(default)]
    pub warnings: Vec<Warning>,
}

impl Meta {
    /// Metadata for a direct, scoped, fresh read.
    pub fn direct_file_scoped() -> Self {
        Self {
            source: Source::DirectFile,
            completeness: Completeness::Scoped,
            freshness: Freshness::Fresh,
            diagnostics: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Metadata for a complete, fresh projection read.
    pub fn memory_projection_complete() -> Self {
        Self {
            source: Source::MemoryProjection,
            completeness: Completeness::Complete,
            freshness: Freshness::Fresh,
            diagnostics: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Attach warnings.
    pub fn with_warnings(mut self, warnings: Vec<Warning>) -> Self {
        self.warnings = warnings;
        self
    }

    /// Attach diagnostic findings.
    pub fn with_diagnostics(mut self, diagnostics: Vec<Finding>) -> Self {
        self.diagnostics = diagnostics;
        self
    }
}

/// Build a success envelope.
pub fn success_envelope(data: Value, meta: Meta) -> Value {
    let mut object = Map::new();
    object.insert("v".to_owned(), json!(ENVELOPE_VERSION));
    object.insert("ok".to_owned(), json!(true));
    object.insert("data".to_owned(), data);
    object.insert("_meta".to_owned(), json!(meta));
    Value::Object(object)
}

/// Build a failure envelope.
pub fn failure_envelope(error: &WaymarkError) -> Value {
    let mut object = Map::new();
    object.insert("v".to_owned(), json!(ENVELOPE_VERSION));
    object.insert("ok".to_owned(), json!(false));
    object.insert("error".to_owned(), error.to_json());
    Value::Object(object)
}

/// Build a failure envelope for a failure the domain registry does not own.
///
/// Adapters use this for their own failures — the CLI's adapter-level
/// `USAGE_ERROR` is the only one today. The shape stays in the library so an
/// adapter that owns a code still cannot drift from the contract.
pub fn adapter_failure_envelope(
    code: &str,
    message: &str,
    details: Value,
    location: Option<&Location>,
    retryable: bool,
) -> Value {
    let mut error = Map::new();
    error.insert("v".to_owned(), json!(1));
    error.insert("code".to_owned(), json!(code));
    error.insert("message".to_owned(), json!(message));
    error.insert("details".to_owned(), details);
    if let Some(location) = location {
        error.insert("location".to_owned(), json!(location));
    }
    error.insert("retryable".to_owned(), json!(retryable));

    let mut object = Map::new();
    object.insert("v".to_owned(), json!(ENVELOPE_VERSION));
    object.insert("ok".to_owned(), json!(false));
    object.insert("error".to_owned(), Value::Object(error));
    Value::Object(object)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_meta_round_trips_through_json() {
        let meta = Meta::direct_file_scoped().with_warnings(vec![Warning::new(
            Code::OwnerMismatch,
            "operator is not the owner",
        )]);
        let json = serde_json::to_value(&meta).unwrap();
        assert_eq!(json["source"], serde_json::json!("direct_file"));
        assert_eq!(
            json["warnings"][0]["code"],
            serde_json::json!("OWNER_MISMATCH")
        );
        let back: Meta = serde_json::from_value(json).unwrap();
        assert_eq!(back, meta);
    }

    #[test]
    fn a_finding_carries_its_registry_severity() {
        let finding = Finding::new(Code::DanglingReference, "wm-000000000000 is missing");
        assert_eq!(finding.severity, Severity::Error);
        assert!(!finding.fixable);
        assert_eq!(
            serde_json::to_value(&finding).unwrap(),
            json!({
                "severity": "error",
                "code": "DANGLING_REFERENCE",
                "message": "wm-000000000000 is missing",
                "fixable": false,
            })
        );
    }
}
