//! Shared JSON renderers for the machine forms adapters publish.
//!
//! The CLI and MCP adapters render the same domain values: one item, one tree
//! node, one flat dependency row, one configuration document, one collection
//! page, and one mediated-write result. Those payloads are part of the contract
//! (the CLI tests pin them byte for byte), so they must not be re-derived in
//! two places where one can drift from the other.
//!
//! This module is the single dependency-free home for that rendering. It reads
//! the domain types and returns either a [`serde_json::Value`] or one of the
//! small display values below; adapters decide where to print and wrap the
//! values in their envelope. Nothing here reads a file, takes a lock, or
//! decides whether a write is allowed.
//!
//! # The shapes
//!
//! - [`item_fields`] — persisted fields, no path, no derived columns.
//! - [`item_json`] — [`item_fields`] plus the document path.
//! - [`tree_node_json`] — one nested tree node; the embedded item has no body.
//! - [`dependency_row_json`] — one flat dependency row.
//! - [`row_json`] — one projected list row (delegates to
//!   [`query::projected_row`](crate::query::projected_row)).
//! - [`read_json`] — one scoped item read (`show`).
//! - [`config_json`] — one configuration document.
//! - [`listing_summary_json`] — the window counts of a collection page.
//! - [`listing_json`] — the complete collection page (summary, rows, projection).
//! - [`write_result_json`] / [`write_result_json_with`] — the shared
//!   `{id, path, …, changed, written, item}` core of every write result.
//! - [`archive_leftovers_json`] — the documents an archive deliberately did
//!   not rewrite, each with the relations that still point at the target.
//! - [`archive_leftovers_summary`] — the one human sentence that precedes them.
//! - [`soft_link_findings`] / [`SoftLinkDisplacement`] / [`Displacement`] — the
//!   one diagnosis of a soft link that is stale (its partner is archived) or
//!   one-sided (its partner never recorded it). `doctor` turns it into
//!   registry findings, `show` renders it under `relationships.soft_link_issues`,
//!   and neither ever offers a command.
//! - [`DisplayState`] / [`display_state`] / [`blocked_reason`] /
//!   [`closed_parent_reason`] — the one derivation of the display state and the
//!   human blocked-reason and closed-parent prose.
//!
//! # The derived display state
//!
//! The persisted `status` is lifecycle, not a verdict: an `open` item can still
//! be blocked, and a blocked `in_progress` item should read as blocked. Every
//! human-facing adapter needs the same four-value answer, so the precedence
//! lives here rather than in each adapter: `closed` → `blocked` → `in_progress`
//! → `ready`. The prose that explains a block
//! (`waiting on wm-… (unfinished|archived|missing), …`) is part of the same
//! rule, because the label for a dependency comes from the graph.

use std::fmt;

use camino::Utf8Path;
use serde_json::{json, Map, Value};

use crate::config::Config;
use crate::envelope::Finding;
use crate::id::WorkItemId;
use crate::model::{Status, WorkItem};
use crate::ops::list::Listing;
use crate::ops::tree::{DependencyRow, TreeNode};
use crate::projection::ScopedRead;
use crate::query::{ArchiveRange, ListedItem, Projection};
use crate::readiness::{Graph, Readiness};

/// The display state a reader sees for one work item.
///
/// This is a rendering of the persisted lifecycle plus derived readiness, not a
/// fourth persisted status. The precedence is `closed` → `blocked` →
/// `in_progress` → `ready`:
///
/// - a closed or archived item is [`Closed`](Self::Closed), whatever readiness
///   says;
/// - an active item with an unfinished, archived, or missing dependency is
///   [`Blocked`](Self::Blocked), even when its status is `in_progress`;
/// - an unblocked `in_progress` item is [`InProgress`](Self::InProgress);
/// - every other active item is [`Ready`](Self::Ready).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayState {
    /// Active and nothing outstanding.
    Ready,
    /// Active, unblocked, and already being worked on.
    InProgress,
    /// Active with at least one unfinished, archived, or missing dependency.
    Blocked,
    /// Closed, or archived (historical rather than work anybody can start).
    Closed,
}

impl DisplayState {
    /// The stable literal the Web API publishes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::InProgress => "in_progress",
            Self::Blocked => "blocked",
            Self::Closed => "closed",
        }
    }

    /// Parse the stable literal.
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "ready" => Some(Self::Ready),
            "in_progress" => Some(Self::InProgress),
            "blocked" => Some(Self::Blocked),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }
}

impl fmt::Display for DisplayState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Derive the display state of `item` from its readiness.
///
/// Closed (including archived) wins before blocked, and blocked wins before
/// in-progress: the persisted status is sent separately, so a blocked
/// in-progress item is primarily blocked here.
pub fn display_state(readiness: &Readiness, item: &WorkItem) -> DisplayState {
    if item.is_archived() || item.status.is_closed() {
        DisplayState::Closed
    } else if readiness.blocked {
        DisplayState::Blocked
    } else if item.status == Status::InProgress {
        DisplayState::InProgress
    } else {
        DisplayState::Ready
    }
}

/// Human-readable reason for a derived block, or `None` when not blocked.
///
/// The prose names every dependency in `readiness.blocked_by` with the fact
/// that makes it an obstacle: an active item is `unfinished`, a known but
/// archived item is `archived`, and an ID the graph does not know is `missing`.
pub fn blocked_reason(graph: &Graph, readiness: &Readiness) -> Option<String> {
    if !readiness.blocked {
        return None;
    }
    let parts: Vec<String> = readiness
        .blocked_by
        .iter()
        .map(|id| {
            let label = if graph.get(id).is_some() {
                "unfinished"
            } else if graph.is_archived(id) {
                "archived"
            } else {
                "missing"
            };
            format!("{} ({label})", id.as_str())
        })
        .collect();
    Some(format!("waiting on {}", parts.join(", ")))
}

/// The human line for a list-family row whose direct parent is closed.
///
/// One adapter (the CLI listing) prints it and no machine form carries prose,
/// so the sentence lives here rather than in the adapter: a second surface that
/// wants to say the same thing says it the same way. `None` when the row has no
/// closed parent, which is the ordinary case.
pub fn closed_parent_reason(closed_parent: &Option<crate::id::WorkItemId>) -> Option<String> {
    closed_parent
        .as_ref()
        .map(|parent| format!("parent {} is closed", parent.as_str()))
}

