---
schema: 1
id: wm-j0hcw5wzzhjc
title: Add shared Web projection and cache lifecycle
category: implement
status: open
owner: Frank
creator: Frank
created_at: 2026-11-06T08:40:44Z
updated_at: 2026-11-06T08:41:26Z
parent: wm-xm4zx172qpd2
---
## Parent

[Waymark Web cache and index status dashboard](wm-xm4zx172qpd2)

## What to build

The read-only Web service starts holding one shared in-process Projection and its derived-data cache across requests, and every API request synchronizes it from the authoritative work-item files before reading. A user browsing the service therefore sees a view of the whole project that reflects current files, and the dashboard that will be built on top of this shared state can describe exactly what the service holds. This ticket makes the existing tree and detail reads run on the shared projection, not on per-request scans.

## Acceptance criteria

- [ ] The service owns one shared Projection plus its cache for its whole lifetime; startup attempts to load it before serving and an incomplete projection does not prevent startup.
- [ ] Every API request performs a lightweight inventory check against the shared projection, and only detected source changes trigger a refresh.
- [ ] Concurrent requests that need a refresh share one in-flight refresh, and readers observe only an atomically published snapshot, never a partially updated Projection.
- [ ] When a refresh fails, the last successful snapshot is retained and marked stale; tree and detail business reads return a standard error instead of serving stale data.
- [ ] Global tree reads reject an incomplete projection rather than presenting a partial tree as complete; item detail keeps its existing scoped-read behavior and readiness contract.
- [ ] No background watcher, timer scan, or browser polling is added, and no new fixed or configurable refresh timeout exists in v1.
- [ ] The existing Web integration tests pass with the shared state in place, and new HTTP tests cover the shared-projection behavior above (prior art: `waymark/crates/waymark-web/tests/reads.rs`).

## Blocked by

None (can start immediately).

**Status:** ready-for-agent
