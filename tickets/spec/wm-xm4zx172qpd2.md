---
schema: 1
id: wm-xm4zx172qpd2
title: Test ticket for waymark CLI usage
category: spec
status: open
owner: Frank
creator: Frank
created_at: 2026-11-06T08:32:26Z
updated_at: 2026-11-06T09:00:22Z
parent: wm-srxj8ztv1zfz
---
## Problem Statement

Waymark Web is a read-only loopback service that answers tree, detail, and health reads. Today each request scans the authoritative work-item files itself, so nothing in the service holds the disposable Projection and its bounded derived-data LRU cache across requests. A person debugging the service cannot see what it has actually loaded: whether the projection is complete and fresh, how many derived cache entries of each class it holds, what the lifetime hit/miss/eviction counters have done, or which active work items the service currently knows. They also cannot tell a partial or stale snapshot from a complete, current one.

## Solution

Add a simple, strictly read-only dashboard to Waymark Web that shows the state of the service's own process-local Projection and derived-data cache, plus a paginated index of the active work items the service holds. The dashboard describes what the Web service actually holds, not a separate inspection scan, and it never displays cache keys or values.

The Web service retains one shared in-process Projection and its bounded derived-data LRU across requests, and every API request synchronizes it from the authoritative work-item files before reading. Work-item files remain the only source of truth; the Projection is disposable derived state. Startup attempts to load the projection before serving; an incomplete projection does not prevent startup. Each request performs a lightweight inventory check and only detected source changes trigger a refresh; there is no watcher, timer scan, browser polling, or refresh timeout. Concurrent requests needing a refresh share one in-flight, atomically published refresh. A failed refresh keeps the last successful snapshot for diagnostics only, marked stale, and ordinary tree/detail reads reject it.

The dashboard has two sections. The status section shows the projection's completeness and freshness, the diagnostic findings and shortfall counts, and the cache's enabled state, configured capacity, total and per-class entry counts, and lifetime hit/miss/eviction counters; a prominent banner appears when the projection is incomplete or stale. The index section lists active work-item summaries — ID, title, category, lifecycle status, and project-relative path — 50 per page ordered by stable ID ascending with previous/next navigation. Incomplete or stale projections may supply parseable rows for diagnosis only behind the same prominent banner, never represented as a complete/current index. The dashboard loads once and refreshes only on browser full-page reload.

## User Stories

1. As a project member, I want the Web service to hold one shared Projection and derived-data cache across requests, so that the dashboard describes what the service actually holds.
2. As a project member, I want ordinary tree and item-detail reads to use the same shared state, so that the dashboard, the tree, and the detail view cannot disagree about the project.
3. As a project member, I want work-item files to remain the only source of truth, so that the shared projection is disposable derived state rather than a second authority.
4. As a project member, I want the service to attempt loading the projection before serving, so that a first dashboard view is not an empty guess.
5. As a project member, I want an incomplete projection at startup not to prevent the service from serving, so that the dashboard can still report what is wrong.
6. As a project member, I want a global tree read to reject an incomplete projection rather than presenting a partial tree as complete, so that no reader mistakes a partial graph for the whole project.
7. As a project member, I want each API request to perform a lightweight inventory check, so that detected source changes trigger a refresh.
8. As a project member, I want no background watcher, timer scan, or browser polling, so that synchronization stays request-triggered and simple.
9. As a project member, I want concurrent requests that need a refresh to share one in-flight refresh, so that a burst of requests cannot rescan the project in parallel.
10. As a project member, I want readers to observe only an atomically published snapshot, so that no request ever sees a partially updated projection.
11. As a project member, I want no new fixed or configurable refresh timeout in v1, so that a request waiting on a refresh waits for the refresh itself.
12. As a project member, I want a failed refresh to retain the last successful snapshot for diagnostics only and mark it stale, so that diagnosis remains possible after a failure.
13. As a project member, I want tree and detail business reads to return a standard error when the snapshot is stale, so that stale data is never served as current.
14. As a project member, I want the status read to succeed and report stale state with the last successful refresh time and diagnostic summary when a refresh failed, so that the dashboard stays useful for diagnosis.
15. As a dashboard user, I want the status section to show whether the projection is complete and fresh, so that I can tell at a glance whether the service is reading the whole project.
16. As a dashboard user, I want the status section to show the failure and uncertain-path shortfall counts and the diagnostic findings, so that I can see exactly what is blocking completeness.
17. As a dashboard user, I want the status section to show the cache enabled state and configured capacity, so that I can see whether the cache is on and how it is bounded.
18. As a dashboard user, I want the status section to show the current total and per-class entry counts, so that I can see how the derived-data cache is populated.
19. As a dashboard user, I want the status section to show the lifetime hit/miss/eviction counters, so that I can gauge cache health since the service started.
20. As a dashboard user, I want the counters to accumulate for the shared Projection's lifetime, surviving successful refreshes and cache-only reconfiguration, so that they are one coherent history.
21. As a dashboard user, I want a fresh Projection to start its counters at zero, so that a service restart resets the history rather than pretending it continues.
22. As a dashboard user, I want the status read itself never to perform a cache lookup or change the counters, so that looking at the dashboard does not pollute the numbers it shows.
23. As a dashboard user, I want no cache keys or values ever displayed, so that the dashboard reveals nothing about cache contents.
24. As a dashboard user, I want the index section to list active work items ordered by stable ID ascending at 50 rows per page with previous/next navigation, so that I can page through what the service holds.
25. As a dashboard user, I want each index row to expose only the stable ID, title, category, lifecycle status, and project-relative path, so that no absolute path, owner, relationship, body, or custom field leaks.
26. As a dashboard user, I want archived items excluded from index rows and counts, so that the index stays focused on active work.
27. As a dashboard user, I want the API to bound the page size rather than trust an arbitrary client limit, so that one large page request cannot be exploited.
28. As a dashboard user, I want an incomplete or stale projection to be shown only behind a prominent banner stating the state, the failure count, the diagnostic summary, and that the rows are not a complete/current index, so that a partial list is never mistaken for the whole project.
29. As a dashboard user, I want the dashboard to load once and refresh only on browser full-page reload, matching the existing Web convention, so that there is no in-page refresh control or polling.
30. As a project member, I want a missing or invalid project to yield a clear page error rather than an empty dashboard, so that a broken launch is diagnosed rather than disguised; the launcher's existing discovery failure already does this, and the behavior when the project becomes invalid mid-run is confirmed during implementation.

