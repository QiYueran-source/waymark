//! The one shared projection a serving Web service holds, plus its lifecycle.
//!
//! [`SharedProjection`] owns the single [`Projection`] for the whole service
//! lifetime. Every business read calls [`SharedProjection::snapshot`], which:
//!
//! 1. runs the lightweight inventory check (paths, sizes, modified times, and
//!    inventory problems — no byte reads),
//! 2. refreshes when the sources moved, sharing one in-flight refresh between
//!    concurrent requests and publishing the rebuilt projection atomically,
//! 3. rejects a stale or missing snapshot with the standard
//!    `INCOMPLETE_PROJECTION` error, so no business read ever serves old data.
//!
//! There is deliberately no background watcher, timer, or refresh timeout:
//! refresh happens only inside a request that observed a change, exactly once
//! no matter how many requests raced to observe it.

use std::sync::Arc;

use tokio::sync::{Mutex, Notify, RwLock};

use waymark_core::envelope::{Completeness, Freshness, Meta, Source};
use waymark_core::error::{Code, WaymarkError};
use waymark_core::projection::Projection;
use waymark_core::store::Project;

/// The atomically published snapshot plus lifecycle bookkeeping.
pub(crate) struct SharedProjection {
    /// The sources the snapshot reflects; refreshes re-read from here.
    project: Project,
    /// The published snapshot; `None` only while the project has never loaded.
    /// Readers lock only to clone the [`Arc`], and `Some → None` never happens
    /// after startup, so a reader's clone is never orphaned.
    current: RwLock<Option<Arc<Projection>>>,
    /// Guards the refresh critical section, serializing the blocking scans.
    refresh: Mutex<()>,
    /// Single-flight: `Some` while a refresh is running; concurrent callers
    /// wait on this and never run their own scan.
    in_flight: Mutex<Option<Arc<Notify>>>,
    /// Set when the last refresh failed; the retained snapshot is stale.
    stale: RwLock<bool>,
    /// When the last successful load happened, in RFC 3339 UTC text, or `None`
    /// before the first success. Set on every successful startup load and every
    /// successful refresh; never cleared on failure (the stale snapshot still
    /// has a last-success instant worth reporting).
    last_success: Mutex<Option<String>>,
}

impl SharedProjection {
    /// Attempt the first load. A failure never prevents serving: the service
    /// starts with no snapshot, and the first request retries the load.
    pub(crate) async fn startup(project: Project) -> Self {
        let this = Self {
            project,
            current: RwLock::new(None),
            refresh: Mutex::new(()),
            in_flight: Mutex::new(None),
            stale: RwLock::new(false),
            last_success: Mutex::new(None),
        };
        match tokio::task::spawn_blocking({
            let project = this.project.clone();
            move || Projection::load(&project)
        })
        .await
        {
            Ok(Ok(projection)) => {
                *this.current.write().await = Some(Arc::new(projection));
                *this.last_success.lock().await = Some(now_rfc3339());
            }
            Ok(Err(_)) => {
                // No snapshot yet; the first request's sync retries the load.
            }
            Err(_) => {
                // A panicking scanner is a load failure like any other.
            }
        }
        this
    }

    /// The snapshot every business read uses.
    ///
    /// Runs the inventory check, refreshes single-flight when the sources
    /// changed, and returns the atomically published snapshot. A snapshot that
    /// is missing (never loaded) or stale (the last refresh failed) is an
    /// error: tree and item reads fail with the standard
    /// `INCOMPLETE_PROJECTION` envelope instead of answering from old data.
    pub(crate) async fn snapshot(&self) -> Result<Arc<Projection>, WaymarkError> {
        self.sync().await;
        if *self.stale.read().await {
            return Err(self.stale_error().await);
        }
        let snapshot = self.current.read().await.clone();
        snapshot.ok_or_else(|| self.missing_error())
    }

