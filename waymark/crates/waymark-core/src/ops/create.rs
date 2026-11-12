//! `waymark create`: write one new work item.
//!
//! Creation is the only operation that mints identity. It allocates the stable
//! ID, stamps `created_at` and `updated_at`, and writes the document atomically
//! under the store lock. Callers cannot supply any of those values.
//!
//! # What a request may carry
//!
//! The catalog promises a complete initial state, not just three fields:
//! `title`, `category`, `owner`, an optional `creator`, a body, custom fields,
//! and the initial `parent`, `dependencies`, and `soft_links`. All of it is one
//! operation. A create with initial relations is an *application-level
//! all-or-nothing batch*: the new document and every soft-link partner it
//! rewrites are staged and renamed together, and a failure anywhere leaves the
//! store exactly as it was.
//!
//! # Soft-link partners need consent and ownership
//!
//! A soft link is symmetric, so a symmetric write would edit a document the
//! caller never named. Creation therefore only rewrites a partner when both
//! conditions hold:
//!
//! - `options.cascade` was given, and
//! - the partner's `owner`, the new item's `owner`, and the effective operator
//!   are the same identity. `creator` never participates.
//!
//! With `cascade` the operator is resolved strictly (a broken local document is
//! `CONFIG_INVALID`) once, before anything is staged. A partner that is left
//! alone — no consent, or an identity mismatch — keeps its bytes and its
//! `updated_at` and is reported in [`Created::untouched`]; a partner that was
//! rewritten is reported in [`Created::partners`]. Because the operator can
//! only ever rewrite partners it owns, the consent rule needs no refusal of its
//! own: the new item is still created and a partner that is not the operator's
//! is simply named as left alone.
//!
//! An archived, missing, or older-generation partner is refused exactly as
//! before whether or not it would have been rewritten: the link the caller asked
//! for cannot be half-formed against a document that no longer accepts writes,
//! and that answer must not depend on consent.
//!
//! # Validation order
//!
//! `title`, `category`, `owner`, `creator`, and every custom-field value are
//! checked together before the lock, so a caller learns about all of its request
//! mistakes at once. Referenced IDs are normalized next (`INVALID_ID`), and a
//! create with relations then checks self-references, duplicates, target
//! existence and active state, and the cycles the planned edges would close
//! against a complete, fresh projection — also before the lock. The same checks
//! are repeated under the lock with the ID actually minted, because only then is
//! the write frozen.
//!
//! # Creator
//!
//! `creator` is provenance, not accountability: it defaults to the acting
//! operator (the operation override, else the configured operator) and is never
//! inferred from the owner. A supplied creator is trimmed and validated exactly
//! like an owner.
//!
//! # Custom fields
//!
//! Declared keys only, exact declared types, required fields present, no
//! duplicates, and no null (the type has no null value). A missing required
//! field fails the whole create; nothing is written.

use std::collections::{BTreeMap, BTreeSet};

use camino::Utf8PathBuf;

use crate::envelope::Meta;
use crate::error::{ValidationIssue, WaymarkError};
use crate::id::WorkItemId;
use crate::location::Location;
use crate::model::{CategoryKey, CustomFields, Status, WorkItem, FORMAT_GENERATION};
use crate::ops::mediated;
use crate::ops::{link, parent, read, relationship, OpOptions};
use crate::projection::Projection;
use crate::readiness::Graph;
use crate::store::{Fingerprint, Project};
use crate::time::Timestamp;
use crate::write::{BatchCreate, BatchTarget};

