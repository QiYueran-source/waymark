---
schema: 1
id: wm-th2jsrz57c98
title: Add the Web active-item index list API and page
category: implement
status: open
owner: Frank
creator: Frank
created_at: 2026-11-06T08:40:44Z
updated_at: 2026-11-06T08:41:56Z
parent: wm-xm4zx172qpd2
dependencies:
  - wm-j0hcw5wzzhjc
---
## Parent

[Waymark Web cache and index status dashboard](wm-xm4zx172qpd2)

## What to build

The dashboard's active-item index half: a read-only API route and page section that lists active work items — ID, title, category, lifecycle status, and project-relative path — 50 per page ordered by stable ID ascending, with previous/next navigation. A user can see at a glance what the service has loaded, and an incomplete or stale projection is shown only behind a prominent diagnostic banner that states the rows are not a complete/current index.

## Acceptance criteria

- [ ] The index route returns active work items only, 50 rows per page ordered by stable ID ascending, with the page bound enforced by the API rather than trusted from the client, and previous/next navigation driven by page offsets.
- [ ] Rows contain only ID, title, category, lifecycle status, and project-relative path; absolute paths, owners, relationships, Markdown bodies, and arbitrary custom fields are never exposed, and archived items are excluded from rows and counts.
- [ ] When the projection is incomplete or stale, the route still returns the parseable rows only with prominent incomplete/stale state, the failure count, the diagnostic summary, and a statement that the rows are not a complete/current index.
- [ ] The dashboard page renders the index section with pagination controls and the diagnostic banner behavior above.
- [ ] Real HTTP tests cover ordering, page size, pagination, archived exclusion, row field limits, the incomplete/stale labeling, and the read-only proof that no dashboard interaction changes one byte of the project (prior art: `waymark/crates/waymark-web/tests/reads.rs` and `waymark/crates/waymark-web/tests/protocol.rs`).

## Blocked by

- [Add shared Web projection and cache lifecycle](wm-j0hcw5wzzhjc)

**Status:** ready-for-agent
