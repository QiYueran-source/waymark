---
schema: 1
id: wm-81dtey36xhtz
title: Add core cache-count and status accessors
category: implement
status: open
owner: Frank
creator: Frank
created_at: 2026-11-06T08:40:44Z
updated_at: 2026-11-06T08:41:14Z
parent: wm-xm4zx172qpd2
---
## Parent

[Waymark Web cache and index status dashboard](wm-xm4zx172qpd2)

## What to build

The shared core gains the safe, read-only aggregate accessors the Web dashboard needs: per-class derived-data cache entry counts and a structured projection status summary. This makes it possible for an adapter to answer "is the projection complete and fresh, what diagnostics explain it, how many cache entries of each class exist, and what are the lifetime hit/miss/eviction counters" without exposing cache keys, values, or any entry internals.

## Acceptance criteria

- [ ] The Projection exposes per-class entry counts for the `body`, `detail`, and `query` cache classes, and the three counts always sum to the total cache length.
- [ ] The Projection exposes a read-only status summary carrying completeness, freshness, the failure/uncertain-path shortfall counts, the diagnostic findings, the cache enabled state, configured capacity, total entry count, per-class counts, and the lifetime hit/miss/eviction counters.
- [ ] The counters are cumulative for the Projection's lifetime: a successful refresh merges its counts into them, a cache-only reconfiguration clears entries but preserves them, and a fresh Projection starts them at zero.
- [ ] A status read never performs a cache lookup and never changes the counters; unit tests prove the counters are unchanged by the aggregation itself.
- [ ] None of the new accessors exposes cache keys, cache values, entry internals, or any work-item content; the types returned are aggregates only.
- [ ] The existing projection cache tests keep passing and the new accessors are covered by core unit tests (prior art: the cache and counter tests in `waymark/crates/waymark-core/tests/projection.rs`).

## Blocked by

None (can start immediately).

**Status:** ready-for-agent