/// What a caller asks for: the required fields plus the complete initial state.
#[derive(Debug, Clone)]
pub struct CreateRequest {
    /// Short single-line label.
    pub title: String,
    /// Category key the item belongs to; must already be declared.
    pub category: String,
    /// Accountable identity.
    pub owner: String,
    /// Provenance identity; defaults to the acting operator when omitted.
    pub creator: Option<String>,
    /// Optional Markdown body.
    pub body: Option<String>,
    /// Declared custom fields and their typed values.
    pub fields: Vec<(crate::model::FieldKey, crate::FieldValue)>,
    /// The single parent the item starts under.
    pub parent: Option<String>,
    /// Items this one waits for.
    pub dependencies: Vec<String>,
    /// Symmetric soft links, stored on both sides.
    pub soft_links: Vec<String>,
}

impl CreateRequest {
    /// A request with the three required fields and nothing else.
    pub fn new(
        title: impl Into<String>,
        category: impl Into<String>,
        owner: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            category: category.into(),
            owner: owner.into(),
            creator: None,
            body: None,
            fields: Vec::new(),
            parent: None,
            dependencies: Vec::new(),
            soft_links: Vec::new(),
        }
    }

    /// Record a creator distinct from the acting operator.
    pub fn with_creator(mut self, creator: impl Into<String>) -> Self {
        self.creator = Some(creator.into());
        self
    }

    /// Attach a body.
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Set one custom field to a typed value.
    pub fn with_field(mut self, key: crate::model::FieldKey, value: crate::FieldValue) -> Self {
        self.fields.push((key, value));
        self
    }

    /// Place the new item under a parent.
    pub fn with_parent(mut self, parent: impl Into<String>) -> Self {
        self.parent = Some(parent.into());
        self
    }

    /// Add a hard dependency.
    pub fn with_dependency(mut self, dependency: impl Into<String>) -> Self {
        self.dependencies.push(dependency.into());
        self
    }

    /// Add a symmetric soft link.
    pub fn with_soft_link(mut self, link: impl Into<String>) -> Self {
        self.soft_links.push(link.into());
        self
    }
}

/// One partner document the creation rewrote.
///
/// Only symmetric soft links rewrite the other side; a parent or dependency is
/// stored on the new item alone.
#[derive(Debug, Clone)]
pub struct ChangedPartner {
    /// The partner as written, or as it would have been written on a dry run.
    pub item: WorkItem,
    /// The partner's document.
    pub path: Utf8PathBuf,
    /// The fingerprint of the written bytes; `None` on a dry run.
    pub fingerprint: Option<Fingerprint>,
}

/// What creation produced.
#[derive(Debug, Clone)]
pub struct Created {
    /// The complete item as written, or as it would have been written on a dry
    /// run.
    pub item: WorkItem,
    /// The document path.
    pub path: Utf8PathBuf,
    /// Every soft-link partner document rewritten, in stable-ID order.
    pub partners: Vec<ChangedPartner>,
    /// Every soft-link partner left exactly as it was, in stable-ID order: no
    /// consent, or an identity that is not the operator's. Empty when every
    /// partner was rewritten or the request named none.
    pub untouched: Vec<UntouchedPartner>,
    /// The fingerprint of the new document; `None` on a dry run.
    pub fingerprint: Option<Fingerprint>,
    /// One fingerprint per written document, in `[new, partners…]` order; empty
    /// on a dry run.
    pub fingerprints: Vec<Fingerprint>,
    /// Whether the store changed.
    pub written: bool,
    /// The `_meta` block this creation justifies.
    pub meta: Meta,
}

/// One soft-link partner the creation left exactly as it was.
///
/// The partner does not know about the new item: its document, its
/// `updated_at`, and its `soft_links` are unchanged. The new item still records
/// the link on its own side, so the pair is one-sided until each side runs its
/// own command.
#[derive(Debug, Clone)]
pub struct UntouchedPartner {
    /// The partner, as it is on disk.
    pub item: WorkItem,
    /// The partner's document.
    pub path: Utf8PathBuf,
}

