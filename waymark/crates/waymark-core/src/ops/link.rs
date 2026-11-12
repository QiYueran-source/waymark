//! `waymark link add|remove`: soft links between items.
//!
//! A soft link is a *symmetric, non-blocking* association: two items point at
//! each other and neither waits for nor constrains the other.
//!
//! # Consent and ownership decide whether the second document is written
//!
//! The association is stored on both sides, and writing the second side is a
//! cross-item write: it needs (a) [`OpOptions::cascade`] and (b) that document's
//! `owner` to equal the effective operator.
//!
//! Without consent neither verb writes the partner: `add` records the link on
//! the caller's document and reports the partner as unsynchronised, `remove`
//! drops it from the caller's document and reports the partner as left alone.
//! Leaving a document alone is never a refusal — a half-written pair is a
//! reported state, not a failure, and `doctor` finds it later.
//!
//! Consent is where a request becomes a claim about both documents, and that is
//! where ownership is enforced: a consented write against a partner owned by
//! somebody else is [`crate::error::Code::OwnedByOther`], because the caller
//! asked for that document explicitly and silently downgrading the request would
//! report a two-sided change that never happened. Its message says re-running
//! cannot help: the owner has to be involved, or ownership transferred first.
//!
//! With both, the two documents are replaced as one batch — see
//! [`crate::write::replace_batch`] for the guarantee, which is what makes a
//! two-sided change all-or-nothing. A refusal writes neither document.
//!
//! # The repeat rule
//!
//! A link whose caller's side already records it is a *repeat*. With consent it
//! is a claim about the pair, and a pair that already records the link on both
//! sides has nothing left to do (`INVALID_RELATIONSHIP`); a pair that records it
//! on one side is the missing half, so the consented add writes it. Without
//! consent the caller's half is all this command ever writes, so a one-sided
//! pair is answered with that state — `changed: false`, `written: false`,
//! `partner_unsynchronised: true` — rather than an error that hides it, and a
//! complete pair is the duplicate it asks about (`INVALID_RELATIONSHIP`).
//!
//! # One-sided pairs still exist
//!
//! A pair whose two sides disagree is a normal, reportable state: an external
//! edit, a hand edit, or a merge can leave one behind. `doctor` and `show`
//! diagnose it ([`crate::error::Code::OneSidedSoftLink`]) and nothing repairs
//! it automatically; each side converges by running its own command.
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
//! - an archived source (`ITEM_ARCHIVED`);
//! - a consented second-document write whose partner belongs to another owner
//!   (`OWNED_BY_OTHER`); without consent nothing is refused, because nothing but
//!   the caller's own document is written.
//!
//! # Removal only needs the caller's side
//!
//! Disconnecting is always possible: a link lives on both documents, but either
//! end may drop it alone. Without [`OpOptions::cascade`] a removal rewrites the
//! caller's document only and reports the partner as left alone
//! ([`LinkOutcome::other_untouched`]), leaving it byte-identical, `updated_at`
//! included. An `add` without consent reports its partner through that same
//! field, so every one-sided link names the document it left alone, whose it is,
//! and whether consent would have been enough.
//!
//! Ownership does not come into a non-consenting removal: the operation writes
//! one document, and that document is the caller's own. With `cascade` the
//! removal is a cross-item write and takes the same two conditions as `add`.
//!
//! Removing a link that is not there is an `INVALID_RELATIONSHIP` with or
//! without consent: the caller named an association that does not exist. When
//! the *other* document is gone or archived — an external deletion, or an
//! archive — only the surviving side is cleaned up, because there is no second
//! document to judge. An archived document is never rewritten, because archive
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

