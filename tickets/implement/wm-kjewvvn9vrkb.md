---
schema: 1
id: wm-kjewvvn9vrkb
title: Graph dependency overlay draws dashed directed edges
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-04T01:04:01Z
updated_at: 2026-11-04T11:30:23Z
closed_at: 2026-11-04T11:30:23Z
parent: wm-7k3m4p9x2cqd
dependencies:
  - wm-tfj5kjrjkwrm
---
## What to build

Bring dependencies into the graph without letting them compete with hierarchy. The existing dependency visibility control governs the graph as well: while it is off the graph shows only hierarchy, and while it is on each dependency relation renders as a dashed edge with an arrowhead pointing at the item being depended on.

The legend gains the dashed-edge meaning, so both line treatments are documented in one place. Dependencies remain derived data; nothing about them is editable from the graph.

## Acceptance criteria

- [ ] with dependency visibility off the graph shows no dependency edges at all
- [ ] with dependency visibility on every dependency of a visible item renders as a dashed edge
- [ ] each dependency edge carries a direction arrowhead pointing at the depended-on item, so direction reads without hovering
- [ ] the legend defines both the solid parent-child edge and the dashed dependency edge
- [ ] the dependency toggle's remembered per-view state continues to work and is not reset by switching presentation
- [ ] dependency edges never replace or hide the parent-child edge between the same two items
- [ ] frontend tests cover the off and on states, dashed treatment, arrow direction, and the two-entry legend