/// Create one work item in `project`.
pub fn create(
    project: &Project,
    request: CreateRequest,
    options: &OpOptions,
) -> Result<Created, WaymarkError> {
    let prepared = prepare(project, request, options)?;

    // A dry run still performs every check `prepare` defines, including the
    // complete-projection requirement and the planned-edge cycle checks, but
    // never takes the lock or writes.
    if options.dry_run {
        return prepare_only(project, prepared, options);
    }

    // The lock is taken *before* the inventory: an ID allocated against a stale
    // inventory could collide with an item another writer just committed.
    crate::lock::with_project_lock(project, options.lock_wait, || {
        write_new_item(project, prepared, options)
    })
}

/// The dry-run half of [`create`]: everything but the write.
fn prepare_only(
    project: &Project,
    prepared: Prepared,
    options: &OpOptions,
) -> Result<Created, WaymarkError> {
    let id = match prepared.planned_id.clone() {
        Some(planned) => planned,
        None => project.allocate_id(&project.inventory()?)?,
    };
    let path = project.path_for(&id, &prepared.category)?;
    let now = Timestamp::new(options.clock.now());
    let item = build_item(&id, &prepared, now);

    // The same mediated read the real write uses: a partner that is archived
    // or older-generation must refuse the preview too, otherwise the dry run
    // would promise a creation the write cannot perform.
    let loaded = load_partners(project, &prepared.soft_links)?;
    // Ownership is resolved here exactly as the real write resolves it, so the
    // preview and the write name the same partners as untouched.
    let operator = match options.cascade {
        true => mediated::effective_operator_strict(project, options)?,
        false => None,
    };

    let mut partners = Vec::new();
    let mut untouched = Vec::new();
    for partner in loaded {
        if !is_planned_for(&partner, &item, options.cascade, operator.as_deref()) {
            untouched.push(UntouchedPartner {
                item: partner.item,
                path: partner.path,
            });
            continue;
        }
        partners.push(ChangedPartner {
            item: link_partner(partner.item, &id, now),
            path: partner.path,
            fingerprint: None,
        });
    }

    let warnings = creation_warnings(project, &partners, options);

    Ok(Created {
        item,
        path,
        partners,
        untouched,
        fingerprint: None,
        fingerprints: Vec::new(),
        written: false,
        meta: mediated::write_meta(warnings),
    })
}

