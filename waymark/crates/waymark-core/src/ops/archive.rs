//! `waymark archive`: irreversible-through-Waymark logical deletion.
//!
//! Archive is *not* a lifecycle transition. It is the one mediated write whose
//! subject is the archive marker itself, and it has to be deliberately narrow
//! about what it may move:
//!
//! - it adds `archived_at` (the operation clock) and, optionally,
//!   `archive_reason`, and it changes **nothing else** on the target: not
//!   `status`, not `closed_at`, not `updated_at`, not the body, not the custom
//!   fields, not the target's own historical relationship fields;
//! - re-archiving is refused with `ALREADY_ARCHIVED` and replaces neither
//!   marker, so the first archive is the durable answer;
//! - an unarchived child blocks it with `HAS_ACTIVE_CHILDREN {children}`.
//!   Archive never cascades: a caller retires children one at a time, moves
//!   them, or clears their parent;
//! - active files that point *at* the target lose the inbound edge
//!   (`dependencies`, `parent`, `soft_links`). Those files changed, so they get
//!   a new `updated_at`; their body and custom fields are untouched, and an
//!   archived document is never rewritten, because its relationships are
//!   historical.
//!
//! # One batch
//!
//! The archived target and every cleaned-up active file are replaced as one
//! all-or-nothing batch ([`crate::write::replace_batch`]), so a conflict on any
//! one of them writes none of them. A project with nothing pointing at the
//! target is a batch of one. `--dry-run` runs every check and reports every
//! planned removal while writing nothing.
//!
//! # Why the target is read without [`mediated::load_for_write`]
//!
//! That helper refuses an archived item with `ITEM_ARCHIVED`, which is exactly
//! what every *other* write wants. Archive is the exception: it must see the
//! marker to answer `ALREADY_ARCHIVED`, so it reads through
//! `mediated::load_entry_any` and does the archived check itself.
//!
//! # Why the inbound-`parent` cleanup is normally empty
//!
//! An item whose `parent` is the target *is* an active child, and active
//! children are refused two checks earlier. The branch still exists because the
//! refusal and the cleanup must not silently disagree: if a future child rule
//! ever admits a case, the cleanup is already correct, and a document that
//! arrives between the scan and the batch is still cleaned rather than left
//! dangling. It is not dead code; it is the rule that keeps the two sets equal
//! by construction.
//!
//! # Provisional decisions
//!
//! Recorded in `docs/implementation-decisions.md`: a whitespace-only `--reason`
//! is no reason (so the key is omitted, not blank); `ALREADY_ARCHIVED` carries
//! `{id}` exactly as the registry already pins it, not the `{archived_at}` the
//! ticket prose mentions; and the archived-only collection range reports
//! archived rows with `Readiness::closed()`, which is also why `ready` and
//! `blocked` are empty with that range.

use camino::Utf8PathBuf;

use crate::envelope::Meta;
use crate::error::WaymarkError;
use crate::id::WorkItemId;
use crate::model::WorkItem;
use crate::ops::link::SOFT_LINKS;
use crate::ops::mediated;
use crate::ops::parent::PARENT;
use crate::ops::relationship::DEPENDENCIES;
use crate::ops::OpOptions;
use crate::store::{Fingerprint, Project};
use crate::time::Timestamp;

/// One recognized inbound relation kind, in the order the spec names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InboundRelation {
    /// An active item lists the archived ID in `dependencies`.
    Dependencies,
    /// An active item names the archived ID in `parent`.
    Parent,
    /// An active item lists the archived ID in `soft_links`.
    SoftLinks,
}

impl InboundRelation {
    /// The persisted field name, shared with the operation that owns the field.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dependencies => DEPENDENCIES,
            Self::Parent => PARENT,
            Self::SoftLinks => SOFT_LINKS,
        }
    }
}

/// One planned relationship removal: which file loses which edge.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlannedRemoval {
    /// The active item whose document loses the edge.
    pub from: WorkItemId,
    /// The field the archived ID is stripped out of.
    pub relation: InboundRelation,
}

/// One active document the archive rewrites besides the target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffectedItem {
    /// The item whose document changes.
    pub id: WorkItemId,
    /// That document.
    pub path: Utf8PathBuf,
}

