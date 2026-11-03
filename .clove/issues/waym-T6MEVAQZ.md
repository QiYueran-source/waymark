---
schema: 1
id: waym-T6MEVAQZ
title: Parent, soft links, and tree reads
status: closed
type: feature
priority: 2
created: 2026-11-03T05:43:38.288Z
updated: 2026-11-03T16:50:39.485Z
closed: 2026-11-03T16:50:39.485Z
parent: waym-WMSPEC01
labels: [ready-for-agent]
deps: [waym-Q658VGH3]
relates: []
duplicates: []
supersedes: []
---
## Parent

waym-WMSPEC01 — Waymark functional specification.

## What to build

Hierarchy and non-blocking relations complete the graph: a single cross-category parent, symmetric soft links maintained on both sides, and tree reads for both the hierarchy and the dependency graph.

## Acceptance criteria

- [ ] a work item has at most one parent, parentage may cross categories, and it neither blocks nor cascades lifecycle
- [ ] a parent cycle is rejected at write time; one introduced by an external edit is diagnosed, never repaired
- [ ] adding or removing a soft link updates both stored sides in one operation, so a one-sided link is never left behind
- [ ] hierarchy and dependency trees read to a bounded depth, with a flat form for the dependency tree
- [ ] a relationship pointing at a missing target is persisted as written and diagnosed as dangling