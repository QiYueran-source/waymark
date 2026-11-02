---
schema: 1
id: waym-M1A7HXC2
title: Implement custom fields and generic queries
status: open
type: feature
priority: 2
created: 2026-11-02T12:09:28.192Z
updated: 2026-11-02T12:10:13.372Z
parent: waym-DZ3Q9D11
labels: [ready-for-agent]
deps: [waym-3HSJH4RT, waym-419ZAWN7]
relates: []
duplicates: []
supersedes: []
---
## Parent

[Waymark general-purpose tracker functional specification](waym-DZ3Q9D11)

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