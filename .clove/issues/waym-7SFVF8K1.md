---
schema: 1
id: waym-7SFVF8K1
title: Complete CLI and MCP adapters
status: open
type: feature
priority: 2
created: 2026-11-02T12:09:28.253Z
updated: 2026-11-02T12:10:13.578Z
parent: waym-DZ3Q9D11
labels: [ready-for-agent]
deps: [waym-419ZAWN7, waym-AN5P37XV, waym-M1A7HXC2, waym-N0NGA6QM, waym-S93FPPE8, waym-TKF37ESV]
relates: []
duplicates: []
supersedes: []
---
## Parent

[Waymark general-purpose tracker functional specification](waym-DZ3Q9D11)

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