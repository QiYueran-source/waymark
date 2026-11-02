---
schema: 1
id: waym-419ZAWN7
title: Establish typed errors, error codes, envelopes, and diagnostics
status: open
type: feature
priority: 2
created: 2026-11-02T12:09:28.164Z
updated: 2026-11-02T12:10:13.274Z
parent: waym-DZ3Q9D11
labels: [ready-for-agent]
deps: [waym-3HSJH4RT]
relates: []
duplicates: []
supersedes: []
---
## Parent

[Waymark general-purpose tracker functional specification](waym-DZ3Q9D11)

## What to build

As a CLI or MCP caller, I receive stable, machine-readable success and failure results with actionable diagnostics, so all adapters share one error contract.

## Acceptance criteria

- [ ] Core modules expose typed enumerable errors with stable string codes and no panic-based normal control flow.
- [ ] Errors support version, message, typed details, location, and retryable classification.
- [ ] CLI exit-code mapping and MCP business-error representation are defined without a second MCP business taxonomy.
- [ ] Success and failure envelopes are contract-tested.
- [ ] Health diagnostics report severity, code, location, message, and fixability; cycle detection reports structured paths.
- [ ] Batch failure details can represent per-target outcomes and rollback failure.

## Blocked by

- T1: Establish Rust core and native work-item file loop.