---
schema: 1
id: wm-9zb8nc7b0vhs
title: Set dashboard refresh and file-change synchronization policy
category: map
status: closed
owner: Frank
creator: Frank
created_at: 2026-11-06T06:49:13Z
updated_at: 2026-11-06T07:46:20Z
closed_at: 2026-11-06T07:46:20Z
parent: wm-ksfpdfyj6rzz
dependencies:
  - wm-26vdk4wzezyh
---
## Type

grilling

## Question

How does the dashboard refresh its display, and how does the Web service detect and synchronize external edits to authoritative work-item files? Decide manual refresh versus polling/push, any polling interval, whether refresh scans happen in the background or on request, and how errors or concurrent changes are reported without blocking normal reads or showing a torn index.

## Decisions confirmed so far

- Confirmed by the user on 2026-11-06: the dashboard loads once when opened and follows the existing Waymark Web convention: users refresh with the browser's full-page reload. It adds no in-page refresh control, polling, push channel, or background scan. A full-page reload may also reload the enclosing host page.
- Confirmed by the user on 2026-11-06: source-file change detection is request-triggered, not watcher- or timer-driven. Each API request performs a lightweight inventory check, and detected changes trigger refresh of the shared Projection. If no requests occur, the service need not detect changes or update its snapshot until the next request.
- Confirmed by the user on 2026-11-06: do not impose a fixed refresh timeout or a user-configurable timeout in v1. A request requiring refresh, and concurrent requests waiting on that same single-flight refresh, may wait until the existing projection refresh operation completes or reports failure.
- Confirmed by the user on 2026-11-06: when refresh fails, the status endpoint returns a successful structured status payload, including stale state, last successful refresh time and diagnostic summary, so the dashboard remains useful for diagnosis. Tree/detail business endpoints return a standard error rather than silently serving the stale projection.
- Confirmed by the user on 2026-11-06: concurrent requests needing refresh share one in-flight refresh and only observe an atomically published complete refresh result or the explicitly stale retained prior snapshot; never a partially updated projection.

## Resolution

The user confirmed on 2026-11-06 that the dashboard uses browser full-page refresh only, matching the current Web UI; backend synchronization remains request-triggered lightweight inventory checking followed by refresh only on detected change. No refresh timeout is added in v1. Status reads report stale/last-success/diagnostic as structured data on failure, while tree/detail reads fail rather than serve stale results. Refresh stays single-flight and atomic.

## Handoff

The current Web UI's reload convention is documented in `waymark/crates/waymark-web/ui/src/lib/components/Explorer.svelte`; the implementation may reuse the existing core inventory/fingerprint checks. The actual UI/API representation and test cases belong in the functional specification after the remaining dashboard metrics and index-summary decisions settle.
