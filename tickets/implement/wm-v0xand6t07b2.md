---
schema: 1
id: wm-v0xand6t07b2
title: Selecting a graph node opens the read-only detail
category: implement
status: open
owner: frank
creator: frank
created_at: 2026-11-04T01:04:15Z
updated_at: 2026-11-04T01:04:15Z
parent: wm-7k3m4p9x2cqd
dependencies:
  - wm-tfj5kjrjkwrm
---
## What to build

Selecting a node in the graph opens the same read-only item detail the tree already opens, so one item behaves the same whichever presentation the reader is in. The selected node is visibly marked while its detail is open, and closing the detail returns focus to the node that opened it and leaves the reader's place in the graph intact.

The detail keeps its existing contents and its safe Markdown rendering; this slice adds a second way to reach it, not a second detail view. Keyboard users can reach and select nodes without a pointer.

## Acceptance criteria

- [ ] selecting a graph node opens the existing detail panel with the same contents as from the tree
- [ ] the selected node is visibly marked for as long as its detail is open
- [ ] closing the detail returns focus to the node that opened it
- [ ] the graph keeps its viewport and collapse state across opening and closing a detail
- [ ] nodes are reachable and selectable by keyboard with a visible focus indicator
- [ ] the detail remains read-only and unsafe body content still cannot execute
- [ ] frontend tests cover node selection, detail contents, selected marking, focus return, viewport preservation, and keyboard selection
