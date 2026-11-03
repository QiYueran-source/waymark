//! `waymark owner transfer`: replace the accountable owner of one item.
//!
//! Owner is a required core field with exactly one writer besides creation: this
//! operation. Transfer is a full replacement, never a patch, and it moves nothing
//! else:
//!
//! - the new identity is trimmed and validated exactly as creation validates an
//!   owner (non-empty, single line);
//! - setting the identity the item already carries is a true no-op — `changed` is
//!   false, no file is written, and `updated_at` does not move;
//! - a real transfer updates `updated_at` from the operation clock;
//! - `creator`, the body, custom fields, relationships, and status are untouched.
//!
//! The write itself is a [`mediated`](crate::ops::mediated) write, so an archived
//! item is `ITEM_ARCHIVED`, an item needing migration is `ITEM_NEEDS_MIGRATION`,
//! and the replacement is fingerprint-checked and atomic under the project lock.
//!
//! # Why the mismatch warning is computed before the change
//!
//! The advisory `OWNER_MISMATCH` warning compares the operator with the item's
//! *current* owner — the identity that initiated the transfer. Computing it after
//! the replacement would warn the legitimate outgoing owner merely for handing
//! the item over, so the warning is derived from the document that was read, not
//! from the document being written.

use camino::Utf8PathBuf;

use crate::envelope::Meta;
use crate::error::WaymarkError;
use crate::model::WorkItem;
use crate::ops::create;
use crate::ops::mediated;
use crate::ops::OpOptions;
use crate::store::{Fingerprint, Project};

/// What one owner transfer produced.
#[derive(Debug, Clone)]
pub struct OwnerOutcome {
    /// The complete item as written, or as it would have been written on a dry
    /// run.
    pub item: WorkItem,
    /// The owner the item had before the operation.
    pub previous_owner: String,
    /// The document the item lives in (a transfer never moves a file).
    pub path: Utf8PathBuf,
    /// Whether the owner actually moved.
    pub changed: bool,
    /// Whether the document on disk changed.
    pub written: bool,
    /// The fingerprint of the written document; `None` on a dry run or a no-op.
    pub fingerprint: Option<Fingerprint>,
    /// The `_meta` block this write justifies, including `OWNER_MISMATCH`.
    pub meta: Meta,
}

/// Replace the owner of one item with `owner`.
///
/// The identity is trimmed at the edges; an empty, whitespace-only, or multi-line
/// identity is `VALIDATION_ERROR` and nothing is read or written. The transfer
/// goes through [`mediated::load_for_write`], so archived and migration-pending
/// documents are refused with the same codes every other mediated write uses.
pub fn transfer_owner(
    project: &Project,
    id: &str,
    owner: &str,
    options: &OpOptions,
) -> Result<OwnerOutcome, WaymarkError> {
    let owner = owner.trim();
    if let Some(issue) = create::owner_issue(owner) {
        return Err(WaymarkError::validation("owner", vec![issue]).at_field("owner"));
    }
    let owner = owner.to_owned();

    crate::lock::with_project_lock(project, options.lock_wait, || {
        let loaded = mediated::load_for_write(project, id, "owner transfer")?;
        apply(project, &loaded, owner, options)
    })
}

/// Compute and commit the replacement for an already-loaded item.
fn apply(
    project: &Project,
    loaded: &mediated::Loaded,
    owner: String,
    options: &OpOptions,
) -> Result<OwnerOutcome, WaymarkError> {
    let mut item = loaded.item.clone();
    let previous_owner = item.owner.clone();

    // The warning belongs to the document that was read: the operator is
    // compared with the owner who is handing the item over, not with its new
    // owner.
    let meta = mediated::write_meta(mediated::owner_warnings(project, &loaded.item, options));

    if previous_owner == owner {
        // Repeating the current owner is not a write: the item comes back
        // unchanged, `updated_at` stays where it was, and no file is touched.
        return Ok(OwnerOutcome {
            item,
            previous_owner,
            path: loaded.path.clone(),
            changed: false,
            written: false,
            fingerprint: None,
            meta,
        });
    }

    item.owner = owner;
    mediated::touch(&mut item, options);
    let fingerprint = mediated::commit(loaded, &item, options)?;
    Ok(OwnerOutcome {
        item,
        previous_owner,
        path: loaded.path.clone(),
        changed: true,
        written: fingerprint.is_some(),
        fingerprint,
        meta,
    })
}
