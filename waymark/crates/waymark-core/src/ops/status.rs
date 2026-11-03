//! `waymark status set`: move one item between the three lifecycle states.
//!
//! Status is exactly `open`, `in_progress`, or `closed`. Any explicit transition
//! among them is allowed for an active item, and:
//!
//! - repeating the current status changes nothing — no write, no `updated_at`
//!   move, no `closed_at` touch;
//! - a real change updates `updated_at` from the operation clock;
//! - entering `closed` sets `closed_at` to that same instant;
//! - leaving `closed` clears `closed_at`, and moving between two non-closed
//!   states leaves it alone.
//!
//! `blocked` and `ready` are derived and never writable, which the signature
//! enforces: [`Status`] has exactly three variants.
//!
//! An archived item rejects the change with `ITEM_ARCHIVED`; an operator that
//! differs from the owner produces an `OWNER_MISMATCH` warning in the result's
//! `_meta` and never blocks.

use crate::envelope::Meta;
use crate::error::{ValidationIssue, WaymarkError};
use crate::model::{Status, WorkItem};
use crate::ops::mediated::{self, Loaded};
use crate::ops::OpOptions;
use crate::store::Project;
use crate::time::Timestamp;

/// What a status change produced.
#[derive(Debug, Clone)]
pub struct StatusOutcome {
    /// The complete item as written, or as it would have been written on a dry
    /// run.
    pub item: WorkItem,
    /// The status the item had before the operation.
    pub previous: Status,
    /// The document the item lives in (unchanged by a status transition).
    pub path: camino::Utf8PathBuf,
    /// Whether the status actually moved.
    pub changed: bool,
    /// Whether the document on disk changed.
    pub written: bool,
    /// The `_meta` block this write justifies, including `OWNER_MISMATCH`.
    pub meta: Meta,
}

impl StatusOutcome {
    /// The `closed_at` the item carries after the operation.
    pub fn closed_at(&self) -> Option<Timestamp> {
        self.item.closed_at
    }
}

/// Parse the status literal a CLI or MCP caller supplied.
///
/// Only the three persisted states are accepted, plus the two human spellings of
/// `in_progress` (`in progress`, `in-progress`) that [`Status::from_cli`]
/// documents. A derived word such as `blocked` is not a status and is rejected
/// with `VALIDATION_ERROR` naming the argument, not with a silent fallback.
pub fn parse_status(input: &str) -> Result<Status, WaymarkError> {
    Status::from_cli(input).ok_or_else(|| {
        WaymarkError::validation(
            "status",
            vec![ValidationIssue::new(
                "status",
                format!(
                    "{input:?} is not a status; expected one of `open`, `in_progress`, `closed`"
                ),
            )],
        )
        .at_field("status")
    })
}

/// Set the status of one item.
pub fn set_status(
    project: &Project,
    id: &str,
    status: Status,
    options: &OpOptions,
) -> Result<StatusOutcome, WaymarkError> {
    crate::lock::with_project_lock(project, options.lock_wait, || {
        let loaded = mediated::load_for_write(project, id, "status")?;
        apply(project, &loaded, status, options)
    })
}

/// Compute and commit the transition for an already-loaded item.
fn apply(
    project: &Project,
    loaded: &Loaded,
    status: Status,
    options: &OpOptions,
) -> Result<StatusOutcome, WaymarkError> {
    let mut item = loaded.item.clone();
    let previous = item.status;
    let meta = mediated::write_meta(mediated::owner_warnings(project, &item, options));

    if previous == status {
        // Repeating the current status is not a write: the item comes back
        // unchanged, `updated_at` stays where it was, and no file is touched.
        return Ok(StatusOutcome {
            item,
            previous,
            path: loaded.path.clone(),
            changed: false,
            written: false,
            meta,
        });
    }

    // One clock read feeds both stamps, so `closed_at` and `updated_at` agree on
    // the transition that created them.
    let now = mediated::stamp_updated_at(&mut item, options);
    item.status = status;
    if status.is_closed() {
        item.closed_at = Some(now);
    } else {
        item.closed_at = None;
    }

    let fingerprint = mediated::commit(loaded, &item, options)?;
    Ok(StatusOutcome {
        item,
        previous,
        path: loaded.path.clone(),
        changed: true,
        written: fingerprint.is_some(),
        meta,
    })
}
