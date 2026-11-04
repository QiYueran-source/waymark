//! `stats`: counts only, never history and never throughput.
//!
//! Statistics answer "how much work is there, and where is it?" over one
//! snapshot. They deliberately carry no time series, no completion rate, and no
//! per-period aggregation: those are questions about history, and history is not
//! a collection read.
//!
//! Every result states:
//!
//! - the archive range it counted ([`ArchiveRange`]),
//! - the completeness and freshness of the projection it read ([`Completeness`],
//!   [`Freshness`]), echoed inside the payload as well as in `_meta` because a
//!   count without its scope is a misleading number,
//! - the metrics that were *not* computed, so a caller never mistakes a count for
//!   a rate.
//!
//! Readiness counts are active-only by definition: an archived item is waiting on
//! nothing, so it is never in the ready or blocked count even when the archive
//! range is `all`.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::envelope::{Completeness, Freshness, Meta};
use crate::error::WaymarkError;
use crate::model::CategoryKey;
use crate::model::FieldKey;
use crate::query::{ArchiveRange, Snapshot};
use crate::store::Project;
use crate::Status;

/// The metrics statistics never report.
const NEVER_REPORTED: &[&str] = &[
    "history",
    "throughput",
    "completion_rate",
    "cycle_time",
    "lead_time",
    "time_series",
];

/// How many buckets an optional distribution may contain.
pub const DEFAULT_TOP: usize = 5;

/// One stats request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsRequest {
    /// Which archive slice to count.
    pub archive: ArchiveRange,
    /// Report the top-N values of this string field.
    pub top: Option<(FieldKey, usize)>,
}

impl StatsRequest {
    /// Active items, no distribution.
    pub fn new() -> Self {
        Self {
            archive: ArchiveRange::Active,
            top: None,
        }
    }

    /// Count the given archive slice.
    pub fn with_archive(mut self, archive: ArchiveRange) -> Self {
        self.archive = archive;
        self
    }

    /// Add a top-N distribution over a string custom field.
    pub fn with_top(mut self, key: FieldKey, limit: usize) -> Self {
        self.top = Some((key, limit));
        self
    }
}

impl Default for StatsRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// One bucket of an optional distribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bucket {
    /// The value; `None` is the "unset" bucket.
    pub value: Option<String>,
    /// How many items carry it.
    pub count: usize,
}

/// The counts-only answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statistics {
    /// The archive range that was counted.
    pub archive: ArchiveRange,
    /// The completeness of the snapshot.
    pub completeness: Completeness,
    /// The freshness of the snapshot.
    pub freshness: Freshness,
    /// Every counted item.
    pub total: usize,
    /// Counts by lifecycle state, in group order.
    pub by_status: BTreeMap<Status, usize>,
    /// Counts by category, sorted by key.
    pub by_category: BTreeMap<CategoryKey, usize>,
    /// Active, not closed, no outstanding dependency.
    pub ready: usize,
    /// Active, not closed, at least one outstanding dependency.
    pub blocked: usize,
    /// The optional distribution over one string field.
    pub distribution: Option<Distribution>,
    /// The metrics that were deliberately not computed.
    pub not_reported: Vec<&'static str>,
    /// The `_meta` block.
    pub meta: Meta,
}

/// An optional top-N distribution over one string custom field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Distribution {
    /// The counted field.
    pub field: FieldKey,
    /// The requested depth.
    pub top: usize,
    /// The buckets, largest first, ties by value, unset last.
    pub buckets: Vec<Bucket>,
}

/// Count the project.
pub fn stats(project: &Project, request: &StatsRequest) -> Result<Statistics, WaymarkError> {
    if let Some((key, _)) = &request.top {
        let Some(declaration) = project.config().fields.get(key) else {
            return Err(WaymarkError::validation(
                "stats",
                vec![crate::error::ValidationIssue::new(
                    format!("top.{key}"),
                    "no declared custom field has this key",
                )],
            )
            .at_field("top"));
        };
        if declaration.ty.as_str() != "string" {
            return Err(WaymarkError::validation(
                "stats",
                vec![crate::error::ValidationIssue::new(
                    format!("top.{key}"),
                    "a stats distribution needs a declared string field",
                )],
            )
            .at_field("top"));
        }
    }
    let snapshot = Snapshot::load(project)?;
    Ok(compute(&snapshot, request))
}

