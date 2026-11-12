//! `waymark link add|remove`: soft links between items.
//!
//! A soft link is a *symmetric, non-blocking* association: two items point at
//! each other and neither waits for nor constrains the other.
//!
//! # Consent decides how many documents are written
//!
//! The association is stored on both sides, but writing the second side is a
//! cross-item write. Adding a link without [`OpOptions::cascade`] therefore
//! records it on the document the caller named and leaves the partner exactly
//! as it is — bytes and `updated_at` included — and the outcome says so by
//! having no `other`. The pair is then *one-sided*: a normal, reportable state
//! that `doctor` and `show` diagnose and that converges when each side records
//! its own half. With `cascade`, both documents are rewritten as one batch —
//! see [`crate::write::replace_batch`] for the guarantee, which is what makes a
//! two-sided change all-or-nothing.
//!
//! This operation performs no ownership check of its own: consent alone decides.
//!
//! # Repeating a non-consenting add
//!
//! A second `link add` from the side that already records the link is not a
//! request to refuse when the pair is still one-sided: what the caller asked
//! for is already true, and the missing half is the other side's to record. It
//! succeeds with `changed: false`, `written: false`, and `other: None`, so a
//! retry neither hides the one-sided state nor restamps `updated_at`. Only a
//! repeat that leaves nothing to do is refused: a `--cascade` repeat, and a
//! plain repeat of a pair whose two sides already record each other.
//!
//! # What is rejected
//!
//! - a self-link (`INVALID_RELATIONSHIP`);
//! - a link that already exists on both sides, when only the caller's side
//!   asking is not enough — a `--cascade` repeat of a complete link
//!   (`INVALID_RELATIONSHIP`), and a plain repeat from the side that does not
//!   record it yet;
//! - a target that does not exist (`ITEM_NOT_FOUND`) or is archived
//!   (`ITEM_ARCHIVED`);
//! - an archived source (`ITEM_ARCHIVED`).
//!
//! # Removal is deliberately more permissive
//!
//! Removing a link that is not there is an `INVALID_RELATIONSHIP`: the caller
//! named an association that does not exist. But when the *other* document is
//! gone or archived — an external deletion, or an archive — only the surviving
//! side is cleaned up. An archived document is never rewritten, because archive
//! is a tombstone and its relationship fields are historical.

use crate::envelope::Meta;
use crate::error::WaymarkError;
use crate::model::WorkItem;
use crate::ops::mediated::{self, Loaded};
use crate::ops::OpOptions;
use crate::store::{Fingerprint, Project};
use crate::WorkItemId;

/// The relation name this operation edits, as it appears in documents and errors.
pub const SOFT_LINKS: &str = "soft_links";

/// What a soft-link change produced.
#[derive(Debug, Clone)]
pub struct LinkOutcome {
    /// The item the caller named, as written or as it would have been written.
    pub item: WorkItem,
    /// The other end, as written; `None` when its document was not written —
    /// without consent, when it was missing, or on a repeat that changed
    /// nothing.
    pub other: Option<WorkItem>,
    /// The other end's stable ID, whether or not its document was written.
    pub other_id: WorkItemId,
    /// Whether this `add` left the pair one-sided: the caller's document
    /// records the link and this operation did not write the other end.
    /// Always `false` for `remove`.
    pub partner_unsynchronised: bool,
    /// The document of the item the caller named.
    pub path: camino::Utf8PathBuf,
    /// The document of the other end, when it was written too.
    pub other_path: Option<camino::Utf8PathBuf>,
    /// Whether the persisted lists actually changed.
    pub changed: bool,
    /// Whether the documents on disk changed.
    pub written: bool,
    /// One fingerprint per written document, in `path`, `other_path` order;
    /// empty on a dry run.
    pub fingerprints: Vec<Fingerprint>,
    /// The `_meta` block this write justifies.
    pub meta: Meta,
}

/// Link two items to each other.
pub fn add_soft_link(
    project: &Project,
    id: &str,
    other: &str,
    options: &OpOptions,
) -> Result<LinkOutcome, WaymarkError> {
    crate::lock::with_project_lock(project, options.lock_wait, || {
        let left = mediated::load_for_write(project, id, "link add")?;
        let right_id = crate::ops::read::normalize(other)?;
        if &right_id == left.id() {
            return Err(WaymarkError::invalid_relationship(
                SOFT_LINKS,
                left.id(),
                &right_id,
                "an item cannot be soft-linked to itself",
            ));
        }
        // The caller's side already holds the link. With consent that is the
        // duplicate the caller asked about; without it the pair may still be
        // one-sided, and a retry has to answer with that state instead of an
        // error that hides it.
        if let Some(outcome) = repeat(project, &left, &right_id, options)? {
            return Ok(outcome);
        }
        let right = load_partner(project, &right_id, "link add")?;
        apply_add(project, left, right, options)
    })
}