## Implementation Decisions

### Shared projection and cache lifecycle

- The Web service owns one shared in-process Projection plus its derived-data cache for its whole lifetime. Startup attempts `Projection::load` before serving; an incomplete projection does not prevent startup and is not served to business reads. The shared state is used by the tree and detail reads and by the dashboard.
- Each API request performs a lightweight inventory check against the shared projection (`Project::inventory` compared with the projection's current content fingerprints and paths). Only detected source changes trigger a refresh. No background watcher, timer scan, browser polling, or push channel is added, and no new fixed or configurable refresh timeout exists in v1.
- A detected change performs a single-flight refresh: concurrent requests needing a refresh share one in-flight refresh task, and readers wait for its result. Publication is atomic — a reader observes either the complete refreshed snapshot or the explicitly stale retained prior snapshot, never a partially updated Projection.
- When a refresh fails, the last successful snapshot is retained for diagnostics only and marked stale. The status endpoint succeeds and reports the stale state with the last successful refresh time and diagnostic summary; tree and detail business reads return a standard `INCOMPLETE_PROJECTION`/freshness error rather than serving stale data.
- Global tree reads reject an incomplete projection (`Projection::require_complete`); item detail keeps its existing scoped-read behavior and readiness contract.
- The snapshot type exposed to handlers is `Arc<Projection>`; the shared state, refresh state, and last-success timestamp are held behind one `tokio::sync` primitive (for example a `RwLock<Arc<Projection>>` plus a refresh `Mutex`/`Notify`), keeping reads lock-free after the inventory check.

### Cache and projection status

- The v1 status summary carries: completeness, freshness, the failure/uncertain-path shortfall counts, the diagnostic findings, the cache enabled state, the configured capacity, the total entry count, per-class counts, and the lifetime hit/miss/eviction counters. It omits estimated memory bytes.
- The counters are the existing `ProjectionStats` counters and accumulate for the shared Projection's lifetime: a successful refresh merges its counts into them, a cache-only reconfiguration clears entries but preserves them, and a fresh Projection starts them at zero.
- The status read never calls `cache_get` or `cache_put` and never changes the counters; it reads immutable aggregate state only. No cache keys or values are ever exposed.
- Per-class entry counts are a newly required safe aggregate: core gains a read-only accessor returning the per-`CacheClass` entry counts, with the three counts always summing to the total cache length. This is the only new core surface; the cache stores values under `CacheKey { class, subject, fingerprint }`, so the aggregate is computed from key classes without exposing keys or values.
- The status payload is served in the versioned envelope; the `_meta` block reflects the current snapshot's completeness and freshness.

### Active-item index

- The index route returns active work items only, 50 rows per page ordered by stable ID ascending, with previous/next navigation driven by page offsets. The page size is enforced by the API, not trusted from the client.
- Each row exposes exactly: stable ID, title, category (key/name/shape via the existing `CategoryView`), lifecycle status, and project-relative path. Absolute paths, owners, relationships, Markdown bodies, and arbitrary custom fields are never exposed, and archived items are excluded from rows and counts.
- When the projection is incomplete or stale, the route still returns the parseable rows only behind prominent state: incomplete/stale state, the failure count, the diagnostic summary, and a statement that the rows are not a complete/current index. No partial or stale result is ever represented as complete/current.
- The index reuses the existing active-item iteration (`Projection::items` / `Graph::from_projection` on a complete snapshot) and the existing project-relative path conventions.

### Scope limits

- Loopback-only, read-only service; no LAN or public binding, no write or maintenance controls, no historical trends, no archive browsing, no search or filter, no in-page refresh, and no CLI/MCP surface.

## Testing Decisions

- Core unit tests cover the new per-class cache-count accessor (the three counts sum to the total length, entries land in the right classes), the lifetime counter semantics (refresh merges, cache-only reconfiguration preserves, fresh Projection starts at zero), and the proof that the status aggregation itself neither performs a cache lookup nor changes any counter. Prior art: `waymark/crates/waymark-core/tests/projection.rs`.
- Web HTTP tests cover the shared-projection behavior: startup with an incomplete projection still serves; a tree read rejects it; a changed source file triggers a refresh; a failed refresh retains the stale snapshot and the status route reports it while tree/detail reads error. Prior art: `waymark/crates/waymark-web/tests/reads.rs`.
- Web HTTP tests cover the status payload fields, the stale-after-failed-refresh behavior, and the read-only proof that no dashboard interaction changes one byte of the project. Prior art: `waymark/crates/waymark-web/tests/reads.rs` and `waymark/crates/waymark-web/tests/protocol.rs`.
- Web HTTP tests cover index ordering, page size, pagination, archived exclusion, row field limits, and the incomplete/stale labeling. Prior art: `waymark/crates/waymark-web/tests/reads.rs`.
- Frontend tests follow the existing UI seam (injected fetch and storage doubles) for the status and index sections, the banner behavior, pagination controls, and the absence of any in-page refresh control. Prior art: `waymark/crates/waymark-web/ui/tests/explorer.test.ts`.
- The design map [Plan Waymark Web cache and index status dashboard](wm-ksfpdfyj6rzz) and the four decision tickets — [Define the Web-shared projection and cache lifecycle](wm-26vdk4wzezyh), [Set dashboard refresh and file-change synchronization policy](wm-9zb8nc7b0vhs), [Define observable LRU cache metrics and size estimate](wm-s9m1g5hm39pq), and [Choose safe projection index details for the dashboard](wm-bzn9qegc2cvt) — record the settled contract this specification implements.

## Out of Scope

- Displaying cache keys or values, cache entry internals, or estimated memory bytes.
- Displaying Markdown bodies, absolute filesystem paths, owners, relationships, or arbitrary custom-field contents in the index.
- Archive browsing, search, or filter controls on the index.
- In-page refresh controls, polling, push channels, background watchers, or timer scans.
- A new fixed or configurable refresh timeout in v1.
- Remote or LAN exposure, historical trends, mutation, or maintenance controls in the dashboard.
- A CLI or MCP surface for the dashboard.
- Any Web write operation.

## Further Notes

- One shared in-process Projection plus its bounded derived-data LRU is the seam this dashboard is built on: because the tree, detail, and dashboard reads all use it, the status section describes exactly what the service holds, and the counters are one coherent lifetime history.
- The only new core surface is a safe per-class cache-count aggregate accessor. No core API or schema changes beyond it, and no error-registry changes: the standard `INCOMPLETE_PROJECTION` envelope and existing freshness vocabulary already carry the incomplete and stale states.
- Open implementation-time question (user story 30): behavior when the project becomes invalid mid-run is unverified. The launcher's discovery failure is already clear; implementation confirms that an invalid project mid-run yields a clear page error rather than an empty dashboard.
- This specification is ready for implementation planning and ticket decomposition after publication.

**Status:** ready-for-agent
