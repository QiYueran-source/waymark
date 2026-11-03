---
schema: 1
id: waym-Q658VGH3
title: Lifecycle, dependencies, and readiness
status: closed
type: feature
priority: 2
created: 2026-11-03T05:43:38.266Z
updated: 2026-11-03T16:50:39.466Z
closed: 2026-11-03T16:50:39.466Z
parent: waym-WMSPEC01
labels: [ready-for-agent]
deps: [waym-0GQKACVA]
relates: []
duplicates: []
supersedes: []
---
## Parent

waym-WMSPEC01 — Waymark functional specification.

## What to build

Lifecycle and readiness become real: explicit status transitions with `closed_at`, hard dependencies, and derived readiness. `ready` and `blocked` report the current truth, a closed item stops blocking its dependents, and a missing dependency target is reported as dangling while it blocks.

## Acceptance criteria

- [ ] status accepts only open, in progress, or closed; entering closed sets `closed_at`, leaving it clears it, and repeating the current status changes nothing
- [ ] adding a dependency rejects self-reference, duplicate edges, and cycles with `CYCLE_DETECTED` and the cycle path
- [ ] a non-closed item with an unfinished or missing dependency is blocked, and `blocked_by` lists those targets without persisting them
- [ ] closing a dependency makes its dependents ready
- [ ] a dangling dependency blocks readiness and is reported rather than hidden
- [ ] `ready` and `blocked` exclude closed and blocked items respectively and state their projection completeness