---
schema: 1
id: wm-7k80455wfcks
title: Graph navigation and per-node density controls
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-04T01:04:08Z
updated_at: 2026-11-04T11:30:23Z
closed_at: 2026-11-04T11:30:23Z
parent: wm-7k3m4p9x2cqd
dependencies:
  - wm-tfj5kjrjkwrm
---
## What to build

Make a large graph navigable without turning it into an editor. On first entry the graph shows the complete parent-child hierarchy fitted to the viewport. From there the reader can pan, zoom, fit the whole graph into view, and reset the viewport to that initial state. Individual nodes can be collapsed and expanded to manage density, and this affects presentation only.

Nodes cannot be dragged: no pointer gesture may imply or cause a relationship change. Common touchpad and touch navigation gestures are supported.

Viewport position and per-node collapse state are remembered per project and kept separate from the tree's own scroll and expansion state. IDs that no longer exist after project data changes are ignored rather than breaking the view.

## Acceptance criteria

- [ ] first entry shows the complete hierarchy fitted to the viewport
- [ ] pan, zoom, fit-to-view, and reset-viewport controls all work and reset returns to the fitted initial state
- [ ] collapsing a node hides its descendants, expanding restores them, and collapsing never changes any work item
- [ ] dragging a node does not move it or alter any relation
- [ ] common touchpad and touch gestures pan and zoom the canvas
- [ ] viewport and collapse state persist per project, are isolated from other projects, and are stored separately from tree scroll and expansion
- [ ] stored node IDs absent from the current data are ignored without error
- [ ] with browser storage unavailable the controls still work for the session and the persistence notice appears once
- [ ] frontend tests cover fit on entry, pan and zoom state, reset, collapse and expand, drag refusal, persistence and isolation, stale IDs, and unavailable storage
