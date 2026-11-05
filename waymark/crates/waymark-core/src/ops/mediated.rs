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
///
/// An archived item is `ITEM_ARCHIVED` naming `operation`; a document whose
/// generation is older than this release's is `ITEM_NEEDS_MIGRATION`.
pub fn load_entry(
    project: &Project,
    entry: InventoryEntry,
    operation: &str,
) -> Result<Loaded, WaymarkError> {
    let loaded = load_entry_any(project, entry)?;
    if loaded.item.is_archived() {
        return Err(WaymarkError::item_archived(&loaded.item.id, operation));
    }
    require_current_generation(project, &loaded.item)?;
    Ok(loaded)
}

/// Refuse an item whose format generation predates what this release writes.
///
/// Ordinary writes never upgrade an older-generation file, so every mediated
/// write (including archive) refuses one until an explicit version migration
/// rewrites it. The error names the item's generation and the project's.
pub fn require_current_generation(project: &Project, item: &WorkItem) -> Result<(), WaymarkError> {
    if !item.needs_migration() {
        return Ok(());
    }
    Err(WaymarkError::item_needs_migration(
        &item.id,
        item.schema,
        project.config().version.generation,
    ))
}

/// Read an already-located item **without** the archived rejection.
///
/// This exists for archive alone: every other mediated write must refuse an
/// archived item, while archive has to see the marker to answer
/// `ALREADY_ARCHIVED` instead of a generic refusal.
pub fn load_entry_any(project: &Project, entry: InventoryEntry) -> Result<Loaded, WaymarkError> {
    let bytes = project.read_bytes(&entry.path)?;
    let item = crate::parse::parse_item_file(&entry.path)?;
    if item.id != entry.id {
        let stem = entry.path.file_stem().unwrap_or_default();
        return Err(WaymarkError::id_path_mismatch(&item.id, &entry.path, stem));
    }
    Ok(Loaded {
        item,
        path: entry.path.clone(),
        fingerprint: Fingerprint::of(&bytes),
        entry,
    })
}

/// The effective operator identity: the operation override, else the local
/// document, else Git's effective `user.name`.
///
/// Resolution belongs to [`crate::user_config`], not here: this is only the
/// operation-override layer the spec puts in front of the local document. Note
/// that the operation override is *not* written anywhere — it applies to this
/// operation alone.
pub fn effective_operator(project: &Project, options: &OpOptions) -> Option<String> {
    if let Some(operator) = options.operator() {
        return Some(operator.to_owned());
    }
    crate::ops::local_config::effective_operator(project)
}

/// The operator an operation is about to *persist*, resolved strictly.
///
/// Provenance recorded in a document must not be invented from a fallback while
/// the local document that should have supplied it is unreadable, so this
/// propagates the document's error instead of degrading the way the advisory
/// [`effective_operator`] does.
pub fn operator_to_record(
    project: &Project,
    options: &OpOptions,
) -> Result<Option<String>, WaymarkError> {
    if let Some(operator) = options.operator() {
        return Ok(Some(operator.to_owned()));
    }
    Ok(crate::ops::local_config::bundle(project)?
        .effective
        .operator)
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
///
/// The name says what it does: this is a timestamp write, not one of the
/// spec's "touches" (a frontmatter query, mutation, or archive).
pub fn stamp_updated_at(item: &mut WorkItem, options: &OpOptions) -> Timestamp {
    let now = Timestamp::new(options.clock.now());
    item.updated_at = now;
    now
}

/// Compatibility name for [`stamp_updated_at`].
///
/// `ops::move` is outside this change's file ownership, so it still calls
/// `touch`; every other call site uses the honest name. Remove this shim when
/// `ops::move` can be updated.
#[doc(hidden)]
pub fn touch(item: &mut WorkItem, options: &OpOptions) -> Timestamp {
    stamp_updated_at(item, options)
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
