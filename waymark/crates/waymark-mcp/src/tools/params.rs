//! Translation from MCP arguments to the shared request types.
//!
//! The MCP surface takes structured values where the CLI takes flags. The
//! vocabulary is the same one `waymark_core::query` defines: the filter
//! document is parsed by [`Filters::from_json`], the sort literal by
//! [`SortField::parse`], the archive range by [`ArchiveRange::parse`], and the
//! projected field names by [`FieldName::parse`]. No rule is re-implemented
//! here; a bad value is a domain `VALIDATION_ERROR`, not a protocol error.

use std::collections::BTreeMap;

use serde_json::Value;
use waymark_core::config::FieldType;
use waymark_core::error::{ValidationIssue, WaymarkError};
use waymark_core::model::{CategoryKey, FieldKey, FieldValue};
use waymark_core::ops::edit::FieldChangeEntry;
use waymark_core::ops::OpOptions;
use waymark_core::query::{
    ArchiveRange, FieldName, Filters, Projection, Query, Sort, SortField, Window, MCP_DEFAULT_LIMIT,
};

/// The MCP collection default, quoted in every collection tool's description.
pub const DEFAULT_LIMIT: usize = MCP_DEFAULT_LIMIT;

/// The production operation options, with the flags a tool asked for.
///
/// The clock and the effective operator are the core defaults.
/// `lock_timeout_ms` is the same per-operation override the CLI exposes as
/// `--lock-timeout-ms`; `OWNER_MISMATCH` is computed against the configured
/// operator exactly as the CLI computes it.
pub fn options(dry_run: bool, lock_timeout_ms: Option<u64>) -> OpOptions {
    let options =
        OpOptions::new().with_lock_wait(waymark_core::ops::lock_wait_from_millis(lock_timeout_ms));
    if dry_run {
        options.dry_run()
    } else {
        options
    }
}

/// The operation options of a write that can reach a second document.
///
/// `cascade` is the consent flag, the same one the CLI exposes as
/// `--cascade`; it defaults to off, so a tool that is not given it writes
/// nothing but its own target.
pub fn write_options(dry_run: bool, cascade: bool, lock_timeout_ms: Option<u64>) -> OpOptions {
    let options = options(dry_run, lock_timeout_ms);
    if cascade {
        options.with_cascade()
    } else {
        options
    }
}

/// A `VALIDATION_ERROR` about one adapter argument, in the shared shape.
///
/// The adapter performs the same checks the CLI performs before a domain call
/// (key grammars, type literals). It reuses the registry code and issue shape;
/// no rule or second taxonomy is introduced.
pub fn validation(field: &str, message: impl Into<String>) -> WaymarkError {
    WaymarkError::validation(field, vec![ValidationIssue::new(field, message.into())])
        .at_field(field)
}

/// Parse a category key from an adapter argument.
pub fn category_key(literal: &str) -> Result<CategoryKey, WaymarkError> {
    CategoryKey::parse(literal).map_err(|error| validation("category", error.to_string()))
}

/// Parse a custom-field key from an adapter argument.
pub fn field_key(literal: &str) -> Result<FieldKey, WaymarkError> {
    FieldKey::parse(literal).map_err(|error| validation("key", error.to_string()))
}

/// Parse a custom-field type from an adapter argument.
pub fn field_type(literal: &str) -> Result<FieldType, WaymarkError> {
    FieldType::parse(literal).map_err(|reason| validation("type", reason))
}

/// Translate a structured `set`/`clear` map into the core's change sequence.
///
/// The adapter does not decide whether a value fits the declaration: every
/// well-formed JSON value is handed to [`waymark_core::ops::edit`], which owns
/// the type check, the null rejection, the duplicate and conflict rules, and
/// the required-field rule. Only values JSON cannot express as a Waymark field
/// value (a float, an object) are refused here, because the core type has no
/// representation for them.
pub fn field_changes(
    set: &BTreeMap<String, Value>,
    clear: &[String],
) -> Result<Vec<FieldChangeEntry>, WaymarkError> {
    let mut changes = Vec::with_capacity(set.len() + clear.len());
    let mut issues = Vec::new();
    for (key, value) in set {
        if value.is_null() {
            // Preserved so the domain reports the explicit null itself.
            changes.push(FieldChangeEntry::null(key.clone()));
            continue;
        }
        match field_value(value) {
            Ok(value) => changes.push(FieldChangeEntry::set(key.clone(), value)),
            Err(reason) => issues.push(ValidationIssue::new(format!("fields.{key}"), reason)),
        }
    }
    for key in clear {
        changes.push(FieldChangeEntry::clear(key.clone()));
    }
    if !issues.is_empty() {
        return Err(WaymarkError::validation("custom fields", issues).at_field("custom_fields"));
    }
    Ok(changes)
}

/// Convert one JSON value into a Waymark custom-field value.
///
/// Shared with `waymark_create_work_item` so a created value and an updated
/// value accept exactly the same JSON shapes; the domain still owns the declared
/// type check.
pub(crate) fn field_value(value: &Value) -> Result<FieldValue, String> {
    match value {
        Value::String(text) => Ok(FieldValue::String(text.clone())),
        Value::Bool(flag) => Ok(FieldValue::Boolean(*flag)),
        Value::Number(number) => number
            .as_i64()
            .map(FieldValue::Integer)
            .ok_or_else(|| format!("expected a 64-bit integer, found {number}")),
        Value::Array(items) => items
            .iter()
            .map(field_value)
            .collect::<Result<Vec<_>, _>>()
            .map(FieldValue::List),
        Value::Null => Err("null is not a value; use `clear` to remove a field".to_owned()),
        Value::Object(_) => Err("expected a scalar or a list of scalars".to_owned()),
    }
}

