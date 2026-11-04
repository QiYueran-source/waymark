---
schema: 1
id: wm-4shr3qh2q0av
title: Complete lifecycle, archive, and relationship semantics
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

As a contributor, I can manage lifecycle, archive, parent, hard dependency, and soft-link relationships and query tree, leaf, ready, and blocked work, so work state and structure remain trustworthy.

## Acceptance criteria

- [ ] Persisted lifecycle status is exactly `open`, `in_progress`, or `closed`; `blocked` and `ready` are computed, not writable states.
- [ ] Set `closed_at` when status becomes closed and clear it when leaving closed; it records only the latest transition, not audit history.
- [ ] Return read-only `blocked_by` listing unfinished hard-dependency IDs; missing hard-dependency targets remain blocking and are separately reported as dangling.
- [ ] Archive is an irreversible-through-Waymark logical deletion: append `archived_at` and optional `archive_reason` to the original frontmatter, preserve the other fields, do not update `updated_at`, and provide no restore command.
- [ ] Only the current owner may archive; archived items are read-only, repeated archive returns `ALREADY_ARCHIVED`, and active children cause `HAS_ACTIVE_CHILDREN` without cascade.
- [ ] Archive requires a complete fresh in-process frontmatter projection and atomically cleans inbound core `dependencies`, `parent`, and `soft_links`; body/custom-field references are untouched and affected active items update `updated_at`.
- [ ] Parent, hard dependency, and symmetric soft-link semantics remain independent across categories; relationships are unique, sorted, reject self-reference, and cannot newly target archived items.
- [ ] Missing soft-link targets do not affect readiness but are reported diagnostically; invalid references and cycles return actionable diagnostics.
- [ ] Missing references, self-links, parent cycles, and dependency cycles return actionable diagnostics.
- [ ] Generic tree and leaf queries work without Wayfinder-specific frontier semantics.

## Blocked by

- T1: Establish Rust core and native work-item file loop.
- T2: Establish typed errors, error codes, envelopes, and diagnostics.
