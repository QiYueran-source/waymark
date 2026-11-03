---
schema: 1
id: waym-TKF37ESV
title: Establish derived index and unified read path
status: open
type: feature
priority: 2
created: 2026-11-02T12:09:28.237Z
updated: 2026-11-02T12:10:13.491Z
parent: waym-DZ3Q9D11
labels: [ready-for-agent]
deps: [waym-419ZAWN7, waym-M1A7HXC2, waym-N0NGA6QM, waym-S93FPPE8]
relates: []
duplicates: []
supersedes: []
---
## Parent

[Waymark general-purpose tracker functional specification](waym-DZ3Q9D11)

## What to build

As a contributor, I can list and query a growing project consistently whether the derived index is fresh, stale, corrupt, or unavailable, so source files remain authoritative without sacrificing practical reads.

## Acceptance criteria

- [ ] Work-item Markdown files are the sole authority; v1 persists no JSON index. A complete in-process static frontmatter projection is rebuilt or safely incrementally synchronized from files.
- [ ] Projection entries track source path, fingerprints, parse diagnostics, and forward/reverse core relationships; derived `ready`, `blocked`, `blocked_by`, and graph diagnostics are computed dynamically.
- [ ] Completeness/freshness checks detect changed, missing, added, deleted, moved, or uncertain files; local show may use scoped temporary reads, while global queries and cross-file writes require a complete fresh projection.
- [ ] Long-lived contexts reuse the projection after lightweight checks; parse failures permit valid local reads but block complete graph operations. No stale projection result is returned as current.
- [ ] Body/detail/query data may use a bounded fingerprint-bound LRU; relation nodes cannot be partially evicted and cache settings cannot change semantics.
- [ ] List, query, tree, leaf, ready, blocked, stats, and search share one read contract with parity tests; relationship queries prefer the validated in-process projection to avoid repeated frontmatter I/O.

## Blocked by

- T2: Establish typed errors, error codes, envelopes, and diagnostics.
- T3: Complete lifecycle, archive, and relationship semantics.
- T4: Implement custom fields and generic queries.
- T5: Implement configuration and frontmatter migration.