/// What is wrong with one soft link, as one diagnosis.
///
/// A soft link is stored on both documents, and the two are written by two
/// owners under the consent rules, so the pair can disagree. These are the two
/// ways it does, and they are different facts:
///
/// - [`Displacement::PartnerArchived`] — the link is intact but its partner has
///   been archived: the association is history, and stale.
/// - [`Displacement::PartnerSilent`] — the link is recorded here and not on the
///   partner's document: the partner never recorded it, and is still live.
///
/// `partner_owner` is the partner's `owner` as recorded on its own document,
/// so a reader knows whose document is missing the link without opening it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoftLinkDisplacement {
    /// The partner the link points at.
    pub partner: WorkItemId,
    /// Which of the two disagreements this is.
    pub kind: Displacement,
    /// Who owns the partner document.
    pub partner_owner: String,
}

/// The two ways a soft link's two sides can disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Displacement {
    /// The partner document is archived and holds the link as history.
    PartnerArchived,
    /// The partner document is live but does not record the link back.
    PartnerSilent,
}

impl Displacement {
    /// Which side of the pair is not there.
    ///
    /// `"link"` is the ordinary one-sided state, where the partner document is
    /// live and simply never recorded the association; `"archived"` is the
    /// partner itself. Machine callers read this, not the prose.
    pub fn missing_side(self) -> &'static str {
        match self {
            Self::PartnerArchived => "archived",
            Self::PartnerSilent => "link",
        }
    }

    /// The prose for this kind, naming the partner.
    pub fn reason(self, partner: &WorkItemId) -> String {
        match self {
            Self::PartnerArchived => {
                format!("{partner} is archived and keeps the association as history only")
            }
            Self::PartnerSilent => {
                format!("the soft link exists on one side only: {partner} never recorded it")
            }
        }
    }
}

/// Every soft link of one document whose two sides disagree.
///
/// Returned in the document's stored soft-link order. A partner that does not
/// exist at all is not included: the missing-target check owns that case, and a
/// document that cannot be read is left alone rather than guessed about.
pub fn soft_link_findings(
    item: &WorkItem,
    projection: &crate::projection::Projection,
) -> Vec<SoftLinkDisplacement> {
    item.soft_links
        .iter()
        .filter_map(|partner| {
            let partner_item = &projection.entry(partner)?.item;
            let kind = if partner_item.is_archived() {
                Displacement::PartnerArchived
            } else if !partner_item.soft_links.contains(&item.id) {
                Displacement::PartnerSilent
            } else {
                return None;
            };
            Some(SoftLinkDisplacement {
                partner: partner.clone(),
                kind,
                partner_owner: partner_item.owner.clone(),
            })
        })
        .collect()
}

/// Read one displacement as the registry finding it is.
///
/// This is the one place the two disagreements become codes, so `doctor` and
/// any future surface cannot disagree about which code a case gets. Neither
/// finding is fixable: completing or dropping the other side rewrites a
/// document its owner may not have consented to, so the diagnosis states the
/// fact and stops there.
pub fn soft_link_finding(item: &WorkItem, displacement: &SoftLinkDisplacement) -> Finding {
    match displacement.kind {
        Displacement::PartnerArchived => Finding::new(
            crate::error::Code::DanglingReference,
            format!(
                "{} soft_links the archived item {}",
                item.id.as_str(),
                displacement.partner.as_str()
            ),
        )
        .with_location(crate::location::Location::work_item(&item.id)),
        Displacement::PartnerSilent => Finding::from_error(
            &crate::error::WaymarkError::one_sided_soft_link(&item.id, &displacement.partner),
        ),
    }
}

/// The machine form of one soft-link disagreement, without any command.
///
/// The same shape is rendered by `show` and named by `doctor`, so an adapter
/// only decides where to print it:
///
/// ```json
/// {"partner": "wm-…", "partner_owner": "frank", "missing": "link",
///  "reason": "the soft link exists on one side only: wm-… never recorded it"}
/// ```
fn soft_link_issue_json(displacement: &SoftLinkDisplacement) -> Value {
    json!({
        "partner": displacement.partner.as_str(),
        "partner_owner": displacement.partner_owner,
        "missing": displacement.kind.missing_side(),
        "reason": displacement.kind.reason(&displacement.partner),
    })
}

/// The persisted fields of one item, without the file that happens to hold it.
///
/// `show` names the document; a page of many items does not, because the path is
/// a fact about one file and a listing is not about files.
pub fn item_fields(item: &WorkItem) -> Value {
    json!({
        "schema": item.schema,
        "needs_migration": item.needs_migration(),
        "id": item.id.as_str(),
        "title": item.title,
        "category": item.category.as_str(),
        "status": item.status.as_str(),
        "owner": item.owner,
        "creator": item.creator,
        "created_at": item.created_at.to_string(),
        "updated_at": item.updated_at.to_string(),
        "closed_at": item.closed_at.map(|value| value.to_string()),
        "parent": item.parent.as_ref().map(|id| id.as_str().to_owned()),
        "dependencies": item.dependencies.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        "soft_links": item.soft_links.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        "custom_fields": item.custom_fields,
        "archived_at": item.archived_at.map(|value| value.to_string()),
        "archive_reason": item.archive_reason,
        "body": item.body,
    })
}

/// The human display forms of one item's persisted timestamps, in `zone`.
///
/// Persisted timestamps stay UTC in every machine field; this is the trio a
/// human-facing surface renders. Producing it here keeps the zone choice and the
/// format in one place, so no adapter can render a different local context. The
/// zone travels with the values, so a reader can say which context produced
/// them.
pub fn item_display_times_json(item: &WorkItem, zone: chrono_tz::Tz) -> Value {
    let render = |value: crate::time::Timestamp| crate::time::format_local(value.get(), zone);
    json!({
        "timezone": zone.name(),
        "created_at": render(item.created_at),
        "updated_at": render(item.updated_at),
        "closed_at": item.closed_at.map(render),
    })
}

/// The persisted fields plus the document path, for a read that names one file.
pub fn item_json(item: &WorkItem, path: &Utf8Path) -> Value {
    let mut value = item_fields(item);
    if let Some(map) = value.as_object_mut() {
        map.insert("path".to_owned(), json!(path.to_string()));
    }
    value
}

/// The nested machine form of one tree node.
///
/// The embedded item keeps every persisted field except the body: a tree is a
/// relationship view, not a document view.
pub fn tree_node_json(node: &TreeNode) -> Value {
    json!({
        "id": node.id.as_str(),
        "depth": node.depth,
        "repeated": node.repeated,
        "resolved": node.item.is_some(),
        "item": node.item.as_ref().map(|item| {
            let mut fields = item_fields(item);
            if let Some(map) = fields.as_object_mut() {
                map.remove("body");
            }
            fields
        }),
        "children": node.children.iter().map(tree_node_json).collect::<Vec<_>>(),
    })
}

