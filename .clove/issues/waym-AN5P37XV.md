---
schema: 1
id: waym-AN5P37XV
title: Implement owner guardrails and safe batch writes
status: open
type: feature
priority: 2
created: 2026-11-02T12:09:28.221Z
updated: 2026-11-02T12:10:13.432Z
parent: waym-DZ3Q9D11
labels: [ready-for-agent]
deps: [waym-419ZAWN7, waym-S93FPPE8]
relates: []
duplicates: []
supersedes: []
---
## Parent

[Waymark general-purpose tracker functional specification](waym-DZ3Q9D11)

## What to build

As a human owner, I can transfer accountability and perform safe mediated single or batch writes, so concurrent edits are not silently overwritten and authorization guardrails are consistent.

## Acceptance criteria

- [ ] Creator, owner, executor, and declared operator remain distinct; creation may assign another human owner.
- [ ] Only the current owner can initiate transfer, and later mediated writes check the declared operator.
- [ ] Project locking covers read-modify-write windows and optimistic fingerprints reject stale overwrites.
- [ ] Batch writes prevalidate all targets and provide application-level all-or-nothing behavior with rollback/recovery reporting.
- [ ] Conflicts, guardrail failures, and rollback failures use the shared structured error contract.

## Blocked by

- T2: Establish typed errors, error codes, envelopes, and diagnostics.
- T3: Complete lifecycle, archive, and relationship semantics.