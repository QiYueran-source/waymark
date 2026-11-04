---
schema: 1
id: wm-ja86cqdfrhhf
title: Add Git history diff statistics and diagnostics entrypoints
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:20Z
updated_at: 2026-11-03T23:52:22Z
closed_at: 2026-11-03T23:52:20Z
parent: wm-y03fa0a9bfbz
dependencies:
  - wm-bn5d8k7zd729
  - wm-c02wbgjab0gh
  - wm-wqhfkq4cgcr0
---
## Parent

[Waymark general-purpose tracker functional specification](wm-y03fa0a9bfbz)

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