/// The write half of [`create`]: inventory, ID, graph checks, and the one
/// batch that creates the new document and rewrites its partners.
///
/// Run while the store lock is held.
fn write_new_item(
    project: &Project,
    prepared: Prepared,
    options: &OpOptions,
) -> Result<Created, WaymarkError> {
    let inventory = project.inventory()?;
    // Reuse the ID the pre-lock checks reasoned about. A concurrent create may
    // have consumed it, in which case a fresh ID is minted and the relation
    // checks below run against that one instead.
    let id = match prepared.planned_id.clone() {
        Some(planned) if !inventory.contains(&planned) => planned,
        _ => project.allocate_id(&inventory)?,
    };
    let path = project.path_for(&id, &prepared.category)?;

    if path.exists() {
        // The inventory said the ID was free, yet a file is there: something
        // created it outside the lock. Refuse rather than overwrite.
        return Err(WaymarkError::write_conflict(
            &id,
            path.to_string(),
            "absent",
            "present",
        ));
    }

    // Load the soft-link partners before taking any timestamp: their exact
    // fingerprints are the batch's conflict guards, and a partner that changed
    // since it was read must fail the whole creation rather than be overwritten.
    let loaded_partners = if prepared.has_relations() {
        // The spec requires a complete, fresh projection for a create with
        // initial relations: a hole in the graph could hide a missing target or
        // the edge that closes a cycle.
        let projection = Projection::load(project)?;
        projection.require_complete()?;
        let graph = Graph::from_projection(&projection)?;
        validate_relations(&graph, &id, &prepared)?;
        load_partners(project, &prepared.soft_links)?
    } else {
        Vec::new()
    };

    let now = Timestamp::new(options.clock.now());
    let item = build_item(&id, &prepared, now);
    let new_document = crate::serialize::to_document(&item);

    // Both conditions of a cross-item write are decoded from the loaded
    // partners before anything is staged: consent, and the three identities —
    // the new item's owner, the partner's owner, the operator — that must agree.
    // The operator is only resolved strictly when consent was given; without it
    // no partner can be rewritten, so a broken local document cannot change the
    // answer, and resolving it would turn an otherwise valid create into a
    // configuration error for a write that will not happen.
    let operator = match options.cascade {
        true => mediated::effective_operator_strict(project, options)?,
        false => None,
    };

    let mut partner_documents: Vec<PartnerDocument> = Vec::new();
    let mut untouched: Vec<UntouchedPartner> = Vec::new();
    for loaded in loaded_partners {
        if !is_planned_for(&loaded, &item, options.cascade, operator.as_deref()) {
            untouched.push(UntouchedPartner {
                item: loaded.item,
                path: loaded.path,
            });
            continue;
        }
        partner_documents.push(PartnerDocument::new(
            link_partner(loaded.item, &id, now),
            loaded.path,
            loaded.fingerprint,
        ));
    }

    let targets: Vec<BatchTarget<'_>> = partner_documents
        .iter()
        .map(|partner| BatchTarget {
            id: &partner.item.id,
            path: &partner.path,
            expected: &partner.fingerprint,
            bytes: partner.document.as_bytes(),
        })
        .collect();
    let create = BatchCreate {
        id: &item.id,
        path: &path,
        bytes: new_document.as_bytes(),
    };
    let applied = crate::write::replace_batch_with_creates(&[create], &targets)?;

    let fingerprints: Vec<Fingerprint> = applied
        .iter()
        .map(|entry| entry.fingerprint.clone())
        .collect();
    let partners: Vec<ChangedPartner> = partner_documents
        .into_iter()
        .zip(applied.iter().skip(1))
        .map(|(partner, entry)| ChangedPartner {
            item: partner.item,
            path: partner.path,
            fingerprint: Some(entry.fingerprint.clone()),
        })
        .collect();

    let warnings = creation_warnings(project, &partners, options);

    Ok(Created {
        item,
        path,
        partners,
        untouched,
        fingerprint: fingerprints.first().cloned(),
        fingerprints,
        written: true,
        meta: mediated::write_meta(warnings),
    })
}

/// The `OWNER_MISMATCH` notes this creation justifies.
///
/// `_meta.warnings` matches the documents this write actually moved: an owner
/// mismatch on a rewritten soft-link partner. The new item itself never warns —
/// creating an item for somebody else is ordinary and unchanged — so a plain
/// create, and a create whose partners were all left alone, stays quiet.
fn creation_warnings(
    project: &Project,
    partners: &[ChangedPartner],
    options: &OpOptions,
) -> Vec<crate::envelope::Warning> {
    let mut warnings = Vec::new();
    for partner in partners {
        warnings.extend(mediated::owner_warnings(project, &partner.item, options));
    }
    warnings
}

/// Whether one loaded soft-link partner is planned for a rewrite.
///
/// Two independent conditions, both required: consent was given, and the new
/// item's owner, the partner's owner, and the operator are the same identity.
/// A partner this returns `false` for is left exactly as it is and reported in
/// [`Created::untouched`]; nothing is refused, because a partner nobody asked
/// this operation to own is simply somebody else's document.
fn is_planned_for(
    partner: &mediated::Loaded,
    new_item: &WorkItem,
    cascade: bool,
    operator: Option<&str>,
) -> bool {
    cascade
        && mediated::is_operators(&partner.item.owner, operator)
        && partner.item.owner == new_item.owner
}

/// One partner document computed for the batch.
///
/// The rendered document is kept beside the item because the batch borrows the
/// bytes, and a `BatchTarget` never owns a partial change.
struct PartnerDocument {
    item: WorkItem,
    path: Utf8PathBuf,
    fingerprint: Fingerprint,
    document: String,
}

