---
schema: 1
id: wm-3v6nq7wdav86
title: A closed parent is visible on list and ready
category: implement
status: closed
owner: frank
creator: Frank
created_at: 2026-11-12T06:27:27Z
updated_at: 2026-11-12T08:42:15Z
closed_at: 2026-11-12T08:42:15Z
parent: wm-62yfmh5j41t5
dependencies:
  - wm-363qccgxcw0q
---
## Parent

[Cross-item write consent, archive safety, and dangling references](wm-62yfmh5j41t5)

## What to build

An active item whose direct parent is closed says so where people look. It appears in list reads and ready reads, and in `doctor`, without any document changing and without altering the item's readiness, blockage, or lifecycle state. A closed child under an open parent produces no signal, and ancestors further up are never inspected.

## Acceptance criteria

- [ ] An active item whose direct parent is closed carries a closed-parent signal in list and ready results
- [ ] Closing a parent writes no file at all, and the child's `ready`, `blocked`, and status are unchanged
- [ ] `doctor` reports the condition as a non-error observation
- [ ] A child of a closed parent that is itself closed produces no signal
- [ ] A parent chain longer than one level produces no signal from an ancestor above the direct parent
- [ ] The signal reaches both adapters through the shared view layer, not separately

## Blocked by

- wm-363qccgxcw0q — Give cross-item writes a place to say yes
