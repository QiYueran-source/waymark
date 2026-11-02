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

- [ ] A deterministic Git-tracked JSON index is derived from work-item files and never overwrites them.
- [ ] Freshness checks detect changed, missing, or uncertain source files.
- [ ] Stale, corrupt, or unsupported indexes rebuild from authoritative files with structured diagnostics.
- [ ] File-scan fallback returns correct results when the index is unavailable.
- [ ] List, query, tree, leaf, ready, blocked, and search share one read contract with parity tests.

## Blocked by

- T2: Establish typed errors, error codes, envelopes, and diagnostics.
- T3: Complete lifecycle, archive, and relationship semantics.
- T4: Implement custom fields and generic queries.
- T5: Implement configuration and frontmatter migration.