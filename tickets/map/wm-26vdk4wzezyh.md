---
schema: 1
id: wm-26vdk4wzezyh
title: Define the Web-shared projection and cache lifecycle
category: map
status: closed
owner: Frank
creator: Frank
created_at: 2026-11-06T06:49:12Z
updated_at: 2026-11-06T07:02:20Z
closed_at: 2026-11-06T06:57:28Z
parent: wm-ksfpdfyj6rzz
---
## Type

grilling

## Question

Should Waymark Web retain one projection and its LRU cache across requests, and what lifecycle and freshness guarantees apply? Decide how the service initializes the shared state, how later tree/item/dashboard reads use it, what happens after writes or configuration changes, how source-file changes are detected and synchronized, and what the Web returns while refresh is incomplete or fails. Preserve the rule that project files remain authoritative and that incomplete or stale data is never presented as complete.

## Decisions confirmed so far

- Confirmed by the user on 2026-11-06: Web retains one in-process Projection and LRU cache across requests; ordinary tree, detail and dashboard reads use the same shared state. This is required for the dashboard's values to describe what the Web service actually holds, rather than a separate inspection scan. Work-item files remain the only source of truth.
- Confirmed by the user on 2026-11-06: startup attempts to load and verify the projection before serving. A projection that is incomplete does not prevent the Web service from starting; the dashboard must show its incomplete state and diagnostics. Global tree reads reject an incomplete projection rather than returning a partial tree as if complete.
- Confirmed by the user on 2026-11-06: each API request performs a lightweight source-file inventory check; only detected changes trigger projection refresh. Do not introduce a file watcher or periodic full scan for v1. This is request-triggered synchronization, so state need not update while there are no requests.
- Confirmed by the user on 2026-11-06: if a change is detected but refresh fails, retain the last successful projection solely for diagnosis and mark it stale. Tree/detail business reads reject stale data; the status dashboard may show it only with a prominent stale state and the refresh diagnostic.
- Confirmed by the user on 2026-11-06: concurrent requests that require refresh share one in-flight refresh. Readers observe an atomically published projection, never a partially updated one; other requests wait for the refresh result before reading.

## Resolution

The user confirmed the shared Web projection/cache model, startup behavior, request-triggered inventory checks, refresh failure behavior and serialized atomic refresh on 2026-11-06. Web holds one process-local disposable projection/cache; project files remain authoritative. An incomplete initial projection does not prevent startup, but global tree reads reject it. Refresh is requested only by API traffic after a lightweight change check. On refresh failure the previous projection is retained as stale for diagnostics only; ordinary data reads reject it. Concurrent refresh is single-flight and publication atomic.

## Handoff

Exact inventory check implementation and bounded retry behavior should reuse the existing core projection fingerprint and snapshot-stability contract rather than introduce a competing freshness rule. Request blocking limits and the error envelope for refresh failures remain to be decided in [Set dashboard refresh and file-change synchronization policy](wm-9zb8nc7b0vhs).
