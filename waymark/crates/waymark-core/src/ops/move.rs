//! `waymark move`: relocate one item to another category.
//!
//! A category is part of an item's persisted identity-for-humans but never part
//! of its stable ID. Moving therefore changes exactly two things:
//!
//! 1. the document lives in the target category's directory, computed by the same
//!    [`Project::path_for`](crate::store::Project::path_for) rule creation uses,
//!    so a moved item lands where a newly created item in that category would;
//! 2. the persisted `category` value becomes the target key.
//!
//! Everything else — body, owner, creator, status, custom fields, relationships,
//! archive state — is untouched. Because the persisted category changes, the
//! move moves `updated_at` from the operation clock; it is a content change, not
//! a pure relocation.
//!
//! # One all-or-nothing operation
//!
//! The sequence under the project lock is: validate the category, read the item
//! through [`mediated::load_for_write`](crate::ops::mediated::load_for_write),
//! refuse an occupied destination, verify the old document still matches the
//! fingerprint that was read, write the new document atomically at the
//! destination, then remove the old path. A failure before the destination write
//! leaves the old document byte-identical and creates nothing; a failure removing
//! the old path removes the newly written destination first, so the store never
//! ends up claiming one ID twice.
//!
//! # Refusals
//!
//! - Same category: a true no-op (`changed: false`, nothing written, no timestamp
//!   move). The persisted category is authoritative, so a document whose bytes
//!   say the target category is not relocated just because it sits elsewhere.
//! - Undeclared category: the same `VALIDATION_ERROR` issue creation reports.
//! - Occupied destination: `WRITE_CONFLICT` (`expected: "absent"`,
//!   `actual: "present"`), the registry code the write engine already uses for a
//!   target that must not be overwritten. `MIGRATION_PATH_CONFLICT` belongs to the
//!   migration vocabulary (`{target, conflict}`, exit class 6) and is not used
//!   here.
//! - Archived item: `ITEM_ARCHIVED`; migration-pending item:
//!   `ITEM_NEEDS_MIGRATION`.

use camino::Utf8PathBuf;

use crate::envelope::Meta;
use crate::error::WaymarkError;
use crate::model::{CategoryKey, WorkItem};
use crate::ops::create;
use crate::ops::mediated;
use crate::ops::OpOptions;
use crate::store::{Fingerprint, Project};

/// What one category move produced, or would produce on a dry run.
#[derive(Debug, Clone)]
pub struct MoveOutcome {
    /// The complete item as written, or as it would have been written on a dry
    /// run.
    pub item: WorkItem,
    /// The document the item lived in before the move.
    pub previous_path: Utf8PathBuf,
    /// The document the item lives in after the move.
    pub path: Utf8PathBuf,
    /// The category the item had before the move.
    pub previous_category: CategoryKey,
    /// Whether the category actually moved.
    pub changed: bool,
    /// Whether the store changed.
    pub written: bool,
    /// The fingerprint of the document written at `path`; `None` on a dry run or
    /// a no-op.
    pub fingerprint: Option<Fingerprint>,
    /// The `_meta` block this write justifies, including `OWNER_MISMATCH`.
    pub meta: Meta,
}

/// Move one item to `category`.
///
/// The category must already be declared in the project configuration; an
/// undeclared one is refused with the same `VALIDATION_ERROR` issue creation
/// reports for it. The destination file must not exist.
pub fn move_item(
    project: &Project,
    id: &str,
    category: &CategoryKey,
    options: &OpOptions,
) -> Result<MoveOutcome, WaymarkError> {
    // The declaration check needs no lock and never reads an item, so a rejected
    // category cannot make a writer wait for one.
    if let Some(issue) = create::category_issue(project, category.as_str()) {
        return Err(WaymarkError::validation("move", vec![issue]).at_field("category"));
    }

    crate::lock::with_project_lock(project, options.lock_wait, || {
        let loaded = mediated::load_for_write(project, id, "move")?;
        apply(project, &loaded, category, options)
    })
}

/// Compute, write, and clean up one move for an already-loaded item.
fn apply(
    project: &Project,
    loaded: &mediated::Loaded,
    category: &CategoryKey,
    options: &OpOptions,
) -> Result<MoveOutcome, WaymarkError> {
    let mut item = loaded.item.clone();
    let previous_path = loaded.path.clone();
    let previous_category = item.category.clone();
    let meta = mediated::write_meta(mediated::owner_warnings(project, &loaded.item, options));

    if previous_category == *category {
        // The persisted category is the authority: asking for the category the
        // document already declares is not a relocation, even if the file were
        // hand-placed in a different directory.
        return Ok(MoveOutcome {
            item,
            previous_path: previous_path.clone(),
            path: previous_path,
            previous_category,
            changed: false,
            written: false,
            fingerprint: None,
            meta,
        });
    }

    let path = project.path_for(&item.id, category)?;

    if path == loaded.path {
        // The document already sits at the destination path even though it
        // declares a different category (a hand-placed document, or a
        // configuration whose category paths touch). There is nothing to
        // relocate, but the persisted value still has to be rewritten — in
        // place, through the fingerprint-checked engine.
        item.category = category.clone();
        mediated::touch(&mut item, options);
        let fingerprint = mediated::commit(loaded, &item, options)?;
        return Ok(MoveOutcome {
            item,
            previous_path: loaded.path.clone(),
            path: loaded.path.clone(),
            previous_category,
            changed: true,
            written: fingerprint.is_some(),
            fingerprint,
            meta,
        });
    }

    // Refuse to overwrite whatever already claims the destination. The check
    // runs under the lock, before anything is staged.
    if path.exists() {
        return Err(WaymarkError::write_conflict(
            &item.id,
            path.to_string(),
            "absent",
            "present",
        ));
    }

    // The write engine's contract is "never replace bytes that were not read".
    // Here the bytes read live at the old path while the new document is written
    // at the new one, so the engine's `replace_checked` cannot express the check;
    // it is re-expressed immediately before the destination write, under the same
    // lock, with the same error shape.
    let current = project.fingerprint(&loaded.path)?;
    if current != loaded.fingerprint {
        return Err(WaymarkError::write_conflict(
            &loaded.item.id,
            loaded.path.to_string(),
            loaded.fingerprint.as_str(),
            current.as_str(),
        ));
    }

    item.category = category.clone();
    mediated::touch(&mut item, options);

    if options.dry_run {
        return Ok(MoveOutcome {
            item,
            previous_path,
            path,
            previous_category,
            changed: true,
            written: false,
            fingerprint: None,
            meta,
        });
    }

    let fingerprint = crate::serialize::write_item_file(&item, &path)?;

    if let Err(source) = std::fs::remove_file(&loaded.path) {
        // The store must never end up with two documents for one stable ID. The
        // destination is the only file this operation created, so dropping it
        // restores the pre-move state: the old document stays the single
        // authority, byte-identical.
        let _ = std::fs::remove_file(&path);
        return Err(WaymarkError::io(loaded.path.to_string(), "remove", &source));
    }

    Ok(MoveOutcome {
        item,
        previous_path,
        path,
        previous_category,
        changed: true,
        written: true,
        fingerprint: Some(fingerprint),
        meta,
    })
}
