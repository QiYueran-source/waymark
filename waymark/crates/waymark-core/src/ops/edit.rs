//! Ordinary editing: the title, the body, and custom fields.
//!
//! Editing is split by data area on purpose. There is no generic frontmatter
//! patch and no raw-document patch, so each rule below is the only thing that can
//! touch its own columns:
//!
//! - **Title** is a full replacement. Surrounding whitespace is trimmed, the
//!   result must be non-empty and single-line, and internal whitespace is
//!   preserved exactly.
//! - **Body** is a full replacement. Absent input leaves it alone, an explicit
//!   empty string clears it, and a present value is canonicalized to exactly one
//!   trailing newline without trimming leading or internal whitespace.
//! - **Custom fields** take an explicit change set of `set`/`clear` entries, each
//!   checked against the declared [`FieldType`]. Nulls, unknown keys, duplicate
//!   keys, keys in both directions, and clearing a required field are rejected as
//!   `VALIDATION_ERROR` with a per-field issue list — and the *whole* set is
//!   validated before the single write, so a partially valid request writes
//!   nothing.
//!
//! All three go through [`mediated`](crate::ops::mediated): an archived item is
//! `ITEM_ARCHIVED`, an operator that is not the owner produces the advisory
//! `OWNER_MISMATCH` warning without blocking, and the replacement is
//! fingerprint-checked and atomic.
//!
//! A request whose effective value already matches the document is not a write:
//! `changed` is false, `updated_at` does not move, and the file's bytes are left
//! exactly as they were.

use camino::Utf8PathBuf;

use crate::config::FieldType;
use crate::envelope::Meta;
use crate::error::{ValidationIssue, WaymarkError};
use crate::model::{FieldKey, FieldValue, WorkItem};
use crate::ops::mediated::{self, Loaded};
use crate::ops::OpOptions;
use crate::store::Project;

/// What one title or body edit produced.
#[derive(Debug, Clone)]
pub struct EditOutcome {
    /// The complete item as written, or as it would have been written on a dry
    /// run.
    pub item: WorkItem,
    /// The document the item lives in (unchanged by an edit).
    pub path: Utf8PathBuf,
    /// Whether the effective value actually moved.
    pub changed: bool,
    /// Whether the document on disk changed.
    pub written: bool,
    /// The `_meta` block this write justifies, including `OWNER_MISMATCH`.
    pub meta: Meta,
}

/// What one custom-field update produced.
#[derive(Debug, Clone)]
pub struct FieldOutcome {
    /// The complete item as written, or as it would have been written on a dry
    /// run.
    pub item: WorkItem,
    /// The document the item lives in.
    pub path: Utf8PathBuf,
    /// Whether any custom value actually moved.
    pub changed: bool,
    /// Whether the document on disk changed.
    pub written: bool,
    /// The declared keys whose value or presence changed, in key order.
    ///
    /// A `set` that repeats the stored value and a `clear` of a key that was
    /// already absent are not changes and do not appear here.
    pub changed_fields: Vec<FieldKey>,
    /// The `_meta` block this write justifies, including `OWNER_MISMATCH`.
    pub meta: Meta,
}

/// One direction of a custom-field change.
///
/// [`FieldChange::Null`] exists only so a structured caller (JSON, MCP) can send
/// an explicit `null` and be *told* that null is not a value. It is never
/// persisted and never clears anything: the interface is total over the values an
/// adapter can hand over, which is what keeps a `null` from silently meaning
/// "clear".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldChange {
    /// Replace the value with this one, which must match the declared type.
    Set(FieldValue),
    /// Remove the value; rejected when the field is declared required.
    Clear,
    /// An explicit null, which is always rejected with `VALIDATION_ERROR`.
    Null,
}

/// One keyed entry of a custom-field change set.
///
/// The input is a *sequence*, not a map, so a duplicate key survives long enough
/// to be reported instead of being silently collapsed by the container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChangeEntry {
    /// The declared field key this entry targets.
    pub key: String,
    /// What to do with it.
    pub change: FieldChange,
}

impl FieldChangeEntry {
    /// An entry that sets `key` to `value`.
    pub fn set(key: impl Into<String>, value: FieldValue) -> Self {
        Self {
            key: key.into(),
            change: FieldChange::Set(value),
        }
    }

    /// An entry that clears `key`.
    pub fn clear(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            change: FieldChange::Clear,
        }
    }

    /// An entry that explicitly asks for `null`, which the operation rejects.
    pub fn null(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            change: FieldChange::Null,
        }
    }

    /// An entry with an explicit direction.
    pub fn new(key: impl Into<String>, change: FieldChange) -> Self {
        Self {
            key: key.into(),
            change,
        }
    }
}

