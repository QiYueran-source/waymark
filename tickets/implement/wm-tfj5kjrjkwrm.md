---
schema: 1
id: wm-tfj5kjrjkwrm
title: Graph presentation renders the hierarchy
category: implement
status: open
owner: frank
creator: frank
created_at: 2026-11-04T01:03:55Z
updated_at: 2026-11-04T01:03:55Z
parent: wm-7k3m4p9x2cqd
dependencies:
  - wm-y0cjx82t4982
---
## What to build

Add a graph presentation beside the existing tree, so the same project can be read spatially. A presentation control switches between the tree and the graph, and the choice is remembered per project.

The graph renders every active work item from the same tree read the tree view already uses, arranged in a fixed top-to-bottom hierarchy: each item sits below its parent, and parent-child relations are solid edges. Nodes carry the category's stable shape and name and the existing work-state colour and blocked marker. Every parentless active item is a root; no synthetic root is invented. A concise legend defines the solid edge as parent-child. Dependency edges are not part of this slice.

The graph is read-only: no gesture creates, edits, moves, or re-parents anything.

## Acceptance criteria

- [ ] a presentation control switches between tree and graph, and the chosen presentation persists per project
- [ ] the graph renders from the existing active tree read and introduces no new endpoint or second data source
- [ ] parents are positioned above their children, and parent-child relations render as solid edges
- [ ] every active work item appears exactly once, and each parentless item is a root with no synthetic root node
- [ ] nodes show the category shape and name together with the existing state colour and blocked marker
- [ ] a legend states that solid edges mean parent-child
- [ ] no dependency edges render, even when dependency visibility is on
- [ ] the graph emits no non-GET request and no mutation, and nodes cannot be dragged
- [ ] frontend tests cover switching and persistence, placement below parents, single appearance of each item, multiple roots, category and state cues, and the legend
