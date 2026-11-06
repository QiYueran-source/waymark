---
schema: 1
id: wm-bzn9qegc2cvt
title: Choose safe projection index details for the dashboard
category: map
status: closed
owner: Frank
creator: Frank
created_at: 2026-11-06T06:49:13Z
updated_at: 2026-11-06T08:09:28Z
closed_at: 2026-11-06T08:09:28Z
parent: wm-ksfpdfyj6rzz
dependencies:
  - wm-26vdk4wzezyh
---
## Type

grilling

## Question

Which projection/index details may the read-only Web dashboard display? Decide whether it shows only counts and distributions or a paginated per-work-item summary, and define the permitted fields, ordering, paging limits, archive treatment, and behavior for incomplete projections. Do not expose Markdown bodies or arbitrary custom-field values unless explicitly justified.

## Decisions confirmed so far

- Confirmed by the user on 2026-11-06: the dashboard includes a paginated index summary list, not only aggregate counts. Each row exposes stable ID, title, category, lifecycle status and project-relative path. It does not expose owner, parent/dependency relationships, Markdown body, custom fields, cache keys or cache values, and it never exposes an absolute filesystem path.
- Confirmed by the user on 2026-11-06: the list contains active work items only. Archived items are excluded from the dashboard's item count and pages in v1; archive browsing is out of scope.
- Confirmed by the user on 2026-11-06: the list is ordered by stable ID ascending and paginated at 50 rows per page with previous/next navigation. No search or filter control is added in v1.
- Confirmed by the user on 2026-11-06: when the Projection is incomplete or stale, the dashboard may return the parseable summary rows for diagnosis, but must prominently label the state, show failure count and diagnostic summary, and state that the rows are not a complete/current index.

## Resolution

The user confirmed the v1 projection summary contract on 2026-11-06: active items only, 50 per page, stable-ID ascending order, and a limited safe summary (ID, title, category, status, relative path). Incomplete or stale snapshots remain diagnostically listable only with explicit state and diagnostics; they are never represented as a complete/current index.

## Handoff

Reuse the existing active-item and relative-path view conventions. The API must bound page size rather than trust arbitrary client limits. The status response should carry completeness/freshness and diagnostic counts/details alongside the paginated rows.
