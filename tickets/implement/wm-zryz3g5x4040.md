---
schema: 1
id: wm-zryz3g5x4040
title: Implement custom fields and generic queries
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:21Z
updated_at: 2026-11-03T23:52:22Z
closed_at: 2026-11-03T23:52:21Z
parent: wm-y03fa0a9bfbz
dependencies:
  - wm-17g2jqzhc935
  - wm-bn5d8k7zd729
---
## Parent

[Waymark general-purpose tracker functional specification](wm-y03fa0a9bfbz)

## What to build

As a project maintainer, I can declare custom fields and query work items by those fields, so project-specific metadata is useful without changing core semantics.

## Acceptance criteria

- [ ] String, integer, boolean, and homogeneous scalar-list fields validate their declared shapes.
- [ ] Optional, required, and static creation-default behavior is deterministic; unset values are omitted and null is rejected.
- [ ] Undeclared fields, wrong types, nested values, and mixed lists return structured errors.
- [ ] Presence, equality, typed comparison, string matching, and explicit list membership queries work.
- [ ] Sorting and projection have deterministic missing-value behavior and strict operand validation.

## Blocked by

- T1: Establish Rust core and native work-item file loop.
- T2: Establish typed errors, error codes, envelopes, and diagnostics.