impl PartnerDocument {
    fn new(item: WorkItem, path: Utf8PathBuf, fingerprint: Fingerprint) -> Self {
        let document = crate::serialize::to_document(&item);
        Self {
            item,
            path,
            fingerprint,
            document,
        }
    }
}

/// Add the new ID to a partner's symmetric list and stamp it with the same
/// instant the new item carries.
fn link_partner(mut partner: WorkItem, new_id: &WorkItemId, now: Timestamp) -> WorkItem {
    partner.soft_links.push(new_id.clone());
    partner.soft_links.sort();
    partner.soft_links.dedup();
    partner.updated_at = now;
    partner
}

/// Read every soft-link partner for writing, so the batch guards their exact
/// fingerprints and an archived partner is refused like any other mediated
/// write.
fn load_partners(
    project: &Project,
    ids: &[WorkItemId],
) -> Result<Vec<mediated::Loaded>, WaymarkError> {
    ids.iter()
        .map(|id| mediated::load_for_write(project, id.as_str(), "create"))
        .collect()
}

/// The item creation produces, before it is written.
fn build_item(id: &WorkItemId, prepared: &Prepared, now: Timestamp) -> WorkItem {
    WorkItem {
        id: id.clone(),
        title: prepared.title.clone(),
        category: prepared.category.clone(),
        status: Status::Open,
        owner: prepared.owner.clone(),
        creator: prepared.creator.clone(),
        created_at: now,
        updated_at: now,
        closed_at: None,
        parent: prepared.parent.clone(),
        dependencies: prepared.dependencies.clone(),
        soft_links: prepared.soft_links.clone(),
        custom_fields: prepared.fields.clone(),
        archived_at: None,
        archive_reason: None,
        schema: FORMAT_GENERATION,
        body: crate::serialize::canonical_body(prepared.body.as_deref().unwrap_or_default()),
    }
}

/// A request whose fields are trimmed and validated.
struct Prepared {
    title: String,
    category: CategoryKey,
    owner: String,
    creator: Option<String>,
    body: Option<String>,
    fields: CustomFields,
    parent: Option<WorkItemId>,
    dependencies: Vec<WorkItemId>,
    soft_links: Vec<WorkItemId>,
    /// The ID the pre-lock relation checks reasoned about, when there are
    /// relations to check.
    planned_id: Option<WorkItemId>,
}

impl Prepared {
    fn has_relations(&self) -> bool {
        self.parent.is_some() || !self.dependencies.is_empty() || !self.soft_links.is_empty()
    }
}

/// The issue creation reports for a category literal that is not usable.
///
/// This is the one definition of "a declared category", shared with `move` so a
/// relocation cannot accept a category that creation would refuse (or refuse one
/// with a different message).
pub(crate) fn category_issue(project: &Project, raw: &str) -> Option<ValidationIssue> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Some(ValidationIssue::new("category", "a category is required"));
    }
    match CategoryKey::parse(raw) {
        Ok(key) if project.category(&key).is_some() => None,
        Ok(key) => Some(ValidationIssue::new(
            "category",
            format!("{key} is not a category declared in the project configuration"),
        )),
        Err(error) => Some(ValidationIssue::new("category", error.to_string())),
    }
}

/// The issue creation reports for an owner literal that is not usable.
///
/// Identities are trimmed at the edges, must be non-empty, and must be single
/// line. `owner transfer` reuses this exact rule so creation and transfer cannot
/// disagree about what a valid owner is.
pub(crate) fn owner_issue(owner: &str) -> Option<ValidationIssue> {
    identity_issue("owner", "an owner", owner)
}

/// The creator variant of [`owner_issue`]: provenance must be a real identity.
fn creator_issue(creator: &str) -> Option<ValidationIssue> {
    identity_issue("creator", "a creator", creator)
}

