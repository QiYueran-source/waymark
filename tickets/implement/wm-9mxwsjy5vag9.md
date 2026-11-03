---
schema: 1
id: wm-9mxwsjy5vag9
title: Parent, soft links, and tree reads
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:21Z
updated_at: 2026-11-03T23:52:22Z
closed_at: 2026-11-03T23:52:21Z
parent: wm-srxj8ztv1zfz
dependencies:
  - wm-8b80a5bk80wf
---
## Parent

wm-srxj8ztv1zfz — Waymark functional specification.

## What to build

Hierarchy and non-blocking relations complete the graph: a single cross-category parent, symmetric soft links maintained on both sides, and tree reads for both the hierarchy and the dependency graph.

## Acceptance criteria

- [ ] a work item has at most one parent, parentage may cross categories, and it neither blocks nor cascades lifecycle
- [ ] a parent cycle is rejected at write time; one introduced by an external edit is diagnosed, never repaired
- [ ] adding or removing a soft link updates both stored sides in one operation, so a one-sided link is never left behind
- [ ] hierarchy and dependency trees read to a bounded depth, with a flat form for the dependency tree
- [ ] a relationship pointing at a missing target is persisted as written and diagnosed as dangling