/// Remove the link between two items.
pub fn remove_soft_link(
    project: &Project,
    id: &str,
    other: &str,
    options: &OpOptions,
) -> Result<LinkOutcome, WaymarkError> {
    crate::lock::with_project_lock(project, options.lock_wait, || {
        let left = mediated::load_for_write(project, id, "link remove")?;
        let right_id = crate::ops::read::normalize(other)?;
        if &right_id == left.id() {
            return Err(WaymarkError::invalid_relationship(
                SOFT_LINKS,
                left.id(),
                &right_id,
                "an item cannot be soft-linked to itself",
            ));
        }
        if !left.item.soft_links.contains(&right_id) {
            return Err(WaymarkError::invalid_relationship(
                SOFT_LINKS,
                left.id(),
                &right_id,
                "the item has no such soft link",
            ));
        }
        match load_partner(project, &right_id, "link remove") {
            Ok(right) => apply_remove(project, left, Some(right), &right_id, options),
            // A missing or archived partner leaves the surviving side as the
            // only document to clean up.
            Err(error)
                if matches!(
                    error.code(),
                    crate::error::Code::ItemNotFound | crate::error::Code::ItemArchived
                ) =>
            {
                apply_remove(project, left, None, &right_id, options)
            }
            Err(error) => Err(error),
        }
    })
}

/// Read the other end of a link for writing.
///
/// A source that does not exist is a plain `ITEM_NOT_FOUND`; a *target* that does
/// not exist is `ITEM_NOT_FOUND` naming the relation, because the caller named a
/// link endpoint rather than a command argument.
fn load_partner(
    project: &Project,
    id: &WorkItemId,
    operation: &str,
) -> Result<Loaded, WaymarkError> {
    match mediated::load_for_write(project, id.as_str(), operation) {
        Ok(loaded) => Ok(loaded),
        Err(error) if error.code() == crate::error::Code::ItemNotFound => {
            Err(WaymarkError::referenced_item_not_found(id, SOFT_LINKS))
        }
        Err(error) => Err(error),
    }
}

/// The answer to an `add` whose caller's side already records the link.
///
/// Returns `None` when the caller's side does not record it yet and the add
/// should proceed. The partner is read only when consent was given (a consented
/// add must judge the *pair*) or, without consent, only to confirm a document
/// exists that itself already records the link — which without consent means
/// nothing is left to do.
fn repeat(
    project: &Project,
    left: &Loaded,
    partner: &WorkItemId,
    options: &OpOptions,
) -> Result<Option<LinkOutcome>, WaymarkError> {
    if !left.item.soft_links.contains(partner) {
        return Ok(None);
    }
    if options.cascade {
        return Err(WaymarkError::invalid_relationship(
            SOFT_LINKS,
            left.id(),
            partner,
            "the soft link already exists",
        ));
    }
    let partner_records_it = match mediated::load_entry_any(project, project.locate(partner)?) {
        Ok(right) => right.item.soft_links.contains(left.id()),
        // A partner whose document is gone has nothing to synchronise, and the
        // surviving side is already what the caller asked for: the outcome is
        // the one-sided state either way.
        Err(error) if error.code() == crate::error::Code::ItemNotFound => false,
        Err(error) => return Err(error),
    };
    if partner_records_it {
        return Err(WaymarkError::invalid_relationship(
            SOFT_LINKS,
            left.id(),
            partner,
            "the soft link already exists",
        ));
    }
    let meta = link_meta(project, std::iter::once(&left.item), options);
    Ok(Some(LinkOutcome {
        item: left.item.clone(),
        other: None,
        other_id: partner.clone(),
        partner_unsynchronised: true,
        path: left.path.clone(),
        other_path: None,
        changed: false,
        written: false,
        fingerprints: Vec::new(),
        meta,
    }))
}