/// The machine form of one flat dependency row.
pub fn dependency_row_json(row: &DependencyRow) -> Value {
    json!({
        "id": row.id.as_str(),
        "depth": row.depth,
        "from": row.from.as_ref().map(|id| id.as_str()),
        "repeated": row.repeated,
        "resolved": row.item.is_some(),
    })
}

/// One projected list row: persisted fields under `item`, derived columns at the
/// top level.
///
/// This is the same value [`query::projected_row`](crate::query::projected_row)
/// returns; it lives here so an adapter has one import for every machine form.
pub fn row_json(row: &ListedItem, projection: &Projection) -> Value {
    crate::query::projected_row(row, projection)
}

/// The machine form of the shared project configuration document.
pub fn config_json(config: &Config) -> Value {
    json!({
        "version": config.version.to_string(),
        "generation": config.version.generation,
        "revision": config.version.revision,
        "storage_root": config.storage_root,
        "categories": config
            .categories
            .iter()
            .map(|(key, category)| {
                (
                    key.as_str().to_owned(),
                    json!({
                        "key": key.as_str(),
                        "path": category.path,
                        "name": category.name,
                    }),
                )
            })
            .collect::<Map<String, Value>>(),
        "fields": config
            .fields
            .iter()
            .map(|(key, declaration)| {
                (
                    key.as_str().to_owned(),
                    json!({
                        "key": key.as_str(),
                        "type": declaration.ty.as_str(),
                        "required": declaration.required,
                        "default": declaration.default,
                    }),
                )
            })
            .collect::<Map<String, Value>>(),
    })
}

/// The machine form of the local user configuration document.
///
/// `null` settings are omitted rather than rendered as `null`, so "the document
/// does not state it" and "the document states a value" stay distinguishable in
/// one glance.
pub fn local_config_json(local: Option<&crate::user_config::UserConfig>) -> Value {
    let Some(local) = local else {
        return Value::Null;
    };
    let mut document = Map::new();
    if let Some(operator) = &local.operator {
        document.insert("operator".to_owned(), json!(operator));
    }
    if let Some(zone) = local.timezone {
        document.insert("timezone".to_owned(), json!(zone.name()));
    }
    if !local.cache.is_absent() {
        let mut cache = Map::new();
        if let Some(enabled) = local.cache.enabled {
            cache.insert("enabled".to_owned(), json!(enabled));
        }
        if let Some(capacity) = local.cache.capacity {
            cache.insert("capacity".to_owned(), json!(capacity));
        }
        document.insert("cache".to_owned(), Value::Object(cache));
    }
    Value::Object(document)
}

/// The machine form of the effective values and the source of each.
///
/// One entry per setting, each naming the value and where it came from, so an
/// adapter renders precedence instead of deciding it.
pub fn effective_config_json(effective: &crate::user_config::EffectiveConfig) -> Value {
    json!({
        "operator": effective.operator,
        "timezone": effective.timezone.name(),
        "cache": {
            "enabled": effective.cache.enabled,
            "capacity": effective.cache.capacity,
        },
    })
}

/// Where each effective value came from, as its own structure.
///
/// The values and their provenance are separate objects on purpose: a client
/// reads the values as configuration, and the sources as an explanation, and
/// neither has to be unpacked from the other.
pub fn effective_sources_json(effective: &crate::user_config::EffectiveConfig) -> Value {
    json!({
        "operator": effective.operator_source.as_str(),
        "timezone": effective.timezone_source.as_str(),
        "cache": {
            "enabled": effective.cache_enabled_source.as_str(),
            "capacity": effective.cache_capacity_source.as_str(),
        },
    })
}

/// The machine form of both documents plus the resolved values.
///
/// This is what `config show` prints in machine mode and what the MCP read tool
/// returns, so the CLI and the MCP server cannot disagree about what "the
/// configuration" is.
pub fn config_bundle_json(bundle: &crate::ops::local_config::Bundle) -> Value {
    json!({
        "project": config_json(&bundle.project),
        "local": local_config_json(bundle.local.as_ref()),
        "effective": effective_config_json(&bundle.effective),
        "sources": effective_sources_json(&bundle.effective),
    })
}

/// The envelope metadata a configuration read carries.
///
/// Resolution can warn (an undetectable machine zone falls back to UTC), and a
/// warning that only the human renderer shows is not reported at all, so the
/// metadata is built here and both adapters pass it through.
pub fn config_bundle_meta(bundle: &crate::ops::local_config::Bundle) -> crate::envelope::Meta {
    crate::envelope::Meta::direct_file_scoped().with_warnings(bundle.effective.warnings.clone())
}

/// One shared-configuration write: the resulting document and its revision move.
///
/// Every adapter returns exactly this shape, so a config write cannot look
/// different through the CLI and MCP.
pub fn config_outcome_json(outcome: &crate::ops::config::ConfigOutcome) -> Value {
    json!({
        "config": config_json(&outcome.config),
        "changed": outcome.changed,
        "changed_keys": outcome.changed_keys,
        "previous_revision": outcome.previous_revision,
        "revision": outcome.revision,
    })
}

/// One local-configuration write: the local document, never a revision move.
///
/// The shared revision is reported unchanged because a local write cannot move
/// it; reporting it keeps the two write results one shape.
pub fn local_outcome_json(outcome: &crate::ops::local_config::LocalOutcome) -> Value {
    json!({
        "local": local_config_json(Some(&outcome.local)),
        "changed": outcome.changed,
        "changed_keys": outcome.changed_keys,
        "previous_revision": outcome.previous_revision,
        "revision": outcome.revision,
    })
}

/// The window counts of one collection page.
///
/// Counts only: the caller merges these into the page it is building, or uses
/// [`listing_json`] for the complete document the CLI and MCP return.
pub fn listing_summary_json(listing: &Listing) -> Value {
    json!({
        "count": listing.len(),
        "total": listing.total,
        "offset": listing.offset,
        "limit": listing.limit,
    })
}

