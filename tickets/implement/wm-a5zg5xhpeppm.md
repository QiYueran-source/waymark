---
schema: 1
id: wm-a5zg5xhpeppm
title: Implement owner guardrails and safe batch writes
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
  - wm-bn5d8k7zd729
---
## Parent

[Waymark general-purpose tracker functional specification](wm-y03fa0a9bfbz)

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
