---
schema: 1
id: wm-0c5qz2jk17sx
title: link add records the link on the caller's side only
category: implement
status: open
owner: frank
creator: Frank
created_at: 2026-11-12T06:27:39Z
updated_at: 2026-11-12T06:27:45Z
parent: wm-62yfmh5j41t5
dependencies:
  - wm-363qccgxcw0q
---
## Parent

[Cross-item write consent, archive safety, and dangling references](wm-62yfmh5j41t5)

## What to build

Adding a soft link becomes a claim about my own item. Without consent, `link add` records the link on the item I named and reports that the other side was not updated. The partner's document, including its `updated_at`, keeps its exact bytes whether or not the other item belongs to me.

## Acceptance criteria

- [ ] `link add` without consent leaves the partner document byte-identical, `updated_at` included
- [ ] The result names the partner as unsynchronised and says the link is recorded on one side only
- [ ] The command succeeds; it is not refused for lack of consent
- [ ] A repeat of the same non-consenting command does not rewrite the caller's document a second time
- [ ] The human output says plainly that the other side did not change

## Blocked by

- wm-363qccgxcw0q — Give cross-item writes a place to say yes