/// The complete machine form of one collection page.
///
/// This is the CLI's `list`/`ready`/`blocked` data block and the MCP listing
/// data block: the summary counts, the projected rows, the archive slice that
/// was read, and the projection the rows were rendered through.
pub fn listing_json(listing: &Listing, projection: &Projection, archive: ArchiveRange) -> Value {
    let mut value = listing_summary_json(listing);
    if let Some(map) = value.as_object_mut() {
        map.insert(
            "items".to_owned(),
            Value::Array(
                listing
                    .items
                    .iter()
                    .map(|row| row_json(row, projection))
                    .collect(),
            ),
        );
        map.insert("archive".to_owned(), json!(archive.as_str()));
        map.insert(
            "fields".to_owned(),
            match &projection.fields {
                Some(fields) => json!(fields
                    .iter()
                    .map(|field| field.as_str())
                    .collect::<Vec<_>>()),
                None => Value::Null,
            },
        );
        map.insert("compact".to_owned(), json!(projection.compact));
    }
    value
}

/// The machine form of one scoped item read, matching `show --json`.
pub fn read_json(read: &ScopedRead) -> Value {
    json!({
        "id": read.item.id.as_str(),
        "path": read.path.to_string(),
        "category": read.category.as_str(),
        "fingerprint": read.fingerprint.as_str(),
        "item": item_json(&read.item, &read.path),
        "needs_migration": read.item.needs_migration(),
        "archived": read.item.is_archived(),
        "relationships": {
            // An archived item's own edges are history: they no longer
            // participate in the active graph, so the payload says so rather
            // than letting a reader mistake them for live ones.
            "historical": read.item.is_archived(),
            "parent": read.item.parent.as_ref().map(|id| id.as_str().to_owned()),
            "dependencies": read.item.dependencies.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
            "soft_links": read.item.soft_links.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
            "children": read.children.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
            "dependents": read.dependents.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
            // A soft link whose two sides disagree. `missing` names the
            // document that does not record the link: "archived" means the
            // partner is gone, "link" means it is live and never recorded it.
            // The diagnosis is the whole answer; no repair command is offered.
            "soft_link_issues": read
                .soft_link_findings
                .iter()
                .map(soft_link_issue_json)
                .collect::<Vec<_>>(),
        },
        "ready": read.readiness.as_ref().map(|readiness| readiness.ready),
        "blocked": read.readiness.as_ref().map(|readiness| readiness.blocked),
        "blocked_by": read
            .readiness
            .as_ref()
            .map(|readiness| {
                readiness
                    .blocked_by
                    .iter()
                    .map(|id| id.as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    })
}

/// The core of every mediated-write result: `{id, path, changed, written, item}`.
///
/// An adapter adds the fields its operation owns (`previous_status`, `verb`,
/// `fingerprints`, …) with [`write_result_json_with`]. `written` is the
/// caller's fact, not something this function can recompute; the domain
/// constructors are what keep it equal to the presence of a fingerprint.
pub fn write_result_json(item: &WorkItem, path: &Utf8Path, changed: bool, written: bool) -> Value {
    write_result_json_with(item, path, changed, written, Map::new())
}

/// [`write_result_json`] plus adapter-specific fields.
///
/// `extra` is merged first, so an extra key can never replace the shared
/// `id`, `path`, `changed`, `written`, or `item` value.
pub fn write_result_json_with(
    item: &WorkItem,
    path: &Utf8Path,
    changed: bool,
    written: bool,
    extra: Map<String, Value>,
) -> Value {
    let mut value = extra;
    value.insert("id".to_owned(), json!(item.id.as_str()));
    value.insert("path".to_owned(), json!(path.to_string()));
    value.insert("changed".to_owned(), json!(changed));
    value.insert("written".to_owned(), json!(written));
    value.insert("item".to_owned(), item_json(item, path));
    Value::Object(value)
}

/// The documents an archive deliberately left as they are.
///
/// One entry per document, in the outcome's stable-ID order, each carrying the
/// relation kinds it still points at the archived item with:
///
/// ```json
/// [{"id": "wm-…", "path": "work/task/wm-….md",
///   "relations": ["dependencies", "soft_links"]}]
/// ```
///
/// The document comes first and its relations second because the reader's
/// question is "whose file", then "why it matters". The path is the document
/// the reader has to open; the relations are persisted field names, so the
/// same literals appear here as in `removals`. An archive that left nothing
/// alone renders an empty list, which is the ordinary quiet result.
pub fn archive_leftovers_json(
    untouched: &[crate::ops::archive::UntouchedItem],
    relations: &[crate::ops::archive::UntouchedRelation],
) -> Value {
    Value::Array(
        untouched
            .iter()
            .map(|item| {
                let kinds: Vec<&str> = relations
                    .iter()
                    .filter(|relation| relation.from == item.id)
                    .map(|relation| relation.relation.as_str())
                    .collect();
                json!({
                    "id": item.id.as_str(),
                    "path": item.path.as_str(),
                    "relations": kinds,
                })
            })
            .collect(),
    )
}

/// The soft-link partners a creation deliberately left as they are.
///
/// The create twin of [`archive_leftovers_json`]: one entry per partner, in the
/// outcome's stable-ID order, with the document and the path its owner would
/// open. No relation list, because a creation references a partner one way only
/// — the new item's `soft_links` — and that side is always written. A creation
/// that left nothing alone renders an empty list.
pub fn leftover_partners_json(untouched: &[crate::ops::create::UntouchedPartner]) -> Value {
    Value::Array(
        untouched
            .iter()
            .map(|partner| {
                json!({
                    "id": partner.item.id.as_str(),
                    "path": partner.path.as_str(),
                })
            })
            .collect(),
    )
}

/// The `data` block of one archive, shared by every adapter.
///
/// It describes the archive itself — the marker, the reason, the status it left
/// alone, whether it was applied — then the three footprint lists: the
/// relationships `removals` dropped, the documents `affected` it rewrote, and
/// the documents `untouched` it left exactly as they were with the relations
/// that still name the target. The lists are rendered from the outcome alone,
/// so an adapter never re-reads the project to describe what just happened, and
/// a dry run carries the same block with `written: false`.
pub fn archive_result_json(outcome: &crate::ops::archive::ArchiveOutcome) -> Value {
    json!({
        "id": outcome.item.id.as_str(),
        "path": outcome.path.as_str(),
        "archived_at": outcome.archived_at.to_string(),
        "archive_reason": outcome.archive_reason,
        "status": outcome.item.status.as_str(),
        "closed_at": outcome.item.closed_at.map(|value| value.to_string()),
        "written": outcome.written,
        "removals": outcome.removals.iter().map(|removal| json!({
            "from": removal.from.as_str(),
            "relation": removal.relation.as_str(),
        })).collect::<Vec<_>>(),
        "affected": outcome
            .affected
            .iter()
            .map(|item| json!({
                "id": item.id.as_str(),
                "path": item.path.as_str(),
            }))
            .collect::<Vec<_>>(),
        "untouched": archive_leftovers_json(&outcome.untouched, &outcome.untouched_relations),
        "item": item_fields(&outcome.item),
    })
}

/// The one human sentence that precedes an archive's leftover list.
///
/// It names the target and the count so the line stands on its own in a
/// terminal where the list below it may scroll away, and it says "left alone"
/// rather than "skipped", which sounds like a failure: the documents were
/// deliberately preserved, not neglected. `None` when nothing was left alone,
/// because a write with nothing to report stays quiet.
pub fn archive_leftovers_summary(target: &WorkItemId, untouched: usize) -> Option<String> {
    match untouched {
        0 => None,
        1 => Some(format!(
            "{} still referenced by 1 document left untouched",
            target.as_str()
        )),
        count => Some(format!(
            "{} still referenced by {count} documents left untouched",
            target.as_str()
        )),
    }
}

/// The partner a one-sided soft-link change left as it is.
///
/// `None` when the change wrote both sides, or when the partner's document is
/// missing or archived and there was nothing to leave alone:
///
/// ```json
/// {"id": "wm-…", "path": "work/task/wm-….md", "owner": "frank", "owned": true}
/// ```
///
/// `owned` says whether consent would have been enough to rewrite it, which is
/// the question a caller asks after seeing that the partner was left alone.
pub fn link_untouched_json(untouched: &crate::ops::link::UntouchedLink) -> Value {
    json!({
        "id": untouched.id.as_str(),
        "path": untouched.path.as_str(),
        "owner": untouched.owner,
        "owned": untouched.owned,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Category, FieldDecl, FieldType};
    use crate::envelope::Meta;
    use crate::model::{FieldKey, FieldValue, Status};
    use crate::ops::tree::TreeNode;
    use crate::query::FieldName;
    use crate::readiness::{Graph, Readiness};
    use crate::store::Fingerprint;
    use crate::testing::{fixtures, instant};
    use crate::WorkItemId;

    fn id(literal: &str) -> WorkItemId {
        WorkItemId::parse(literal).expect("a valid stable ID")
    }

    fn item_with_status(status: Status) -> WorkItem {
        fixtures::work_item("wm-000000000001", "task", "Fix the thing")
            .with_status(status)
            .build()
    }

    fn blocked_readiness() -> Readiness {
        Readiness {
            ready: false,
            blocked: true,
            blocked_by: vec![id("wm-000000000003")],
        }
    }

    #[test]
    fn display_state_is_closed_then_blocked_then_in_progress_then_ready() {
        // Closed beats blocked: a closed item is never waiting on anything.
        let closed = item_with_status(Status::Closed);
        assert_eq!(
            display_state(&blocked_readiness(), &closed),
            DisplayState::Closed
        );
        // Archived is historical, so it reads closed even with a ready verdict.
        let mut archived = item_with_status(Status::InProgress);
        archived.archived_at = Some(instant());
        assert_eq!(
            display_state(&Readiness::ready(), &archived),
            DisplayState::Closed
        );
        // Blocked beats in-progress: the persisted status is sent separately.
        let in_progress = item_with_status(Status::InProgress);
        assert_eq!(
            display_state(&blocked_readiness(), &in_progress),
            DisplayState::Blocked
        );
        // Unblocked in-progress is its own state, and everything else is ready.
        assert_eq!(
            display_state(&Readiness::ready(), &in_progress),
            DisplayState::InProgress
        );
        let open = item_with_status(Status::Open);
        assert_eq!(
            display_state(&Readiness::ready(), &open),
            DisplayState::Ready
        );
    }

    #[test]
    fn display_state_literals_round_trip() {
        for state in [
            DisplayState::Ready,
            DisplayState::InProgress,
            DisplayState::Blocked,
            DisplayState::Closed,
        ] {
            assert_eq!(DisplayState::parse(state.as_str()), Some(state));
            assert_eq!(state.to_string(), state.as_str());
        }
        assert_eq!(DisplayState::parse("done"), None);
    }

    #[test]
    fn blocked_reason_names_unfinished_archived_and_missing_dependencies() {
        let unfinished = fixtures::work_item("wm-000000000003", "task", "Unfinished").build();
        let mut archived = fixtures::work_item("wm-000000000002", "task", "Archived").build();
        archived.archived_at = Some(instant());
        let mut dependent = fixtures::work_item("wm-000000000001", "task", "Dependent").build();
        dependent.dependencies = vec![
            id("wm-000000000009"), // missing: never created
            id("wm-000000000003"), // active
            id("wm-000000000002"), // archived
        ];
        let graph = Graph::from_items(&[dependent.clone(), unfinished, archived]);
        let readiness = graph.readiness(&dependent.id).expect("an active item");

        assert!(readiness.blocked);
        assert_eq!(
            blocked_reason(&graph, &readiness).as_deref(),
            Some(
                "waiting on wm-000000000002 (archived), wm-000000000003 (unfinished), \
                 wm-000000000009 (missing)"
            )
        );
        assert_eq!(blocked_reason(&graph, &Readiness::ready()), None);
    }

    fn item() -> WorkItem {
        let mut item = fixtures::work_item("wm-000000000001", "task", "Fix the thing")
            .with_body("details\n")
            .build();
        item.creator = Some("alice".to_owned());
        item.parent = Some(id("wm-000000000002"));
        item.dependencies = vec![id("wm-000000000003")];
        item.soft_links = vec![id("wm-000000000004")];
        item.custom_fields
            .insert("effort".to_owned(), FieldValue::Integer(3));
        item
    }

    fn listed() -> ListedItem {
        ListedItem {
            item: item(),
            readiness: Readiness {
                ready: false,
                blocked: true,
                blocked_by: vec![id("wm-000000000003")],
            },
            children: vec![id("wm-000000000005")],
            dependents: vec![],
            needs_migration: false,
            closed_parent: None,
        }
    }

    #[test]
    fn item_fields_has_the_established_persisted_shape() {
        let value = item_fields(&item());
        let object = value.as_object().expect("an object");
        assert_eq!(object.len(), 18, "one field per persisted column + body");
        for key in [
            "schema",
            "needs_migration",
            "id",
            "title",
            "category",
            "status",
            "owner",
            "creator",
            "created_at",
            "updated_at",
            "closed_at",
            "parent",
            "dependencies",
            "soft_links",
            "custom_fields",
            "archived_at",
            "archive_reason",
            "body",
        ] {
            assert!(object.contains_key(key), "missing {key}: {value}");
        }
        assert_eq!(value["schema"], json!(1));
        assert_eq!(value["needs_migration"], json!(false));
        assert_eq!(value["id"], json!("wm-000000000001"));
        assert_eq!(value["title"], json!("Fix the thing"));
        assert_eq!(value["category"], json!("task"));
        assert_eq!(value["status"], json!("open"));
        assert_eq!(value["owner"], json!("frank"));
        assert_eq!(value["creator"], json!("alice"));
        assert_eq!(value["parent"], json!("wm-000000000002"));
        assert_eq!(value["dependencies"], json!(["wm-000000000003"]));
        assert_eq!(value["soft_links"], json!(["wm-000000000004"]));
        assert_eq!(value["custom_fields"], json!({"effort": 3}));
        assert_eq!(value["closed_at"], Value::Null);
        assert_eq!(value["body"], json!("details\n"));
    }

    #[test]
    fn item_json_adds_the_path_to_the_persisted_fields() {
        let path = Utf8Path::new("work/task/wm-000000000001.md");
        let value = item_json(&item(), path);
        assert_eq!(value["path"], json!("work/task/wm-000000000001.md"));
        assert_eq!(value["body"], json!("details\n"));
        let mut without_path = item_fields(&item());
        without_path.as_object_mut().unwrap().remove("body");
        let mut with_path = value;
        with_path.as_object_mut().unwrap().remove("path");
        with_path.as_object_mut().unwrap().remove("body");
        assert_eq!(
            with_path, without_path,
            "item_json differs from item_fields only by path"
        );
    }

    #[test]
    fn tree_node_json_keeps_the_nested_shape_and_drops_the_body() {
        let node = TreeNode {
            id: id("wm-000000000001"),
            item: Some(item()),
            depth: 0,
            repeated: false,
            children: vec![TreeNode {
                id: id("wm-000000000009"),
                item: None,
                depth: 1,
                repeated: true,
                children: Vec::new(),
            }],
        };
        let value = tree_node_json(&node);
        assert_eq!(value["id"], json!("wm-000000000001"));
        assert_eq!(value["depth"], json!(0));
        assert_eq!(value["repeated"], json!(false));
        assert_eq!(value["resolved"], json!(true));
        assert_eq!(value["item"]["title"], json!("Fix the thing"));
        assert!(
            value["item"].get("body").is_none(),
            "a tree node never carries the body"
        );
        assert_eq!(value["children"][0]["id"], json!("wm-000000000009"));
        assert_eq!(value["children"][0]["depth"], json!(1));
        assert_eq!(value["children"][0]["repeated"], json!(true));
        assert_eq!(value["children"][0]["resolved"], json!(false));
        assert_eq!(value["children"][0]["item"], Value::Null);
        assert_eq!(value["children"][0]["children"], json!([]));
    }

    #[test]
    fn dependency_row_json_is_the_flat_shape() {
        let row = DependencyRow {
            id: id("wm-000000000003"),
            item: Some(item()),
            depth: 2,
            from: Some(id("wm-000000000001")),
            repeated: true,
        };
        let value = dependency_row_json(&row);
        assert_eq!(
            value,
            json!({
                "id": "wm-000000000003",
                "depth": 2,
                "from": "wm-000000000001",
                "repeated": true,
                "resolved": true,
            })
        );

        let missing = DependencyRow {
            id: id("wm-000000000009"),
            item: None,
            depth: 0,
            from: None,
            repeated: false,
        };
        let value = dependency_row_json(&missing);
        assert_eq!(value["from"], Value::Null);
        assert_eq!(value["resolved"], json!(false));
        assert!(value.get("item").is_none(), "a flat row carries no item");
    }

    #[test]
    fn row_json_is_the_projected_list_row() {
        let listed = listed();
        let all = Projection::all();
        assert_eq!(
            row_json(&listed, &all),
            crate::query::projected_row(&listed, &all)
        );
        let value = row_json(&listed, &all);
        assert_eq!(value["id"], json!("wm-000000000001"));
        assert_eq!(value["status"], json!("open"));
        assert_eq!(value["item"]["title"], json!("Fix the thing"));
        assert_eq!(value["ready"], json!(false));
        assert_eq!(value["blocked"], json!(true));
        assert_eq!(value["blocked_by"], json!(["wm-000000000003"]));
        assert!(
            value["item"].get("body").is_none(),
            "a listing row never carries the body"
        );

        // The compact projection keeps only the identity columns plus the
        // always-present derived readiness columns that are non-null.
        let compact = row_json(&listed, &Projection::compact());
        assert_eq!(compact["item"]["title"], json!("Fix the thing"));
        assert!(compact.get("children").is_none());

        // An explicit projection names exactly the columns it includes.
        let named = Projection::of([FieldName::Id, FieldName::Title]);
        let value = row_json(&listed, &named);
        assert_eq!(
            value["item"],
            json!({"id": "wm-000000000001", "title": "Fix the thing"})
        );
        assert!(value.get("blocked").is_none());
    }

    #[test]
    fn config_json_renders_the_document_the_adapters_publish() {
        let mut config = Config::fresh();
        config.categories.insert(
            crate::model::CategoryKey::parse("bug").unwrap(),
            Category {
                path: "work/bugs".to_owned(),
                name: "Bugs".to_owned(),
            },
        );
        config.fields.insert(
            FieldKey::parse("effort").unwrap(),
            FieldDecl {
                ty: FieldType::Integer,
                required: true,
                default: Some(FieldValue::Integer(3)),
            },
        );

        let value = config_json(&config);
        assert_eq!(value["version"], json!(config.version.to_string()));
        assert_eq!(value["generation"], json!(1));
        assert_eq!(value["revision"], json!(config.version.revision));
        assert_eq!(value["storage_root"], json!("work"));
        assert!(value.get("operator").is_none());
        assert!(value.get("timezone").is_none());
        assert!(value.get("cache").is_none());
        assert_eq!(value["categories"]["task"]["key"], json!("task"));
        assert_eq!(value["categories"]["task"]["path"], json!("work/task"));
        assert_eq!(value["categories"]["bug"]["name"], json!("Bugs"));
        assert_eq!(value["fields"]["effort"]["key"], json!("effort"));
        assert_eq!(value["fields"]["effort"]["type"], json!("integer"));
        assert_eq!(value["fields"]["effort"]["required"], json!(true));
        assert_eq!(value["fields"]["effort"]["default"], json!(3));
    }

    #[test]
    fn local_and_effective_json_render_every_source() {
        use crate::user_config::{EffectiveConfig, LocalEnvironment, UserConfig};

        let mut user = UserConfig::empty();
        user.operator = Some("frank".to_owned());
        let environment = LocalEnvironment {
            operator: Some("ignored".to_owned()),
            timezone: Some(chrono_tz::Tz::Asia__Shanghai),
        };
        let effective = EffectiveConfig::resolve(Some(&user), &environment);

        let local = local_config_json(Some(&user));
        assert_eq!(local["operator"], json!("frank"));
        assert!(local.get("timezone").is_none());

        let resolved = effective_config_json(&effective);
        assert_eq!(resolved["operator"], json!("frank"));
        assert_eq!(resolved["timezone"], json!("Asia/Shanghai"));
        assert_eq!(resolved["cache"]["capacity"], json!(512));

        // Provenance is its own structure, so a client reads values as
        // configuration and sources as the explanation.
        let sources = effective_sources_json(&effective);
        assert_eq!(sources["operator"], json!("local_document"));
        assert_eq!(sources["timezone"], json!("system_time_zone"));
        assert_eq!(sources["cache"]["enabled"], json!("built_in_default"));
        assert_eq!(sources["cache"]["capacity"], json!("built_in_default"));
        assert_eq!(local_config_json(None), Value::Null);
    }

    #[test]
    fn a_bundle_carries_its_resolution_warnings_as_envelope_metadata() {
        use crate::user_config::{EffectiveConfig, LocalEnvironment};

        let effective = EffectiveConfig::resolve(None, &LocalEnvironment::isolated());
        let bundle = crate::ops::local_config::Bundle {
            project: crate::config::Config::fresh(),
            local: None,
            effective,
        };
        let meta = config_bundle_meta(&bundle);
        assert_eq!(meta.warnings.len(), 1, "{:?}", meta.warnings);
        assert_eq!(
            meta.warnings[0].code,
            crate::error::Code::TimeZoneUndetected
        );

        // A resolvable zone produces no warning at all.
        let effective = EffectiveConfig::resolve(
            None,
            &LocalEnvironment {
                operator: None,
                timezone: Some(chrono_tz::Tz::UTC),
            },
        );
        let bundle = crate::ops::local_config::Bundle {
            project: crate::config::Config::fresh(),
            local: None,
            effective,
        };
        assert!(config_bundle_meta(&bundle).warnings.is_empty());
    }

    fn listing() -> Listing {
        Listing {
            items: vec![listed()],
            total: 7,
            offset: 2,
            limit: 25,
            meta: Meta::memory_projection_complete(),
        }
    }

    #[test]
    fn listing_summary_json_counts_the_window() {
        let value = listing_summary_json(&listing());
        assert_eq!(
            value,
            json!({"count": 1, "total": 7, "offset": 2, "limit": 25})
        );
    }

    #[test]
    fn listing_json_is_the_complete_collection_page() {
        let listing = listing();
        let all = Projection::all();
        let value = listing_json(&listing, &all, ArchiveRange::Active);
        assert_eq!(value["count"], json!(1));
        assert_eq!(value["total"], json!(7));
        assert_eq!(value["offset"], json!(2));
        assert_eq!(value["limit"], json!(25));
        assert_eq!(value["archive"], json!("active"));
        assert_eq!(value["compact"], json!(false));
        assert_eq!(value["fields"], Value::Null);
        assert_eq!(value["items"].as_array().unwrap().len(), 1);
        assert_eq!(value["items"][0]["id"], json!("wm-000000000001"));

        let named = Projection::of([FieldName::Id, FieldName::Title]);
        let value = listing_json(&listing, &named, ArchiveRange::Archived);
        assert_eq!(value["archive"], json!("archived"));
        assert_eq!(value["fields"], json!(["id", "title"]));

        let empty = Listing {
            items: vec![],
            total: 0,
            offset: 0,
            limit: 0,
            meta: Meta::memory_projection_complete(),
        };
        let value = listing_json(&empty, &all, ArchiveRange::All);
        assert_eq!(value["items"], json!([]));
        assert_eq!(value["archive"], json!("all"));
    }

    #[test]
    fn read_json_is_the_scoped_show_shape() {
        let path = Utf8Path::new("work/task/wm-000000000001.md");
        let fingerprint = Fingerprint::of(b"document");
        let read = ScopedRead {
            item: item(),
            path: path.to_path_buf(),
            category: crate::model::CategoryKey::parse("task").unwrap(),
            fingerprint: fingerprint.clone(),
            readiness: None,
            children: vec![id("wm-000000000005")],
            dependents: vec![id("wm-000000000006")],
            soft_link_findings: vec![],
            meta: Meta::direct_file_scoped(),
        };
        let value = read_json(&read);
        assert_eq!(value["id"], json!("wm-000000000001"));
        assert_eq!(value["path"], json!("work/task/wm-000000000001.md"));
        assert_eq!(value["category"], json!("task"));
        assert_eq!(value["fingerprint"], json!(fingerprint.as_str()));
        assert_eq!(value["item"]["body"], json!("details\n"));
        assert_eq!(value["needs_migration"], json!(false));
        assert_eq!(value["archived"], json!(false));
        assert_eq!(value["relationships"]["historical"], json!(false));
        assert_eq!(value["relationships"]["parent"], json!("wm-000000000002"));
        assert_eq!(
            value["relationships"]["children"],
            json!(["wm-000000000005"])
        );
        assert_eq!(
            value["relationships"]["dependents"],
            json!(["wm-000000000006"])
        );
        assert_eq!(value["ready"], Value::Null);
        assert_eq!(value["blocked"], Value::Null);
        assert_eq!(value["blocked_by"], json!([]));
        assert_eq!(value["relationships"]["soft_link_issues"], json!([]));
    }

    #[test]
    fn a_soft_link_issue_names_the_missing_side_and_never_a_command() {
        let partner = id("wm-000000000007");
        let read = ScopedRead {
            item: item(),
            path: Utf8Path::new("work/task/wm-000000000001.md").to_path_buf(),
            category: crate::model::CategoryKey::parse("task").unwrap(),
            fingerprint: Fingerprint::of(b"document"),
            readiness: None,
            children: vec![],
            dependents: vec![],
            soft_link_findings: vec![
                SoftLinkDisplacement {
                    partner: partner.clone(),
                    kind: Displacement::PartnerSilent,
                    partner_owner: "robin".to_owned(),
                },
                SoftLinkDisplacement {
                    partner: id("wm-000000000008"),
                    kind: Displacement::PartnerArchived,
                    partner_owner: "frank".to_owned(),
                },
            ],
            meta: Meta::direct_file_scoped(),
        };
        let issues = read_json(&read)["relationships"]["soft_link_issues"].clone();
        assert_eq!(
            issues,
            json!([
                {
                    "partner": "wm-000000000007",
                    "partner_owner": "robin",
                    "missing": "link",
                    "reason": "the soft link exists on one side only: wm-000000000007 never recorded it",
                },
                {
                    "partner": "wm-000000000008",
                    "partner_owner": "frank",
                    "missing": "archived",
                    "reason": "wm-000000000008 is archived and keeps the association as history only",
                },
            ])
        );
    }

    #[test]
    fn write_result_json_is_the_shared_write_core_plus_extras() {
        let path = Utf8Path::new("work/task/wm-000000000001.md");
        let value = write_result_json(&item(), path, true, false);
        assert_eq!(value["id"], json!("wm-000000000001"));
        assert_eq!(value["path"], json!("work/task/wm-000000000001.md"));
        assert_eq!(value["changed"], json!(true));
        assert_eq!(value["written"], json!(false));
        assert_eq!(value["item"]["title"], json!("Fix the thing"));

        let mut extra = Map::new();
        extra.insert("previous_status".to_owned(), json!("open"));
        extra.insert("written".to_owned(), json!(true));
        let value = write_result_json_with(&item(), path, true, false, extra);
        assert_eq!(value["previous_status"], json!("open"));
        assert_eq!(
            value["written"],
            json!(false),
            "an extra key never overrides the shared written flag"
        );
        assert_eq!(value["changed"], json!(true));
    }

    #[test]
    fn archive_leftovers_pair_each_document_with_the_relations_it_keeps() {
        use crate::ops::archive::{InboundRelation, UntouchedItem, UntouchedRelation};

        let first = id("wm-000000000002");
        let second = id("wm-000000000003");
        let untouched = vec![
            UntouchedItem {
                id: first.clone(),
                path: Utf8Path::new("work/task/wm-000000000002.md").to_owned(),
            },
            UntouchedItem {
                id: second.clone(),
                path: Utf8Path::new("work/task/wm-000000000003.md").to_owned(),
            },
        ];
        let relations = vec![
            UntouchedRelation {
                from: first.clone(),
                relation: InboundRelation::Dependencies,
            },
            UntouchedRelation {
                from: second.clone(),
                relation: InboundRelation::Parent,
            },
            UntouchedRelation {
                from: second.clone(),
                relation: InboundRelation::SoftLinks,
            },
        ];

        let value = archive_leftovers_json(&untouched, &relations);

        assert_eq!(
            value,
            json!([
                {
                    "id": "wm-000000000002",
                    "path": "work/task/wm-000000000002.md",
                    "relations": ["dependencies"],
                },
                {
                    "id": "wm-000000000003",
                    "path": "work/task/wm-000000000003.md",
                    "relations": ["parent", "soft_links"],
                },
            ])
        );
        assert_eq!(archive_leftovers_json(&[], &[]), json!([]));
    }

    #[test]
    fn the_archive_result_block_carries_the_footprint_lists() {
        use crate::ops::archive::{
            AffectedItem, ArchiveOutcome, InboundRelation, PlannedRemoval, UntouchedItem,
            UntouchedRelation,
        };
        use crate::ops::mediated::write_meta;
        use crate::testing::fixtures;
        use camino::Utf8PathBuf;

        let mut item = fixtures::work_item("wm-000000000001", "task", "Retire me")
            .with_status(Status::Open)
            .build();
        item.archived_at = Some(instant());
        let mut outcome = ArchiveOutcome {
            item,
            path: Utf8PathBuf::from("work/task/wm-000000000001.md"),
            archived_at: instant(),
            archive_reason: Some("superseded".to_owned()),
            removals: vec![PlannedRemoval {
                from: id("wm-000000000002"),
                relation: InboundRelation::Dependencies,
            }],
            affected: vec![AffectedItem {
                id: id("wm-000000000002"),
                path: Utf8PathBuf::from("work/task/wm-000000000002.md"),
            }],
            untouched: vec![UntouchedItem {
                id: id("wm-000000000003"),
                path: Utf8PathBuf::from("work/task/wm-000000000003.md"),
            }],
            untouched_relations: vec![UntouchedRelation {
                from: id("wm-000000000003"),
                relation: InboundRelation::SoftLinks,
            }],
            written: true,
            fingerprints: Vec::new(),
            meta: write_meta(Vec::new()),
        };

        let value = archive_result_json(&outcome);

        assert_eq!(value["id"], "wm-000000000001");
        assert_eq!(value["path"], "work/task/wm-000000000001.md");
        assert_eq!(value["archive_reason"], "superseded");
        assert_eq!(value["status"], "open", "archive is not a status");
        assert_eq!(value["written"], true);
        assert_eq!(
            value["removals"],
            json!([{"from": "wm-000000000002", "relation": "dependencies"}])
        );
        assert_eq!(
            value["affected"],
            json!([{
                "id": "wm-000000000002",
                "path": "work/task/wm-000000000002.md",
            }])
        );
        assert_eq!(
            value["untouched"],
            json!([{
                "id": "wm-000000000003",
                "path": "work/task/wm-000000000003.md",
                "relations": ["soft_links"],
            }])
        );
        assert_eq!(value["item"]["id"], "wm-000000000001");

        // A dry run carries the same block and says so.
        outcome.written = false;
        assert_eq!(archive_result_json(&outcome)["written"], false);
    }

    #[test]
    fn the_leftover_summary_counts_documents_and_stays_quiet_at_zero() {
        let target = id("wm-000000000001");
        assert_eq!(archive_leftovers_summary(&target, 0), None);
        assert_eq!(
            archive_leftovers_summary(&target, 1).as_deref(),
            Some("wm-000000000001 still referenced by 1 document left untouched")
        );
        assert_eq!(
            archive_leftovers_summary(&target, 3).as_deref(),
            Some("wm-000000000001 still referenced by 3 documents left untouched")
        );
    }
}
