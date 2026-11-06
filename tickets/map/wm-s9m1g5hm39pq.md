---
schema: 1
id: wm-s9m1g5hm39pq
title: Define observable LRU cache metrics and size estimate
category: map
status: closed
owner: Frank
creator: Frank
created_at: 2026-11-06T06:49:13Z
updated_at: 2026-11-06T08:06:19Z
closed_at: 2026-11-06T08:06:19Z
parent: wm-ksfpdfyj6rzz
dependencies:
  - wm-26vdk4wzezyh
---
## Type

grilling

## Question

Which live LRU cache metrics belong on the simple Web dashboard, and how should estimated cache memory use be calculated and caveated? Decide whether to show enabled state, capacity, current entries, per-class counts, hits/misses/evictions and approximate bytes; define whether counters are service-lifetime or since refresh/rebuild, and ensure dashboard reads do not create misleading cache activity. Never display raw cache keys or values.

## Decisions confirmed so far

- Confirmed by the user on 2026-11-06: the first dashboard version shows whether the LRU is enabled, configured capacity, current entry count, total hit/miss/eviction counters, and entry counts grouped by cache class (`body`, `detail`, `query`). Do not show an estimated memory-byte figure in v1; the current implementation has no byte accounting and the estimate would need a separately agreed measurement convention.
- Cache counters are the existing `ProjectionStats` counters and therefore accumulate over the lifetime of the Web process's shared Projection: successful refreshes merge new counts rather than resetting them; a process restart starts a new Projection and resets them. A cache-only configuration change clears entries but does not reset these counters.
- The dashboard status read itself must not call `cache_get` or otherwise count as a cache lookup. It may read immutable aggregate state; no cache keys or values are exposed.
- Per-class entry counts are a newly required aggregate; current core exposes only total `cache_len()`, so the functional specification must require a safe count-by-class projection API without exposing entries or cache contents.

## Resolution

The user accepted the first-version LRU metrics on 2026-11-06: enabled, capacity, total entries, per-class entry counts, hits, misses and evictions; no memory estimate. Counter lifetime follows the shared Projection (merge across refresh, reset on restart), and status inspection itself does not alter cache statistics. Cache keys/values are not exposed.

## Handoff

The core's existing data and semantics are in `waymark/crates/waymark-core/src/projection.rs`: `ProjectionStats` at lines 162-189; `CacheClass` at 192-204; cache storage at 244-298; `stats`, `cache_len`, and `cache_enabled` at 645-693; refresh merges counters at 394-407; reconfiguration clears entries without resetting stats at 700-715. Per-class counts need a new aggregate accessor or equivalent safe summary in core.
