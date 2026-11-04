---
schema: 1
id: wm-4833swnedkzf
title: 'Diagnostics: validation and health checks'
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:20Z
updated_at: 2026-11-03T23:52:21Z
closed_at: 2026-11-03T23:52:20Z
parent: wm-srxj8ztv1zfz
dependencies:
  - wm-khr7gj3jj03s
---
## Parent

wm-srxj8ztv1zfz — Waymark functional specification.

## What to build

An operator can ask the project whether it is healthy and get precise, actionable findings: validation and diagnostics report data-integrity problems with severity, code, location, and fixability, never repair business data, and surface a held write lock without waiting.

## Acceptance criteria

- [ ] findings reuse the shared registry and carry severity, code, an optional location, a message, and a fixability flag
- [ ] validation and diagnostics cover malformed frontmatter, duplicate and mismatched IDs, unknown keys, missing or archived relationship targets, cycles, unsupported versions, projection state, and inventory changes
- [ ] diagnostics never delete a relationship, rewrite a body or custom field, or infer intent; only cache and projection rebuild is repaired, and it reports what changed
- [ ] a held write lock is reported as an info-level `LOCK_HELD` finding from a non-blocking probe, with no wait and no holder identity
- [ ] a warning-only report succeeds, and strict mode returns the invalid-data class when error findings remain