/// What an archive produced, or would produce on a dry run.
#[derive(Debug, Clone)]
pub struct ArchiveOutcome {
    /// The archived target as written, or as it would have been written.
    pub item: WorkItem,
    /// The target's document.
    pub path: Utf8PathBuf,
    /// The archive instant the operation clock supplied.
    pub archived_at: Timestamp,
    /// The normalized reason, or `None`.
    pub archive_reason: Option<String>,
    /// Every planned inbound removal, in stable-ID then relation order.
    pub removals: Vec<PlannedRemoval>,
    /// Every active document cleaned up, sorted by stable ID.
    pub affected: Vec<AffectedItem>,
    /// Whether the batch was applied. Always `false` on a dry run.
    pub written: bool,
    /// One fingerprint per replaced document, target first then `affected`
    /// order; empty on a dry run.
    pub fingerprints: Vec<Fingerprint>,
    /// The `_meta` block this write justifies, including `OWNER_MISMATCH`.
    pub meta: Meta,
}

impl ArchiveOutcome {
    /// The relations one active file loses, in relation order.
    pub fn removals_for(&self, from: &WorkItemId) -> Vec<InboundRelation> {
        self.removals
            .iter()
            .filter(|removal| &removal.from == from)
            .map(|removal| removal.relation)
            .collect()
    }
}

/// Archive one item.
///
/// `reason` is free text stored as `archive_reason`; `None` (or a
/// whitespace-only value) stores no reason at all. Every check runs on a dry
/// run too, and a dry run writes nothing.
pub fn archive(
    project: &Project,
    id: &str,
    reason: Option<&str>,
    options: &OpOptions,
) -> Result<ArchiveOutcome, WaymarkError> {
    let reason = normalize_reason(reason);
    crate::lock::with_project_lock(project, options.lock_wait, || {
        apply(project, id, reason, options)
    })
}

