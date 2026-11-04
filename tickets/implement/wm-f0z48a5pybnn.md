---
schema: 1
id: wm-f0z48a5pybnn
title: Presentation-scoped expansion controls for tree and graph
category: implement
status: open
owner: frank
created_at: 2026-11-04T13:04:49Z
updated_at: 2026-11-04T13:04:49Z
parent: wm-7k3m4p9x2cqd
---
## What to build

Make expansion actions presentation-scoped. Tree presentation shows the tree's Expand all/Collapse all; graph presentation shows graph-wide Expand all/Collapse all in the graph control area; neither changes the other presentation's state. The structured text alternative mirrors the graph's visible model: a collapsed branch appears as a "Collapsed — N children" row with its own expand control, and a "Show all" action restores the full hierarchy.

## Acceptance criteria

- [ ] tree Expand all/Collapse all affects only the tree
- [ ] graph Expand all/Collapse all lives in the graph controls and affects only the graph
- [ ] the two presentations' expansion states stay separate across switches
- [ ] the structured list shows a collapsed branch as a "Collapsed — N children" row with its own expand control
- [ ] the structured list offers "Show all"
- [ ] tests cover cross-presentation isolation and structured-list expansion