    /// The one entry point: check the sources, and refresh only when they
    /// moved. The refresh outcome is deliberately swallowed — every caller
    /// reads the *published* snapshot (or the stale marker) and decides its own
    /// error policy, which is what lets a failed refresh keep the retained
    /// snapshot visible while business reads refuse it.
    pub(crate) async fn sync(&self) {
        if !self.inventory_changed().await {
            return;
        }
        let _ = self.refresh().await;
    }

    /// The atomically published snapshot, without the business-read gate.
    ///
    /// The dashboard routes use this directly after [`SharedProjection::sync`]:
    /// they want to report the retained snapshot even when it is stale or the
    /// project has never loaded, so they deliberately skip the stale/missing
    /// rejection [`SharedProjection::snapshot`] applies to tree and item reads.
    pub(crate) async fn snapshot_unchecked(&self) -> Option<Arc<Projection>> {
        self.current.read().await.clone()
    }

    /// Whether the last refresh failed; the retained snapshot is stale.
    pub(crate) async fn is_stale(&self) -> bool {
        *self.stale.read().await
    }

    /// When the last successful load happened, in RFC 3339 UTC text, or `None`
    /// before the first success.
    pub(crate) async fn last_success(&self) -> Option<String> {
        self.last_success.lock().await.clone()
    }

    /// The standard `_meta` the dashboard attaches: the memory projection as
    /// source, and the *lifecycle* completeness/freshness — the stale flag and
    /// the missing snapshot override the retained projection's own (still
    /// `Fresh`) freshness, exactly as the dashboard payloads report it.
    pub(crate) async fn status_meta(&self, snapshot: Option<&Projection>) -> Meta {
        let (completeness, freshness, diagnostics) = match snapshot {
            Some(projection) => {
                let (completeness, freshness) = if self.is_stale().await {
                    // A failed refresh: the retained snapshot is stale no matter
                    // what its own freshness says.
                    (projection.completeness(), Freshness::Stale)
                } else {
                    // Lifecycle-clean: the retained snapshot's own freshness is
                    // authoritative (this is the `complete + fresh` case).
                    (projection.completeness(), projection.freshness())
                };
                (completeness, freshness, projection.diagnostics())
            }
            // Never loaded: there is no projection to describe.
            None => (Completeness::Scoped, Freshness::Unknown, Vec::new()),
        };
        Meta {
            source: Source::MemoryProjection,
            completeness,
            freshness,
            diagnostics,
            warnings: Vec::new(),
        }
    }

    /// Rebuild the snapshot from the files, single-flight and atomic.
    ///
    /// The first caller runs the blocking scan; concurrent callers wait on the
    /// in-flight ticket and return once the owner publishes, never scanning
    /// themselves. A failed scan keeps the last successful snapshot and marks
    /// it stale; only a successful scan clears the marker.
    pub(crate) async fn refresh(&self) -> Result<(), WaymarkError> {
        // 1. Claim the single-flight slot, or wait for the owner.
        let ticket = {
            let mut slot = self.in_flight.lock().await;
            match slot.as_ref() {
                Some(notify) => {
                    // Another refresh is running: wait for its publication,
                    // then re-check — the waiter does not run its own scan.
                    let notify = notify.clone();
                    drop(slot);
                    notify.notified().await;
                    return Ok(());
                }
                None => {
                    let ticket = Arc::new(Notify::new());
                    *slot = Some(ticket.clone());
                    ticket
                }
            }
        };

        // 2. Serialize against the blocking scan; re-check under the lock so an
        //    owner and a late arrival cannot overlap two scans.
        let _guard = self.refresh.lock().await;
        if !self.inventory_changed().await {
            self.finish_refresh(&ticket).await;
            return Ok(());
        }

        // 3. Run the blocking scan; readers keep the old snapshot meanwhile.
        let project = self.project.clone();
        let loaded = match tokio::task::spawn_blocking(move || Projection::load(&project)).await {
            Ok(Ok(projection)) => projection,
            Ok(Err(error)) => {
                // Refresh FAILED: retain the last successful snapshot, stale.
                *self.stale.write().await = true;
                self.finish_refresh(&ticket).await;
                return Err(error);
            }
            Err(join_error) => {
                *self.stale.write().await = true;
                self.finish_refresh(&ticket).await;
                return Err(WaymarkError::new(Code::IoError, join_error.to_string()));
            }
        };

        // 4. Publish atomically; a successful scan clears the stale marker.
        *self.current.write().await = Some(Arc::new(loaded));
        *self.stale.write().await = false;
        *self.last_success.lock().await = Some(now_rfc3339());
        self.finish_refresh(&ticket).await;
        Ok(())
    }