/// Replace the title of one item.
///
/// The title is trimmed, must be non-empty, and must not contain `\n` or `\r`.
/// Internal whitespace (including runs of spaces and tabs) is preserved. The
/// identity, status, category, owner, relationships, and body are untouched.
pub fn set_title(
    project: &Project,
    id: &str,
    title: &str,
    options: &OpOptions,
) -> Result<EditOutcome, WaymarkError> {
    let title = normalize_title(title)?;
    crate::lock::with_project_lock(project, options.lock_wait, || {
        let loaded = mediated::load_for_write(project, id, "title")?;
        let mut item = loaded.item.clone();
        item.title = title;
        finish_edit(project, &loaded, item, options)
    })
}

/// Replace, or with `None` leave alone, the body of one item.
///
/// `Some("")` (or a body that is only newlines) clears the body; any other value
/// fully replaces it, canonicalized to exactly one trailing newline. Leading and
/// internal whitespace are part of the content and are never trimmed. Body text is
/// never interpreted as a relationship language.
pub fn replace_body(
    project: &Project,
    id: &str,
    body: Option<&str>,
    options: &OpOptions,
) -> Result<EditOutcome, WaymarkError> {
    let replacement = body.map(crate::serialize::canonical_body);
    crate::lock::with_project_lock(project, options.lock_wait, || {
        let loaded = mediated::load_for_write(project, id, "body")?;
        let mut item = loaded.item.clone();
        if let Some(body) = replacement {
            item.body = body;
        }
        finish_edit(project, &loaded, item, options)
    })
}

/// Apply one validated custom-field change set to one item.
///
/// Every entry is checked against the project's declaration before anything is
/// read for writing: the key must be declared, the direction must not be `null`,
/// no key may appear twice, and a `set` value must match the declared
/// [`FieldType`] exactly — no implicit conversion. Clearing a required field is
/// rejected. All issues are reported together.
pub fn update_custom_fields(
    project: &Project,
    id: &str,
    changes: &[FieldChangeEntry],
    options: &OpOptions,
) -> Result<FieldOutcome, WaymarkError> {
    // The change set is checked against the declarations before the lock is
    // taken: a rejected set never even reads the item, so a bad request cannot
    // block a concurrent writer for the duration of a lock wait.
    validate_changes(project, changes)?;
    crate::lock::with_project_lock(project, options.lock_wait, || {
        let loaded = mediated::load_for_write(project, id, "custom fields")?;
        apply_field_changes(project, &loaded, changes, options)
    })
}

/// Validate, apply, and commit one field change set for an already-loaded item.
fn apply_field_changes(
    project: &Project,
    loaded: &Loaded,
    changes: &[FieldChangeEntry],
    options: &OpOptions,
) -> Result<FieldOutcome, WaymarkError> {
    validate_changes(project, changes)?;

    let mut item = loaded.item.clone();
    let before = item.custom_fields.clone();
    let mut changed_fields: Vec<FieldKey> = Vec::new();

    for entry in changes {
        let key =
            FieldKey::parse(&entry.key).expect("validation proved every key parses as a field key");
        match &entry.change {
            FieldChange::Set(value) => {
                item.custom_fields.insert(entry.key.clone(), value.clone());
            }
            FieldChange::Clear => {
                item.custom_fields.remove(&entry.key);
            }
            FieldChange::Null => {
                unreachable!("validation rejects every null entry before this point")
            }
        }
        if before.get(&entry.key) != item.custom_fields.get(&entry.key)
            && !changed_fields.contains(&key)
        {
            changed_fields.push(key);
        }
    }
    changed_fields.sort();

    let meta = mediated::write_meta(mediated::owner_warnings(project, &item, options));
    if changed_fields.is_empty() {
        return Ok(FieldOutcome {
            item,
            path: loaded.path.clone(),
            changed: false,
            written: false,
            changed_fields,
            meta,
        });
    }

    mediated::touch(&mut item, options);
    let fingerprint = mediated::commit(loaded, &item, options)?;
    Ok(FieldOutcome {
        item,
        path: loaded.path.clone(),
        changed: true,
        written: fingerprint.is_some(),
        changed_fields,
        meta,
    })
}

