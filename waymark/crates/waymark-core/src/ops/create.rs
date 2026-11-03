//! `waymark create`: write one new work item.
//!
//! Creation is the only operation that mints identity. It allocates the stable
//! ID, stamps `created_at` and `updated_at`, and writes the document atomically
//! under the store lock. Callers cannot supply any of those values.
//!
//! # Required input
//!
//! `title`, `category`, and `owner` are required and are never derived from one
//! another or from the environment. A missing or malformed one fails the whole
//! creation with `VALIDATION_ERROR` naming every offending field — Waymark never
//! writes a partial item and never guesses a title from a body.
//!
//! Custom fields are not an input here: declaring them belongs to the
//! configuration surface, and editing them to the field-editing operation. A
//! new item simply starts with none.

use camino::Utf8PathBuf;

use crate::error::{ValidationIssue, WaymarkError};
use crate::location::Location;
use crate::model::{CategoryKey, CustomFields, Status, WorkItem, FORMAT_GENERATION};
use crate::ops::OpOptions;
use crate::serialize::write_item_file;
use crate::store::{Fingerprint, Project};
use crate::time::Timestamp;

/// What a caller asks for: the three required fields plus an optional body.
#[derive(Debug, Clone)]
pub struct CreateRequest {
    /// Short single-line label.
    pub title: String,
    /// Category key the item belongs to; must already be declared.
    pub category: String,
    /// Accountable identity.
    pub owner: String,
    /// Optional Markdown body.
    pub body: Option<String>,
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
            body: None,
        }
    }

    /// Attach a body.
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }
}

/// What creation produced.
#[derive(Debug, Clone)]
pub struct Created {
    /// The complete item as written, or as it would have been written on a dry
    /// run.
    pub item: WorkItem,
    /// The document path.
    pub path: Utf8PathBuf,
    /// The fingerprint of the written bytes; `None` on a dry run.
    pub fingerprint: Option<Fingerprint>,
    /// Whether the store changed.
    pub written: bool,
}

/// Create one work item in `project`.
pub fn create(
    project: &Project,
    request: CreateRequest,
    options: &OpOptions,
) -> Result<Created, WaymarkError> {
    let prepared = prepare(project, request)?;

    // The lock is taken *before* the inventory: an ID allocated against a stale
    // inventory could collide with an item another writer just committed.
    if options.dry_run {
        return prepare_only(project, prepared, options);
    }

    crate::lock::with_project_lock(project, options.lock_wait, || {
        commit(project, prepared, options)
    })
}

/// The dry-run half of [`create`]: everything but the write.
fn prepare_only(
    project: &Project,
    prepared: Prepared,
    options: &OpOptions,
) -> Result<Created, WaymarkError> {
    let inventory = project.inventory()?;
    let id = project.allocate_id(&inventory)?;
    let path = project.path_for(&id, &prepared.category)?;
    let item = build_item(id, prepared, options);
    Ok(Created {
        item,
        path,
        fingerprint: None,
        written: false,
    })
}

/// The write half of [`create`], run while the store lock is held.
fn commit(
    project: &Project,
    prepared: Prepared,
    options: &OpOptions,
) -> Result<Created, WaymarkError> {
    let inventory = project.inventory()?;
    let id = project.allocate_id(&inventory)?;
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

    let item = build_item(id, prepared, options);

    // Writing goes through the canonical serializer, which renders the whole
    // document in memory first: a writer fault surfaces as an error instead of a
    // half-written file.
    let fingerprint = write_item_file(&item, &path)?;

    Ok(Created {
        item,
        path,
        fingerprint: Some(fingerprint),
        written: true,
    })
}

/// The item creation produces, before it is written.
fn build_item(id: crate::id::WorkItemId, prepared: Prepared, options: &OpOptions) -> WorkItem {
    let now = Timestamp::new(options.clock.now());
    WorkItem {
        id,
        title: prepared.title,
        category: prepared.category,
        status: Status::Open,
        owner: prepared.owner,
        creator: options.operator().map(str::to_owned),
        created_at: now,
        updated_at: now,
        closed_at: None,
        parent: None,
        dependencies: Vec::new(),
        soft_links: Vec::new(),
        custom_fields: CustomFields::new(),
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
    body: Option<String>,
}

fn prepare(project: &Project, request: CreateRequest) -> Result<Prepared, WaymarkError> {
    let mut issues = Vec::new();

    let title = request.title.trim();
    if title.is_empty() {
        issues.push(ValidationIssue::new("title", "a title is required"));
    } else if title.contains('\n') || title.contains('\r') {
        issues.push(ValidationIssue::new(
            "title",
            "a title must be a single line",
        ));
    }

    let category = {
        let raw = request.category.trim();
        if raw.is_empty() {
            issues.push(ValidationIssue::new("category", "a category is required"));
            None
        } else {
            match CategoryKey::parse(raw) {
                Ok(key) if project.category(&key).is_some() => Some(key),
                Ok(key) => {
                    issues.push(ValidationIssue::new(
                        "category",
                        format!("{key} is not a category declared in the project configuration"),
                    ));
                    None
                }
                Err(error) => {
                    issues.push(ValidationIssue::new("category", error.to_string()));
                    None
                }
            }
        }
    };

    let owner = request.owner.trim();
    if owner.is_empty() {
        issues.push(ValidationIssue::new("owner", "an owner is required"));
    } else if owner.contains('\n') || owner.contains('\r') {
        issues.push(ValidationIssue::new(
            "owner",
            "an owner must be a single line",
        ));
    }

    if !issues.is_empty() {
        return Err(
            WaymarkError::validation("create", issues).with_location(Location::field("create"))
        );
    }

    Ok(Prepared {
        title: title.to_owned(),
        category: category.expect("validation reported a missing category"),
        owner: owner.to_owned(),
        body: request.body,
    })
}
