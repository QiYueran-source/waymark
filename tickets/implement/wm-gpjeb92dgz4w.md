---
schema: 1
id: wm-gpjeb92dgz4w
title: Filtering keeps required ancestors as marked temporary context
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-04T01:03:49Z
updated_at: 2026-11-04T11:30:23Z
closed_at: 2026-11-04T11:30:23Z
parent: wm-7k3m4p9x2cqd
dependencies:
  - wm-9060dq38kwb2
---
## What to build

Filtering by category must never falsify parentage. When a selected work item's real parent belongs to an unselected category, that ancestor stays in the tree so the item keeps its true depth and parent-child relation, and it is visibly marked as outside the current selection. A selected child is never promoted to a root.

Ancestors kept this way are presentation context only: they are excluded from the summary counts of selected work and must be unmistakable at a glance, not merely a slightly different shade.

## Acceptance criteria

- [ ] a selected item whose parent is in an unselected category stays under that parent instead of becoming a root
- [ ] every ancestor kept only for context is marked as outside the selection in both the row treatment and its accessible name
- [ ] the marking is visually prominent enough to distinguish context rows from selected rows without hovering
- [ ] the whole ancestor chain is kept when several levels are outside the selection
- [ ] context rows do not inflate the count of selected work items in the summary
- [ ] the existing unresolved-relations area is unaffected and is not used to hide context ancestors
- [ ] frontend tests cover one-level context, multi-level context, no promotion to root, the accessible marking, and summary counts
