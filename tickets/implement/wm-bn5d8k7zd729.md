---
schema: 1
id: wm-bn5d8k7zd729
title: Establish typed errors, error codes, envelopes, and diagnostics
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:20Z
updated_at: 2026-11-03T23:52:21Z
closed_at: 2026-11-03T23:52:20Z
parent: wm-y03fa0a9bfbz
dependencies:
  - wm-17g2jqzhc935
---
## Parent

[Waymark general-purpose tracker functional specification](wm-y03fa0a9bfbz)

## What to build

As a CLI or MCP caller, I receive stable, machine-readable success and failure results with actionable diagnostics, so all adapters share one error contract.

## Acceptance criteria

- [ ] Core modules expose typed enumerable errors with stable string codes and no panic-based normal control flow.
- [ ] Errors support version, message, typed details, location, and retryable classification.
- [ ] CLI exit-code mapping and MCP business-error representation are defined without a second MCP business taxonomy.
- [ ] Success and failure envelopes are contract-tested.
- [ ] Health diagnostics report severity, code, location, message, and fixability; cycle detection reports structured paths.
- [ ] Diagnostics and serialized read results distinguish derived `blocked_by` IDs from dangling hard-dependency references; `blocked_by` is never accepted as persisted input.
- [ ] Archive and projection failures have stable codes for `ALREADY_ARCHIVED`, `HAS_ACTIVE_CHILDREN`, incomplete projection, and rollback/recovery failure; body-edit conflicts reuse `WRITE_CONFLICT`.
- [ ] Batch failure details can represent per-target outcomes and rollback failure.

## Blocked by

- T1: Establish Rust core and native work-item file loop.