fn compute(snapshot: &Snapshot, request: &StatsRequest) -> Statistics {
    let mut total = 0usize;
    let mut by_status: BTreeMap<Status, usize> = BTreeMap::new();
    let mut by_category: BTreeMap<CategoryKey, usize> = BTreeMap::new();
    let mut ready = 0usize;
    let mut blocked = 0usize;
    let mut buckets: BTreeMap<Option<String>, usize> = BTreeMap::new();

    let mut count = |item: &crate::model::WorkItem| {
        total += 1;
        *by_status.entry(item.status).or_default() += 1;
        *by_category.entry(item.category.clone()).or_default() += 1;
        if let Some((key, _)) = &request.top {
            let value = match item.custom_fields.get(key.as_str()) {
                Some(crate::FieldValue::String(text)) => Some(text.clone()),
                _ => None,
            };
            *buckets.entry(value).or_default() += 1;
        }
    };

    if request.archive.includes(true) {
        for item in snapshot.archived() {
            count(item);
        }
    }
    if request.archive.includes(false) {
        for item in snapshot.graph().items() {
            count(item);
            if let Some(readiness) = snapshot.graph().readiness(&item.id) {
                if readiness.ready {
                    ready += 1;
                } else if readiness.blocked {
                    blocked += 1;
                }
            }
        }
    }

    let distribution = request.top.as_ref().map(|(key, limit)| {
        let mut entries: Vec<(Option<String>, usize)> = buckets.into_iter().collect();
        entries.sort_by(|left, right| match (&left.0, &right.0) {
            (Some(left_value), Some(right_value)) => right
                .1
                .cmp(&left.1)
                .then_with(|| left_value.cmp(right_value)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        entries.truncate(*limit);
        Distribution {
            field: key.clone(),
            top: *limit,
            buckets: entries
                .into_iter()
                .map(|(value, count)| Bucket { value, count })
                .collect(),
        }
    });

    Statistics {
        archive: request.archive,
        completeness: Completeness::Complete,
        freshness: Freshness::Fresh,
        total,
        by_status,
        by_category,
        ready,
        blocked,
        distribution,
        not_reported: NEVER_REPORTED.to_vec(),
        meta: snapshot.meta(),
    }
}

/// Render statistics as the `data` payload.
///
/// The archive range and the projection state are always present, even when the
/// project is empty: a count whose scope is missing is not an answer.
pub fn statistics_json(stats: &Statistics) -> Value {
    let mut by_status = serde_json::Map::new();
    for status in [Status::Open, Status::InProgress, Status::Closed] {
        by_status.insert(
            status.as_str().to_owned(),
            json!(stats.by_status.get(&status).copied().unwrap_or(0)),
        );
    }
    let by_category: BTreeMap<&str, usize> = stats
        .by_category
        .iter()
        .map(|(key, count)| (key.as_str(), *count))
        .collect();
    let distribution = stats.distribution.as_ref().map(|distribution| {
        json!({
            "field": distribution.field.as_str(),
            "top": distribution.top,
            "buckets": distribution.buckets.iter().map(|bucket| json!({
                "value": bucket.value,
                "count": bucket.count,
            })).collect::<Vec<_>>(),
        })
    });

    json!({
        "archive": stats.archive.as_str(),
        "completeness": stats.completeness.as_str(),
        "freshness": stats.freshness.as_str(),
        "total": stats.total,
        "by_status": by_status,
        "by_category": by_category,
        "ready": stats.ready,
        "blocked": stats.blocked,
        "distribution": distribution,
        "counts_only": true,
        "not_reported": stats.not_reported,
    })
}
