---
schema: 1
id: waym-DRMHS4C3
title: Read-only Web tree overview
status: open
type: feature
priority: 2
created: 2026-11-03T05:43:42.082Z
updated: 2026-11-03T05:43:42.082Z
parent: waym-WMSPEC01
labels: [ready-for-agent]
deps: [waym-Y1ZTWK25]
relates: []
duplicates: []
supersedes: []
---
## Parent

waym-WMSPEC01 — Waymark functional specification.

## What to build

A strictly read-only overview of the work tree in a browser: combined or category-grouped trees, work-state colouring, optional dependency overlays, item details, and view state remembered locally, with no write path anywhere.

## Acceptance criteria

- [ ] the tree renders non-archived items top-down, combined by default and grouped by category, with cross-category children still under their real parent
- [ ] categories are distinguished by stable shapes, and work state by colour plus a text marker for blocked
- [ ] dependency overlays are optional dashed edges with reference nodes for cross-category targets, toggled per view
- [ ] selecting an item opens a read-only detail with body, category, actual status, blocked reason, owner or executor, and relationships (the Web read model must exist even where the item's executor is a project custom field rather than a core field)
- [ ] expansion, dependency visibility, and scroll position persist in the browser and never in the repository
- [ ] archived items are excluded, and empty and unresolved-relations states are legible
- [ ] no request from the Web view can modify project data