/// Build the shared query from the pieces a collection tool accepts.
///
/// `archive` overrides `filter.archive` when both are present: the top-level
/// argument is the explicit request, and silently mixing two archive ranges
/// would make "active" or "all" mean nothing. Everything else is the filter
/// document's business.
pub fn query_from(
    filter: Option<&Value>,
    archive: Option<&str>,
    sort: Option<&str>,
    descending: bool,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Query, WaymarkError> {
    let mut filters = match filter {
        Some(document) => Filters::from_json(document)?,
        None => Filters::new(),
    };
    if let Some(literal) = archive {
        filters.archive = archive_range(literal)?;
    }
    Ok(Query {
        filters,
        sort: Sort {
            field: match sort {
                Some(literal) => SortField::parse(literal)?,
                None => SortField::Rank,
            },
            descending,
        },
        window: Window {
            limit: limit.unwrap_or(DEFAULT_LIMIT),
            offset: offset.unwrap_or(0),
        },
    })
}

/// Parse one archive-range literal.
///
/// The error is `VALIDATION_ERROR` with the field named, which is the same
/// classification the CLI produces for `--archive`.
pub fn archive_range(literal: &str) -> Result<ArchiveRange, WaymarkError> {
    ArchiveRange::parse(literal).ok_or_else(|| {
        WaymarkError::validation(
            "archive",
            vec![ValidationIssue::new(
                "archive",
                format!("{literal:?} is not `active`, `archived`, or `all`"),
            )],
        )
        .at_field("archive")
    })
}

/// Build the row projection.
///
/// `fields` and `compact` are mutually exclusive, exactly as `--fields` and
/// `--compact` are on the CLI. An empty `fields` list is rejected instead of
/// silently meaning "everything".
pub fn projection(fields: Option<&[String]>, compact: bool) -> Result<Projection, WaymarkError> {
    match (fields, compact) {
        (Some(_), true) => Err(WaymarkError::validation(
            "projection",
            vec![ValidationIssue::new(
                "fields",
                "`fields` and `compact` are mutually exclusive",
            )],
        )
        .at_field("fields")),
        (Some(fields), false) => {
            if fields.is_empty() {
                return Err(WaymarkError::validation(
                    "projection",
                    vec![ValidationIssue::new(
                        "fields",
                        "`fields` must name at least one projected field",
                    )],
                )
                .at_field("fields"));
            }
            let mut names = Vec::with_capacity(fields.len());
            for literal in fields {
                let field = FieldName::parse(literal).ok_or_else(|| {
                    WaymarkError::validation(
                        "projection",
                        vec![ValidationIssue::new(
                            "fields",
                            format!(
                                "{literal:?} is not a projected field; expected one of {}",
                                FieldName::ALL
                                    .iter()
                                    .map(|field| field.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        )],
                    )
                    .at_field("fields")
                })?;
                names.push(field);
            }
            Ok(Projection::of(names))
        }
        (None, true) => Ok(Projection::compact()),
        (None, false) => Ok(Projection::all()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_mcp_default_limit_is_the_shared_fifty() {
        assert_eq!(DEFAULT_LIMIT, 50);
        let query = query_from(None, None, None, false, None, None).unwrap();
        assert_eq!(query.window.limit, 50);
        assert_eq!(query.window.offset, 0);
        assert_eq!(query.sort, Sort::rank());
        assert_eq!(query.filters, Filters::new());
    }

    #[test]
    fn the_filter_document_owns_every_filter_and_the_archive_argument_wins() {
        let document = json!({"status": ["open"], "owner": "frank", "archive": "all"});
        let query = query_from(
            Some(&document),
            Some("archived"),
            Some("title"),
            true,
            Some(0),
            Some(3),
        )
        .unwrap();
        assert_eq!(query.filters.owner.as_deref(), Some("frank"));
        assert_eq!(query.filters.statuses.len(), 1);
        assert_eq!(query.filters.archive, ArchiveRange::Archived);
        assert_eq!(query.sort.field, SortField::Title);
        assert!(query.sort.descending);
        assert_eq!(query.window.limit, 0);
        assert_eq!(query.window.offset, 3);
    }

    #[test]
    fn a_bad_filter_is_a_domain_validation_error() {
        let document = json!({"status": "doing"});
        let error = query_from(Some(&document), None, None, false, None, None).unwrap_err();
        assert_eq!(error.code(), waymark_core::Code::ValidationError);
        assert!(error.details().contains_key("issues"));
    }

    #[test]
    fn a_bad_archive_literal_is_a_domain_validation_error() {
        let error = archive_range("retired").unwrap_err();
        assert_eq!(error.code(), waymark_core::Code::ValidationError);
        assert_eq!(
            error.location(),
            Some(&waymark_core::Location::field("archive"))
        );
    }

    #[test]
    fn fields_and_compact_cannot_be_combined() {
        let fields = vec!["id".to_owned()];
        let error = projection(Some(&fields), true).unwrap_err();
        assert_eq!(error.code(), waymark_core::Code::ValidationError);
        let projected = projection(Some(&["id".to_owned(), "title".to_owned()]), false).unwrap();
        assert!(projected.fields.as_ref().unwrap().len() == 2);
        assert!(projection(None, true).unwrap().compact);
    }
}
