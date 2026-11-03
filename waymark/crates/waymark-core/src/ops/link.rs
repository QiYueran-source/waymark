//! `waymark link add|remove`: soft links between items.
//!
//! A soft link is a *symmetric, non-blocking* association: two items point at
//! each other and neither waits for nor constrains the other. Because it is
//! symmetric, it is stored on both sides, and both documents are rewritten as
//! one batch — see [`crate::write::replace_batch`] for the guarantee. A crash
//! or a failure never leaves one side pointing at the other with nothing coming
//! back.
//!
//! # What is rejected
//!
//! - a self-link (`INVALID_RELATIONSHIP`);
//! - a link that already exists (`INVALID_RELATIONSHIP`, never a silent no-op:
//!   a caller that asked to add a link should learn it was already there);
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
    /// The other end, as written; `None` when only the surviving side changed.
    pub other: Option<WorkItem>,
    /// The other end's stable ID, whether or not its document was written.
    pub other_id: WorkItemId,
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
        if left.item.soft_links.contains(&right_id) {
            return Err(WaymarkError::invalid_relationship(
                SOFT_LINKS,
                left.id(),
                &right_id,
                "the soft link already exists",
            ));
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

fn apply_add(
    project: &Project,
    left: Loaded,
    right: Loaded,
    options: &OpOptions,
) -> Result<LinkOutcome, WaymarkError> {
    let mut left_item = left.item.clone();
    left_item.soft_links.push(right.id().clone());
    normalize_links(&mut left_item);

    let mut right_item = right.item.clone();
    right_item.soft_links.push(left.id().clone());
    normalize_links(&mut right_item);

    mediated::touch(&mut left_item, options);
    mediated::touch(&mut right_item, options);

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
    mediated::touch(&mut left_item, options);

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
            other_path: None,
            changed: true,
            written: fingerprint.is_some(),
            fingerprints: fingerprint.into_iter().collect(),
        });
    };

    let mut right_item = right.item.clone();
    right_item.soft_links.retain(|known| known != left.id());
    normalize_links(&mut right_item);
    mediated::touch(&mut right_item, options);

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