fn identity_issue(field: &str, label: &str, value: &str) -> Option<ValidationIssue> {
    let value = value.trim();
    if value.is_empty() {
        Some(ValidationIssue::new(field, format!("{label} is required")))
    } else if value.contains(['\n', '\r']) {
        Some(ValidationIssue::new(
            field,
            format!("{label} must be a single line"),
        ))
    } else {
        None
    }
}

fn prepare(
    project: &Project,
    request: CreateRequest,
    options: &OpOptions,
) -> Result<Prepared, WaymarkError> {
    let mut issues = Vec::new();

    let title = request.title.trim();
    if title.is_empty() {
        issues.push(ValidationIssue::new("title", "a title is required"));
    } else if title.contains(['\n', '\r']) {
        issues.push(ValidationIssue::new(
            "title",
            "a title must be a single line",
        ));
    }

    let category = {
        let raw = request.category.trim();
        if let Some(issue) = category_issue(project, raw) {
            issues.push(issue);
            None
        } else {
            Some(CategoryKey::parse(raw).expect("the category issue check proved it parses"))
        }
    };

    let owner = request.owner.trim();
    if let Some(issue) = owner_issue(owner) {
        issues.push(issue);
    }

    if let Some(raw) = request.creator.as_deref() {
        if let Some(issue) = creator_issue(raw) {
            issues.push(issue);
        }
    }

    validate_fields(project, &request.fields, &mut issues);

    if !issues.is_empty() {
        return Err(
            WaymarkError::validation("create", issues).with_location(Location::field("create"))
        );
    }

    let creator = match request.creator.as_deref() {
        Some(raw) => Some(raw.trim().to_owned()),
        None => mediated::operator_to_record(project, options)?,
    };

    let parent = match request.parent.as_deref() {
        Some(raw) => Some(read::normalize(raw)?),
        None => None,
    };
    let mut dependencies: Vec<WorkItemId> = request
        .dependencies
        .iter()
        .map(|raw| read::normalize(raw))
        .collect::<Result<_, _>>()?;
    let mut soft_links: Vec<WorkItemId> = request
        .soft_links
        .iter()
        .map(|raw| read::normalize(raw))
        .collect::<Result<_, _>>()?;
    dependencies.sort();
    soft_links.sort();

    let mut prepared = Prepared {
        title: title.to_owned(),
        category: category.expect("validation reported a missing category"),
        owner: owner.to_owned(),
        creator,
        body: request.body,
        fields: request
            .fields
            .into_iter()
            .map(|(key, value)| (key.as_str().to_owned(), value))
            .collect(),
        parent,
        dependencies,
        soft_links,
        planned_id: None,
    };

    if prepared.has_relations() {
        let inventory = project.inventory()?;
        let planned = project.allocate_id(&inventory)?;
        let projection = Projection::load(project)?;
        projection.require_complete()?;
        let graph = Graph::from_projection(&projection)?;
        validate_relations(&graph, &planned, &prepared)?;
        prepared.planned_id = Some(planned);
    }

    Ok(prepared)
}

/// Validate every requested custom field against the declarations.
///
/// Nothing reaches the document until this returns: an undeclared key, a
/// duplicate key, a wrong type, or an absent required field is an issue. The
/// value type has no null variant, so a null can only arrive through an adapter,
/// which refuses it before building the request.
fn validate_fields(
    project: &Project,
    fields: &[(crate::model::FieldKey, crate::FieldValue)],
    issues: &mut Vec<ValidationIssue>,
) {
    let declared = &project.config().fields;
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (key, _) in fields {
        *counts.entry(key.as_str()).or_default() += 1;
    }

    let mut reported: BTreeSet<&str> = BTreeSet::new();
    for (key, value) in fields {
        let name = key.as_str();
        let path = format!("custom_fields.{name}");
        let count = counts.get(name).copied().unwrap_or(0);
        if count > 1 {
            if reported.insert(name) {
                issues.push(ValidationIssue::new(
                    path,
                    format!("the key appears {count} times; name it once"),
                ));
            }
            continue;
        }
        match declared.get(key) {
            None => issues.push(ValidationIssue::new(
                path,
                format!("`{name}` is not a declared custom field"),
            )),
            Some(declaration) if !declaration.ty.accepts(value) => {
                issues.push(ValidationIssue::new(
                    path,
                    format!(
                        "expected a value of type `{}`, found {}",
                        declaration.ty.as_str(),
                        value.type_name()
                    ),
                ))
            }
            Some(_) => {}
        }
    }

    for (key, declaration) in declared {
        if declaration.required && !counts.contains_key(key.as_str()) {
            issues.push(ValidationIssue::new(
                format!("custom_fields.{key}"),
                format!("`{key}` is required and has no value"),
            ));
        }
    }
}

