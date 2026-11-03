//! The shape every *existing-item* mediated write shares.
//!
//! A lifecycle change and a relationship change are different domain rules, but
//! they commit the same way: resolve the ID, read the document under the store
//! lock, refuse an archived target, compute the new document, compare it to the
//! bytes that were read, and replace the file atomically. That sequence is
//! written once, here, so a later write family (title, body, custom fields,
//! parent, soft links, owner transfer) reuses it instead of re-deriving the
//! order — and so the `ITEM_ARCHIVED` and `OWNER_MISMATCH` rules cannot drift
//! between commands.
//!
//! Nothing here decides *what* a valid change is: that is the caller's rule.

use camino::Utf8PathBuf;

use crate::envelope::{Meta, Warning};
use crate::error::WaymarkError;
use crate::id::WorkItemId;
use crate::model::WorkItem;
use crate::ops::OpOptions;
use crate::store::{Fingerprint, InventoryEntry, Project};
use crate::time::Timestamp;

/// One item read for a write, together with the facts the commit needs.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// The parsed document.
    pub item: WorkItem,
    /// The inventory entry it came from.
    pub entry: InventoryEntry,
    /// The fingerprint of the exact bytes that were parsed.
    pub fingerprint: Fingerprint,
    /// The document path, for convenience.
    pub path: Utf8PathBuf,
}

impl Loaded {
    /// The item's stable ID.
    pub fn id(&self) -> &WorkItemId {
        &self.item.id
    }
}

/// Resolve `id` and read it for a mediated write.
///
/// The ID literal may be any case. A missing item is `ITEM_NOT_FOUND`, a
/// document whose own ID disagrees with its file name is `ID_PATH_MISMATCH`, and
/// an archived item is `ITEM_ARCHIVED` naming `operation`.
pub fn load_for_write(
    project: &Project,
    id: &str,
    operation: &str,
) -> Result<Loaded, WaymarkError> {
    let id = crate::ops::read::normalize(id)?;
    let entry = project.locate(&id)?;
    load_entry(project, entry, operation)
}

/// Read an already-located item for a mediated write.
pub fn load_entry(
    project: &Project,
    entry: InventoryEntry,
    operation: &str,
) -> Result<Loaded, WaymarkError> {
    let bytes = project.read_bytes(&entry.path)?;
    let item = crate::parse::parse_item_file(&entry.path)?;
    if item.id != entry.id {
        let stem = entry.path.file_stem().unwrap_or_default();
        return Err(WaymarkError::id_path_mismatch(&item.id, &entry.path, stem));
    }
    if item.is_archived() {
        return Err(WaymarkError::item_archived(&item.id, operation));
    }
    Ok(Loaded {
        item,
        path: entry.path.clone(),
        fingerprint: Fingerprint::of(&bytes),
        entry,
    })
}

/// The effective operator identity: the operation override, else the project
/// configuration.
pub fn effective_operator<'a>(project: &'a Project, options: &'a OpOptions) -> Option<&'a str> {
    options.operator().or(project.config().operator.as_deref())
}

/// The `OWNER_MISMATCH` warnings this write justifies.
///
/// The mismatch is advisory: the caller attaches these to the result's `_meta`
/// and still writes. A project with no declared operator produces no warning,
/// because there is no identity to compare.
pub fn owner_warnings(project: &Project, item: &WorkItem, options: &OpOptions) -> Vec<Warning> {
    match effective_operator(project, options) {
        Some(operator) if operator != item.owner => vec![Warning::from_error(
            &WaymarkError::owner_mismatch(&item.id, operator, &item.owner),
        )],
        _ => Vec::new(),
    }
}

/// The `_meta` block for a single-document write result.
pub fn write_meta(warnings: Vec<Warning>) -> Meta {
    Meta::direct_file_scoped().with_warnings(warnings)
}

/// Stamp `updated_at` from the operation clock and return the instant used.
pub fn touch(item: &mut WorkItem, options: &OpOptions) -> Timestamp {
    let now = Timestamp::new(options.clock.now());
    item.updated_at = now;
    now
}

/// Replace the document at `loaded.path` with the canonical bytes of `item`,
/// but only if its fingerprint still matches what was read.
///
/// Returns `None` on a dry run, where nothing is written.
pub fn commit(
    loaded: &Loaded,
    item: &WorkItem,
    options: &OpOptions,
) -> Result<Option<Fingerprint>, WaymarkError> {
    if options.dry_run {
        return Ok(None);
    }
    let document = crate::serialize::to_document(item);
    crate::write::replace_checked(
        &loaded.path,
        document.as_bytes(),
        &loaded.fingerprint,
        &item.id,
    )
    .map(Some)
}
