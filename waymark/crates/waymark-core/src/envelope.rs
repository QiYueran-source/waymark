//! The versioned machine envelope: `{v, ok, data, _meta}` on success and
//! `{v, ok: false, error}` on failure.
//!
//! The envelope is built in the library, not in an adapter, so the CLI, the MCP
//! server, and any future surface cannot drift in shape. Adapters only decide
//! *where* to write it.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::error::WaymarkError;
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

/// A non-fatal note attached to a successful answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    /// The registry code, for example `OWNER_MISMATCH`.
    pub code: String,
    /// Human-readable prose.
    pub message: String,
    /// Where the condition was observed.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub location: Option<Location>,
}

impl Warning {
    /// A warning at a location.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            location: None,
        }
    }

    /// Attach a location.
    pub fn with_location(mut self, location: Location) -> Self {
        self.location = Some(location);
        self
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
    pub diagnostics: Vec<Value>,
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

    /// Attach warnings.
    pub fn with_warnings(mut self, warnings: Vec<Warning>) -> Self {
        self.warnings = warnings;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_meta_round_trips_through_json() {
        let meta = Meta::direct_file_scoped().with_warnings(vec![Warning::new(
            "OWNER_MISMATCH",
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
}
