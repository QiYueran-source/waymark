---
schema: 1
id: waym-940D5VC9
title: Projection, completeness, and cache
status: closed
type: feature
priority: 2
created: 2026-11-03T05:43:42.003Z
updated: 2026-11-03T16:50:39.546Z
closed: 2026-11-03T16:50:39.546Z
parent: waym-WMSPEC01
labels: [ready-for-agent]
deps: [waym-Y1ZTWK25]
relates: []
duplicates: []
supersedes: []
---
## Parent

waym-WMSPEC01 — Waymark functional specification.

## What to build

Reads stop assuming a complete world. An in-process, disposable projection is derived from files with explicit completeness and freshness, scoped local reads still work while another file is broken, global graph reads fail explicitly instead of returning a partial graph, and a bounded cache can never serve a stale answer.

## Acceptance criteria

- [ ] the projection is rebuilt from files, is never authoritative, and is either complete and fresh or explicitly incomplete
- [ ] a scoped local read works while another file is unparseable, and identifies itself as scoped
- [ ] global list, tree, readiness, blocked, statistics, archive, and migration fail with `INCOMPLETE_PROJECTION` when completeness is unavailable
- [ ] a metadata-only file change with an equal content fingerprint retains the parsed entry
- [ ] the bounded cache holds body, detail, or query data only, is fingerprint-bound, and never evicts relationship nodes from a complete projection
- [ ] a global read that meets a concurrent multi-file write returns retryable `SNAPSHOT_UNSTABLE` after bounded retries rather than a torn snapshot