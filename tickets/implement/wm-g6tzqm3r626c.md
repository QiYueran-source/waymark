---
schema: 1
id: wm-g6tzqm3r626c
title: Add the Web cache and projection status API and page
category: implement
status: open
owner: Frank
creator: Frank
created_at: 2026-11-06T08:40:44Z
updated_at: 2026-11-06T08:41:49Z
parent: wm-xm4zx172qpd2
dependencies:
  - wm-81dtey36xhtz
  - wm-j0hcw5wzzhjc
---
## Parent

[Waymark Web cache and index status dashboard](wm-xm4zx172qpd2)

## What to build

The dashboard's cache and projection status half: a read-only API route and page section that shows the shared projection's completeness and freshness, the diagnostic summary, and the derived-data cache's enabled state, configured capacity, total and per-class entry counts, and lifetime hit/miss/eviction counters. A user debugging the service can tell whether the projection is complete, whether the cache is on and healthy, and what the counters have done since the service started — without ever seeing cache keys or values.

## Acceptance criteria

- [ ] The status route returns the structured status in the versioned envelope: completeness, freshness, failure and uncertain-path counts, the diagnostic findings, cache enabled state, configured capacity, total entries, per-class counts, lifetime hit/miss/eviction counters, and stale state with last successful refresh time and diagnostic summary when the snapshot is stale.
- [ ] The status read itself never performs a cache lookup and never changes the counters.
- [ ] When a refresh failed, the status route still succeeds and reports the stale state and diagnostics instead of failing; no cache keys or values are ever returned.
- [ ] The dashboard page renders the status section with a prominent banner when the projection is incomplete or stale, showing the failure count and diagnostic summary.
- [ ] The dashboard loads once and refreshes only on browser full-page reload; no in-page refresh control, polling, watcher, or push channel is added.
- [ ] Real HTTP tests cover the status payload fields, the stale-after-failed-refresh behavior, and the read-only proof that no dashboard interaction changes one byte of the project (prior art: `waymark/crates/waymark-web/tests/reads.rs` and `waymark/crates/waymark-web/tests/protocol.rs`).

## Blocked by

- [Add core cache-count and status accessors](wm-81dtey36xhtz)
- [Add shared Web projection and cache lifecycle](wm-j0hcw5wzzhjc)

**Status:** ready-for-agent
