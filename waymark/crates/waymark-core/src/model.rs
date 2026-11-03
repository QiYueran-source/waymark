//! Work-item model.
//!
//! The model is the typed shape of one Markdown document: the persisted v1
//! frontmatter plus the Markdown body. Derived values (`ready`, `blocked`,
//! `blocked_by`, children, dependents) never live here — they are computed from
//! the projection.

use crate::id::WorkItemId;
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};

/// The on-disk format generation this release writes and reads.
pub const FORMAT_GENERATION: u32 = 1;

/// Frontmatter keys a custom field may not shadow: the persisted v1 fields plus
/// the values computed from the projection.
pub const RESERVED_FIELD_NAMES: &[&str] = &[
    "schema",
    "id",
    "title",
    "category",
    "status",
    "owner",
    "creator",
    "created_at",
    "updated_at",
    "closed_at",
    "parent",
    "dependencies",
    "soft_links",
    "custom_fields",
    "archived_at",
    "archive_reason",
    "ready",
    "blocked",
    "blocked_by",
    "children",
    "dependents",
];

/// The lifecycle states a work item can be in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Created and not yet started.
    Open,
    /// Being worked on.
    InProgress,
    /// Completed; no longer blocks its dependents.
    Closed,
}

impl Status {
    /// The persisted literal.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::InProgress => "in_progress",
            Self::Closed => "closed",
        }
    }

    /// Parse the persisted literal. Accepts `in progress` and `in-progress` as
    /// human-friendly spellings only through [`Self::from_cli`].
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "open" => Some(Self::Open),
            "in_progress" => Some(Self::InProgress),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }

    /// Parse a CLI literal, accepting `in progress` and `in-progress`.
    pub fn from_cli(input: &str) -> Option<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "open" => Some(Self::Open),
            "in_progress" | "in-progress" | "in progress" => Some(Self::InProgress),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }

    /// Whether this state counts as finished for readiness.
    pub fn is_closed(self) -> bool {
        matches!(self, Self::Closed)
    }

    /// The status group order used by default `rank` ordering.
    pub fn rank_group(self) -> u8 {
        match self {
            Self::Open => 0,
            Self::InProgress => 1,
            Self::Closed => 2,
        }
    }
}

/// A complete work item: persisted frontmatter plus body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkItem {
    /// Stable identity.
    pub id: WorkItemId,
    /// Short single-line label.
    pub title: String,
    /// Category key this item belongs to.
    pub category: CategoryKey,
    /// Lifecycle state.
    pub status: Status,
    /// Accountable human.
    pub owner: String,
    /// Immutable provenance.
    pub creator: Option<String>,
    /// Creation instant.
    pub created_at: Timestamp,
    /// Last effective change.
    pub updated_at: Timestamp,
    /// Most recent close transition.
    pub closed_at: Option<Timestamp>,
    /// Containing item.
    pub parent: Option<WorkItemId>,
    /// Hard blocking prerequisites.
    pub dependencies: Vec<WorkItemId>,
    /// Symmetric non-blocking associations.
    pub soft_links: Vec<WorkItemId>,
    /// Declared custom values.
    pub custom_fields: CustomFields,
    /// Archive marker; presence means archived.
    pub archived_at: Option<Timestamp>,
    /// Optional archive reason.
    pub archive_reason: Option<String>,
    /// Format generation this document was written at.
    pub schema: u32,
    /// Markdown content after the frontmatter, canonicalized.
    pub body: String,
}

impl WorkItem {
    /// Whether this item is archived.
    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }

    /// Whether this document's generation matches the release's generation.
    pub fn needs_migration(&self) -> bool {
        self.schema < FORMAT_GENERATION
    }
}

/// A validated category key: lowercase ASCII, starting with a letter and
/// continuing with lowercase letters, digits, or hyphens.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CategoryKey(String);

