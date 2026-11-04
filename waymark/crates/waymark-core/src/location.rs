//! Where a problem is: a tagged union over the five place kinds the spec names.
//!
//! Every variant serializes with a `kind` discriminator so machine consumers can
//! branch without parsing the message. The union is additive: a new place kind
//! is a new variant, never a new string field.

use serde::{Deserialize, Serialize};

use crate::id::WorkItemId;

/// A locatable place in the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Location {
    /// A work item, addressed by stable ID.
    WorkItem {
        /// The normalized stable ID.
        id: WorkItemId,
    },
    /// A file, addressed by its path and — when a parser can report it — the
    /// 1-based position inside it.
    File {
        /// The path as it was resolved (repository-relative or absolute).
        path: String,
        /// 1-based line, when known.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        line: Option<u32>,
        /// 1-based column, when known.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        column: Option<u32>,
    },
    /// A field of a document, addressed by its persisted key or a dotted path.
    Field {
        /// The field path, for example `title` or `custom_fields.effort`.
        field: String,
        /// The document the field belongs to, when the failure was recorded
        /// against a document. A field parsed outside one stays pathless.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        path: Option<String>,
        /// 1-based line in that document, when known.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        line: Option<u32>,
        /// 1-based column in that document, when known.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        column: Option<u32>,
    },
    /// A key of the project configuration.
    ConfigKey {
        /// The configuration key path, for example `categories.bug.path`.
        key: String,
    },
    /// A directed relation between two work items.
    Relationship {
        /// The item the relation starts at.
        from: WorkItemId,
        /// The persisted relation name, for example `dependencies`.
        relation: String,
        /// The item the relation points to.
        to: WorkItemId,
    },
}

impl Location {
    /// A work-item location.
    pub fn work_item(id: &WorkItemId) -> Self {
        Self::WorkItem { id: id.clone() }
    }

    /// A file location.
    pub fn file(path: impl Into<String>) -> Self {
        Self::File {
            path: path.into(),
            line: None,
            column: None,
        }
    }

    /// A file location with a 1-based position inside it.
    pub fn file_at(path: impl Into<String>, line: u32, column: u32) -> Self {
        Self::File {
            path: path.into(),
            line: Some(line),
            column: Some(column),
        }
    }

    /// A field location.
    pub fn field(field: impl Into<String>) -> Self {
        Self::Field {
            field: field.into(),
            path: None,
            line: None,
            column: None,
        }
    }

    /// Point this location at the document it was found in.
    ///
    /// Only a field location gains a document: a file location already names
    /// one, and a work item or relationship is not a document place. Any other
    /// location is returned unchanged, so callers can attach unconditionally.
    pub fn in_document(mut self, document: impl Into<String>) -> Self {
        if let Self::Field {
            path: field_path, ..
        } = &mut self
        {
            *field_path = Some(document.into());
        }
        self
    }

    /// A configuration-key location.
    pub fn config_key(key: impl Into<String>) -> Self {
        Self::ConfigKey { key: key.into() }
    }

    /// A relationship location.
    pub fn relationship(from: &WorkItemId, relation: impl Into<String>, to: &WorkItemId) -> Self {
        Self::Relationship {
            from: from.clone(),
            relation: relation.into(),
            to: to.clone(),
        }
    }
}

/// The five place kinds a [`Location`] can be, as registry metadata.
///
/// This is the *kind*, not a value: the registry declares which kinds a code may
/// carry, and a golden snapshot pins that declaration. It serializes as the same
/// `snake_case` discriminator the [`Location`] union uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationKind {
    /// A work item, addressed by stable ID.
    WorkItem,
    /// A file.
    File,
    /// A field of a document.
    Field,
    /// A key of the project configuration.
    ConfigKey,
    /// A directed relation between two work items.
    Relationship,
}

impl LocationKind {
    /// Every kind, in the order the registry table lists them.
    pub const ALL: &'static [LocationKind] = &[
        LocationKind::WorkItem,
        LocationKind::File,
        LocationKind::Field,
        LocationKind::ConfigKey,
        LocationKind::Relationship,
    ];

    /// The `snake_case` literal, identical to the [`Location`] discriminator.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WorkItem => "work_item",
            Self::File => "file",
            Self::Field => "field",
            Self::ConfigKey => "config_key",
            Self::Relationship => "relationship",
        }
    }
}

impl Location {
    /// Which kind this location is.
    pub fn kind(&self) -> LocationKind {
        match self {
            Self::WorkItem { .. } => LocationKind::WorkItem,
            Self::File { .. } => LocationKind::File,
            Self::Field { .. } => LocationKind::Field,
            Self::ConfigKey { .. } => LocationKind::ConfigKey,
            Self::Relationship { .. } => LocationKind::Relationship,
        }
    }
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WorkItem { id } => write!(f, "work item {id}"),
            Self::File { path, line, column } => match (line, column) {
                (Some(line), Some(column)) => write!(f, "file {path}:{line}:{column}"),
                _ => write!(f, "file {path}"),
            },
            Self::Field {
                field,
                path,
                line,
                column,
            } => match path {
                Some(path) => match (line, column) {
                    (Some(line), Some(column)) => {
                        write!(f, "field {field} in {path}:{line}:{column}")
                    }
                    _ => write!(f, "field {field} in {path}"),
                },
                None => write!(f, "field {field}"),
            },
            Self::ConfigKey { key } => write!(f, "config key {key}"),
            Self::Relationship { from, relation, to } => {
                write!(f, "relationship {relation} {from} -> {to}")
            }
        }
    }
}
