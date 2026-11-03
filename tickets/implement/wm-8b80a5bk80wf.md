---
schema: 1
id: wm-8b80a5bk80wf
title: Lifecycle, dependencies, and readiness
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:21Z
updated_at: 2026-11-03T23:52:22Z
closed_at: 2026-11-03T23:52:21Z
parent: wm-srxj8ztv1zfz
dependencies:
  - wm-5212df47s7mx
---
## Parent

wm-srxj8ztv1zfz — Waymark functional specification.

## What to build

Lifecycle and readiness become real: explicit status transitions with `closed_at`, hard dependencies, and derived readiness. `ready` and `blocked` report the current truth, a closed item stops blocking its dependents, and a missing dependency target is reported as dangling while it blocks.

## Acceptance criteria

- [ ] status accepts only open, in progress, or closed; entering closed sets `closed_at`, leaving it clears it, and repeating the current status changes nothing
- [ ] adding a dependency rejects self-reference, duplicate edges, and cycles with `CYCLE_DETECTED` and the cycle path
- [ ] a non-closed item with an unfinished or missing dependency is blocked, and `blocked_by` lists those targets without persisting them
- [ ] closing a dependency makes its dependents ready
- [ ] a dangling dependency blocks readiness and is reported rather than hidden
- [ ] `ready` and `blocked` exclude closed and blocked items respectively and state their projection completeness