fn apply_add(
    project: &Project,
    left: Loaded,
    right: Loaded,
    options: &OpOptions,
) -> Result<LinkOutcome, WaymarkError> {
    let mut left_item = left.item.clone();
    left_item.soft_links.push(right.id().clone());
    normalize_links(&mut left_item);
    mediated::stamp_updated_at(&mut left_item, options);

    if !options.cascade {
        // Without consent the partner's document is not touched at all: no
        // list change, no `updated_at`, not even a timestamp repair.
        let meta = link_meta(project, std::iter::once(&left_item), options);
        let fingerprint = mediated::commit(&left, &left_item, options)?;
        return Ok(LinkOutcome {
            other: None,
            other_id: right.id().clone(),
            partner_unsynchronised: true,
            other_path: None,
            changed: true,
            written: fingerprint.is_some(),
            fingerprints: fingerprint.into_iter().collect(),
            meta,
            path: left.path.clone(),
            item: left_item,
        });
    }

    let mut right_item = right.item.clone();
    right_item.soft_links.push(left.id().clone());
    normalize_links(&mut right_item);
    mediated::stamp_updated_at(&mut right_item, options);

    let meta = link_meta(project, [&left_item, &right_item], options);
    commit_pair(&left, &left_item, &right, &right_item, meta, options)
}

fn apply_remove(
    project: &Project,
    left: Loaded,
    right: Option<Loaded>,
    partner: &WorkItemId,
    options: &OpOptions,
) -> Result<LinkOutcome, WaymarkError> {
    let mut left_item = left.item.clone();
    left_item.soft_links.retain(|known| known != partner);
    normalize_links(&mut left_item);
    mediated::stamp_updated_at(&mut left_item, options);

    let Some(right) = right else {
        // Only one document to replace: a batch of one is still an atomic write.
        let meta = link_meta(project, [&left_item], options);
        let fingerprint = mediated::commit(&left, &left_item, options)?;
        return Ok(LinkOutcome {
            meta,
            path: left.path.clone(),
            item: left_item,
            other: None,
            other_id: partner.clone(),
            partner_unsynchronised: false,
            other_path: None,
            changed: true,
            written: fingerprint.is_some(),
            fingerprints: fingerprint.into_iter().collect(),
        });
    };

    let mut right_item = right.item.clone();
    right_item.soft_links.retain(|known| known != left.id());
    normalize_links(&mut right_item);
    mediated::stamp_updated_at(&mut right_item, options);

    let meta = link_meta(project, [&left_item, &right_item], options);
    commit_pair(&left, &left_item, &right, &right_item, meta, options)
}

/// Write both documents as one batch.
fn commit_pair(
    left: &Loaded,
    left_item: &WorkItem,
    right: &Loaded,
    right_item: &WorkItem,
    meta: Meta,
    options: &OpOptions,
) -> Result<LinkOutcome, WaymarkError> {
    if options.dry_run {
        return Ok(LinkOutcome {
            meta,
            item: left_item.clone(),
            other: Some(right_item.clone()),
            other_id: right.id().clone(),
            partner_unsynchronised: false,
            path: left.path.clone(),
            other_path: Some(right.path.clone()),
            changed: true,
            written: false,
            fingerprints: Vec::new(),
        });
    }

    let left_document = crate::serialize::to_document(left_item);
    let right_document = crate::serialize::to_document(right_item);
    let targets = [
        crate::write::BatchTarget {
            id: left.id(),
            path: &left.path,
            expected: &left.fingerprint,
            bytes: left_document.as_bytes(),
        },
        crate::write::BatchTarget {
            id: right.id(),
            path: &right.path,
            expected: &right.fingerprint,
            bytes: right_document.as_bytes(),
        },
    ];
    let applied = crate::write::replace_batch(&targets)?;
    Ok(LinkOutcome {
        meta,
        item: left_item.clone(),
        other: Some(right_item.clone()),
        other_id: right.id().clone(),
        partner_unsynchronised: false,
        path: left.path.clone(),
        other_path: Some(right.path.clone()),
        changed: true,
        written: true,
        fingerprints: applied.into_iter().map(|entry| entry.fingerprint).collect(),
    })
}

/// `_meta` for a change that touches two documents, so both owners are checked.
fn link_meta<'a>(
    project: &Project,
    items: impl IntoIterator<Item = &'a WorkItem>,
    options: &OpOptions,
) -> Meta {
    let mut warnings = Vec::new();
    for item in items {
        warnings.extend(mediated::owner_warnings(project, item, options));
    }
    mediated::write_meta(warnings)
}

/// Keep the stored list canonical: sorted and free of duplicates.
fn normalize_links(item: &mut WorkItem) {
    item.soft_links.sort();
    item.soft_links.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fixtures;

    fn id(literal: &str) -> WorkItemId {
        WorkItemId::parse(literal).unwrap()
    }

    #[test]
    fn normalization_sorts_and_deduplicates() {
        let mut item = fixtures::minimal_item("wm-000000000001");
        item.soft_links = vec![
            id("wm-000000000003"),
            id("wm-000000000002"),
            id("wm-000000000003"),
        ];
        normalize_links(&mut item);
        assert_eq!(
            item.soft_links,
            vec![id("wm-000000000002"), id("wm-000000000003")]
        );
    }
}
