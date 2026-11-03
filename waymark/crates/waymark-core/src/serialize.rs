//! The canonical document writer.
//!
//! The bytes on disk are a contract: one fixed key order, unset optional fields
//! omitted (never `null`), relationship lists sorted and de-duplicated, and one
//! canonical body newline rule. A general YAML serializer cannot promise any of
//! that across versions, so the *structure* is hand-rolled here.
//!
//! YAML *quoting* is not hand-rolled: each free-text scalar is rendered by
//! [`serde_norway`], which decides plain vs single- vs double-quoted correctly.
//! The library renders one scalar at a time; the ordering — the diff-critical
//! part — stays under our control.
//!
//! Re-serializing an unchanged [`WorkItem`] is byte-identical, and parsing the
//! output reproduces the item. Both are tested.

use std::io::{self, Write};

use camino::Utf8Path;

use crate::id::WorkItemId;
use crate::model::{CustomFields, FieldValue, WorkItem};
use crate::time::format_utc;

/// Render one work item as its complete canonical document, body included.
pub fn to_document(item: &WorkItem) -> String {
    let mut buffer: Vec<u8> = Vec::new();
    write_document(item, &mut buffer).expect("writing to a Vec cannot fail");
    String::from_utf8(buffer).expect("the canonical writer only emits UTF-8")
}

/// Render one work item into `sink`.
///
/// The body is canonicalized: an empty body emits nothing after the closing
/// fence, a non-empty body emits exactly one trailing newline with its leading
/// and internal bytes untouched.
pub fn write_document<W: Write>(item: &WorkItem, sink: &mut W) -> io::Result<()> {
    writeln!(sink, "---")?;

    // Always-present fields, in the pinned order.
    writeln!(sink, "schema: {}", item.schema)?;
    writeln!(sink, "id: {}", item.id)?; // grammar-safe: always plain
    writeln!(sink, "title: {}", render_scalar(&item.title))?;
    writeln!(sink, "category: {}", item.category)?; // grammar-safe: always plain
    writeln!(sink, "status: {}", item.status.as_str())?;
    writeln!(sink, "owner: {}", render_scalar(&item.owner))?;
    if let Some(creator) = &item.creator {
        writeln!(sink, "creator: {}", render_scalar(creator))?;
    }
    writeln!(sink, "created_at: {}", format_utc(item.created_at.get()))?;
    writeln!(sink, "updated_at: {}", format_utc(item.updated_at.get()))?;

    // Optional scalars; absent means omitted.
    if let Some(closed_at) = item.closed_at {
        writeln!(sink, "closed_at: {}", format_utc(closed_at.get()))?;
    }
    if let Some(parent) = &item.parent {
        writeln!(sink, "parent: {parent}")?; // grammar-safe: always plain
    }

    // Relationship lists: omitted when empty, else sorted and de-duplicated.
    write_id_list(sink, "dependencies", &item.dependencies)?;
    write_id_list(sink, "soft_links", &item.soft_links)?;
    write_custom_fields(sink, &item.custom_fields)?;

    if let Some(archived_at) = item.archived_at {
        writeln!(sink, "archived_at: {}", format_utc(archived_at.get()))?;
    }
    if let Some(reason) = &item.archive_reason {
        writeln!(sink, "archive_reason: {}", render_scalar(reason))?;
    }

    writeln!(sink, "---")?;
    sink.write_all(canonical_body(&item.body).as_bytes())
}

/// The canonical form of a body: zero bytes when it holds no content, otherwise
/// exactly one trailing newline with leading and internal whitespace untouched.
///
/// "No content" means empty or newline-only. Anything else is content and keeps
/// its exact bytes apart from the trailing run of newlines, which collapses to
/// one.
pub fn canonical_body(body: &str) -> String {
    // Line endings are part of "canonical": a CRLF file reads successfully, but
    // the store keeps one representation, so a Windows checkout and a Linux one
    // do not fight over the bytes.
    let normalized = body.replace("\r\n", "\n");
    let trimmed = normalized.trim_end_matches('\n');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}

/// Render a free-text scalar with correct YAML quoting, delegated to
/// [`serde_norway`].
///
/// A multi-line rendering becomes one double-quoted line: every frontmatter key
/// must stay on one line, and a `|-` block scalar cannot be emitted inside the
/// indented `custom_fields:` mapping without re-indenting every following line.
/// Titles are single-line by validation; this keeps the writer total for
/// free-form values such as `archive_reason`.
fn render_scalar(value: &str) -> String {
    let rendered =
        serde_norway::to_string(value).expect("serializing a string scalar to YAML cannot fail");
    let rendered = rendered.trim_end_matches('\n');
    if rendered.contains('\n') {
        double_quote(value)
    } else {
        rendered.to_owned()
    }
}

/// A minimal double-quoted YAML scalar, used only for multi-line values.
fn double_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// `key:` followed by a block sequence of sorted, de-duplicated IDs, or nothing
/// at all when the list is empty.
///
/// Block form (rather than `[a, b]` flow form) puts every ID on its own line, so
/// a relationship change is a one-line diff and no flow-context quoting rule can
/// corrupt a value.
fn write_id_list<W: Write>(sink: &mut W, key: &str, ids: &[WorkItemId]) -> io::Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let mut values: Vec<&str> = ids.iter().map(WorkItemId::as_str).collect();
    values.sort_unstable();
    values.dedup();
    writeln!(sink, "{key}:")?;
    for value in values {
        writeln!(sink, "  - {value}")?;
    }
    Ok(())
}

/// `custom_fields:` with its keys in sorted order (the map is a `BTreeMap`, so
/// iteration order is already canonical).
fn write_custom_fields<W: Write>(sink: &mut W, fields: &CustomFields) -> io::Result<()> {
    if fields.is_empty() {
        return Ok(());
    }
    writeln!(sink, "custom_fields:")?;
    for (key, value) in fields {
        match value {
            FieldValue::String(text) => writeln!(sink, "  {key}: {}", render_scalar(text))?,
            FieldValue::Integer(number) => writeln!(sink, "  {key}: {number}")?,
            FieldValue::Boolean(flag) => writeln!(sink, "  {key}: {flag}")?,
            FieldValue::List(values) if values.is_empty() => writeln!(sink, "  {key}: []")?,
            FieldValue::List(values) => {
                writeln!(sink, "  {key}:")?;
                for element in values {
                    writeln!(sink, "    - {}", render_scalar_value(element))?;
                }
            }
        }
    }
    Ok(())
}

/// Render one custom-field scalar inside a block sequence.
fn render_scalar_value(value: &FieldValue) -> String {
    match value {
        FieldValue::String(text) => render_scalar(text),
        FieldValue::Integer(number) => number.to_string(),
        FieldValue::Boolean(flag) => flag.to_string(),
        // Nested lists are rejected by the parser and never produced by an
        // operation, so reaching this arm is a programming fault.
        FieldValue::List(_) => double_quote("invalid nested list"),
    }
}

/// Write `item` to `path` through the atomic replacement engine.
///
/// The caller must hold the project write lock; this is the file half of a
/// mediated write, not a public operation.
pub fn write_item_file(
    item: &WorkItem,
    path: &Utf8Path,
) -> Result<crate::write::Fingerprint, crate::error::WaymarkError> {
    let document = to_document(item);
    crate::write::replace_unchecked(path, document.as_bytes())
}
