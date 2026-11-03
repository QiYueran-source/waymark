---
schema: 1
id: waym-CN27B5ND
title: Add Git history diff statistics and diagnostics entrypoints
status: closed
type: feature
priority: 2
created: 2026-11-02T12:09:28.266Z
updated: 2026-11-03T03:15:31.150Z
closed: 2026-11-03T03:15:31.150Z
parent: waym-DZ3Q9D11
labels: [ready-for-agent]
deps: [waym-419ZAWN7, waym-7SFVF8K1, waym-TKF37ESV]
relates: []
duplicates: []
supersedes: []
---
## Parent

[Waymark general-purpose tracker functional specification](waym-DZ3Q9D11)

## What to build

As a project maintainer, I can inspect Git history/diffs and obtain project statistics and health reports through supported interfaces, so repository state is observable without a separate audit log.

## Acceptance criteria

- [ ] Git history and worktree/baseline diff operations return structured success or Git errors.
- [ ] Statistics use the shared read semantics and do not invent a second data model.
- [ ] Diagnostic entrypoints expose findings and strict/fix behavior defined by the shared contract.
- [ ] Git failure is reported honestly; no synthetic history or independent application audit log is introduced.

## Blocked by

- T2: Establish typed errors, error codes, envelopes, and diagnostics.
- T7: Establish derived index and unified read path.
- T8: Complete CLI and MCP adapters.