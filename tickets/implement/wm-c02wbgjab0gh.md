---
schema: 1
id: wm-c02wbgjab0gh
title: Complete CLI and MCP adapters
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:20Z
updated_at: 2026-11-03T23:52:21Z
closed_at: 2026-11-03T23:52:20Z
parent: wm-y03fa0a9bfbz
dependencies:
  - wm-4shr3qh2q0av
  - wm-a5zg5xhpeppm
  - wm-bn5d8k7zd729
  - wm-exzem3rwbmc6
  - wm-wqhfkq4cgcr0
  - wm-zryz3g5x4040
---
## Parent

[Waymark general-purpose tracker functional specification](wm-y03fa0a9bfbz)

## What to build

As a human or agent, I can use CLI and MCP to perform the supported Waymark operations through the same domain semantics, so interfaces do not drift.

## Acceptance criteria

- [ ] CLI covers initialization, configuration, diagnostics, CRUD, lifecycle, relationships, queries, migration, statistics, history/diff access, and index maintenance where available.
- [ ] MCP covers agent-facing reads/search and mediated writes.
- [ ] Both adapters reuse shared lifecycle, relationship, custom-field, authorization, concurrency, migration, and index rules.
- [ ] CLI human output, CLI machine envelope, and MCP structured results are contract-tested.
- [ ] Waymark map/frontier semantics and import/export are absent.

## Blocked by

- T2: Establish typed errors, error codes, envelopes, and diagnostics.
- T3: Complete lifecycle, archive, and relationship semantics.
- T4: Implement custom fields and generic queries.
- T5: Implement configuration and frontmatter migration.
- T6: Implement owner guardrails and safe batch writes.
- T7: Establish derived index and unified read path.