impl CategoryKey {
    /// Validate and build a category key.
    pub fn parse(input: &str) -> Result<Self, CategoryKeyError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(CategoryKeyError::Empty);
        }
        let mut chars = trimmed.chars();
        let first = chars
            .next()
            .expect("a non-empty string has a first character");
        if !first.is_ascii_lowercase() {
            return Err(CategoryKeyError::Invalid {
                value: trimmed.to_owned(),
                reason: "must start with a lowercase ASCII letter".to_owned(),
            });
        }
        for character in chars {
            let ok =
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-';
            if !ok {
                return Err(CategoryKeyError::Invalid {
                    value: trimmed.to_owned(),
                    reason: format!(
                        "may only contain lowercase ASCII letters, digits, and hyphens, found {character:?}"
                    ),
                });
            }
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The persisted literal.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CategoryKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for CategoryKey {
    type Err = CategoryKeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<String> for CategoryKey {
    type Error = CategoryKeyError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<CategoryKey> for String {
    fn from(value: CategoryKey) -> Self {
        value.0
    }
}

/// Why a category key literal was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CategoryKeyError {
    /// The key is empty or whitespace only.
    #[error("a category key cannot be empty")]
    Empty,
    /// The key breaks the key grammar.
    #[error("{value:?} is not a valid category key: {reason}")]
    Invalid {
        /// The rejected literal.
        value: String,
        /// Why it was rejected.
        reason: String,
    },
}

/// A custom-field key: lowercase ASCII `snake_case`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FieldKey(String);

impl FieldKey {
    /// Validate and build a field key.
    pub fn parse(input: &str) -> Result<Self, FieldKeyError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(FieldKeyError::Empty);
        }
        if trimmed.starts_with('_') || trimmed.ends_with('_') {
            return Err(FieldKeyError::Invalid {
                value: trimmed.to_owned(),
                reason: "must not start or end with an underscore".to_owned(),
            });
        }
        if trimmed.contains("__") {
            return Err(FieldKeyError::Invalid {
                value: trimmed.to_owned(),
                reason: "must not contain two consecutive underscores".to_owned(),
            });
        }
        for character in trimmed.chars() {
            let ok =
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_';
            if !ok {
                return Err(FieldKeyError::Invalid {
                    value: trimmed.to_owned(),
                    reason: format!(
                        "may only contain lowercase ASCII letters, digits, and underscores, found {character:?}"
                    ),
                });
            }
        }
        if trimmed
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit())
        {
            return Err(FieldKeyError::Invalid {
                value: trimmed.to_owned(),
                reason: "must not start with a digit".to_owned(),
            });
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The persisted literal.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for FieldKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for FieldKey {
    type Err = FieldKeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<String> for FieldKey {
    type Error = FieldKeyError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<FieldKey> for String {
    fn from(value: FieldKey) -> Self {
        value.0
    }
}

/// Why a field key literal was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FieldKeyError {
    /// The key is empty or whitespace only.
    #[error("a custom-field key cannot be empty")]
    Empty,
    /// The key breaks the key grammar.
    #[error("{value:?} is not a valid custom-field key: {reason}")]
    Invalid {
        /// The rejected literal.
        value: String,
        /// Why it was rejected.
        reason: String,
    },
}

/// Typed custom-field values, keyed by declared field name.
///
/// A `BTreeMap` keeps serialization deterministic; the canonical writer emits
/// the keys in sorted order.
pub type CustomFields = std::collections::BTreeMap<String, FieldValue>;

/// One custom-field value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldValue {
    /// A string, including the empty string.
    String(String),
    /// A whole number.
    Integer(i64),
    /// A boolean.
    Boolean(bool),
    /// A homogeneous list of scalars.
    List(Vec<FieldValue>),
}

impl FieldValue {
    /// A short, stable type name used in validation issues.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::String(_) => "string",
            Self::Integer(_) => "integer",
            Self::Boolean(_) => "boolean",
            Self::List(_) => "list",
        }
    }
}