/// The partner a one-sided change deliberately left as it is.
///
/// A soft link is stored on both ends, but [`LinkOutcome::other`] only carries a
/// partner that was *rewritten*. When consent is absent the partner keeps its
/// document, its bytes, and its `updated_at`, and this is what reports that
/// fact: enough to say whose file still lists the link, without re-reading it.
#[derive(Debug, Clone)]
pub struct UntouchedLink {
    /// The partner's stable ID.
    pub id: WorkItemId,
    /// The partner's document.
    pub path: camino::Utf8PathBuf,
    /// The partner's `owner`, as its document records it.
    pub owner: String,
    /// Whether `owner` is the effective operator.
    pub owned: bool,
}

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
    /// The other end as it still stands unchanged in this project.
    ///
    /// `Some` when the change deliberately wrote only the caller's document —
    /// an `add` or a `remove` without consent — and `None` otherwise: a
    /// two-sided write, or a partner whose document is missing or archived and
    /// so cannot be named at all.
    pub other_untouched: Option<UntouchedLink>,
    /// Whether an `add` left the pair one-sided: the caller's document records
    /// the link and this operation did not write the other end. Always `false`
    /// for `remove`, which reports the untouched partner through
    /// [`Self::other_untouched`] instead.
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
///
/// The partner is read and judged before anything is staged: a missing consent
/// or a partner the operator does not own is refused, and a refusal writes
/// neither document.
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
        let right = load_partner(project, &right_id, "link add")?;
        // The caller's side may already hold the link. A complete pair is the
        // duplicate the caller asked about; a one-sided pair without consent is
        // answered with that state rather than an error that hides it. Both are
        // read-only answers, so they are given before the consent refusal: a
        // retry that changes nothing must not fail.
        if let Some(outcome) = repeat(project, &left, &right, options)? {
            return Ok(outcome);
        }
        // Without consent this is not a cross-item write at all: only the
        // caller's document changes and the partner is reported as left alone.
        // Consent is where the pair is judged as a pair.
        if options.cascade {
            require_partner_write(
                project,
                "link add",
                left.id(),
                &right,
                options,
                "re-run with --cascade to also record the link there",
            )?;
        }
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
        if !options.cascade {
            // The partner is reported, not written, so its absence is no reason
            // to fail: a vanished or archived document is already the state the
            // removal wants, and there is nothing to name as left alone.
            let right = match load_partner(project, &right_id, "link remove") {
                Ok(right) => Some(right),
                Err(error)
                    if matches!(
                        error.code(),
                        crate::error::Code::ItemNotFound | crate::error::Code::ItemArchived
                    ) =>
                {
                    None
                }
                Err(error) => return Err(error),
            };
            return remove_own_side(project, left, &right_id, right, options);
        }
        match load_partner(project, &right_id, "link remove") {
            Ok(right) => {
                require_partner_write(
                    project,
                    "link remove",
                    left.id(),
                    &right,
                    options,
                    "re-run with --cascade to also drop the link there",
                )?;
                apply_remove(project, left, Some(right), &right_id, options)
            }
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
/// should proceed. The question is only whether the pair already holds what the
/// caller asked for: a complete pair is a duplicate, a consented add of a
/// one-sided pair writes the missing half, and a non-consenting add of a
/// one-sided pair answers with that state.
fn repeat(
    project: &Project,
    left: &Loaded,
    right: &Loaded,
    options: &OpOptions,
) -> Result<Option<LinkOutcome>, WaymarkError> {
    if !left.item.soft_links.contains(right.id()) {
        return Ok(None);
    }
    // A pair that records the link on both sides has nothing left to do, with or
    // without consent: the caller asked for an association that exists.
    if right.item.soft_links.contains(left.id()) {
        return Err(WaymarkError::invalid_relationship(
            SOFT_LINKS,
            left.id(),
            right.id(),
            "the soft link already exists",
        ));
    }
    // A consented add of a one-sided pair is the missing half, not a repeat.
    if options.cascade {
        return Ok(None);
    }
    // One-sided, without consent: the caller's half exists, so there is nothing
    // to write. The answer is that state — no write, `partner_unsynchronised`,
    // and the partner named as left alone — so a retry is neither an error that
    // hides it nor a timestamp for an edit nobody made.
    let meta = link_meta(project, std::iter::once(&left.item), options);
    let untouched = untouched_partner(project, right, options);
    Ok(Some(LinkOutcome {
        item: left.item.clone(),
        other: None,
        other_id: right.id().clone(),
        other_untouched: Some(untouched),
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
        // list change, no `updated_at`, not even a timestamp repair. The pair is
        // one-sided, and the partner is named as the document that stays as it
        // is, so an `add` and a `remove` answer "whose file did I leave alone"
        // through the same field.
        let meta = link_meta(project, std::iter::once(&left_item), options);
        let untouched = untouched_partner(project, &right, options);
        let fingerprint = mediated::commit(&left, &left_item, options)?;
        return Ok(LinkOutcome {
            other: None,
            other_id: right.id().clone(),
            other_untouched: Some(untouched),
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

/// The operator identity, resolved strictly, or the refusal that names what is
/// missing.
///
/// Ownership is a permission-style question, so it must not inherit the lenient
/// path's "a broken document means no operator" answer: that would turn an
/// unreadable identity into a write nobody is entitled to. The resolution is
/// the same one every other check uses — the local document, then Git's
/// `user.name`, then nothing — so a person identified only by Git is the
/// operator like anybody else. Only "nothing" is the refusal, and it names the
/// local document the caller would write the identity into.
fn require_operator(
    project: &Project,
    operation: &str,
    other: &WorkItemId,
) -> Result<String, WaymarkError> {
    let (operator, _) = mediated::strict_operator(project)?;
    match operator {
        Some(operator) => Ok(operator),
        None => Err(WaymarkError::missing_operator_source(
            operation,
            other,
            project.user_config_path().as_str(),
        )),
    }
}

/// Refuse the second-document write `partner` would receive unless consent was
/// given and the partner is the operator's.
///
/// Two conditions, both required, checked before anything is staged. The
/// messages say which one failed and what can be done about it: `consent_hint`
/// names the option to re-run with, and the ownership refusal says re-running
/// cannot change the outcome.
fn require_partner_write(
    project: &Project,
    operation: &str,
    caller: &WorkItemId,
    partner: &Loaded,
    options: &OpOptions,
    consent_hint: &str,
) -> Result<(), WaymarkError> {
    if !options.cascade {
        return Err(WaymarkError::cascade_consent_required(
            operation,
            caller,
            partner.id(),
            consent_hint,
        ));
    }
    // The operation override is the identity the caller is acting as; without
    // one the local document (or Git, when it is all there is) supplies it,
    // through the strict path so a broken document is a configuration error.
    let operator = match options.operator() {
        Some(operator) => operator.to_owned(),
        None => require_operator(project, operation, partner.id())?,
    };
    if !mediated::is_operators(&partner.item.owner, Some(&operator)) {
        return Err(WaymarkError::owned_by_other(
            partner.id(),
            &partner.item.owner,
            operation,
        ));
    }
    Ok(())
}

/// Drop the link from the caller's document only, leaving the partner as is.
fn remove_own_side(
    project: &Project,
    left: Loaded,
    partner: &WorkItemId,
    right: Option<Loaded>,
    options: &OpOptions,
) -> Result<LinkOutcome, WaymarkError> {
    let mut left_item = left.item.clone();
    left_item.soft_links.retain(|known| known != partner);
    normalize_links(&mut left_item);
    mediated::stamp_updated_at(&mut left_item, options);

    // The partner is read, never written, and already loaded unless its
    // document is gone: the result says whose file still lists the link so the
    // caller does not have to look it up.
    let untouched = right
        .as_ref()
        .map(|right| untouched_partner(project, right, options));

    let meta = link_meta(project, [&left_item], options);
    let fingerprint = mediated::commit(&left, &left_item, options)?;
    Ok(LinkOutcome {
        meta,
        path: left.path.clone(),
        item: left_item,
        other: None,
        other_id: partner.clone(),
        other_untouched: untouched,
        partner_unsynchronised: false,
        other_path: None,
        changed: true,
        written: fingerprint.is_some(),
        fingerprints: fingerprint.into_iter().collect(),
    })
}

/// Report a partner whose document this change deliberately did not write.
///
/// The partner has already been loaded, so nothing here re-reads the project:
/// the entry says which document still stands unchanged, whose it is, and
/// whether that owner is the operator — the question a caller asks next, since
/// an owned partner is the one consent would have been enough to rewrite.
fn untouched_partner(project: &Project, partner: &Loaded, options: &OpOptions) -> UntouchedLink {
    UntouchedLink {
        id: partner.id().clone(),
        path: partner.path.clone(),
        owner: partner.item.owner.clone(),
        owned: mediated::is_operators(
            &partner.item.owner,
            mediated::effective_operator(project, options).as_deref(),
        ),
    }
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
            other_untouched: None,
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
            other_untouched: None,
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
        other_untouched: None,
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
