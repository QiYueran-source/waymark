---
schema: 1
id: waym-S93FPPE8
title: Complete lifecycle, archive, and relationship semantics
status: open
type: feature
priority: 2
created: 2026-11-02T12:09:28.178Z
updated: 2026-11-02T12:10:13.344Z
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

As a contributor, I can manage lifecycle, archive, parent, hard dependency, and soft-link relationships and query tree, leaf, ready, and blocked work, so work state and structure remain trustworthy.

## Acceptance criteria

- [ ] Open, in-progress, and closed states work; blocked is derived only from unfinished hard dependencies.
- [ ] Archive is reversible, preserves status and relationships, and does not satisfy dependencies.
- [ ] Parent, hard dependency, and soft link semantics remain independent across categories.
- [ ] Missing references, self-links, parent cycles, and dependency cycles return actionable diagnostics.
- [ ] Generic tree and leaf queries work without Wayfinder-specific frontier semantics.

## Blocked by

- T1: Establish Rust core and native work-item file loop.
- T2: Establish typed errors, error codes, envelopes, and diagnostics.