/// Validate the planned initial edges against the active graph.
///
/// The new item is not a node yet, so the planned edges are reasoned about
/// through the graph: a dangling reference *to* the new ID is what can close a
/// parent or dependency cycle, and the shared walkers already find it.
fn validate_relations(
    graph: &Graph,
    new_id: &WorkItemId,
    prepared: &Prepared,
) -> Result<(), WaymarkError> {
    if let Some(parent) = &prepared.parent {
        if parent == new_id {
            return Err(WaymarkError::invalid_relationship(
                parent::PARENT,
                new_id,
                parent,
                "an item cannot be its own parent",
            ));
        }
    }
    for dependency in &prepared.dependencies {
        if dependency == new_id {
            return Err(WaymarkError::invalid_relationship(
                relationship::DEPENDENCIES,
                new_id,
                dependency,
                "an item cannot depend on itself",
            ));
        }
    }
    for link in &prepared.soft_links {
        if link == new_id {
            return Err(WaymarkError::invalid_relationship(
                link::SOFT_LINKS,
                new_id,
                link,
                "an item cannot be soft-linked to itself",
            ));
        }
    }

    duplicate_reference(&prepared.dependencies, relationship::DEPENDENCIES, new_id)?;
    duplicate_reference(&prepared.soft_links, link::SOFT_LINKS, new_id)?;

    if let Some(parent) = &prepared.parent {
        require_active(graph, parent, parent::PARENT)?;
    }
    for dependency in &prepared.dependencies {
        require_active(graph, dependency, relationship::DEPENDENCIES)?;
    }
    for link in &prepared.soft_links {
        require_active(graph, link, link::SOFT_LINKS)?;
    }

    if let Some(parent) = &prepared.parent {
        if let Some(cycle) = parent::parent_cycle(graph, new_id, parent) {
            return Err(WaymarkError::cycle_detected(parent::PARENT, cycle));
        }
    }
    for dependency in &prepared.dependencies {
        if let Some(cycle) = relationship::dependency_cycle(graph, new_id, dependency) {
            return Err(WaymarkError::cycle_detected("dependency", cycle));
        }
    }

    Ok(())
}

/// Refuse the same normalized ID named twice in one relation list.
fn duplicate_reference(
    ids: &[WorkItemId],
    relation: &str,
    new_id: &WorkItemId,
) -> Result<(), WaymarkError> {
    for pair in ids.windows(2) {
        if pair[0] == pair[1] {
            return Err(WaymarkError::invalid_relationship(
                relation,
                new_id,
                &pair[0],
                "the reference appears more than once",
            ));
        }
    }
    Ok(())
}

/// Refuse a target that is missing or archived, using the codes the standalone
/// relationship operations use.
fn require_active(graph: &Graph, target: &WorkItemId, relation: &str) -> Result<(), WaymarkError> {
    if graph.get(target).is_some() {
        Ok(())
    } else if graph.is_archived(target) {
        Err(WaymarkError::item_archived(target, "create"))
    } else {
        Err(WaymarkError::referenced_item_not_found(target, relation))
    }
}
