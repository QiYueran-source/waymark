---
schema: 1
id: wm-nqa1t7nq8wt1
title: Implement read-only Web tree overview
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:21Z
updated_at: 2026-11-03T23:52:23Z
closed_at: 2026-11-03T23:52:21Z
parent: wm-y03fa0a9bfbz
dependencies:
  - wm-4shr3qh2q0av
  - wm-bn5d8k7zd729
  - wm-c02wbgjab0gh
  - wm-wqhfkq4cgcr0
---
## Parent

[Waymark general-purpose tracker functional specification](wm-y03fa0a9bfbz)

## What to build

As a project member, I can browse non-archived work items in a simple read-only Web tree, so hierarchy, state, and relationships are understandable without editing project data.

## Acceptance criteria

- [ ] Combined and category-grouped trees work, with combined as default and no synthetic root.
- [ ] Parent edges, optional dependency overlays, category shapes, state colors, blocked markers, and cross-category dependency references render correctly.
- [ ] Read-only detail, node expand/collapse, Expand all, Collapse all, and browser-local view/scroll persistence work.
- [ ] Small screens, empty projects, no categories, and invalid/missing parent references have explicit behavior.
- [ ] Archived items are excluded, Web never writes project data, and no separate archive browser is provided.

## Blocked by

- T2: Establish typed errors, error codes, envelopes, and diagnostics.
- T3: Complete lifecycle, archive, and relationship semantics.
- T7: Establish derived index and unified read path.
- T8: Complete CLI and MCP adapters.
