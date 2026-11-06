---
schema: 1
id: wm-ksfpdfyj6rzz
title: Plan Waymark Web cache and index status dashboard
category: map
status: open
owner: Frank
creator: Frank
created_at: 2026-11-06T06:48:35Z
updated_at: 2026-11-06T08:09:28Z
---
## Destination

Produce a handoff-ready functional specification for a simple, read-only Waymark Web dashboard showing the current in-process LRU cache and projection index state. The map resolves product behavior and observability semantics; it does not implement the dashboard.

## Notes

- This is a Waymark Web feature, not a general cache monitoring system.
- Distinguish the disposable work-item projection (index) from its bounded derived-data LRU cache.
- The Web dashboard is read-only and remains on the existing loopback-only service.
- The Web service retains one projection/cache across requests; source files remain authoritative and refresh/failure rules are in the lifecycle decision ticket.
- Keep the dashboard simple. Do not expose cache keys or values.
- Consult grilling and domain-modeling for decision tickets. Do not synchronize the functional spec or create implementation tickets until asked.

## Decisions so far

- [Define the Web-shared projection and cache lifecycle](wm-26vdk4wzezyh): Web retains one process-local projection/cache; startup can remain available for diagnosis with an incomplete projection, while complete business reads fail. Requests detect file changes and trigger a single-flight atomic refresh; failed refresh keeps a stale diagnostic snapshot, never used as current business data.
- [Set dashboard refresh and file-change synchronization policy](wm-9zb8nc7b0vhs): The dashboard loads once and refreshes only on browser full-page reload; no in-page button, polling, push, watcher or background scan. Requests check for file changes; no fixed refresh timeout is added. On refresh failure status reads report stale state and diagnostics, while tree/detail reads return errors.
- [Define observable LRU cache metrics and size estimate](wm-s9m1g5hm39pq): Show enabled state, configured capacity, current total and per-class entry counts, and hit/miss/eviction counters; omit estimated bytes. Counters accumulate over the shared Projection lifetime, surviving refresh and cache reconfiguration; status inspection does not change them.
- [Choose safe projection index details for the dashboard](wm-bzn9qegc2cvt): Show active work-item summaries (ID, title, category, status, relative path), 50 per page ordered by stable ID; incomplete/stale results remain diagnostic-only and explicitly labeled.

## Not yet specified

None for the v1 dashboard contract. The remaining work is synchronization into the functional specification and implementation tickets, only after user confirmation.

## Out of scope

- General-purpose metrics, historical trends, remote/LAN exposure, and write or maintenance controls in the dashboard.
- Displaying raw cache keys/values, Markdown bodies, absolute paths, owners, relationships, or arbitrary custom-field contents.