/// Treat a blank reason as no reason, so the document omits the key entirely.
fn normalize_reason(reason: Option<&str>) -> Option<String> {
    reason
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn apply(
    project: &Project,
    id: &str,
    reason: Option<String>,
    options: &OpOptions,
) -> Result<ArchiveOutcome, WaymarkError> {
    let id = crate::ops::read::normalize(id)?;

    // One complete scan answers identity and inbound edges together: a document
    // the scan cannot read might be the one holding an inbound edge, so a broken
    // sibling must fail the operation rather than be silently skipped.
    let documents = scan(project)?;

    // Read the target from that same scan: re-archive is a cheap, exact state
    // rejection, and it must be answered without rewriting either marker.
    let target = documents
        .iter()
        .find(|document| document.id == id)
        .ok_or_else(|| WaymarkError::item_not_found(&id))?;
    if target.item.is_archived() {
        return Err(WaymarkError::already_archived(&target.item.id));
    }

    // Children first: an active child means the hierarchy would be detached,
    // and archive is one item per call, never a cascade.
    let children = active_children(&documents, &target.item.id);
    if !children.is_empty() {
        return Err(WaymarkError::has_active_children(
            &target.item.id,
            &children,
        ));
    }

    let archived_at = Timestamp::new(options.clock.now());
    let mut archived = target.item.clone();
    archived.archived_at = Some(archived_at);
    archived.archive_reason = reason.clone();

    let mut removals: Vec<PlannedRemoval> = Vec::new();
    let mut affected: Vec<AffectedItem> = Vec::new();
    let mut staged: Vec<StagedDocument> = Vec::new();
    staged.push(StagedDocument::new(
        &target.item.id,
        target.path.clone(),
        target.fingerprint.clone(),
        &archived,
    ));

    for document in &documents {
        if document.id == target.item.id || document.item.is_archived() {
            // The target is handled above; an archived document is history and
            // is never rewritten.
            continue;
        }
        let (mut cleaned, relations) = strip_inbound(&document.item, &target.item.id);
        if relations.is_empty() {
            continue;
        }
        // The file really changed, so it gets a new modification time. The
        // target does not: only its markers moved.
        mediated::touch(&mut cleaned, options);
        for relation in &relations {
            removals.push(PlannedRemoval {
                from: document.id.clone(),
                relation: *relation,
            });
        }
        affected.push(AffectedItem {
            id: document.id.clone(),
            path: document.path.clone(),
        });
        staged.push(StagedDocument::new(
            &document.id,
            document.path.clone(),
            document.fingerprint.clone(),
            &cleaned,
        ));
    }

    removals.sort();

    let meta = mediated::write_meta(mediated::owner_warnings(project, &target.item, options));

    if options.dry_run {
        return Ok(ArchiveOutcome {
            item: archived,
            path: target.path.clone(),
            archived_at,
            archive_reason: reason,
            removals,
            affected,
            written: false,
            fingerprints: Vec::new(),
            meta,
        });
    }

    let targets: Vec<crate::write::BatchTarget<'_>> =
        staged.iter().map(StagedDocument::target).collect();
    let applied = crate::write::replace_batch(&targets)?;
    let fingerprints = applied
        .iter()
        .map(|entry| entry.fingerprint.clone())
        .collect();

    Ok(ArchiveOutcome {
        item: archived,
        path: target.path.clone(),
        archived_at,
        archive_reason: reason,
        removals,
        affected,
        written: true,
        fingerprints,
        meta,
    })
}

/// Every active item whose `parent` is `id`, sorted by stable ID.
///
/// The target itself is never its own child, even if an external edit made it
/// self-parented: that damage is diagnosed elsewhere and must not turn archive
/// into a refusal that names the wrong item.
fn active_children(documents: &[Document], id: &WorkItemId) -> Vec<WorkItemId> {
    let mut children: Vec<WorkItemId> = documents
        .iter()
        .filter(|document| &document.id != id)
        .filter(|document| !document.item.is_archived())
        .filter(|document| document.item.parent.as_ref() == Some(id))
        .map(|document| document.id.clone())
        .collect();
    children.sort();
    children.dedup();
    children
}

/// Strip every recognized inbound edge to `archived` from `item`.
///
/// The returned item is *not* touched: the relation list is what tells the
/// caller whether anything was stripped, and therefore whether the file really
/// changed and deserves a new `updated_at`.
fn strip_inbound(item: &WorkItem, archived: &WorkItemId) -> (WorkItem, Vec<InboundRelation>) {
    let mut cleaned = item.clone();
    let mut relations: Vec<InboundRelation> = Vec::new();

    if cleaned.dependencies.contains(archived) {
        cleaned.dependencies.retain(|target| target != archived);
        relations.push(InboundRelation::Dependencies);
    }
    if cleaned.parent.as_ref() == Some(archived) {
        cleaned.parent = None;
        relations.push(InboundRelation::Parent);
    }
    if cleaned.soft_links.contains(archived) {
        cleaned.soft_links.retain(|target| target != archived);
        relations.push(InboundRelation::SoftLinks);
    }

    (cleaned, relations)
}

/// One document staged for the batch, owning the bytes the batch borrows.
struct StagedDocument {
    id: WorkItemId,
    path: Utf8PathBuf,
    expected: Fingerprint,
    bytes: Vec<u8>,
}

impl StagedDocument {
    fn new(id: &WorkItemId, path: Utf8PathBuf, expected: Fingerprint, item: &WorkItem) -> Self {
        Self {
            id: id.clone(),
            path,
            expected,
            bytes: crate::serialize::to_document(item).into_bytes(),
        }
    }

    fn target(&self) -> crate::write::BatchTarget<'_> {
        crate::write::BatchTarget {
            id: &self.id,
            path: &self.path,
            expected: &self.expected,
            bytes: &self.bytes,
        }
    }
}

/// One document of the complete project scan.
struct Document {
    id: WorkItemId,
    path: Utf8PathBuf,
    fingerprint: Fingerprint,
    item: WorkItem,
}

/// Build the complete projection the archive plans against.
///
/// A full scan is the operation's completeness story: archive is allowed to
/// rewrite files it did not name, so it has to know about all of them. An
/// unparseable document could be hiding an inbound edge, so the projection is
/// required to be complete and fresh (`INCOMPLETE_PROJECTION` otherwise).
fn scan(project: &Project) -> Result<Vec<Document>, WaymarkError> {
    let projection = crate::projection::Projection::load(project)?;
    projection.require_complete()?;
    let mut documents: Vec<Document> = projection
        .entries()
        .map(|entry| Document {
            id: entry.id.clone(),
            path: entry.path.clone(),
            fingerprint: entry.fingerprint.clone(),
            item: entry.item.clone(),
        })
        .collect();
    // Stable-ID order, not directory order: the batch, the planned removals and
    // the affected list then read the same way on every machine regardless of
    // which category directory a document happens to live in.
    documents.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(documents)
}
