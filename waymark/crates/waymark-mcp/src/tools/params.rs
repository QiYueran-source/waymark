//! Translation from MCP arguments to the shared request types.
//!
//! The MCP surface takes structured values where the CLI takes flags. The
//! vocabulary is the same one `waymark_core::query` defines: the filter
//! document is parsed by [`Filters::from_json`], the sort literal by
//! [`SortField::parse`], the archive range by [`ArchiveRange::parse`], and the
//! projected field names by [`FieldName::parse`]. No rule is re-implemented
//! here; a bad value is a domain `VALIDATION_ERROR`, not a protocol error.

use serde_json::Value;
use waymark_core::error::{ValidationIssue, WaymarkError};
use waymark_core::query::{
    ArchiveRange, FieldName, Filters, Projection, Query, Sort, SortField, Window, MCP_DEFAULT_LIMIT,
};

/// The MCP collection default, quoted in every collection tool's description.
pub const DEFAULT_LIMIT: usize = MCP_DEFAULT_LIMIT;

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
