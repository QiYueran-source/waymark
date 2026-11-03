//! `waymark show`: read one work item by its stable ID.
//!
//! A read never writes, never takes the write lock, and never consults a cache in
//! this ticket: the bytes on disk are the answer, so the result is always
//! `source: direct_file`, `completeness: scoped`, `freshness: fresh`.
//!
//! # Identity resolution
//!
//! The stable ID is normalized to lowercase before comparison. A duplicate claim
//! (two files with the same ID) or an ID whose file stem disagrees with the
//! document's own `id` is an error: the store never picks one arbitrarily.

use camino::Utf8PathBuf;

use crate::envelope::Meta;
use crate::error::{ValidationIssue, WaymarkError};
use crate::id::WorkItemId;
use crate::model::WorkItem;
use crate::store::{Fingerprint, InventoryEntry, Project};

/// One work item read from disk, with the facts a caller needs to describe where
/// it came from.
#[derive(Debug, Clone)]
pub struct ReadItem {
    /// The document.
    pub item: WorkItem,
    /// The document path.
    pub path: Utf8PathBuf,
    /// The fingerprint of the bytes that were read.
    pub fingerprint: Fingerprint,
    /// The category directory the document was found in.
    pub category: crate::model::CategoryKey,
    /// The `_meta` block this read justifies.
    pub meta: Meta,
}

/// Read one item by stable ID.
///
/// The ID is accepted in its canonical form only: `wm-` plus twelve Crockford
/// Base32 characters, compared case-insensitively. Anything else is
/// `INVALID_ID` rather than a not-found.
pub fn read(project: &Project, id: &str) -> Result<ReadItem, WaymarkError> {
    let id = normalize(id)?;
    let entry = project.locate(&id)?;
    read_entry(project, &entry, &id)
}

/// Read the item at `path`, without consulting the inventory.
///
/// Used by adapters that already resolved a path (an editor integration, a Web
/// request). The document's own `id` must match its file stem.
pub fn read_path(project: &Project, path: &camino::Utf8Path) -> Result<ReadItem, WaymarkError> {
    let bytes = project.read_bytes(path)?;
    let item = crate::parse::parse_item_file(path)?;
    let category = project.category_for_path(path).ok_or_else(|| {
        WaymarkError::config_invalid(
            project.config_path().to_string(),
            vec![format!(
                "{path} is not inside any configured category directory"
            )],
        )
    })?;
    Ok(ReadItem {
        path: path.to_path_buf(),
        fingerprint: Fingerprint::of(&bytes),
        category,
        meta: Meta::direct_file_scoped(),
        item,
    })
}

fn read_entry(
    project: &Project,
    entry: &InventoryEntry,
    id: &WorkItemId,
) -> Result<ReadItem, WaymarkError> {
    let bytes = project.read_bytes(&entry.path)?;
    let item = crate::parse::parse_item_file(&entry.path)?;
    if item.id != *id {
        return Err(WaymarkError::validation(
            format!("work item {id}"),
            vec![ValidationIssue::new(
                "id",
                format!("the document at {} declares {}", entry.path, item.id),
            )],
        ));
    }
    Ok(ReadItem {
        item,
        path: entry.path.clone(),
        fingerprint: Fingerprint::of(&bytes),
        category: entry.category.clone(),
        meta: Meta::direct_file_scoped(),
    })
}

/// Normalize a caller-supplied ID literal.
///
/// The spec persists IDs lowercase and compares case-insensitively, so `WM-…` is
/// the same item. Nothing else is lenient: the prefix and the twelve-character
/// Crockford body are required.
pub fn normalize(input: &str) -> Result<WorkItemId, WaymarkError> {
    let trimmed = input.trim();
    WorkItemId::parse(trimmed).map_err(|error| {
        WaymarkError::invalid_id(trimmed, &error).with_location(crate::Location::field("id"))
    })
}
