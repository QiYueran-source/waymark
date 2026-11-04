---
schema: 1
id: wm-2e3712xbs4fd
title: Graph filtering, archive exclusion, and unresolved display
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-04T01:04:22Z
updated_at: 2026-11-04T11:30:23Z
closed_at: 2026-11-04T11:30:23Z
parent: wm-7k3m4p9x2cqd
dependencies:
  - wm-gpjeb92dgz4w
  - wm-tfj5kjrjkwrm
---
## What to build

Make the graph obey the same rules the tree already obeys. Category selection filters the graph exactly as it filters the by-category tree, including keeping required ancestors as prominently marked temporary context rather than promoting a selected child to a root. The summary counting rule is the same in both presentations.

Archived work items are treated as deleted: they appear nowhere in the graph, not as nodes, not as dependency targets, and not as placeholders. Work items whose parent reference is missing or forms a cycle appear in the existing unresolved-relations area with their reason, never as ordinary roots and never behind an invented parent node.

Clearing every category, or selecting categories that contain no active work, shows an explicit empty state with a way to select all categories. An incomplete or damaged project read stays the explicit read failure it already is; no partial graph is presented as complete.

## Acceptance criteria

- [ ] the category selection control filters the graph with the same multi-select, select-all, and clear-all behaviour as the tree
- [ ] required ancestors outside the selection remain as prominently marked temporary context in the graph
- [ ] a selected item whose parent is unselected is not promoted to a root in the graph
- [ ] archived items appear nowhere in the graph, including as dependency targets and as placeholders
- [ ] missing-parent and parent-cycle items appear in the unresolved-relations area with their reason, not as roots
- [ ] no synthetic parent node is created for unresolved items
- [ ] clearing all categories and selecting empty categories each show an explicit empty state with a select-all recovery action
- [ ] an incomplete projection still renders the existing read failure, never a partial graph
- [ ] frontend tests cover filtering parity, context marking and non-promotion, archive absence, unresolved display with reasons, both empty states, and the incomplete-projection failure
