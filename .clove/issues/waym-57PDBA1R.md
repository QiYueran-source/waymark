---
schema: 1
id: waym-57PDBA1R
title: 'Diagnostics: validation and health checks'
status: closed
type: feature
priority: 2
created: 2026-11-03T05:43:42.043Z
updated: 2026-11-03T16:50:39.644Z
closed: 2026-11-03T16:50:39.644Z
parent: waym-WMSPEC01
labels: [ready-for-agent]
deps: [waym-EKWF2GCK]
relates: []
duplicates: []
supersedes: []
---
## Parent

waym-WMSPEC01 — Waymark functional specification.

## What to build

An operator can ask the project whether it is healthy and get precise, actionable findings: validation and diagnostics report data-integrity problems with severity, code, location, and fixability, never repair business data, and surface a held write lock without waiting.

## Acceptance criteria

- [ ] findings reuse the shared registry and carry severity, code, an optional location, a message, and a fixability flag
- [ ] validation and diagnostics cover malformed frontmatter, duplicate and mismatched IDs, unknown keys, missing or archived relationship targets, cycles, unsupported versions, projection state, and inventory changes
- [ ] diagnostics never delete a relationship, rewrite a body or custom field, or infer intent; only cache and projection rebuild is repaired, and it reports what changed
- [ ] a held write lock is reported as an info-level `LOCK_HELD` finding from a non-blocking probe, with no wait and no holder identity
- [ ] a warning-only report succeeds, and strict mode returns the invalid-data class when error findings remain