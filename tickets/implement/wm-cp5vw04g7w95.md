---
schema: 1
id: wm-cp5vw04g7w95
title: Projection, completeness, and cache
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:20Z
updated_at: 2026-11-03T23:52:21Z
closed_at: 2026-11-03T23:52:20Z
parent: wm-srxj8ztv1zfz
dependencies:
  - wm-b9ac6qtng58a
---
## Parent

wm-srxj8ztv1zfz — Waymark functional specification.

## What to build

Reads stop assuming a complete world. An in-process, disposable projection is derived from files with explicit completeness and freshness, scoped local reads still work while another file is broken, global graph reads fail explicitly instead of returning a partial graph, and a bounded cache can never serve a stale answer.

## Acceptance criteria

- [ ] the projection is rebuilt from files, is never authoritative, and is either complete and fresh or explicitly incomplete
- [ ] a scoped local read works while another file is unparseable, and identifies itself as scoped
- [ ] global list, tree, readiness, blocked, statistics, archive, and migration fail with `INCOMPLETE_PROJECTION` when completeness is unavailable
- [ ] a metadata-only file change with an equal content fingerprint retains the parsed entry
- [ ] the bounded cache holds body, detail, or query data only, is fingerprint-bound, and never evicts relationship nodes from a complete projection
- [ ] a global read that meets a concurrent multi-file write returns retryable `SNAPSHOT_UNSTABLE` after bounded retries rather than a torn snapshot