    /// The lightweight source check: path, size, modified time, and inventory
    /// problems — no byte reads, no fingerprint recomputation. Both sides sort
    /// by path, so a pairwise `zip` is the whole comparison.
    ///
    /// Called only from the async paths (`sync`, `refresh`), never from a
    /// blocking context.
    async fn inventory_changed(&self) -> bool {
        // Clone the comparison vectors out of the read guard, then release it:
        // the check must never hold the lock across the inventory walk.
        let (entries, projected_problems) = {
            let current = self.current.read().await;
            let Some(projection) = current.as_ref() else {
                return true;
            };
            let entries = projection.entries().cloned().collect::<Vec<_>>();
            let projected_problems: Vec<(camino::Utf8PathBuf, String)> = projection
                .problems()
                .iter()
                .map(|problem| (problem.path.clone(), problem.reason.clone()))
                .collect();
            (entries, projected_problems)
        };
        let Ok(inventory) = self.project.inventory() else {
            // An unreadable source is a change: the refresh path surfaces the
            // failure and marks the retained snapshot stale.
            return true;
        };
        if inventory.entries.len() != entries.len()
            || inventory.problems.len() != projected_problems.len()
        {
            return true;
        }
        for (entry, projected) in inventory.entries.iter().zip(entries.iter()) {
            if entry.path != projected.path
                || entry.size != projected.size
                || entry.modified != projected.modified
            {
                return true;
            }
        }
        for (problem, (path, reason)) in inventory.problems.iter().zip(projected_problems.iter()) {
            if problem.path != *path || problem.reason != *reason {
                return true;
            }
        }
        false
    }

    /// The standard error for a stale snapshot: the retained projection is
    /// fresh as built, but the refresh that should have replaced it failed.
    async fn stale_error(&self) -> WaymarkError {
        let current = self.current.read().await;
        let Some(projection) = current.as_ref() else {
            drop(current);
            return self.missing_error();
        };
        let completeness = projection.completeness().as_str();
        let diagnostics = projection_diagnostics(projection);
        let (unreadable, uncertain) = projection.shortfall_counts();
        drop(current);
        WaymarkError::incomplete_projection_shortfall(
            completeness,
            Freshness::Stale.as_str(),
            unreadable,
            uncertain,
            diagnostics,
        )
    }

    /// The standard error for a missing snapshot: the project has never
    /// loaded (startup failed and no refresh has succeeded yet).
    fn missing_error(&self) -> WaymarkError {
        WaymarkError::incomplete_projection(
            Completeness::Scoped.as_str(),
            Freshness::Unknown.as_str(),
            Vec::new(),
        )
    }

    /// Clear the single-flight slot and wake every waiter.
    async fn finish_refresh(&self, ticket: &Arc<Notify>) {
        *self.in_flight.lock().await = None;
        ticket.notify_waiters();
    }
}

/// The diagnostics a stale snapshot reports, in the same `_meta` shape
/// [`Projection::require_complete`] produces for an incomplete one.
fn projection_diagnostics(projection: &Projection) -> Vec<serde_json::Value> {
    projection
        .diagnostics()
        .iter()
        .filter_map(|finding| serde_json::to_value(finding).ok())
        .collect()
}

/// The current instant in the canonical persisted form (RFC 3339 UTC, second
/// precision, `Z` suffix), reused for `last_success`.
fn now_rfc3339() -> String {
    waymark_core::time::format_utc(chrono::Utc::now())
}