/// Validate an entire change set against the project's declarations.
///
/// Everything is checked before the first entry is applied, so a caller learns
/// about all of its mistakes at once and a rejected set writes nothing.
fn validate_changes(project: &Project, changes: &[FieldChangeEntry]) -> Result<(), WaymarkError> {
    let declared = &project.config().fields;
    let mut issues: Vec<ValidationIssue> = Vec::new();

    // Group by key, in first-appearance order, so a key named twice is reported
    // once and the *set* of directions is visible.
    let mut order: Vec<&str> = Vec::new();
    let mut directions: std::collections::BTreeMap<&str, Vec<&FieldChange>> =
        std::collections::BTreeMap::new();
    for entry in changes {
        if !directions.contains_key(entry.key.as_str()) {
            order.push(entry.key.as_str());
        }
        directions
            .entry(entry.key.as_str())
            .or_default()
            .push(&entry.change);
    }

    for key in order {
        let path = format!("custom_fields.{key}");
        let parsed = match FieldKey::parse(key) {
            Ok(parsed) => parsed,
            Err(error) => {
                issues.push(ValidationIssue::new(
                    path,
                    format!("`{key}` is not a valid custom-field key: {error}"),
                ));
                continue;
            }
        };
        let Some(declaration) = declared.get(&parsed) else {
            issues.push(ValidationIssue::new(
                path,
                format!("`{key}` is not a declared custom field"),
            ));
            continue;
        };

        let entries = &directions[key];
        if entries.len() > 1 {
            let sets = entries
                .iter()
                .filter(|change| matches!(change, FieldChange::Set(_)))
                .count();
            let clears = entries
                .iter()
                .filter(|change| matches!(change, FieldChange::Clear))
                .count();
            issues.push(ValidationIssue::new(
                path,
                if sets > 0 && clears > 0 {
                    "the key appears in both directions; use exactly one set or clear".to_owned()
                } else {
                    format!(
                        "the key appears {} times; a change set may name it once",
                        entries.len()
                    )
                },
            ));
            continue;
        }

        match entries[0] {
            FieldChange::Null => issues.push(ValidationIssue::new(
                path,
                "null is not a custom-field value; use a clear entry to remove it",
            )),
            FieldChange::Clear if declaration.required => issues.push(ValidationIssue::new(
                path,
                format!("`{key}` is required and cannot be cleared"),
            )),
            FieldChange::Clear => {}
            FieldChange::Set(value) if !matches_declared_type(&declaration.ty, value) => {
                issues.push(ValidationIssue::new(
                    path,
                    format!(
                        "expected a value of type `{}`, found {}",
                        declaration.ty.as_str(),
                        value.type_name()
                    ),
                ));
            }
            FieldChange::Set(_) => {}
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(WaymarkError::validation("custom_fields", issues).at_field("custom_fields"))
    }
}

/// Whether `value` is exactly a value of `ty`, including a homogeneous list.
///
/// [`FieldType::accepts`] is the declaration's own rule and is reused rather than
/// restated, so the configuration validator and the edit operation cannot drift.
fn matches_declared_type(ty: &FieldType, value: &FieldValue) -> bool {
    ty.accepts(value)
}

/// Trim and validate a title, returning the value to persist.
fn normalize_title(title: &str) -> Result<String, WaymarkError> {
    let trimmed = title.trim();
    let mut issues: Vec<ValidationIssue> = Vec::new();
    if trimmed.is_empty() {
        issues.push(ValidationIssue::new("title", "a title is required"));
    }
    if trimmed.contains(['\n', '\r']) {
        issues.push(ValidationIssue::new(
            "title",
            "a title must be a single line, without line breaks",
        ));
    }
    if !issues.is_empty() {
        return Err(WaymarkError::validation("title", issues).at_field("title"));
    }
    Ok(trimmed.to_owned())
}

/// Commit `item` as the new state of `loaded`, or report that nothing changed.
///
/// The comparison is against the item that was read, so "no effective change"
/// means exactly "the document would be rewritten with the same bytes" — no
/// write, no `updated_at` move.
fn finish_edit(
    project: &Project,
    loaded: &Loaded,
    mut item: WorkItem,
    options: &OpOptions,
) -> Result<EditOutcome, WaymarkError> {
    let meta = mediated::write_meta(mediated::owner_warnings(project, &item, options));
    if item == loaded.item {
        return Ok(EditOutcome {
            item,
            path: loaded.path.clone(),
            changed: false,
            written: false,
            meta,
        });
    }
    mediated::touch(&mut item, options);
    let fingerprint = mediated::commit(loaded, &item, options)?;
    Ok(EditOutcome {
        item,
        path: loaded.path.clone(),
        changed: true,
        written: fingerprint.is_some(),
        meta,
    })
}
